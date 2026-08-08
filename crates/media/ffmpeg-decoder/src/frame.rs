use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct Rgba8Frame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Rgba8Frame {
    pub fn byte_cost(&self) -> i64 {
        self.data.len() as i64
    }
}

pub struct VideoFrameStore {
    frames: Mutex<HashMap<String, Arc<Rgba8Frame>>>,
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

    pub fn set_frame(&self, key: &str, frame: Arc<Rgba8Frame>) {
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

    pub fn frame(&self, key: &str) -> Option<Arc<Rgba8Frame>> {
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
