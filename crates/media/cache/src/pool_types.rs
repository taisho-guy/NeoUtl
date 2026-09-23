use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use neo_media_core::PixelFormat;

pub const KIND_PLAYBACK: u8 = 0;
pub const KIND_THUMBNAIL: u8 = 1;
pub const KIND_LUA_SAMPLE: u8 = 2;

pub const MIN_CAPACITY: usize = 3;
pub(super) const FALLBACK_CAPACITY_NO_BUDGET: usize = 6;
pub(super) const HARD_CEILING_CAPACITY: usize = 64;
pub(super) const REQUERY_INTERVAL_ACQUIRES: u64 = 120;
pub(super) const RECENT_DURATION_SAMPLES: usize = 16;
pub(super) const STALL_MEDIAN_MULTIPLIER: u32 = 3;
pub(super) const SAFETY_RATIO_PERMILLE_INITIAL: u32 = 500;
pub(super) const SAFETY_RATIO_PERMILLE_FLOOR: u32 = 300;
pub(super) const SAFETY_RATIO_PERMILLE_CEIL: u32 = 700;
pub(super) const SAFETY_RATIO_TIGHTEN_STEP: u32 = 50;
pub(super) const SAFETY_RATIO_RELAX_STEP: u32 = 10;
pub(super) const BUDGET_PRESSURE_DROP_PERMILLE: u64 = 900;

pub const RAM_MIN_CAPACITY: usize = 8;
pub(super) const RAM_FALLBACK_CAPACITY_NO_BUDGET: usize = 64;
pub(super) const RAM_HARD_CEILING_CAPACITY: usize = 900;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlotState {
    Free,
    Writing,
    Ready,
    Reading,
}

pub(super) struct Slot {
    pub(super) texture: wgpu::Texture,
    pub(super) state: SlotState,
    pub(super) fence: Option<wgpu::SubmissionIndex>,
    pub(super) kind_id: u8,
    pub(super) last_used: u64,
    pub(super) write_started_at: Option<Instant>,
}

impl Slot {
    pub(super) fn matches(&self, texture: &wgpu::Texture) -> bool {
        &self.texture == texture
    }
}

pub struct ConsumerQuota {
    pub kind_id: u8,
    pub priority: u8,
    pub(super) min_reserved: AtomicUsize,
}

impl ConsumerQuota {
    pub(super) fn new(kind_id: u8, priority: u8) -> Self {
        Self {
            kind_id,
            priority,
            min_reserved: AtomicUsize::new(0),
        }
    }
}

pub(super) fn distribute_min_reserved(quotas: &[ConsumerQuota], total_capacity: usize) {
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

pub(super) fn bytes_per_frame(format: PixelFormat, width: u32, height: u32) -> u64 {
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
