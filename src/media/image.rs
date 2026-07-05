// src/media/image.rs
use super::DecodedFrame;
use image::GenericImageView;
use std::path::Path;

pub struct ImageDecoder {
    frame: DecodedFrame,
}

impl ImageDecoder {
    pub fn open(path: &Path) -> Result<Self, image::ImageError> {
        let img = image::open(path)?;
        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8().into_raw();
        Ok(Self {
            frame: DecodedFrame {
                width,
                height,
                rgba,
            },
        })
    }

    pub fn width(&self) -> u32 {
        self.frame.width
    }

    pub fn height(&self) -> u32 {
        self.frame.height
    }

    pub fn frame(&self) -> &DecodedFrame {
        &self.frame
    }
}
