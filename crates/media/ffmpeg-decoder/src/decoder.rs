use std::ffi::{CString, c_void};
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use ffmpeg_sys_next as sys;

use crate::cache::{FrameLruCache, GopCache, GopCacheBlock};
use crate::frame::{GpuFrame, Rgba8Frame, VideoFrame, VideoFrameStore};
use crate::index::{FrameIndex, build_index};
use crate::vaapi_probe::probe_vaapi_node;
use crate::vulkan::{self, NeoutlVulkanContext};

const SWS_BILINEAR: i32 = 2;
fn pf(fmt: sys::AVPixelFormat) -> i32 {
    fmt as i32
}

fn av_pix_fmt_none() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_NONE)
}
#[allow(dead_code)]
fn av_pix_fmt_vulkan() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_VULKAN)
}
fn av_pix_fmt_rgb0() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_RGB0)
}
fn av_pix_fmt_bgr0() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_BGR0)
}
fn av_pix_fmt_nv12() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_NV12)
}
fn av_pix_fmt_p010le() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_P010LE)
}
fn av_pix_fmt_p012le() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_P012LE)
}
fn av_pix_fmt_p016le() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_P016LE)
}
fn av_pix_fmt_yuv420p10le() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_YUV420P10LE)
}
fn av_pix_fmt_yuv420p12le() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_YUV420P12LE)
}
fn av_pix_fmt_yuv420p() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_YUV420P)
}
fn av_pix_fmt_yuvj420p() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_YUVJ420P)
}

const AV_CODEC_CAP_FRAME_THREADS: i32 = 1 << 12;
const AV_CODEC_CAP_SLICE_THREADS: i32 = 1 << 13;
const AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX: i32 = 0x01;
const FF_THREAD_FRAME: i32 = 1;
const FF_THREAD_SLICE: i32 = 2;

const GOP_CACHE_CAPACITY: usize = 3;
const FORWARD_DECODE_THRESHOLD: i64 = 120;
const DEFAULT_FRAME_CACHE_BYTES: i64 = 512 * 1024 * 1024;
const HW_DEVICE_TYPE_NAMES: &[&str] = &["cuda", "vaapi", "d3d11va", "dxva2", "videotoolbox"];

pub struct VideoMeta {
    pub total_frames: i64,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
}

struct Mailbox {
    target_frame: Option<i64>,
    stopped: bool,
}

pub struct VideoDecoder {
    shared: Arc<(Mutex<Mailbox>, Condvar)>,
    join: Option<JoinHandle<()>>,
    pub is_ready: Arc<AtomicBool>,
    pub last_requested_frame: Arc<AtomicI64>,
}

impl VideoDecoder {
    pub fn open(
        path: impl AsRef<Path>,
        clip_key: String,
        store: Arc<VideoFrameStore>,
        gpu_device: Option<Arc<wgpu::Device>>,
        gpu_queue: Option<Arc<wgpu::Queue>>,
        on_ready: impl FnOnce(VideoMeta) + Send + 'static,
    ) -> Self {
        let shared = Arc::new((
            Mutex::new(Mailbox {
                target_frame: None,
                stopped: false,
            }),
            Condvar::new(),
        ));
        let is_ready = Arc::new(AtomicBool::new(false));
        let last_requested_frame = Arc::new(AtomicI64::new(-1));

        let shared_thread = shared.clone();
        let is_ready_thread = is_ready.clone();
        let last_requested_thread = last_requested_frame.clone();
        let path = path.as_ref().to_owned();

        let join = std::thread::Builder::new()
            .name("neoutl-video-decoder".into())
            .spawn(move || {
                run_worker(
                    path,
                    clip_key,
                    store,
                    gpu_device,
                    gpu_queue,
                    shared_thread,
                    is_ready_thread,
                    last_requested_thread,
                    on_ready,
                );
            })
            .expect("video decoder thread spawn failed");

        Self {
            shared,
            join: Some(join),
            is_ready,
            last_requested_frame,
        }
    }

    pub fn seek_to_frame(&self, frame: i64) {
        if frame < 0 {
            return;
        }
        self.last_requested_frame.store(frame, Ordering::Release);
        let (lock, cvar) = &*self.shared;
        let mut mailbox = lock.lock().expect("mailbox mutex poisoned");
        mailbox.target_frame = Some(frame);
        cvar.notify_one();
    }

