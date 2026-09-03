use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use neo_media_core::{NeoFramePool, PixelFormat, PoolError};

pub const KIND_PLAYBACK: u8 = 0;
pub const KIND_THUMBNAIL: u8 = 1;
pub const KIND_LUA_SAMPLE: u8 = 2;

pub const MIN_CAPACITY: usize = 3;
const FALLBACK_CAPACITY_NO_BUDGET: usize = 6;
const HARD_CEILING_CAPACITY: usize = 64;
const REQUERY_INTERVAL_ACQUIRES: u64 = 120;
const RECENT_DURATION_SAMPLES: usize = 16;
const STALL_MEDIAN_MULTIPLIER: u32 = 3;
const SAFETY_RATIO_PERMILLE_INITIAL: u32 = 500;
const SAFETY_RATIO_PERMILLE_FLOOR: u32 = 300;
const SAFETY_RATIO_PERMILLE_CEIL: u32 = 700;
const SAFETY_RATIO_TIGHTEN_STEP: u32 = 50;
const SAFETY_RATIO_RELAX_STEP: u32 = 10;
const BUDGET_PRESSURE_DROP_PERMILLE: u64 = 900;

pub const RAM_MIN_CAPACITY: usize = 8;
const RAM_FALLBACK_CAPACITY_NO_BUDGET: usize = 64;
const RAM_HARD_CEILING_CAPACITY: usize = 900;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotState {
    Free,
    Writing,
    Ready,
    Reading,
}

struct Slot {
    texture: wgpu::Texture,
    state: SlotState,
    fence: Option<wgpu::SubmissionIndex>,
    kind_id: u8,
    last_used: u64,
    write_started_at: Option<Instant>,
}

impl Slot {
    fn matches(&self, texture: &wgpu::Texture) -> bool {
        &self.texture == texture
    }
}

pub struct ConsumerQuota {
    pub kind_id: u8,
    pub priority: u8,
    min_reserved: AtomicUsize,
}

impl ConsumerQuota {
    fn new(kind_id: u8, priority: u8) -> Self {
        Self {
            kind_id,
            priority,
            min_reserved: AtomicUsize::new(0),
        }
    }
}

fn distribute_min_reserved(quotas: &[ConsumerQuota], total_capacity: usize) {
    let priority_sum: u32 = quotas.iter().map(|q| q.priority as u32).sum();
    if priority_sum == 0 || quotas.is_empty() {
        return;
    }
    let mut remaining = total_capacity;
    for quota in quotas {
        let share =
            ((quota.priority as u64 * total_capacity as u64) / priority_sum as u64) as usize;
        let share = share.min(remaining).max(if remaining > 0 { 1 } else { 0 });
        quota.min_reserved.store(share, Ordering::Relaxed);
        remaining = remaining.saturating_sub(share);
    }
}

fn bytes_per_frame(format: PixelFormat, width: u32, height: u32) -> u64 {
    let pixels = width as u64 * height as u64;
    match format {
        PixelFormat::Nv12 => pixels + pixels / 2,
        PixelFormat::P010 | PixelFormat::P012 | PixelFormat::P016 => (pixels + pixels / 2) * 2,
        PixelFormat::Rgba8 => pixels * 4,
        PixelFormat::Rgba16Float => pixels * 8,
        PixelFormat::Yuv444 => pixels * 3,
        PixelFormat::Yuv420p => pixels + pixels / 2,
    }
}

struct FormatPool {
    format: PixelFormat,
    width: u32,
    height: u32,
    slots: Vec<Slot>,
    recent_write_durations_micros: Vec<u64>,
}

fn wgpu_texture_format(format: PixelFormat) -> Result<wgpu::TextureFormat, PoolError> {
    match format {
        PixelFormat::Nv12 => Ok(wgpu::TextureFormat::NV12),
        PixelFormat::Rgba8 => Ok(wgpu::TextureFormat::Rgba8Unorm),
        PixelFormat::Rgba16Float => Ok(wgpu::TextureFormat::Rgba16Float),
        PixelFormat::P010
        | PixelFormat::P012
        | PixelFormat::P016
        | PixelFormat::Yuv444
        | PixelFormat::Yuv420p => Err(PoolError::UnsupportedFormat(format)),
    }
}

