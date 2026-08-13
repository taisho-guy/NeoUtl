use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use neoutl_media_api::{MediaKind, MediaMeta, MediaVTable, VideoSource};

use crate::decoder::{VideoDecoder, VideoMeta};
use crate::frame::{VideoFrame, VideoFrameStore};

const FRAME_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const FRAME_WAIT_POLL: Duration = Duration::from_millis(2);
const CLIP_KEY: &str = "ffmpeg_decoder_source";

pub struct FfmpegVideoSource {
    decoder: VideoDecoder,
    store: Arc<VideoFrameStore>,
    width: u32,
    height: u32,
    fps: f64,
    total_frames: i64,
}

impl FfmpegVideoSource {
    fn open(path: &Path) -> Result<Self, String> {
        let store = VideoFrameStore::new();
        let (tx, rx) = mpsc::channel::<VideoMeta>();
        let store_thread = store.clone();
        let decoder = VideoDecoder::open(
            path,
            CLIP_KEY.to_owned(),
            store_thread,
            crate::decoder::shared_wgpu_device(),
            crate::decoder::shared_wgpu_queue(),
            move |meta| {
                let _ = tx.send(meta);
            },
        );
        let meta = rx
            .recv_timeout(FRAME_WAIT_TIMEOUT)
            .map_err(|e| format!("動画メタ情報取得タイムアウト: {e}"))?;
        Ok(Self {
            decoder,
            store,
            width: meta.width,
            height: meta.height,
            fps: meta.fps,
            total_frames: meta.total_frames,
        })
    }
}

impl VideoSource for FfmpegVideoSource {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn fps(&self) -> f64 {
        self.fps
    }
    fn total_frames(&self) -> i64 {
        self.total_frames
    }

    fn prefetch(&mut self, frame_index: i64) -> Result<(), String> {
        self.decoder.seek_to_frame(frame_index);
        Ok(())
    }

    fn frame_gpu(
        &mut self,
        frame_index: i64,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        self.store.invalidate_frame(CLIP_KEY);
        self.decoder.seek_to_frame(frame_index);

        let deadline = Instant::now() + FRAME_WAIT_TIMEOUT;
        let frame = loop {
            if let Some(frame) = self.store.frame(CLIP_KEY) {
                break frame;
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "フレーム取得タイムアウト frame_index={frame_index}"
                ));
            }
            std::thread::sleep(FRAME_WAIT_POLL);
        };

        Ok(frame.0.texture.clone())
    }
}

static EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v"];

static META: MediaMeta = MediaMeta {
    id: "neoutl.media.ffmpeg",
    name: "FFmpeg Video Decoder",
    kind: MediaKind::Video,
    extensions_ptr: EXTENSIONS.as_ptr(),
    extensions_len: EXTENSIONS.len(),
};

fn meta() -> &'static MediaMeta {
    &META
}

fn open_video(path: &Path) -> Result<Box<dyn VideoSource>, String> {
    FfmpegVideoSource::open(path).map(|s| Box::new(s) as Box<dyn VideoSource>)
}

pub fn native_vtable() -> MediaVTable {
    MediaVTable {
        meta,
        open_video: Some(open_video),
        open_image: None,
        decode_audio: None,
    }
}