    pub fn seek_to_time(&self, seconds: f64, index: &FrameIndex, time_base: (i32, i32)) {
        let frame = index.index_from_seconds(seconds.max(0.0), time_base);
        self.seek_to_frame(frame);
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        {
            let (lock, cvar) = &*self.shared;
            let mut mailbox = lock.lock().expect("mailbox mutex poisoned");
            mailbox.stopped = true;
            cvar.notify_one();
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct HwPixFmtBox {
    pix_fmt: i32,
    stream_sw_format: i32,
    hw_device_ctx: *mut sys::AVBufferRef,
}

fn semi_planar_view_formats(sw_format_i32: i32) -> Option<(ash::vk::Format, ash::vk::Format)> {
    if sw_format_i32 == av_pix_fmt_nv12() {
        Some((ash::vk::Format::R8_UNORM, ash::vk::Format::R8G8_UNORM))
    } else if sw_format_i32 == av_pix_fmt_p010le()
        || sw_format_i32 == av_pix_fmt_p012le()
        || sw_format_i32 == av_pix_fmt_p016le()
    {
        Some((ash::vk::Format::R16_UNORM, ash::vk::Format::R16G16_UNORM))
    } else {
        None
    }
}

fn resolve_hw_sw_format(stream_sw_format: i32) -> Option<i32> {
    if stream_sw_format == av_pix_fmt_nv12() {
        Some(av_pix_fmt_nv12())
    } else if stream_sw_format == av_pix_fmt_yuv420p() || stream_sw_format == av_pix_fmt_yuvj420p()
    {
        Some(av_pix_fmt_nv12())
    } else if stream_sw_format == av_pix_fmt_yuv420p10le() {
        Some(av_pix_fmt_p010le())
    } else if stream_sw_format == av_pix_fmt_yuv420p12le() {
        Some(av_pix_fmt_p012le())
    } else if stream_sw_format == av_pix_fmt_p010le()
        || stream_sw_format == av_pix_fmt_p012le()
        || stream_sw_format == av_pix_fmt_p016le()
    {
        Some(stream_sw_format)
    } else if stream_sw_format == av_pix_fmt_rgb0() || stream_sw_format == av_pix_fmt_bgr0() {
        Some(stream_sw_format)
    } else {
        None
    }
}

unsafe fn try_request_hw_frames_ctx(
    ctx: *mut sys::AVCodecContext,
    hw_device_ctx: *mut sys::AVBufferRef,
    hw_pix_fmt: i32,
    stream_sw_format: i32,
) {
    unsafe {
        if hw_device_ctx.is_null() || !(*ctx).hw_frames_ctx.is_null() {
            return;
        }
        let Some(sw_format) = resolve_hw_sw_format(stream_sw_format) else {
            return;
        };
        let frames_ref = sys::av_hwframe_ctx_alloc(hw_device_ctx);
        if frames_ref.is_null() {
            return;
        }
        let frames_ctx = (*frames_ref).data as *mut sys::AVHWFramesContext;
        (*frames_ctx).format = std::mem::transmute::<i32, sys::AVPixelFormat>(hw_pix_fmt);
        (*frames_ctx).sw_format = std::mem::transmute::<i32, sys::AVPixelFormat>(sw_format);
        (*frames_ctx).width = (*ctx).width;
        (*frames_ctx).height = (*ctx).height;
        (*frames_ctx).initial_pool_size = 20;
        if sys::av_hwframe_ctx_init(frames_ref) == 0 {
            (*ctx).hw_frames_ctx = sys::av_buffer_ref(frames_ref);
        }
        sys::av_buffer_unref(&mut { frames_ref });
    }
}

unsafe extern "C" fn hw_get_format(
    ctx: *mut sys::AVCodecContext,
    pixfmts: *const sys::AVPixelFormat,
) -> sys::AVPixelFormat {
    unsafe {
        let hw_box = if (*ctx).opaque.is_null() {
            None
        } else {
            Some(&*((*ctx).opaque as *const HwPixFmtBox))
        };
        let hw_pix_fmt = hw_box.map(|b| b.pix_fmt).unwrap_or(av_pix_fmt_none());
        let mut p = pixfmts;
        while std::mem::transmute::<sys::AVPixelFormat, i32>(*p) != av_pix_fmt_none() {
            if std::mem::transmute::<sys::AVPixelFormat, i32>(*p) == hw_pix_fmt {
                if let Some(hw_box) = hw_box {
                    try_request_hw_frames_ctx(
                        ctx,
                        hw_box.hw_device_ctx,
                        hw_pix_fmt,
                        hw_box.stream_sw_format,
                    );
                }
                return *p;
            }
            p = p.add(1);
        }
        *pixfmts
    }
}

struct GpuPipeline {
    wgpu_device: Arc<wgpu::Device>,
    vulkan_ctx: Arc<NeoutlVulkanContext>,
    derived_frames_ctx: *mut sys::AVBufferRef,
    semi_planar_engine: Option<vulkan::SemiPlanarConvertEngine>,
}

unsafe impl Send for GpuPipeline {}

impl Drop for GpuPipeline {
    fn drop(&mut self) {
        unsafe {
            if !self.derived_frames_ctx.is_null() {
                sys::av_buffer_unref(&mut self.derived_frames_ctx);
            }
        }
    }
}

struct OpenContext {
    fmt_ctx: *mut sys::AVFormatContext,
    dec_ctx: *mut sys::AVCodecContext,
    stream_index: i32,
    #[allow(dead_code)]
    time_base: (i32, i32),
    fps: f64,
    width: u32,
    height: u32,
    sws_ctx: *mut sys::SwsContext,
    index: FrameIndex,
    hw_device_ctx: *mut sys::AVBufferRef,
    hw_pix_fmt_box: Option<Box<HwPixFmtBox>>,
    gpu_pipeline: Option<GpuPipeline>,
    last_good_frame: Option<VideoFrame>,
    last_convert_path: ConvertPath,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum ConvertPath {
    Unknown,
    GpuZeroCopy,
    CpuRam,
}

unsafe impl Send for OpenContext {}

impl Drop for OpenContext {
    fn drop(&mut self) {
        unsafe {
            if !self.sws_ctx.is_null() {
                sys::sws_freeContext(self.sws_ctx);
            }
            if !self.dec_ctx.is_null() {
                sys::avcodec_free_context(&mut self.dec_ctx);
            }
            if !self.fmt_ctx.is_null() {
                sys::avformat_close_input(&mut self.fmt_ctx);
            }
            if !self.hw_device_ctx.is_null() {
                sys::av_buffer_unref(&mut self.hw_device_ctx);
            }
        }
    }
}

fn is_10bit_pix_fmt(stream_sw_format_i32: i32) -> bool {
    stream_sw_format_i32 == av_pix_fmt_yuv420p10le()
        || stream_sw_format_i32 == av_pix_fmt_yuv420p12le()
        || stream_sw_format_i32 == av_pix_fmt_p010le()
        || stream_sw_format_i32 == av_pix_fmt_p012le()
}

unsafe fn config_supports_sw_format(
    hw_device_ctx: *mut sys::AVBufferRef,
    config: *const sys::AVCodecHWConfig,
    stream_sw_format: sys::AVPixelFormat,
    device_type: sys::AVHWDeviceType,
) -> bool {
    unsafe {
        let stream_sw_format_i32 = std::mem::transmute::<sys::AVPixelFormat, i32>(stream_sw_format);
        let Some(target_sw_format) = resolve_hw_sw_format(stream_sw_format_i32) else {
            eprintln!(
                "[neoutl-video-decoder][diag] resolve_hw_sw_format失敗 stream_sw_format={stream_sw_format_i32}"
            );
            return false;
        };
        let want_sw_format = std::mem::transmute::<i32, sys::AVPixelFormat>(target_sw_format);
        eprintln!(
            "[neoutl-video-decoder][diag] config_supports_sw_format開始 config_pix_fmt={:?} want_sw_format={target_sw_format}",
            (*config).pix_fmt
        );

        if device_type == sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI {
            eprintln!(
                "[neoutl-video-decoder][diag] VAAPIはprobe段階でvaCreateConfig実検証済みのためFFmpeg制約問い合わせを省略、対応扱い"
            );
            return true;
        }

        let mut constraints =
            sys::av_hwdevice_get_hwframe_constraints(hw_device_ctx, config as *const c_void);
        if constraints.is_null() {
            eprintln!(
                "[neoutl-video-decoder][diag] av_hwdevice_get_hwframe_constraints=NULL(制約問い合わせ失敗) config_pix_fmt={:?} → 非対応扱いとしてフォールバック",
                (*config).pix_fmt
            );
            return false;
        }
        let valid_sw_formats = (*constraints).valid_sw_formats;
        let supported = if valid_sw_formats.is_null() {
            true
        } else {
            let mut p = valid_sw_formats;
            let mut found = false;
            let mut listed = Vec::new();
            while std::mem::transmute::<sys::AVPixelFormat, i32>(*p) != av_pix_fmt_none() {
                listed.push(std::mem::transmute::<sys::AVPixelFormat, i32>(*p));
                if *p == want_sw_format {
                    found = true;
                    break;
                }
                p = p.add(1);
            }
            eprintln!(
                "[neoutl-video-decoder][diag] valid_sw_formats={listed:?} want={target_sw_format} found={found}"
            );
            found
        };
        sys::av_hwframe_constraints_free(&mut constraints);
        supported
    }
}

fn vaapi_render_node_candidates(
    codec: *const sys::AVCodec,
    stream_sw_format: sys::AVPixelFormat,
) -> Vec<CString> {
    let codec_id = unsafe { (*codec).id };
    let stream_sw_format_i32 =
        unsafe { std::mem::transmute::<sys::AVPixelFormat, i32>(stream_sw_format) };
    let want_10bit = is_10bit_pix_fmt(stream_sw_format_i32);
    match probe_vaapi_node(codec_id, want_10bit) {
        Some(node) => vec![node.device_path],
        None => Vec::new(),
    }
}

unsafe fn try_init_hw_device(
    codec: *const sys::AVCodec,
    stream_sw_format: sys::AVPixelFormat,
) -> Option<(*mut sys::AVBufferRef, i32)> {
    unsafe {
        eprintln!(
            "[neoutl-video-decoder][diag] try_init_hw_device開始 codec={:?} stream_sw_format={:?}",
            (*codec).id,
            stream_sw_format
        );
        for name in HW_DEVICE_TYPE_NAMES {
            let c_name = CString::new(*name).ok()?;
            let device_type = sys::av_hwdevice_find_type_by_name(c_name.as_ptr());
            if device_type == sys::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE {
                eprintln!("[neoutl-video-decoder][diag] デバイスタイプ未検出 name={name}");
                continue;
            }

            let device_paths: Vec<Option<CString>> =
                if device_type == sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI {
                    let nodes = vaapi_render_node_candidates(codec, stream_sw_format);
                    eprintln!(
                        "[neoutl-video-decoder][diag] VAAPIノード候補数={} nodes={:?}",
                        nodes.len(),
                        nodes
                            .iter()
                            .map(|c| c.to_string_lossy())
                            .collect::<Vec<_>>()
                    );
                    if nodes.is_empty() {
                        continue;
                    }
                    nodes.into_iter().map(Some).collect()
                } else {
                    vec![None]
                };

            for device_path in &device_paths {
                let mut i = 0;
                loop {
                    let config = sys::avcodec_get_hw_config(codec, i);
                    if config.is_null() {
                        eprintln!(
                            "[neoutl-video-decoder][diag] avcodec_get_hw_config終端 name={name} config_index={i}"
                        );
                        break;
                    }
                    let methods = (*config).methods;
                    let matches_method = (methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX) != 0;
                    eprintln!(
                        "[neoutl-video-decoder][diag] config走査 name={name} index={i} device_type={:?} config_device_type={:?} methods={methods:#x} matches_method={matches_method}",
                        device_type,
                        (*config).device_type
                    );
                    if matches_method && (*config).device_type == device_type {
                        let path_ptr = device_path
                            .as_ref()
                            .map(|p| p.as_ptr())
                            .unwrap_or(ptr::null());
                        let path_label = device_path
                            .as_ref()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "既定".to_owned());
                        eprintln!(
                            "[neoutl-video-decoder][diag] av_hwdevice_ctx_create呼出 name={name} node={path_label} config_pix_fmt={:?}",
                            (*config).pix_fmt
                        );
                        let mut hw_device_ctx: *mut sys::AVBufferRef = ptr::null_mut();
                        let ret = sys::av_hwdevice_ctx_create(
                            &mut hw_device_ctx,
                            device_type,
                            path_ptr,
                            ptr::null_mut(),
                            0,
                        );
                        eprintln!(
                            "[neoutl-video-decoder][diag] av_hwdevice_ctx_create結果 name={name} node={path_label} ret={ret}"
                        );
                        if ret == 0 {
                            let format_ok = config_supports_sw_format(
                                hw_device_ctx,
                                config,
                                stream_sw_format,
                                device_type,
                            );
                            eprintln!(
                                "[neoutl-video-decoder][diag] config_supports_sw_format結果 name={name} node={path_label} format_ok={format_ok}"
                            );
                            if format_ok {
                                return Some((
                                    hw_device_ctx,
                                    std::mem::transmute::<sys::AVPixelFormat, i32>(
                                        (*config).pix_fmt,
                                    ),
                                ));
                            }
                            eprintln!(
                                "[neoutl-video-decoder] HWデバイス({name}, node={path_label})はストリームフォーマット非対応、次候補探索"
                            );
                            sys::av_buffer_unref(&mut hw_device_ctx);
                        } else {
                            eprintln!(
                                "[neoutl-video-decoder] HWデバイス初期化失敗 name={name} node={path_label} ret={ret}"
                            );
                        }
                    }
                    i += 1;
                }
            }
        }
        eprintln!("[neoutl-video-decoder][diag] try_init_hw_device全候補探索終了、HW初期化失敗");
        None
    }
}

fn open_input(path: &Path, gpu_device: &Option<Arc<wgpu::Device>>) -> Result<OpenContext, String> {
    unsafe {
        let mut fmt_ctx: *mut sys::AVFormatContext = ptr::null_mut();
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("パスにNUL文字が含まれる: {e}"))?;
        if sys::avformat_open_input(
            &mut fmt_ctx,
            c_path.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
        ) != 0
        {
            return Err(format!("avformat_open_input失敗: {}", path.display()));
        }
        if sys::avformat_find_stream_info(fmt_ctx, ptr::null_mut()) < 0 {
            sys::avformat_close_input(&mut fmt_ctx);
            return Err("avformat_find_stream_info失敗".to_owned());
        }

        let stream_index = sys::av_find_best_stream(
            fmt_ctx,
            sys::AVMediaType::AVMEDIA_TYPE_VIDEO,
            -1,
            -1,
            ptr::null_mut(),
            0,
        );
        if stream_index < 0 {
            sys::avformat_close_input(&mut fmt_ctx);
            return Err("映像ストリームが見つからない".to_owned());
        }

        let stream = *(*fmt_ctx).streams.add(stream_index as usize);
        let time_base = ((*stream).time_base.num, (*stream).time_base.den);
        let mut fps =
            (*stream).avg_frame_rate.num as f64 / (*stream).avg_frame_rate.den.max(1) as f64;
        if fps <= 0.0 {
            fps = (*stream).r_frame_rate.num as f64 / (*stream).r_frame_rate.den.max(1) as f64;
        }

        let codec = sys::avcodec_find_decoder((*(*stream).codecpar).codec_id);
        if codec.is_null() {
            sys::avformat_close_input(&mut fmt_ctx);
            return Err("対応デコーダなし".to_owned());
        }

        let dec_ctx = sys::avcodec_alloc_context3(codec);
        if dec_ctx.is_null() {
            sys::avformat_close_input(&mut fmt_ctx);
            return Err("avcodec_alloc_context3失敗".to_owned());
        }
        if sys::avcodec_parameters_to_context(dec_ctx, (*stream).codecpar) < 0 {
            sys::avcodec_free_context(&mut { dec_ctx });
            sys::avformat_close_input(&mut fmt_ctx);
            return Err("avcodec_parameters_to_context失敗".to_owned());
        }

        let mut hw_device_ctx: *mut sys::AVBufferRef = ptr::null_mut();
        let mut hw_pix_fmt_box: Option<Box<HwPixFmtBox>> = None;

        let stream_sw_format =
            std::mem::transmute::<i32, sys::AVPixelFormat>((*(*stream).codecpar).format);

        if let Some((created_hw_ctx, hw_pix_fmt)) = try_init_hw_device(codec, stream_sw_format) {
            hw_device_ctx = created_hw_ctx;
            let boxed = Box::new(HwPixFmtBox {
                pix_fmt: hw_pix_fmt,
                stream_sw_format: std::mem::transmute::<sys::AVPixelFormat, i32>(stream_sw_format),
                hw_device_ctx: created_hw_ctx,
            });
            (*dec_ctx).opaque = boxed.as_ref() as *const HwPixFmtBox as *mut c_void;
            (*dec_ctx).get_format = Some(hw_get_format);
            (*dec_ctx).hw_device_ctx = sys::av_buffer_ref(hw_device_ctx);
            hw_pix_fmt_box = Some(boxed);
        } else {
            let capabilities = (*codec).capabilities;
            if (capabilities & AV_CODEC_CAP_FRAME_THREADS) != 0 {
                (*dec_ctx).thread_type = FF_THREAD_FRAME;
                (*dec_ctx).thread_count = 0;
            } else if (capabilities & AV_CODEC_CAP_SLICE_THREADS) != 0 {
                (*dec_ctx).thread_type = FF_THREAD_SLICE;
                (*dec_ctx).thread_count = 0;
            }
        }

        if sys::avcodec_open2(dec_ctx, codec, ptr::null_mut()) != 0 {
            if !hw_device_ctx.is_null() {
                sys::av_buffer_unref(&mut hw_device_ctx);
            }
            sys::avcodec_free_context(&mut { dec_ctx });
            sys::avformat_close_input(&mut fmt_ctx);
            return Err("avcodec_open2失敗".to_owned());
        }

        let width = (*dec_ctx).width.max(0) as u32;
        let height = (*dec_ctx).height.max(0) as u32;

        let gpu_pipeline = build_gpu_pipeline(gpu_device, hw_device_ctx);

        if sys::av_seek_frame(fmt_ctx, stream_index, 0, sys::AVSEEK_FLAG_BACKWARD) < 0 {
            sys::av_seek_frame(fmt_ctx, -1, 0, sys::AVSEEK_FLAG_BACKWARD);
        }
        let index = build_index(fmt_ctx, stream_index);
        if sys::av_seek_frame(fmt_ctx, stream_index, 0, sys::AVSEEK_FLAG_BACKWARD) < 0 {
            sys::av_seek_frame(fmt_ctx, -1, 0, sys::AVSEEK_FLAG_BACKWARD);
        }
        sys::avcodec_flush_buffers(dec_ctx);

        Ok(OpenContext {
            fmt_ctx,
            dec_ctx,
            stream_index,
            time_base,
            fps: if fps > 0.0 { fps } else { 30.0 },
            width,
            height,
            sws_ctx: ptr::null_mut(),
            index,
            hw_device_ctx,
            hw_pix_fmt_box,
            gpu_pipeline,
            last_good_frame: None,
            last_convert_path: ConvertPath::Unknown,
        })
    }
}

fn build_gpu_pipeline(
    gpu_device: &Option<Arc<wgpu::Device>>,
    hw_device_ctx: *mut sys::AVBufferRef,
) -> Option<GpuPipeline> {
    let wgpu_device = gpu_device.clone()?;
    if hw_device_ctx.is_null() {
        return None;
    }

    let entry = unsafe { ash::Entry::load().ok()? };
    let vulkan_ctx = match vulkan::init_vulkan_context(&wgpu_device, &entry) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("[neoutl-video-decoder] Vulkan相互運用初期化失敗、CPU経路使用: {e}");
            return None;
        }
    };