fn texture_usage(format: PixelFormat) -> wgpu::TextureUsages {
    match format {
        PixelFormat::Rgba8 | PixelFormat::Rgba16Float => {
            wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
        }
        _ => wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
    }
}

fn hal_texture_uses(format: PixelFormat) -> wgpu::TextureUses {
    match format {
        PixelFormat::Rgba8 | PixelFormat::Rgba16Float => {
            wgpu::TextureUses::COPY_DST
                | wgpu::TextureUses::RESOURCE
                | wgpu::TextureUses::STORAGE_READ_WRITE
                | wgpu::TextureUses::COLOR_TARGET
        }
        _ => wgpu::TextureUses::COPY_DST | wgpu::TextureUses::RESOURCE,
    }
}

fn create_texture(
    device: &wgpu::Device,
    format: PixelFormat,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, PoolError> {
    let texture_format = wgpu_texture_format(format)?;
    Ok(device.create_texture(&wgpu::TextureDescriptor {
        label: Some("neo-media-cache-slot"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: texture_format,
        usage: texture_usage(format),
        view_formats: &[],
    }))
}

struct AcquireWriteRequest<'a> {
    device: &'a wgpu::Device,
    capacity: usize,
    kind_id: u8,
    quotas: &'a [ConsumerQuota],
    acquire_seq: u64,
    clip_key_hint: &'a str,
}

impl FormatPool {
    fn new(format: PixelFormat, width: u32, height: u32) -> Self {
        Self {
            format,
            width,
            height,
            slots: Vec::new(),
            recent_write_durations_micros: Vec::with_capacity(RECENT_DURATION_SAMPLES),
        }
    }

    fn reclaim_completed(&mut self, device: &wgpu::Device) {
        for slot in self.slots.iter_mut() {
            if slot.state != SlotState::Reading {
                continue;
            }
            if slot.fence.is_none() {
                continue;
            }
            let poll = device.poll(wgpu::PollType::Poll);
            if poll.is_ok_and(|status| status.wait_finished()) {
                slot.state = SlotState::Free;
                slot.fence = None;
            }
        }
    }

    fn record_write_duration(&mut self, micros: u64) {
        if self.recent_write_durations_micros.len() >= RECENT_DURATION_SAMPLES {
            self.recent_write_durations_micros.remove(0);
        }
        self.recent_write_durations_micros.push(micros);
    }

    fn median_write_duration_micros(&self) -> Option<u64> {
        if self.recent_write_durations_micros.is_empty() {
            return None;
        }
        let mut sorted = self.recent_write_durations_micros.clone();
        sorted.sort_unstable();
        Some(sorted[sorted.len() / 2])
    }

    fn detect_stalled_writers(&self, clip_key_hint: &str) {
        let Some(median) = self.median_write_duration_micros() else {
            return;
        };
        let threshold = median.saturating_mul(STALL_MEDIAN_MULTIPLIER as u64);
        for slot in self.slots.iter() {
            if slot.state != SlotState::Writing {
                continue;
            }
            let Some(started) = slot.write_started_at else {
                continue;
            };
            let elapsed_micros = started.elapsed().as_micros() as u64;
            if elapsed_micros > threshold && threshold > 0 {
                eprintln!(
                    "[neo-media-cache][異常検知] {clip_key_hint} writing状態滞留 経過={elapsed_micros}us 閾値={threshold}us(中央値{median}us x{STALL_MEDIAN_MULTIPLIER})"
                );
            }
        }
    }

    fn kind_usage_count(&self, kind_id: u8) -> usize {
        self.slots
            .iter()
            .filter(|s| s.kind_id == kind_id && s.state != SlotState::Free)
            .count()
    }

    fn find_over_quota_victim(
        &self,
        requesting_kind: u8,
        quotas: &[ConsumerQuota],
    ) -> Option<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.kind_id != requesting_kind
                    && matches!(s.state, SlotState::Free | SlotState::Ready)
            })
            .filter(|(_, s)| {
                let reserved = quotas
                    .iter()
                    .find(|q| q.kind_id == s.kind_id)
                    .map(|q| q.min_reserved.load(Ordering::Relaxed))
                    .unwrap_or(0);
                self.kind_usage_count(s.kind_id) > reserved
            })
            .min_by_key(|(_, s)| s.last_used)
            .map(|(i, _)| i)
    }

    fn acquire_for_write(
        &mut self,
        ctx: AcquireWriteRequest<'_>,
    ) -> Result<wgpu::Texture, PoolError> {
        let AcquireWriteRequest {
            device,
            capacity,
            kind_id,
            quotas,
            acquire_seq,
            clip_key_hint,
        } = ctx;
        self.reclaim_completed(device);
        let write_started = Instant::now();

        if let Some(slot) = self.slots.iter_mut().find(|s| s.state == SlotState::Free) {
            slot.state = SlotState::Writing;
            slot.kind_id = kind_id;
            slot.last_used = acquire_seq;
            slot.write_started_at = Some(write_started);
            return Ok(slot.texture.clone());
        }

        if self.slots.len() < capacity {
            let texture = create_texture(device, self.format, self.width, self.height)?;
            self.slots.push(Slot {
                texture: texture.clone(),
                state: SlotState::Writing,
                fence: None,
                kind_id,
                last_used: acquire_seq,
                write_started_at: Some(write_started),
            });
            return Ok(texture);
        }

        let reserved_for_kind = quotas
            .iter()
            .find(|q| q.kind_id == kind_id)
            .map(|q| q.min_reserved.load(Ordering::Relaxed))
            .unwrap_or(0);
        if self.kind_usage_count(kind_id) < reserved_for_kind {
            if let Some(idx) = self.find_over_quota_victim(kind_id, quotas) {
                let slot = &mut self.slots[idx];
                slot.state = SlotState::Writing;
                slot.kind_id = kind_id;
                slot.last_used = acquire_seq;
                slot.write_started_at = Some(write_started);
                return Ok(slot.texture.clone());
            }
        }

        self.detect_stalled_writers(clip_key_hint);
        Err(PoolError::Exhausted)
    }

    fn mark_ready(&mut self, texture: &wgpu::Texture, submission_index: wgpu::SubmissionIndex) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.matches(texture)) {
            slot.state = SlotState::Ready;
            slot.fence = Some(submission_index);
            if let Some(started) = slot.write_started_at.take() {
                self.record_write_duration(started.elapsed().as_micros() as u64);
            }
        }
    }

    unsafe fn finalize_write(
        &mut self,
        device: &wgpu::Device,
        texture: wgpu::Texture,
    ) -> Result<wgpu::Texture, PoolError> {
        let Some(slot) = self.slots.iter_mut().find(|s| s.matches(&texture)) else {
            return Err(PoolError::Exhausted);
        };

        let vk_image = unsafe {
            let Some(hal_texture) = texture.as_hal::<wgpu_hal::api::Vulkan>() else {
                return Err(PoolError::Exhausted);
            };
            hal_texture.raw_handle()
        };

        let hal_desc = wgpu_hal::TextureDescriptor {
            label: Some("neo-media-cache-slot-finalized"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_texture_format(self.format)?,
            usage: hal_texture_uses(self.format),
            memory_flags: wgpu_hal::MemoryFlags::empty(),
            view_formats: vec![],
        };

        let hal_wrapped = unsafe {
            let Some(hal_device) = device.as_hal::<wgpu_hal::api::Vulkan>() else {
                return Err(PoolError::Exhausted);
            };
            hal_device.texture_from_raw(
                vk_image,
                &hal_desc,
                Some(Box::new(|| {})),
                wgpu_hal::vulkan::TextureMemory::External,
            )
        };

        let wgpu_desc = wgpu::TextureDescriptor {
            label: Some("neo-media-cache-slot-finalized"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_texture_format(self.format)?,
            usage: texture_usage(self.format),
            view_formats: &[],
        };

        let finalized = unsafe {
            device.create_texture_from_hal::<wgpu_hal::api::Vulkan>(
                hal_wrapped,
                &wgpu_desc,
                wgpu::TextureUses::RESOURCE,
            )
        };

        slot.texture = finalized.clone();
        Ok(finalized)
    }

    fn acquire_for_read(
        &mut self,
        device: &wgpu::Device,
        acquire_seq: u64,
    ) -> Option<wgpu::Texture> {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.state == SlotState::Ready) {
            if let Some(index) = slot.fence.clone() {
                let _ = device.poll(wgpu::PollType::Wait {
                    submission_index: Some(index),
                    timeout: None,
                });
            }
            slot.state = SlotState::Reading;
            slot.fence = None;
            slot.last_used = acquire_seq;
            return Some(slot.texture.clone());
        }
        None
    }

    fn release_read(&mut self, texture: &wgpu::Texture, submission_index: wgpu::SubmissionIndex) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.matches(texture)) {
            slot.state = SlotState::Reading;
            slot.fence = Some(submission_index);
        }
    }

    fn release_free(&mut self, texture: &wgpu::Texture) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.matches(texture)) {
            slot.state = SlotState::Free;
            slot.fence = None;
        }
    }
}

