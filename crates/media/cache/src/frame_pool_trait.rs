use neo_media_core::{NeoFramePool, PixelFormat, PoolError};

use super::neo_media_cache::NeoMediaCache;

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