    let semi_planar_engine = unsafe {
        vulkan::extract_vulkan_raw_handles(&wgpu_device).and_then(|handles| {
            static SEMI_PLANAR_SPIRV: &[u8] =
                include_bytes!(concat!(env!("OUT_DIR"), "/semi_planar_to_rgba.spv"));
            vulkan::SemiPlanarConvertEngine::new(&handles, &entry, SEMI_PLANAR_SPIRV).ok()
        })
    };

    Some(GpuPipeline {
        wgpu_device,
        vulkan_ctx,
        derived_frames_ctx: ptr::null_mut(),
        semi_planar_engine,
    })
}

fn seek_to_keyframe(ctx: &mut OpenContext, keyframe_index: i64) {
    unsafe {
        let seek_pts = ctx.index.pts_at(keyframe_index);
        if sys::avformat_seek_file(
            ctx.fmt_ctx,
            ctx.stream_index,
            i64::MIN,
            seek_pts,
            seek_pts,
            sys::AVSEEK_FLAG_BACKWARD,
        ) < 0
        {
            sys::av_seek_frame(
                ctx.fmt_ctx,
                ctx.stream_index,
                seek_pts,
                sys::AVSEEK_FLAG_BACKWARD,
            );
        }
        sys::avcodec_flush_buffers(ctx.dec_ctx);
    }
}