pub type VramBudgetProvider = dyn Fn() -> Option<u64> + Send + Sync;
pub type RamBudgetProvider = dyn Fn() -> Option<u64> + Send + Sync;

pub struct NeoMediaCache {
    device: wgpu::Device,
    pools: Mutex<HashMap<(PixelFormat, u32, u32), FormatPool>>,
    quotas: Mutex<Vec<ConsumerQuota>>,
    vram_budget_bytes: AtomicU64,
    prev_vram_budget_bytes: AtomicU64,
    safety_ratio_permille: AtomicU32,
    acquire_counter: AtomicU64,
    budget_provider: Option<Arc<VramBudgetProvider>>,
    ram_budget_bytes: AtomicU64,
    ram_budget_provider: Option<Arc<RamBudgetProvider>>,
    ram_requery_counter: AtomicU64,
}

impl NeoMediaCache {
    pub fn new(
        device: wgpu::Device,
        budget_provider: Option<Arc<VramBudgetProvider>>,
        ram_budget_provider: Option<Arc<RamBudgetProvider>>,
    ) -> Self {
        let initial_budget = budget_provider.as_ref().and_then(|p| p()).unwrap_or(0);
        if initial_budget == 0 {
            eprintln!(
                "[neo-media-cache][診断] 初期VRAM予算取得失敗 フォールバック容量={FALLBACK_CAPACITY_NO_BUDGET}適用中"
            );
        }
        let initial_ram_budget = ram_budget_provider.as_ref().and_then(|p| p()).unwrap_or(0);
        if initial_ram_budget == 0 {
            eprintln!(
                "[neo-media-cache][診断] 初期RAM予算取得失敗 フォールバック容量={RAM_FALLBACK_CAPACITY_NO_BUDGET}適用中"
            );
        }
        Self {
            device,
            pools: Mutex::new(HashMap::new()),
            quotas: Mutex::new(Vec::new()),
            vram_budget_bytes: AtomicU64::new(initial_budget),
            prev_vram_budget_bytes: AtomicU64::new(initial_budget),
            safety_ratio_permille: AtomicU32::new(SAFETY_RATIO_PERMILLE_INITIAL),
            acquire_counter: AtomicU64::new(0),
            budget_provider,
            ram_budget_bytes: AtomicU64::new(initial_ram_budget),
            ram_budget_provider,
            ram_requery_counter: AtomicU64::new(0),
        }
    }

