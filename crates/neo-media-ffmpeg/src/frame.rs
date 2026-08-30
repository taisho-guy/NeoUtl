use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use neo_media_cache::NeoMediaCache;
use neo_media_core::PixelFormat;

pub struct GpuFrame {
    pub texture: wgpu::Texture,
    pub width: u32,
    pub height: u32,
    cache: Arc<NeoMediaCache>,
    format: PixelFormat,
}

impl GpuFrame {
    pub fn new(
        texture: wgpu::Texture,
        width: u32,
        height: u32,
        cache: Arc<NeoMediaCache>,
        format: PixelFormat,
    ) -> Self {
        Self {
            texture,
            width,
            height,
            cache,
            format,
        }
    }
}

impl Drop for GpuFrame {
    fn drop(&mut self) {
        self.cache
            .release_free_as(self.format, self.width, self.height, &self.texture);
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

#[derive(Clone)]
pub struct RamFrame {
    pub bytes: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
}

impl RamFrame {
    pub fn new(bytes: Arc<[u8]>, width: u32, height: u32) -> Self {
        Self {
            bytes,
            width,
            height,
        }
    }
}

pub struct VideoFrameStore {
    frames: Mutex<HashMap<String, (i64, VideoFrame)>>,
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

    pub fn set_frame(&self, key: &str, frame_index: i64, frame: VideoFrame) {
        self.frames
            .lock()
            .expect("frames mutex poisoned")
            .insert(key.to_owned(), (frame_index, frame));
        for listener in self
            .listeners
            .lock()
            .expect("listeners mutex poisoned")
            .iter()
        {
            listener(key);
        }
    }

    pub fn frame(&self, key: &str, expected_index: i64) -> Option<VideoFrame> {
        self.frames
            .lock()
            .expect("frames mutex poisoned")
            .get(key)
            .and_then(|(index, frame)| {
                if *index == expected_index {
                    Some(frame.clone())
                } else {
                    None
                }
            })
    }

    pub fn has_frame(&self, key: &str, expected_index: i64) -> bool {
        self.frames
            .lock()
            .expect("frames mutex poisoned")
            .get(key)
            .is_some_and(|(index, _)| *index == expected_index)
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