enum ConvertOutcome {
    Gpu(VideoFrame),
    CpuFallback(&'static str),
}

fn try_convert_to_gpu(ctx: &mut OpenContext, av_frame: *mut sys::AVFrame) -> ConvertOutcome {
    let Some(gpu) = ctx.gpu_pipeline.as_mut() else {
        return ConvertOutcome::CpuFallback("GPUパイプライン未初期化(Vulkan相互運用不可)");
    };
    let frame_format = unsafe { (*av_frame).format };
    let Some(hw_pix_fmt_box) = ctx.hw_pix_fmt_box.as_ref() else {
        return ConvertOutcome::CpuFallback("HWデコード非有効(全候補非対応または失敗)");
    };
    let hw_pix_fmt = hw_pix_fmt_box.pix_fmt;
    if frame_format != hw_pix_fmt {
        return ConvertOutcome::CpuFallback("デコード結果がHWサーフェスでない");
    }
    let sw_format = unsafe { (*ctx.dec_ctx).sw_pix_fmt };
    let sw_format_i32 = unsafe { std::mem::transmute::<sys::AVPixelFormat, i32>(sw_format) };
    let is_direct_rgba = sw_format_i32 == av_pix_fmt_rgb0() || sw_format_i32 == av_pix_fmt_bgr0();
    let semi_planar_view_formats = semi_planar_view_formats(sw_format_i32);
    if !is_direct_rgba && semi_planar_view_formats.is_none() {
        return ConvertOutcome::CpuFallback("sw_pix_fmtがRGB0/BGR0/NV12/P010LE/P012LE/P016LE以外");
    }
    if semi_planar_view_formats.is_some() && gpu.semi_planar_engine.is_none() {
        return ConvertOutcome::CpuFallback("セミプラナー変換エンジン未初期化");
    }

    if gpu.derived_frames_ctx.is_null() {
        let src_frames_ctx = unsafe { (*ctx.dec_ctx).hw_frames_ctx };
        if src_frames_ctx.is_null() {
            return ConvertOutcome::CpuFallback("hw_frames_ctx未設定");
        }
        match vulkan::create_derived_vulkan_frames_ctx(src_frames_ctx, &gpu.vulkan_ctx.device_ctx) {
            Ok(derived) => gpu.derived_frames_ctx = derived,
            Err(e) => {
                eprintln!("[neoutl-video-decoder] Vulkan導出フレームコンテキスト生成失敗: {e}");
                ctx.gpu_pipeline = None;
                return ConvertOutcome::CpuFallback("Vulkan導出フレームコンテキスト生成失敗");
            }
        }
    }

    let derived = match vulkan::transfer_to_vulkan_frame(av_frame, gpu.derived_frames_ctx) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[neoutl-video-decoder] Vulkanフレーム導出転送失敗: {e}");
            return ConvertOutcome::CpuFallback("Vulkanフレーム導出転送失敗");
        }
    };

    let src_image = unsafe { vulkan::vk_image_of(&derived) };

    let target_desc = wgpu::TextureDescriptor {
        label: Some("neoutl-video-gpu-frame"),
        size: wgpu::Extent3d {
            width: ctx.width,
            height: ctx.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    };
    let target_texture = gpu.wgpu_device.create_texture(&target_desc);

    let dst_vk_image = unsafe {
        target_texture
            .as_hal::<wgpu_hal::api::Vulkan>()
            .map(|hal_texture| hal_texture.raw_handle())
    };
    let Some(dst_vk_image) = dst_vk_image else {
        return ConvertOutcome::CpuFallback("wgpuテクスチャからVkImageハンドル取得失敗");
    };

    let convert_result = if let Some((y_format, uv_format)) = semi_planar_view_formats {
        let Some(engine) = gpu.semi_planar_engine.as_ref() else {
            return ConvertOutcome::CpuFallback("セミプラナー変換エンジン未初期化");
        };
        unsafe {
            engine.convert(
                src_image.image,
                src_image.layout,
                dst_vk_image,
                ctx.width,
                ctx.height,
                y_format,
                uv_format,
            )
        }
    } else {
        unsafe {
            gpu.vulkan_ctx
                .copy_engine
                .copy_image(src_image, dst_vk_image, ctx.width, ctx.height)
        }
    };
    if let Err(e) = convert_result {
        eprintln!("[neoutl-video-decoder] GPUフレーム変換失敗: {e}");
        return ConvertOutcome::CpuFallback("GPUフレーム変換(VkImageコピー/コンピュート)失敗");
    }

    let gpu_frame = GpuFrame::new(target_texture, ctx.width, ctx.height);
    ConvertOutcome::Gpu(VideoFrame::Gpu(Arc::new(gpu_frame)))
}

