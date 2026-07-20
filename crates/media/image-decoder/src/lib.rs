use image::GenericImageView;
use neoutl_media_api::ImageSource;
use std::path::Path;

pub struct StaticImageDecoder {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl StaticImageDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        let img = image::open(path).map_err(|e| e.to_string())?;
        let (width, height) = img.dimensions();
        Ok(Self {
            width,
            height,
            rgba: img.to_rgba8().into_raw(),
        })
    }
}

impl ImageSource for StaticImageDecoder {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }

    fn texture(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("static-image"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        texture
    }
}

use neoutl_media_api::{EntryFn, MediaKind, MediaMeta, MediaVTable};

static EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "webp", "gif", "tiff"];

static META: MediaMeta = MediaMeta {
    id: "neoutl.media.image",
    name: "Static Image Decoder",
    kind: MediaKind::Image,
    extensions_ptr: EXTENSIONS.as_ptr(),
    extensions_len: EXTENSIONS.len(),
};
static VTABLE: std::sync::OnceLock<MediaVTable> = std::sync::OnceLock::new();

fn meta() -> &'static MediaMeta {
    &META
}

fn open_image(path: &Path) -> Result<Box<dyn ImageSource>, String> {
    StaticImageDecoder::open(path).map(|d| Box::new(d) as Box<dyn ImageSource>)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn neoutl_media_entry() -> *const MediaVTable {
    VTABLE.get_or_init(|| MediaVTable {
        meta,
        open_video: None,
        open_image: Some(open_image),
        decode_audio: None,
    })
}

const _: EntryFn = neoutl_media_entry;
