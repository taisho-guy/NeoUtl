use std::collections::HashMap;
use std::sync::Arc;

use neoutl_media_api::{ColorMeta, VideoSource};

#[derive(Clone, Debug)]
pub enum PropertyValue {
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
}

pub enum FrameImage {
    Rgba(wgpu::Texture),
    Planar {
        texture: wgpu::Texture,
        color_meta: ColorMeta,
    },
    None,
}

pub struct Frame {
    pub position: i64,
    pub image: FrameImage,
    pub properties: HashMap<String, PropertyValue>,
}

impl Frame {
    pub fn empty(position: i64) -> Self {
        Self {
            position,
            image: FrameImage::None,
            properties: HashMap::new(),
        }
    }

    pub fn with_image(position: i64, image: FrameImage) -> Self {
        Self {
            position,
            image,
            properties: HashMap::new(),
        }
    }

    pub fn texture(&self) -> Option<&wgpu::Texture> {
        match &self.image {
            FrameImage::Rgba(tex) => Some(tex),
            FrameImage::Planar { texture, .. } => Some(texture),
            FrameImage::None => None,
        }
    }

    pub fn color_meta(&self) -> ColorMeta {
        match &self.image {
            FrameImage::Planar { color_meta, .. } => *color_meta,
            _ => ColorMeta::default(),
        }
    }
}

pub trait Producer: Send {
    fn get_frame(&mut self, position: i64) -> Frame;
    fn length(&self) -> i64;
    fn fps(&self) -> f64;
}

pub trait Filter: Send {
    fn process(&self, frame: Frame) -> Frame;
}

pub trait Consumer: Send {
    fn push(&mut self, frame: Frame);
}

pub struct FilterChain {
    filters: Vec<Box<dyn Filter>>,
}

impl FilterChain {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    pub fn push(&mut self, filter: Box<dyn Filter>) {
        self.filters.push(filter);
    }

    pub fn apply(&self, mut frame: Frame) -> Frame {
        for filter in &self.filters {
            frame = filter.process(frame);
        }
        frame
    }
}

impl Default for FilterChain {
    fn default() -> Self {
        Self::new()
    }
}

pub struct VideoSourceProducer {
    source: Box<dyn VideoSource>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

impl VideoSourceProducer {
    pub fn new(
        source: Box<dyn VideoSource>,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> Self {
        Self {
            source,
            device,
            queue,
        }
    }
}

impl Producer for VideoSourceProducer {
    fn get_frame(&mut self, position: i64) -> Frame {
        match self.source.frame_gpu(position, &self.device, &self.queue) {
            Ok(texture) => {
                let color_meta = self.source.last_color_meta();
                Frame::with_image(
                    position,
                    FrameImage::Planar {
                        texture,
                        color_meta,
                    },
                )
            }
            Err(_) => Frame::empty(position),
        }
    }

    fn length(&self) -> i64 {
        self.source.total_frames()
    }

    fn fps(&self) -> f64 {
        self.source.fps()
    }
}