unsafe fn convert_to_rgba8_cpu(ctx: &mut OpenContext, av_frame: *mut sys::AVFrame) -> Rgba8Frame {
    unsafe {
        let hw_pix_fmt = ctx
            .hw_pix_fmt_box
            .as_ref()
            .map(|b| b.pix_fmt)
            .unwrap_or(av_pix_fmt_none());

        let mut sw_frame: *mut sys::AVFrame = ptr::null_mut();
        let src_frame = if (*av_frame).format == hw_pix_fmt && hw_pix_fmt != av_pix_fmt_none() {
            sw_frame = sys::av_frame_alloc();
            if sys::av_hwframe_transfer_data(sw_frame, av_frame, 0) == 0 {
                sw_frame
            } else {
                sys::av_frame_free(&mut sw_frame);
                av_frame
            }
        } else {
            av_frame
        };

        let width = ctx.width as i32;
        let height = ctx.height as i32;
        let src_format = std::mem::transmute::<i32, sys::AVPixelFormat>((*src_frame).format);

        let result = if src_format == sys::AVPixelFormat::AV_PIX_FMT_RGBA {
            let src_linesize = (*src_frame).linesize[0] as usize;
            let dst_stride = (width * 4) as usize;
            let mut data = vec![0u8; dst_stride * height as usize];
            let src_ptr = (*src_frame).data[0];
            for row in 0..height as usize {
                std::ptr::copy_nonoverlapping(
                    src_ptr.add(row * src_linesize),
                    data.as_mut_ptr().add(row * dst_stride),
                    dst_stride,
                );
            }
            Rgba8Frame {
                width: width as u32,
                height: height as u32,
                data,
            }
        } else {
            ctx.sws_ctx = sys::sws_getCachedContext(
                ctx.sws_ctx,
                width,
                height,
                src_format,
                width,
                height,
                sys::AVPixelFormat::AV_PIX_FMT_RGBA,
                SWS_BILINEAR,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            );

            let mut data: Vec<u8> = vec![0u8; (width * height * 4) as usize];
            let mut dst_data = [
                data.as_mut_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            ];
            let dst_linesize = [width * 4, 0, 0, 0];

            sys::sws_scale(
                ctx.sws_ctx,
                (*src_frame).data.as_ptr() as *const *const u8,
                (*src_frame).linesize.as_ptr(),
                0,
                height,
                dst_data.as_mut_ptr(),
                dst_linesize.as_ptr(),
            );

            Rgba8Frame {
                width: width as u32,
                height: height as u32,
                data,
            }
        };

        if !sw_frame.is_null() {
            sys::av_frame_free(&mut sw_frame);
        }
        result
    }
}

