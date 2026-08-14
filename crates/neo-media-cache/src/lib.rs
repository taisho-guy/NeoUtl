use std::collections::HashMap;
use std::sync::Mutex;

use neo_media_core::{NeoFramePool, PixelFormat, PoolError};

pub const MIN_CAPACITY: usize = 3;
pub const MAX_CAPACITY: usize = 6;

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
}

impl Slot {
    fn matches(&self, texture: &wgpu::Texture) -> bool {
        &self.texture == texture
    }
}

struct FormatPool {
    format: PixelFormat,
    width: u32,
    height: u32,
    slots: Vec<Slot>,
    capacity: usize,
}

fn wgpu_texture_format(format: PixelFormat) -> Result<wgpu::TextureFormat, PoolError> {
    match format {
        PixelFormat::Nv12 => Ok(wgpu::TextureFormat::NV12),
        PixelFormat::P010 => Ok(wgpu::TextureFormat::P010),
        PixelFormat::Rgba8 => Ok(wgpu::TextureFormat::Rgba8Unorm),
        PixelFormat::P012 | PixelFormat::P016 | PixelFormat::Yuv444 => {
            Err(PoolError::UnsupportedFormat(format))
        }
    }
}

fn texture_usage(format: PixelFormat) -> wgpu::TextureUsages {
    match format {
        PixelFormat::Rgba8 => {
            wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
        }
        _ => wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
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

impl FormatPool {
    fn new(
        _device: &wgpu::Device,
        format: PixelFormat,
        width: u32,
        height: u32,
        capacity: usize,
    ) -> Self {
        Self {
            format,
            width,
            height,
            slots: Vec::with_capacity(capacity),
            capacity,
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

    fn acquire_for_write(&mut self, device: &wgpu::Device) -> Result<wgpu::Texture, PoolError> {
        self.reclaim_completed(device);
        if let Some(slot) = self.slots.iter_mut().find(|s| s.state == SlotState::Free) {
            slot.state = SlotState::Writing;
            return Ok(slot.texture.clone());
        }
        if self.slots.len() < self.capacity {
            let texture = create_texture(device, self.format, self.width, self.height)?;
            self.slots.push(Slot {
                texture: texture.clone(),
                state: SlotState::Writing,
                fence: None,
            });
            return Ok(texture);
        }
        Err(PoolError::Exhausted)
    }

    fn mark_ready(&mut self, texture: &wgpu::Texture, submission_index: wgpu::SubmissionIndex) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.matches(texture)) {
            slot.state = SlotState::Ready;
            slot.fence = Some(submission_index);
        }
    }

    fn acquire_for_read(&mut self, device: &wgpu::Device) -> Option<wgpu::Texture> {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.state == SlotState::Ready) {
            if let Some(index) = slot.fence.clone() {
                let _ = device.poll(wgpu::PollType::Wait {
                    submission_index: Some(index),
                    timeout: None,
                });
            }
            slot.state = SlotState::Reading;
            slot.fence = None;
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

pub struct NeoMediaCache {
    device: wgpu::Device,
    capacity: usize,
    pools: Mutex<HashMap<(PixelFormat, u32, u32), FormatPool>>,
}

impl NeoMediaCache {
    pub fn new(device: wgpu::Device, capacity: usize) -> Self {
        Self {
            device,
            capacity: capacity.clamp(MIN_CAPACITY, MAX_CAPACITY),
            pools: Mutex::new(HashMap::new()),
        }
    }

    pub fn acquire_for_write(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
    ) -> Result<wgpu::Texture, PoolError> {
        let mut pools = self.pools.lock().expect("pools mutex poisoned");
        let pool = pools
            .entry((format, width, height))
            .or_insert_with(|| FormatPool::new(&self.device, format, width, height, self.capacity));
        pool.acquire_for_write(&self.device)
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
        let mut pools = self.pools.lock().expect("pools mutex poisoned");
        let pool = pools.get_mut(&(format, width, height))?;
        pool.acquire_for_read(&self.device)
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
}
