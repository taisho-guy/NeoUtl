#![allow(non_camel_case_types)]

pub const DEFAULT_DECODE_CACHE_BYTES: i64 = 512 * 1024 * 1024;

pub const VIDEO_TEXTURE_POOL_CAPACITY: usize = 32;

pub trait VideoSource: Send {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn fps(&self) -> f64;
    fn total_frames(&self) -> i64;
    fn prefetch(&mut self, frame_index: i64) -> Result<(), String>;
    fn frame_gpu(
        &mut self,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String>;
}

pub trait ImageSource: Send {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn texture(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture;
}

pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }

    pub fn range(&self, start_sample: usize, sample_count: usize) -> &[f32] {
        let channels = self.channels.max(1) as usize;
        let start = start_sample
            .saturating_mul(channels)
            .min(self.samples.len());
        let end = (start_sample + sample_count)
            .saturating_mul(channels)
            .min(self.samples.len());
        &self.samples[start..end]
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MediaKind {
    Video = 0,
    Image = 1,
    Audio = 2,
}

#[repr(C)]
pub struct MediaMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: MediaKind,
    pub extensions_ptr: *const &'static str,
    pub extensions_len: usize,
}
unsafe impl Send for MediaMeta {}
unsafe impl Sync for MediaMeta {}

pub type OpenVideoFn = fn(path: &std::path::Path) -> Result<Box<dyn VideoSource>, String>;
pub type OpenImageFn = fn(path: &std::path::Path) -> Result<Box<dyn ImageSource>, String>;
pub type DecodeAudioFn = fn(path: &std::path::Path) -> Result<AudioBuffer, String>;

pub struct MediaVTable {
    pub meta: fn() -> &'static MediaMeta,
    pub open_video: Option<OpenVideoFn>,
    pub open_image: Option<OpenImageFn>,
    pub decode_audio: Option<DecodeAudioFn>,
}

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_media_entry\0";
pub type EntryFn = unsafe extern "C" fn() -> *const MediaVTable;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VideoCodec {
    H264 = 0,
    H265 = 1,
}

#[derive(Clone, Copy, Debug)]
pub struct EncodeParameters {
    pub width: u32,
    pub height: u32,
    pub framerate: f64,
    pub average_bitrate: u32,
    pub max_bitrate: u32,
    pub keyframe_interval: u32,
}

pub struct EncodedChunk {
    pub data: Vec<u8>,
    pub pts: i64,
    pub keyframe: bool,
}

pub trait VideoEncoder: Send {
    fn encode_rgba(
        &mut self,
        rgba: &wgpu::Texture,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pts: i64,
        force_keyframe: bool,
    ) -> Result<Vec<EncodedChunk>, String>;
    fn flush(&mut self) -> Result<Vec<EncodedChunk>, String>;
}

#[repr(C)]
pub struct EncoderMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub codec: VideoCodec,
    pub hardware: bool,
}
unsafe impl Send for EncoderMeta {}
unsafe impl Sync for EncoderMeta {}

pub type CreateEncoderFn = fn(EncodeParameters) -> Result<Box<dyn VideoEncoder>, String>;

pub struct EncoderVTable {
    pub meta: fn() -> &'static EncoderMeta,
    pub create: CreateEncoderFn,
}
unsafe impl Send for EncoderVTable {}
unsafe impl Sync for EncoderVTable {}
