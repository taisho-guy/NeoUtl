use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ffmpeg_sys_next as sys;
use neo_media_core::NeoFramePool;

pub struct OwnedAvFrame {
    pub(crate) raw: *mut sys::AVFrame,
}

unsafe impl Send for OwnedAvFrame {}
unsafe impl Sync for OwnedAvFrame {}

impl Drop for OwnedAvFrame {
    fn drop(&mut self) {
        unsafe {
            if !self.raw.is_null() {
                sys::av_frame_free(&mut self.raw);
            }
        }
    }
}

pub struct GpuFrame {
    pub texture: wgpu::Texture,
    pub width: u32,
    pub height: u32,
    _keep_alive: Option<Arc<OwnedAvFrame>>,
    pool_release: Option<Arc<neo_media_cache::NeoMediaCache>>,
    _pool_owner_token: Option<Arc<neo_media_cache::SlotOwnerToken>>,
}

impl GpuFrame {
    pub fn new(texture: wgpu::Texture, width: u32, height: u32) -> Self {
        Self {
            texture,
            width,
            height,
            _keep_alive: None,
            pool_release: None,
            _pool_owner_token: None,
        }
    }

    pub fn new_pooled(
        texture: wgpu::Texture,
        width: u32,
        height: u32,
        cache: Arc<neo_media_cache::NeoMediaCache>,
        pool_owner_token: Option<Arc<neo_media_cache::SlotOwnerToken>>,
    ) -> Self {
        Self {
            texture,
            width,
            height,
            _keep_alive: None,
            pool_release: Some(cache),
            _pool_owner_token: pool_owner_token,
        }
    }
}

impl Drop for GpuFrame {
    fn drop(&mut self) {
        if let Some(cache) = self.pool_release.take() {
            cache.release(self.texture.clone());
        }
    }
}

#[derive(Clone)]
pub struct VideoFrame(pub Arc<GpuFrame>);

impl VideoFrame {
    pub fn width(&self) -> u32 {
        self.0.width
    }

    pub fn height(&self) -> u32 {
        self.0.height
    }
}

pub struct VideoFrameStore {
    frames: Mutex<HashMap<String, VideoFrame>>,
    listeners: Mutex<Vec<Box<dyn Fn(&str) + Send + Sync>>>,
}

impl VideoFrameStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            frames: Mutex::new(HashMap::new()),
            listeners: Mutex::new(Vec::new()),
        })
    }

    pub fn on_frame_updated(&self, listener: impl Fn(&str) + Send + Sync + 'static) {
        self.listeners
            .lock()
            .expect("listeners mutex poisoned")
            .push(Box::new(listener));
    }

    pub fn set_frame(&self, key: &str, frame: VideoFrame) {
        self.frames
            .lock()
            .expect("frames mutex poisoned")
            .insert(key.to_owned(), frame);
        for listener in self
            .listeners
            .lock()
            .expect("listeners mutex poisoned")
            .iter()
        {
            listener(key);
        }
    }

    pub fn frame(&self, key: &str) -> Option<VideoFrame> {
        self.frames
            .lock()
            .expect("frames mutex poisoned")
            .get(key)
            .cloned()
    }

    pub fn has_frame(&self, key: &str) -> bool {
        self.frames
            .lock()
            .expect("frames mutex poisoned")
            .contains_key(key)
    }

    pub fn invalidate_frame(&self, key: &str) {
        self.frames
            .lock()
            .expect("frames mutex poisoned")
            .remove(key);
    }

    pub fn invalidate_scene(&self, scene_id: i32) {
        let prefix = format!("{scene_id}_");
        self.frames
            .lock()
            .expect("frames mutex poisoned")
            .retain(|k, _| !k.starts_with(&prefix));
    }

    pub fn clear(&self) {
        self.frames.lock().expect("frames mutex poisoned").clear();
    }
}