fn convert_frame(ctx: &mut OpenContext, av_frame: *mut sys::AVFrame) -> VideoFrame {
    let (frame, path, reason) = match try_convert_to_gpu(ctx, av_frame) {
        ConvertOutcome::Gpu(frame) => (frame, ConvertPath::GpuZeroCopy, None),
        ConvertOutcome::CpuFallback(reason) => {
            let rgba = unsafe { convert_to_rgba8_cpu(ctx, av_frame) };
            (
                VideoFrame::Cpu(Arc::new(rgba)),
                ConvertPath::CpuRam,
                Some(reason),
            )
        }
    };

    if ctx.last_convert_path != path {
        match path {
            ConvertPath::GpuZeroCopy => {
                eprintln!(
                    "[neoutl-video-decoder][転送経路] VRAM内ゼロコピーへ切替(RAM転送なし) size={}x{}",
                    ctx.width, ctx.height
                );
            }
            ConvertPath::CpuRam => {
                eprintln!(
                    "[neoutl-video-decoder][転送経路] CPU/RAM経由へ切替(GPU→RAM→GPU転送発生) 理由={} size={}x{}",
                    reason.unwrap_or("不明"),
                    ctx.width,
                    ctx.height
                );
            }
            ConvertPath::Unknown => {}
        }
        ctx.last_convert_path = path;
    }

    frame
}