    pub fn register_consumer(&self, kind_id: u8, priority: u8) {
        let mut quotas = self.quotas.lock().expect("quotas mutex poisoned");
        if quotas.iter().any(|q| q.kind_id == kind_id) {
            return;
        }
        quotas.push(ConsumerQuota::new(kind_id, priority));
    }

    fn maybe_requery_budget(&self) {
        let seq = self.acquire_counter.fetch_add(1, Ordering::Relaxed);
        if seq % REQUERY_INTERVAL_ACQUIRES != 0 {
            return;
        }
        let Some(provider) = self.budget_provider.as_ref() else {
            return;
        };
        let Some(fresh) = provider() else {
            eprintln!(
                "[neo-media-cache][診断] VRAM予算取得失敗 acquire_seq={seq} フォールバック容量={FALLBACK_CAPACITY_NO_BUDGET}適用中"
            );
            return;
        };
        if fresh == 0 {
            eprintln!(
                "[neo-media-cache][診断] VRAM予算取得結果0バイト acquire_seq={seq} フォールバック容量={FALLBACK_CAPACITY_NO_BUDGET}適用中"
            );
        }
        let prev = self.vram_budget_bytes.swap(fresh, Ordering::Relaxed);
        self.prev_vram_budget_bytes.store(prev, Ordering::Relaxed);
        if prev > 0 {
            let ratio_permille = fresh.saturating_mul(1000) / prev.max(1);
            let current = self.safety_ratio_permille.load(Ordering::Relaxed);
            let adjusted = if ratio_permille < BUDGET_PRESSURE_DROP_PERMILLE {
                current
                    .saturating_sub(SAFETY_RATIO_TIGHTEN_STEP)
                    .max(SAFETY_RATIO_PERMILLE_FLOOR)
            } else {
                current
                    .saturating_add(SAFETY_RATIO_RELAX_STEP)
                    .min(SAFETY_RATIO_PERMILLE_CEIL)
            };
            self.safety_ratio_permille
                .store(adjusted, Ordering::Relaxed);
        }
    }

