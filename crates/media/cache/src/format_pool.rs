use std::sync::atomic::Ordering;
use std::time::Instant;

use neo_media_core::{PixelFormat, PoolError};

use super::pool_types::{
    ConsumerQuota, RECENT_DURATION_SAMPLES, STALL_MEDIAN_MULTIPLIER, Slot, SlotState,
};

pub(super) struct FormatPool {
    format: PixelFormat,
    width: u32,
    height: u32,
    pub(super) slots: Vec<Slot>,
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

pub(super) struct AcquireWriteRequest<'a> {
    pub(super) device: &'a wgpu::Device,
    pub(super) capacity: usize,
    pub(super) kind_id: u8,
    pub(super) quotas: &'a [ConsumerQuota],
    pub(super) acquire_seq: u64,
    pub(super) clip_key_hint: &'a str,
}

impl FormatPool {
    pub(super) fn new(format: PixelFormat, width: u32, height: u32) -> Self {
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

    pub(super) fn acquire_for_write(
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

    pub(super) fn mark_ready(
        &mut self,
        texture: &wgpu::Texture,
        submission_index: wgpu::SubmissionIndex,
    ) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.matches(texture)) {
            slot.state = SlotState::Ready;
            slot.fence = Some(submission_index);
            if let Some(started) = slot.write_started_at.take() {
                self.record_write_duration(started.elapsed().as_micros() as u64);
            }
        }
    }

    pub(super) unsafe fn finalize_write(
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

    pub(super) fn acquire_for_read(
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

    pub(super) fn release_read(
        &mut self,
        texture: &wgpu::Texture,
        submission_index: wgpu::SubmissionIndex,
    ) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.matches(texture)) {
            slot.state = SlotState::Reading;
            slot.fence = Some(submission_index);
        }
    }

    pub(super) fn release_free(&mut self, texture: &wgpu::Texture) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.matches(texture)) {
            slot.state = SlotState::Free;
            slot.fence = None;
        }
    }
}