#[allow(clippy::too_many_arguments)]
fn run_worker(
    path: std::path::PathBuf,
    clip_key: String,
    store: Arc<VideoFrameStore>,
    gpu_device: Option<Arc<wgpu::Device>>,
    _gpu_queue: Option<Arc<wgpu::Queue>>,
    shared: Arc<(Mutex<Mailbox>, Condvar)>,
    is_ready: Arc<AtomicBool>,
    last_requested_frame: Arc<AtomicI64>,
    on_ready: impl FnOnce(VideoMeta) + Send + 'static,
) {
    let mut ctx = match open_input(&path, &gpu_device) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("[neoutl-video-decoder] open失敗: {e}");
            return;
        }
    };

    is_ready.store(true, Ordering::Release);
    on_ready(VideoMeta {
        total_frames: ctx.index.len(),
        fps: ctx.fps,
        width: ctx.width,
        height: ctx.height,
    });

    let mut frame_cache = FrameLruCache::new(DEFAULT_FRAME_CACHE_BYTES);
    let mut gop_cache = GopCache::new(GOP_CACHE_CAPACITY);
    let mut last_decoded_frame: i64 = -1;

    let (lock, cvar) = &*shared;
    loop {
        let target = {
            let mut guard = lock.lock().expect("mailbox mutex poisoned");
            loop {
                if let Some(target) = guard.target_frame.take() {
                    break target;
                }
                if guard.stopped {
                    return;
                }
                guard = cvar.wait(guard).expect("mailbox condvar poisoned");
            }
        };

        decode_task(
            &mut ctx,
            target,
            &clip_key,
            &store,
            &mut frame_cache,
            &mut gop_cache,
            &mut last_decoded_frame,
            &last_requested_frame,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_task(
    ctx: &mut OpenContext,
    requested_target: i64,
    clip_key: &str,
    store: &Arc<VideoFrameStore>,
    frame_cache: &mut FrameLruCache,
    gop_cache: &mut GopCache,
    last_decoded_frame: &mut i64,
    last_requested_frame: &AtomicI64,
) {
    if ctx.index.is_empty() {
        return;
    }
    let target = requested_target.clamp(0, ctx.index.len() - 1);

    if let Some(frame) = gop_cache.get(target) {
        store.set_frame(clip_key, frame.clone());
        ctx.last_good_frame = Some(frame);
        return;
    }
    if let Some(frame) = frame_cache.get(target) {
        store.set_frame(clip_key, frame.clone());
        ctx.last_good_frame = Some(frame);
        return;
    }

    let key_index = ctx.index.preceding_keyframe(target);
    let gop_end = ctx.index.gop_end_of(target);

    let contiguous_forward = *last_decoded_frame != -1
        && target > *last_decoded_frame
        && target <= *last_decoded_frame + FORWARD_DECODE_THRESHOLD;
    let need_seek = !contiguous_forward;
    let should_fill_gop = need_seek;

    if need_seek {
        seek_to_keyframe(ctx, key_index);
        *last_decoded_frame = key_index - 1;
    }

    let mut new_gop_block = GopCacheBlock {
        keyframe_index: key_index,
        start: key_index,
        end: gop_end,
        frames: std::collections::HashMap::new(),
    };

    let mut target_dispatched = false;
    let mut decode_budget = (gop_end - key_index + 10).max(500);
    let mut eof = false;
    let pkt = unsafe { sys::av_packet_alloc() };
    let av_frame = unsafe { sys::av_frame_alloc() };

    while decode_budget > 0 {
        decode_budget -= 1;

        let mut send_ret = 0;
        if !eof {
            let read_ret = unsafe { sys::av_read_frame(ctx.fmt_ctx, pkt) };
            if read_ret < 0 {
                eof = true;
            }
        }
        unsafe {
            if eof {
                send_ret = sys::avcodec_send_packet(ctx.dec_ctx, ptr::null());
            } else if (*pkt).stream_index == ctx.stream_index {
                send_ret = sys::avcodec_send_packet(ctx.dec_ctx, pkt);
            }
            if !eof {
                sys::av_packet_unref(pkt);
            }
        }
        if send_ret < 0 && send_ret != averror_eagain() {
            break;
        }

        loop {
            let recv_ret = unsafe { sys::avcodec_receive_frame(ctx.dec_ctx, av_frame) };
            if recv_ret == averror_eagain() {
                break;
            }
            if recv_ret == averror_eof() {
                eof = true;
                break;
            }
            if recv_ret < 0 {
                break;
            }

            let pts = unsafe {
                if (*av_frame).pts != sys::AV_NOPTS_VALUE {
                    (*av_frame).pts
                } else {
                    (*av_frame).pkt_dts
                }
            };
            let Some(decoded_index) = ctx.index.index_of_pts(pts) else {
                continue;
            };
            *last_decoded_frame = decoded_index;

            if !frame_cache.contains(decoded_index) {
                let frame = convert_frame(ctx, av_frame);
                frame_cache.insert(decoded_index, frame.clone());
                new_gop_block.frames.insert(decoded_index, frame.clone());
                ctx.last_good_frame = Some(frame.clone());

                if decoded_index == target && !target_dispatched {
                    store.set_frame(clip_key, frame);
                    target_dispatched = true;
                }
            } else if decoded_index == target && !target_dispatched {
                if let Some(frame) = frame_cache.get(decoded_index) {
                    store.set_frame(clip_key, frame.clone());
                    ctx.last_good_frame = Some(frame);
                }
                target_dispatched = true;
            }

            if last_requested_frame.load(Ordering::Acquire) != requested_target {
                unsafe {
                    sys::av_packet_unref(pkt);
                    sys::av_frame_free(&mut { av_frame });
                    sys::av_packet_free(&mut { pkt });
                }
                return;
            }

            if (!should_fill_gop && *last_decoded_frame >= target) || *last_decoded_frame >= gop_end
            {
                break;
            }
        }

        if eof
            || (!should_fill_gop && *last_decoded_frame >= target)
            || *last_decoded_frame >= gop_end
        {
            break;
        }
    }

    unsafe {
        sys::av_frame_free(&mut { av_frame });
        sys::av_packet_free(&mut { pkt });
    }

    if !new_gop_block.frames.is_empty() {
        gop_cache.put(new_gop_block);
    }

    if !target_dispatched {
        if let Some(frame) = ctx.last_good_frame.clone() {
            store.set_frame(clip_key, frame);
        }
    }
}

fn averror_eagain() -> i32 {
    -(libc::EAGAIN as i32)
}

fn averror_eof() -> i32 {
    sys::AVERROR_EOF
}