    fn maybe_requery_ram_budget(&self) {
        let seq = self.ram_requery_counter.fetch_add(1, Ordering::Relaxed);
        if seq % REQUERY_INTERVAL_ACQUIRES != 0 {
            return;
        }
        let Some(provider) = self.ram_budget_provider.as_ref() else {
            return;
        };
        let Some(fresh) = provider() else {
            eprintln!(
                "[neo-media-cache][診断] RAM予算取得失敗 acquire_seq={seq} フォールバック容量={RAM_FALLBACK_CAPACITY_NO_BUDGET}適用中"
            );
            return;
        };
        if fresh == 0 {
            eprintln!(
                "[neo-media-cache][診断] RAM予算取得結果0バイト acquire_seq={seq} フォールバック容量={RAM_FALLBACK_CAPACITY_NO_BUDGET}適用中"
            );
        }
        self.ram_budget_bytes.store(fresh, Ordering::Relaxed);
    }

    pub fn effective_capacity(&self, frame_bytes: u64) -> usize {
        let budget = self.vram_budget_bytes.load(Ordering::Relaxed);
        if budget == 0 || frame_bytes == 0 {
            return FALLBACK_CAPACITY_NO_BUDGET;
        }
        let ratio_permille = self.safety_ratio_permille.load(Ordering::Relaxed) as u64;
        let usable_bytes = budget.saturating_mul(ratio_permille) / 1000;
        let raw_capacity = (usable_bytes / frame_bytes) as usize;
        raw_capacity.clamp(MIN_CAPACITY, HARD_CEILING_CAPACITY)
    }

