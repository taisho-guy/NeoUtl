mod decoder_core;
mod worker;

use decoder_core::DecoderCore;
use ffmpeg_next as ffmpeg;
use neoutl_media_api::{MediaKind, MediaMeta, MediaVTable, VideoSource};
use std::path::Path;
use std::sync::mpsc;
use worker::{Command, WorkerHandle};

pub struct FfmpegVideoDecoder {
    worker: WorkerHandle,
    width: u32,
    height: u32,
    fps: f64,
    total_frames: i64,
}

impl FfmpegVideoDecoder {
    pub fn open(path: &Path) -> Result<Self, ffmpeg::Error> {
        let core = DecoderCore::open(path)?;
        let width = core.width;
        let height = core.height;
        let fps = core.fps;
        let total_frames = core.total_frames();
        Ok(Self {
            worker: WorkerHandle::spawn(core),
            width,
            height,
            fps,
            total_frames,
        })
    }
}

impl VideoSource for FfmpegVideoDecoder {
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
        self.worker.send(Command::Prefetch(frame_index))
    }

    fn frame_gpu(
        &mut self,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        let (resp_tx, resp_rx) = mpsc::channel();
        self.worker.send(Command::FrameGpu(
            frame_index,
            device.clone(),
            queue.clone(),
            resp_tx,
        ))?;
        resp_rx
            .recv()
            .map_err(|_| "decode worker response channel closed".to_string())?
    }
}

static EXTENSIONS: &[&str] = &[
    "mp4", "mov", "mkv", "webm", "avi", "m4v", "ts", "flv", "wmv", "mpg", "mpeg",
];

static META: MediaMeta = MediaMeta {
    id: "neoutl.media.software-ffmpeg",
    name: "FFmpeg Decoder (hardware decode + CPU transfer)",
    kind: MediaKind::Video,
    extensions_ptr: EXTENSIONS.as_ptr(),
    extensions_len: EXTENSIONS.len(),
};

pub fn meta() -> &'static MediaMeta {
    &META
}

fn open_video(path: &Path) -> Result<Box<dyn VideoSource>, String> {
    FfmpegVideoDecoder::open(path)
        .map(|d| Box::new(d) as Box<dyn VideoSource>)
        .map_err(|e| format!("open_video failed path={} err={e}", path.display()))
}

pub fn native_vtable() -> MediaVTable {
    MediaVTable {
        meta,
        open_video: Some(open_video),
        open_image: None,
        decode_audio: Some(decode_audio),
    }
}

pub fn decode_audio(path: &Path) -> Result<neoutl_media_api::AudioBuffer, String> {
    use ffmpeg_next::software::resampling::Context as ResamplingContext;
    use ffmpeg_next::util::format::sample::{Sample, Type as SampleType};
    use ffmpeg_next::util::frame::Audio as AudioFrame;

    let mut input = ffmpeg::format::input(path).map_err(|e| e.to_string())?;
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or(ffmpeg::Error::StreamNotFound)
        .map_err(|e| e.to_string())?;
    let stream_index = stream.index();

    let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| e.to_string())?;
    let mut decoder = context.decoder().audio().map_err(|e| e.to_string())?;

    let out_rate = decoder.rate();
    let out_channels = decoder.channels();
    let out_layout = decoder.channel_layout();

    let mut resampler = ResamplingContext::get(
        decoder.format(),
        decoder.channel_layout(),
        decoder.rate(),
        Sample::F32(SampleType::Packed),
        out_layout,
        out_rate,
    )
    .map_err(|e| e.to_string())?;

    let mut samples: Vec<f32> = Vec::new();
    let mut decoded = AudioFrame::empty();
    let mut resampled = AudioFrame::empty();

    for (stream, packet) in input.packets() {
        if stream.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet).map_err(|e| e.to_string())?;
        while decoder.receive_frame(&mut decoded).is_ok() {
            resampler
                .run(&decoded, &mut resampled)
                .map_err(|e| e.to_string())?;
            append_planar_f32(&resampled, out_channels, &mut samples);
        }
    }
    decoder.send_eof().map_err(|e| e.to_string())?;
    while decoder.receive_frame(&mut decoded).is_ok() {
        resampler
            .run(&decoded, &mut resampled)
            .map_err(|e| e.to_string())?;
        append_planar_f32(&resampled, out_channels, &mut samples);
    }

    Ok(neoutl_media_api::AudioBuffer {
        sample_rate: out_rate,
        channels: out_channels,
        samples,
    })
}

fn append_planar_f32(frame: &ffmpeg_next::util::frame::Audio, channels: u16, out: &mut Vec<f32>) {
    let data = frame.data(0);
    let sample_count = frame.samples() * channels as usize;
    let bytes = &data[..sample_count * 4];
    out.extend(
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
    );
}
