use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use neo_media_core::{PixelFormat, PoolError};

use super::format_pool::{AcquireWriteRequest, FormatPool};
use super::pool_types::{
    BUDGET_PRESSURE_DROP_PERMILLE, FALLBACK_CAPACITY_NO_BUDGET, HARD_CEILING_CAPACITY,
    RAM_FALLBACK_CAPACITY_NO_BUDGET, RAM_HARD_CEILING_CAPACITY, REQUERY_INTERVAL_ACQUIRES,
    SAFETY_RATIO_PERMILLE_CEIL, SAFETY_RATIO_PERMILLE_FLOOR, SAFETY_RATIO_PERMILLE_INITIAL,
    SAFETY_RATIO_RELAX_STEP, SAFETY_RATIO_TIGHTEN_STEP,
};
use super::pool_types::{
    ConsumerQuota, KIND_PLAYBACK, MIN_CAPACITY, RAM_MIN_CAPACITY, bytes_per_frame,
    distribute_min_reserved,
};

pub type VramBudgetProvider = dyn Fn() -> Option<u64> + Send + Sync;
pub type RamBudgetProvider = dyn Fn() -> Option<u64> + Send + Sync;

pub struct NeoMediaCache {
    device: wgpu::Device,
    pub(super) pools: Mutex<HashMap<(PixelFormat, u32, u32), FormatPool>>,
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
