use crate::app_state::{self, SharedAppState};
use crate::ecs::systems::get_active_objects_system;
use neoutl_media_api::{EncodeParameters, VideoCodec, VideoEncoder};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExportCodec {
    H264,
    H265,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum EncoderBackend {
    Auto,
    GpuVideo,
    Gstreamer,
}

pub struct ExportJob {
    pub output_path: PathBuf,
    pub codec: ExportCodec,
    pub backend: EncoderBackend,
    pub average_bitrate: u32,
    pub max_bitrate: u32,
    pub start_frame: i32,
    pub end_frame: i32,
    pub progress: Option<Box<dyn FnMut(i32, i32) + Send>>,
    pub cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportPreset {
    pub name: String,
    pub codec: ExportCodec,
    pub backend: EncoderBackend,
    pub average_bitrate: u32,
    pub max_bitrate: u32,
    pub container_ext: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct ExportPresetFile {
    presets: Vec<ExportPreset>,
}

pub fn default_export_presets() -> Vec<ExportPreset> {
    vec![
        ExportPreset {
            name: "H.264 標準 (MP4)".into(),
            codec: ExportCodec::H264,
            backend: EncoderBackend::Auto,
            average_bitrate: 8_000_000,
            max_bitrate: 12_000_000,
            container_ext: "mp4".into(),
        },
        ExportPreset {
            name: "H.265 標準 (MP4)".into(),
            codec: ExportCodec::H265,
            backend: EncoderBackend::Auto,
            average_bitrate: 6_000_000,
            max_bitrate: 10_000_000,
            container_ext: "mp4".into(),
        },
    ]
}

fn presets_path() -> PathBuf {
    crate::project::projects_dir()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("settings")
        .join("export_presets.yaml")
}

pub fn load_export_presets() -> Vec<ExportPreset> {
    let path = presets_path();
    let Ok(text) = std::fs::read_to_string(path) else {
        return default_export_presets();
    };
    rust_yaml::from_str::<ExportPresetFile>(&text)
        .map(|f| f.presets)
        .unwrap_or_else(|_| default_export_presets())
}

pub fn save_export_presets(presets: &[ExportPreset]) -> Result<(), String> {
    let path = presets_path();
    std::fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|e| e.to_string())?;
    let text = rust_yaml::to_string(&ExportPresetFile {
        presets: presets.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueState {
    Idle,
    Running,
    #[allow(dead_code)]
    Paused,
    CancelRequested,
    Completed,
}

pub struct QueuedJob {
    pub job: ExportJob,
    pub project_dir: PathBuf,
    pub id: u64,
}
#[allow(dead_code)]
pub struct JobHandle {
    pub id: u64,
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

struct QueueInner {
    jobs: VecDeque<QueuedJob>,
    current: Option<JobHandle>,
    state: QueueState,
    next_id: u64,
}

#[derive(Clone)]
pub struct RenderQueue {
    inner: Arc<Mutex<QueueInner>>,
}

#[allow(dead_code)]
impl RenderQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(QueueInner {
                jobs: VecDeque::new(),
                current: None,
                state: QueueState::Idle,
                next_id: 1,
            })),
        }
    }
    pub fn enqueue(&self, mut job: ExportJob, project_dir: PathBuf) -> u64 {
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        job.cancel = Some(cancel);
        let mut q = self.inner.lock().unwrap();
        let id = q.next_id;
        q.next_id += 1;
        q.jobs.push_back(QueuedJob {
            job,
            project_dir,
            id,
        });
        id
    }
    pub fn pending_count(&self) -> usize {
        self.inner.lock().unwrap().jobs.len()
    }
    pub fn state(&self) -> QueueState {
        self.inner.lock().unwrap().state
    }
    pub fn cancel_current(&self) {
        let mut q = self.inner.lock().unwrap();
        if let Some(h) = &q.current {
            h.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            q.state = QueueState::CancelRequested;
        }
    }
    pub fn start(&self, state: SharedAppState) {
        {
            let q = self.inner.lock().unwrap();
            if q.state == QueueState::Running || q.state == QueueState::CancelRequested {
                return;
            }
        }
        let inner = self.inner.clone();
        std::thread::spawn(move || {
            loop {
                let next = {
                    let mut q = inner.lock().unwrap();
                    let Some(item) = q.jobs.pop_front() else {
                        q.state = QueueState::Completed;
                        q.current = None;
                        break;
                    };
                    let cancel = item.job.cancel.clone().unwrap();
                    q.current = Some(JobHandle {
                        id: item.id,
                        cancel,
                    });
                    q.state = QueueState::Running;
                    item
                };
                if let Err(error) = app_state::activate_session_by_dir(&state, &next.project_dir) {
                    eprintln!("[NeoUtl][export] キュージョブをスキップ: {error}");
                    let mut q = inner.lock().unwrap();
                    q.current = None;
                    continue;
                }
                let _ = run(&state, next.job);
                let mut q = inner.lock().unwrap();
                q.current = None;
            }
        });
    }
}

impl Default for RenderQueue {
    fn default() -> Self {
        Self::new()
    }
}

fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Vec<u8> {
    let width = texture.width();
    let height = texture.height();
    let bytes_per_row = (width * 8).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("export readback"),
        size: (bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    rx.recv().ok();

    let padded = slice.get_mapped_range();
    let mut float_dense = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * bytes_per_row) as usize;
        let end = start + (width * 8) as usize;
        float_dense.extend(
            padded[start..end]
                .chunks_exact(2)
                .map(|b| half::f16::from_le_bytes([b[0], b[1]])),
        );
    }
    drop(padded);
    buffer.unmap();
    neoutl_color::rgba16f_to_u8(&float_dense)
}

fn to_rgba8_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src: &wgpu::Texture,
) -> wgpu::Texture {
    let width = src.width();
    let height = src.height();
    let pixels = read_texture_rgba(device, queue, src);
    let dst = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("export nv12-bridge rgba8"),
        size: wgpu::Extent3d {
            width,
            height,
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
            texture: &dst,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    dst
}

fn try_create_gpuvideo_encoder(
    _codec: ExportCodec,
    _params: EncodeParameters,
) -> Option<Box<dyn VideoEncoder>> {
    None
}

pub fn run(state: &SharedAppState, mut job: ExportJob) -> Result<(), String> {
    let world_holder = app_state::active_world(state);
    let mixer_holder = app_state::active_audio_mixer(state);
    let engine_holder = app_state::active_engine(state);

    let (width, height, fps, sample_rate, channels) = {
        let world = world_holder.lock().unwrap();
        let proj = world.get_project();
        (
            proj.width,
            proj.height,
            proj.fps as f64,
            proj.audio_sample_rate,
            proj.audio_channels as u16,
        )
    };

    let params = EncodeParameters {
        width,
        height,
        framerate: fps,
        average_bitrate: job.average_bitrate,
        max_bitrate: job.max_bitrate,
        keyframe_interval: fps.round() as u32 * 2,
    };
    let codec = match job.codec {
        ExportCodec::H264 => VideoCodec::H264,
        ExportCodec::H265 => VideoCodec::H265,
    };

    let frame_duration_ns = (1_000_000_000f64 / fps).round() as i64;
    let mut gpuvideo_encoder = match job.backend {
        EncoderBackend::Gstreamer => None,
        EncoderBackend::Auto => try_create_gpuvideo_encoder(job.codec, params),
        EncoderBackend::GpuVideo => match try_create_gpuvideo_encoder(job.codec, params) {
            Some(enc) => Some(enc),
            None => {
                return Err(
                    "gpuvideo-encoderは現在ビルドから除外されています(gstreamer-encoderをご利用下さい)"
                        .to_owned(),
                );
            }
        },
    };
    let total_frames = job.end_frame - job.start_frame;
    let mut chunks: Vec<neoutl_media_api::EncodedChunk> = Vec::new();

    {
        let engine_lock = engine_holder.lock().unwrap();
        let Some(engine) = engine_lock.as_ref() else {
            return Err(
                "RenderEngine未初期化（プレビューを一度開いてから実行して下さい）".to_owned(),
            );
        };
        let device = std::sync::Arc::clone(&engine.device);
        let queue = std::sync::Arc::clone(&engine.queue);
        drop(engine_lock);

        let mut mixer = mixer_holder.lock().unwrap();
        mixer.reset();
        drop(mixer);

        for frame_index in job.start_frame..job.end_frame {
            if job
                .cancel
                .as_ref()
                .is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed))
            {
                return Err("ユーザーにより中止".to_owned());
            }
            let texture = {
                let mut world = world_holder.lock().unwrap();
                world.set_current_frame(frame_index);
                let (active, captured) = get_active_objects_system(&world);
                let proj = world.get_project();
                let mut engine_lock = engine_holder.lock().unwrap();
                let Some(engine) = engine_lock.as_mut() else {
                    return Err("RenderEngine消失".to_owned());
                };
                engine.render(&world, &active, &captured, &proj);
                engine.texture.clone()
            };

            let pts = (frame_index - job.start_frame) as i64 * frame_duration_ns;
            if let Some(enc) = gpuvideo_encoder.as_mut() {
                let force_keyframe = pts == 0;
                let bridge_texture = to_rgba8_texture(&device, &queue, &texture);
                match enc.encode_rgba(&bridge_texture, &device, &queue, pts, force_keyframe) {
                    Ok(mut c) => chunks.append(&mut c),
                    Err(e) => {
                        if job.backend == EncoderBackend::GpuVideo {
                            return Err(format!("gpuvideo-encoder(GPU HW)エンコード失敗: {e}"));
                        }
                        eprintln!(
                            "{}",
                            t!(
                                "[NeoUtl][export] gpuvideo-encoder失敗、gstreamer-encoder全体経路へ縮退: %{arg0}",
                                arg0 = format!("{}", e)
                            )
                        );
                        gpuvideo_encoder = None;
                        chunks.clear();
                        break;
                    }
                }
            }

            if let Some(cb) = job.progress.as_mut() {
                cb(frame_index - job.start_frame + 1, total_frames);
            }
        }
    }

    let world_holder_for_audio = world_holder.clone();
    let mixer_holder_for_audio = mixer_holder.clone();
    let start_frame = job.start_frame;
    let make_audio_producer = || -> Box<neoutl_media_gstreamer_encoder::AudioProducer<'static>> {
        Box::new(move |frame_index: i64, sample_count: usize| {
            let world = world_holder_for_audio.lock().unwrap();
            let mut mixer = mixer_holder_for_audio.lock().unwrap();
            mixer.render_frame_offline(&world, start_frame + frame_index as i32, sample_count)
        })
    };

    if gpuvideo_encoder.is_some() {
        let mut it = chunks.into_iter();
        neoutl_media_gstreamer_encoder::mux_encoded(
            &job.output_path,
            mux_container_for(&job.output_path),
            codec,
            Box::new(move || Ok(it.next().map(|c| (c.data, c.pts, c.keyframe)))),
            Some(neoutl_media_gstreamer_encoder::AudioFrameSource {
                sample_rate,
                channels,
            }),
            total_frames as i64,
            fps.round() as i32,
            1,
            Some(make_audio_producer()),
        )
    } else {
        let world_holder = world_holder.clone();
        let engine_holder = engine_holder.clone();
        let start_frame = job.start_frame;
        neoutl_media_gstreamer_encoder::export(
            &job.output_path,
            preset_for(job.codec, &job.output_path),
            neoutl_media_gstreamer_encoder::VideoFrameSource {
                width,
                height,
                fps_num: fps.round() as i32,
                fps_denom: 1,
                total_frames: total_frames as i64,
            },
            Some(neoutl_media_gstreamer_encoder::AudioFrameSource {
                sample_rate,
                channels,
            }),
            Box::new(move |frame_index: i64| {
                let mut world = world_holder.lock().unwrap();
                world.set_current_frame(start_frame + frame_index as i32);
                let (active, captured) = get_active_objects_system(&world);
                let proj = world.get_project();
                let mut engine_lock = engine_holder.lock().unwrap();
                let engine = engine_lock
                    .as_mut()
                    .ok_or_else(|| "RenderEngine消失".to_owned())?;
                engine.render(&world, &active, &captured, &proj);
                Ok(read_texture_rgba(
                    &engine.device,
                    &engine.queue,
                    &engine.texture,
                ))
            }),
            Some(make_audio_producer()),
        )
    }
}

fn preset_for(
    codec: ExportCodec,
    output_path: &Path,
) -> neoutl_media_gstreamer_encoder::ExportPreset {
    let is_mkv = output_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mkv"));
    match (codec, is_mkv) {
        (ExportCodec::H264, false) => neoutl_media_gstreamer_encoder::ExportPreset::Mp4H264Aac,
        (ExportCodec::H265, false) => neoutl_media_gstreamer_encoder::ExportPreset::Mp4H265Aac,
        (ExportCodec::H264, true) => neoutl_media_gstreamer_encoder::ExportPreset::MkvH264Opus,
        (ExportCodec::H265, true) => neoutl_media_gstreamer_encoder::ExportPreset::MkvH265Opus,
    }
}

fn mux_container_for(output_path: &Path) -> neoutl_media_gstreamer_encoder::MuxContainer {
    let is_mkv = output_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mkv"));
    if is_mkv {
        neoutl_media_gstreamer_encoder::MuxContainer::Mkv
    } else {
        neoutl_media_gstreamer_encoder::MuxContainer::Mp4
    }
}