    pub fn effective_ram_capacity(&self, frame_bytes: u64) -> usize {
        self.maybe_requery_ram_budget();
        let budget = self.ram_budget_bytes.load(Ordering::Relaxed);
        if budget == 0 || frame_bytes == 0 {
            return RAM_FALLBACK_CAPACITY_NO_BUDGET;
        }
        let ratio_permille = self.safety_ratio_permille.load(Ordering::Relaxed) as u64;
        let usable_bytes = budget.saturating_mul(ratio_permille) / 1000;
        let raw_capacity = (usable_bytes / frame_bytes) as usize;
        raw_capacity.clamp(RAM_MIN_CAPACITY, RAM_HARD_CEILING_CAPACITY)
    }

    pub fn acquire_for_write(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
    ) -> Result<wgpu::Texture, PoolError> {
        self.acquire_for_write_as(KIND_PLAYBACK, format, width, height)
    }

    pub fn acquire_for_write_as(
        &self,
        kind_id: u8,
        format: PixelFormat,
        width: u32,
        height: u32,
    ) -> Result<wgpu::Texture, PoolError> {
        self.maybe_requery_budget();
        let frame_bytes = bytes_per_frame(format, width, height);
        let capacity = self.effective_capacity(frame_bytes);
        let quotas_guard = self.quotas.lock().expect("quotas mutex poisoned");
        distribute_min_reserved(&quotas_guard, capacity);

        let mut pools = self.pools.lock().expect("pools mutex poisoned");
        let pool = pools
            .entry((format, width, height))
            .or_insert_with(|| FormatPool::new(format, width, height));
        let acquire_seq = self.acquire_counter.load(Ordering::Relaxed);
        pool.acquire_for_write(AcquireWriteRequest {
            device: &self.device,
            capacity,
            kind_id,
            quotas: &quotas_guard,
            acquire_seq,
            clip_key_hint: "cache",
        })
    }

    pub fn mark_ready(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
        texture: &wgpu::Texture,
        submission_index: wgpu::SubmissionIndex,
    ) {
        let mut pools = self.pools.lock().expect("pools mutex poisoned");
        if let Some(pool) = pools.get_mut(&(format, width, height)) {
            pool.mark_ready(texture, submission_index);
        }
    }

    pub fn acquire_for_read(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
    ) -> Option<wgpu::Texture> {
        let acquire_seq = self.acquire_counter.load(Ordering::Relaxed);
        let mut pools = self.pools.lock().expect("pools mutex poisoned");
        let pool = pools.get_mut(&(format, width, height))?;
        pool.acquire_for_read(&self.device, acquire_seq)
    }

    pub fn release_read(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
        texture: &wgpu::Texture,
        submission_index: wgpu::SubmissionIndex,
    ) {
        let mut pools = self.pools.lock().expect("pools mutex poisoned");
        if let Some(pool) = pools.get_mut(&(format, width, height)) {
            pool.release_read(texture, submission_index);
        }
    }

    pub fn release_free_as(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
        texture: &wgpu::Texture,
    ) {
        let mut pools = self.pools.lock().expect("pools mutex poisoned");
        if let Some(pool) = pools.get_mut(&(format, width, height)) {
            pool.release_free(texture);
        }
    }
}

impl NeoFramePool for NeoMediaCache {
    fn acquire(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
    ) -> Result<wgpu::Texture, PoolError> {
        self.acquire_for_write(format, width, height)
    }

    fn release(&self, texture: wgpu::Texture) {
        let mut pools = self.pools.lock().expect("pools mutex poisoned");
        for pool in pools.values_mut() {
            pool.release_free(&texture);
        }
    }

    unsafe fn finalize_write(
        &self,
        device: &wgpu::Device,
        texture: wgpu::Texture,
    ) -> Result<wgpu::Texture, PoolError> {
        let mut pools = self.pools.lock().expect("pools mutex poisoned");
        for pool in pools.values_mut() {
            if pool.slots.iter().any(|s| s.matches(&texture)) {
                return unsafe { pool.finalize_write(device, texture) };
            }
        }
        Err(PoolError::Exhausted)
    }
}
