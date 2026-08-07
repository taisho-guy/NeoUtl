//! Phase 2（縮退版・確定）: ハードウェアデコード有効化 + GPU→CPU 1回転送。
//! swscale/CPU RGBA8色空間変換は排除済み（NV12のままCPU側でRGBA合成のみ行う）。
//! 当初計画のVulkan統一ゼロコピー（hwmap=derive_device=vulkan + AVVkFrame直接ラップ）は
//! wgpu-hal 29系のtexture_from_rawが常にVkDeviceMemory所有権を要求する（非所有ラップ
//! モード不在、gfx-rs/wgpu Issue #2320未解決）ため撤回した。FFmpeg側フレームプールとの
//! 二重所有・二重解放を回避できないため。
//!
//! 真のゼロコピー（DRM-fd/D3D11共有ハンドル経由のexternal memory import）はPhase3で
//! OS別に実機検証のうえ再着手する。本ファイルは「ハードウェアデコード + 単回CPU転送」
//! を安定した中間状態として確定させたもの。
//!
//! Phase 2.1: prefetch(バックグラウンドスレッド)とframe_gpu(UIスレッド)が同一の
//! AVFormatContext/AVCodecContextへ独立にシークする設計はseek競合(ライブロック)を
//! 引き起こしていた。デコード専用ワーカースレッドを1本化し(`worker.rs`)、両要求を
//! 単一キューへ直列化することで解消する。実デコード状態は`decoder_core.rs`の
//! `DecoderCore`が保持し、ワーカースレッドのみが排他所有する。

mod decoder_core;
mod worker;

use decoder_core::DecoderCore;
use ffmpeg_next as ffmpeg;
use neoutl_media_api::{MediaKind, MediaMeta, MediaVTable, VideoSource};
use std::path::Path;
use std::sync::mpsc;
use worker::{Command, WorkerHandle};

/// `VideoSource`実装。実体はワーカースレッドへのハンドルのみを保持する。
/// `width`/`height`/`fps`/`total_frames`は`open`時点で確定するためハンドル側に
/// キャッシュし、ワーカースレッドへの往復なしに即値を返す。
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

    /// バックグラウンドスレッド専用。ワーカースレッドのキューへ要求を投入するのみで
    /// 即座に返る（結果を待たない）。実行はワーカースレッド側で行われるため、
    /// 呼び出し元スレッドはAVFormatContextへ一切アクセスしない。
    fn prefetch(&mut self, frame_index: i64) -> Result<(), String> {
        self.worker.send(Command::Prefetch(frame_index))
    }

    /// UIスレッド専用。ワーカースレッドのキューへ要求を投入し、応答をブロック待機する。
    /// キュー内の先行prefetch要求はワーカー側で最新値へ縮約されるため、本要求より前段の
    /// 古いprefetch分によって待ち時間が線形に伸びることはない。
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

/// gpuvideo-decoder（H.264ゼロコピー専用）のCPUフォールバック対象。
/// idを"neoutl.media.gpuvideo"より辞書順で後ろにすることで、
/// loader::find_all_by_extensionのid昇順フォールバック列挙において
/// gpuvideo失敗後の次候補として試行される（本体側の順序決定ロジックは変更しない）。
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

/// src/media/loader.rsのネイティブプラグインレジストリへ直接登録するためのVTable生成。
/// dlsymプラグインではなくNeoUtl本体へ直接静的リンクされる（root Cargo.tomlの
/// [dependencies]経由）。gpuvideo-decoderと同一の登録方式。
pub fn native_vtable() -> MediaVTable {
    MediaVTable {
        meta,
        open_video: Some(open_video),
        open_image: None,
        decode_audio: Some(decode_audio),
    }
}

/// 音声デコード経路。Phase 2のゼロコピー対象外（音声はPCM展開が必須のためCPU経路のまま）。
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
