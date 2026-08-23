use std::ffi::{CString, c_void};
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use ffmpeg_sys_next as sys;

use neo_media_cache::NeoMediaCache;
use neo_media_core::{
    ColorPrimaries, MatrixCoefficients, Rect, Size, TransferBackend, TransferCharacteristics,
};
#[cfg(target_os = "windows")]
use neo_media_transfer_d3d11::D3d11TransferBackend;
#[cfg(unix)]
use neo_media_transfer_vaapi::VaapiTransferBackend;

use crate::cache::{GopCache, GopCacheBlock, PooledFrameCache};
use crate::frame::{GpuFrame, VideoFrame, VideoFrameStore};
use crate::index::{FrameIndex, build_index};
use crate::vaapi_probe::probe_vaapi_node;

static SHARED_WGPU: OnceLock<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> = OnceLock::new();
static SHARED_CACHE: OnceLock<Arc<NeoMediaCache>> = OnceLock::new();
static SHARED_QUEUE_SUBMIT_LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();

pub fn shared_wgpu_submit_lock() -> Arc<Mutex<()>> {
    SHARED_QUEUE_SUBMIT_LOCK
        .get_or_init(|| Arc::new(Mutex::new(())))
        .clone()
}

fn rgba8_frame_bytes(width: u32, height: u32) -> u64 {
    width as u64 * height as u64 * 4
}

pub fn set_shared_wgpu_device(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) {
    let device_for_query = device.clone();
    let budget_provider: Arc<neo_media_cache::VramBudgetProvider> = Arc::new(move || unsafe {
        neo_media_transfer_vaapi::query_vram_budget_bytes(&device_for_query)
    });
    let _ = SHARED_CACHE.set(Arc::new(NeoMediaCache::new(
        (*device).clone(),
        Some(budget_provider),
    )));
    if let Some(cache) = SHARED_CACHE.get() {
        cache.register_consumer(neo_media_cache::KIND_PLAYBACK, 3);
        cache.register_consumer(neo_media_cache::KIND_THUMBNAIL, 1);
        cache.register_consumer(neo_media_cache::KIND_LUA_SAMPLE, 1);
    }
    let _ = SHARED_WGPU.set((device, queue));
}

pub fn shared_wgpu_device() -> Option<Arc<wgpu::Device>> {
    SHARED_WGPU.get().map(|(device, _)| device.clone())
}

pub fn shared_wgpu_queue() -> Option<Arc<wgpu::Queue>> {
    SHARED_WGPU.get().map(|(_, queue)| queue.clone())
}

fn shared_media_cache() -> Option<Arc<NeoMediaCache>> {
    SHARED_CACHE.get().cloned()
}
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
const POOLED_FRAME_CACHE_MARGIN: usize = 2;
const HW_DEVICE_TYPE_NAMES: &[&str] = &["cuda", "vaapi", "d3d11va", "dxva2", "videotoolbox"];

fn pooled_frame_cache_capacity(width: u32, height: u32) -> usize {
    shared_media_cache()
        .map(|cache| cache.effective_capacity(rgba8_frame_bytes(width, height)))
        .unwrap_or(neo_media_cache::MIN_CAPACITY)
        .saturating_sub(POOLED_FRAME_CACHE_MARGIN)
        .max(1)
}

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
        if let Some(overwritten) = mailbox.target_frame.replace(frame) {
            if overwritten != frame {
                eprintln!(
                    "[neoutl-video-decoder][診断][seek上書き] overwritten={overwritten} new={frame}"
                );
            }
        }
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

fn resolve_hw_sw_format(stream_sw_format: i32) -> Option<i32> {
    if stream_sw_format == av_pix_fmt_nv12()
        || stream_sw_format == av_pix_fmt_yuv420p()
        || stream_sw_format == av_pix_fmt_yuvj420p()
    {
        Some(av_pix_fmt_p010le())
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
    wgpu_queue: Arc<wgpu::Queue>,
    cache: Arc<NeoMediaCache>,
    #[cfg(unix)]
    backend: VaapiTransferBackend,
    #[cfg(target_os = "windows")]
    backend: D3d11TransferBackend,
}

unsafe impl Send for GpuPipeline {}

struct OpenContext {
    fmt_ctx: *mut sys::AVFormatContext,
    dec_ctx: *mut sys::AVCodecContext,
    stream_index: i32,
    #[allow(dead_code)]
    time_base: (i32, i32),
    fps: f64,
    width: u32,
    height: u32,
    index: FrameIndex,
    hw_device_ctx: *mut sys::AVBufferRef,
    hw_pix_fmt_box: Option<Box<HwPixFmtBox>>,
    gpu_pipeline: Option<GpuPipeline>,
    last_good_frame: Option<VideoFrame>,
}

unsafe impl Send for OpenContext {}

impl Drop for OpenContext {
    fn drop(&mut self) {
        unsafe {
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

        if device_type == sys::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA {
            eprintln!(
                "[neoutl-video-decoder][diag] D3D11VAはNV12/P010固定のためFFmpeg制約問い合わせを省略、対応扱い"
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
        Some(node) => {
            eprintln!(
                "[neoutl-video-decoder][diag] VAAPIノード確定 path={:?} matched_profile={:?}",
                node.device_path, node.matched_profile
            );
            vec![node.device_path]
        }
        None => Vec::new(),
    }
}

unsafe fn try_init_hw_device(
    codec: *const sys::AVCodec,
    stream_sw_format: sys::AVPixelFormat,
    gpu_device: &Option<Arc<wgpu::Device>>,
) -> Option<(*mut sys::AVBufferRef, i32)> {
    unsafe {
        sys::av_log_set_level(sys::AV_LOG_DEBUG);
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

            #[cfg(target_os = "windows")]
            if device_type == sys::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA {
                let Some(wgpu_device) = gpu_device.as_ref() else {
                    eprintln!(
                        "[neoutl-video-decoder][diag] D3D11VA: wgpu_device未取得のため候補除外"
                    );
                    continue;
                };
                let Ok(luid) = neo_media_transfer_d3d11::dx12_adapter_luid(wgpu_device) else {
                    eprintln!("[neoutl-video-decoder][diag] D3D11VA: LUID取得失敗");
                    continue;
                };
                let Ok(d3d11_device) = neo_media_transfer_d3d11::create_d3d11_device_on_luid(luid)
                else {
                    eprintln!("[neoutl-video-decoder][diag] D3D11VA: 同一LUIDデバイス生成失敗");
                    continue;
                };
                let Ok(device_ctx) =
                    neo_media_transfer_d3d11::create_av_d3d11va_device_ctx(&d3d11_device)
                else {
                    eprintln!("[neoutl-video-decoder][diag] D3D11VA: av_hwdevice_ctx_init失敗");
                    continue;
                };
                let mut i = 0;
                loop {
                    let config = sys::avcodec_get_hw_config(codec, i);
                    if config.is_null() {
                        break;
                    }
                    let methods = (*config).methods;
                    let matches_method = (methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX) != 0;
                    if matches_method
                        && (*config).device_type == sys::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA
                        && config_supports_sw_format(
                            device_ctx.av_hw_device_ctx,
                            config,
                            stream_sw_format,
                            device_type,
                        )
                    {
                        let hw_device_ctx = sys::av_buffer_ref(device_ctx.av_hw_device_ctx);
                        return Some((
                            hw_device_ctx,
                            std::mem::transmute::<sys::AVPixelFormat, i32>((*config).pix_fmt),
                        ));
                    }
                    i += 1;
                }
                continue;
            }

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

        if let Some((created_hw_ctx, hw_pix_fmt)) =
            try_init_hw_device(codec, stream_sw_format, gpu_device)
        {
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

        let gpu_pipeline = build_gpu_pipeline(gpu_device, hw_device_ctx, width, height);

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
            index,
            hw_device_ctx,
            hw_pix_fmt_box,
            gpu_pipeline,
            last_good_frame: None,
        })
    }
}

fn build_gpu_pipeline(
    gpu_device: &Option<Arc<wgpu::Device>>,
    hw_device_ctx: *mut sys::AVBufferRef,
    #[cfg_attr(unix, allow(unused_variables))] dec_width: u32,
    #[cfg_attr(unix, allow(unused_variables))] dec_height: u32,
) -> Option<GpuPipeline> {
    let wgpu_device = gpu_device.clone()?;
    let wgpu_queue = shared_wgpu_queue()?;
    let cache = shared_media_cache()?;
    if hw_device_ctx.is_null() {
        return None;
    }
    #[cfg(unix)]
    {
        match VaapiTransferBackend::new(&wgpu_device, shared_wgpu_submit_lock()) {
            Ok(backend) => Some(GpuPipeline {
                wgpu_device,
                wgpu_queue,
                cache,
                backend,
            }),
            Err(e) => {
                eprintln!("[neoutl-video-decoder] Vulkan相互運用初期化失敗、GPU経路無効: {e}");
                None
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        let coded_size = Size {
            width: dec_width,
            height: dec_height,
        };
        let d3d11_device = unsafe {
            let device_ctx = (*hw_device_ctx).data as *mut sys::AVHWDeviceContext;
            let hwctx = (*device_ctx).hwctx as *mut sys::AVD3D11VADeviceContext;
            neo_media_transfer_d3d11::device_from_raw((*hwctx).device)
        };
        match D3d11TransferBackend::new(&wgpu_device, d3d11_device, coded_size) {
            Ok(backend) => Some(GpuPipeline {
                wgpu_device,
                wgpu_queue,
                cache,
                backend,
            }),
            Err(e) => {
                eprintln!("[neoutl-video-decoder] D3D12相互運用初期化失敗、GPU経路無効: {e}");
                None
            }
        }
    }
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
    Unsupported(&'static str),
}

fn map_color_primaries(v: sys::AVColorPrimaries) -> ColorPrimaries {
    match v {
        sys::AVColorPrimaries::AVCOL_PRI_BT709 => ColorPrimaries::Bt709,
        sys::AVColorPrimaries::AVCOL_PRI_BT2020 => ColorPrimaries::Bt2020,
        sys::AVColorPrimaries::AVCOL_PRI_SMPTE170M | sys::AVColorPrimaries::AVCOL_PRI_SMPTE240M => {
            ColorPrimaries::Smpte170m
        }
        _ => ColorPrimaries::Unknown,
    }
}

fn map_transfer_characteristics(v: sys::AVColorTransferCharacteristic) -> TransferCharacteristics {
    match v {
        sys::AVColorTransferCharacteristic::AVCOL_TRC_BT709 => TransferCharacteristics::Bt709,
        sys::AVColorTransferCharacteristic::AVCOL_TRC_SMPTE2084 => {
            TransferCharacteristics::Smpte2084
        }
        sys::AVColorTransferCharacteristic::AVCOL_TRC_ARIB_STD_B67 => {
            TransferCharacteristics::AribStdB67
        }
        _ => TransferCharacteristics::Unknown,
    }
}

fn map_matrix_coefficients(v: sys::AVColorSpace) -> MatrixCoefficients {
    match v {
        sys::AVColorSpace::AVCOL_SPC_BT709 => MatrixCoefficients::Bt709,
        sys::AVColorSpace::AVCOL_SPC_BT2020_NCL => MatrixCoefficients::Bt2020Ncl,
        sys::AVColorSpace::AVCOL_SPC_SMPTE170M => MatrixCoefficients::Smpte170m,
        _ => MatrixCoefficients::Unknown,
    }
}

fn is_full_range(v: sys::AVColorRange) -> bool {
    v == sys::AVColorRange::AVCOL_RANGE_JPEG
}

fn try_convert_to_gpu(
    ctx: &mut OpenContext,
    av_frame: *mut sys::AVFrame,
    frame_cache: &mut PooledFrameCache,
) -> ConvertOutcome {
    let Some(gpu) = ctx.gpu_pipeline.as_mut() else {
        return ConvertOutcome::Unsupported("GPUパイプライン未初期化(Vulkan相互運用不可)");
    };
    let frame_format = unsafe { (*av_frame).format };
    let Some(hw_pix_fmt_box) = ctx.hw_pix_fmt_box.as_ref() else {
        return ConvertOutcome::Unsupported("HWデコード非有効(全候補非対応または失敗)");
    };
    let hw_pix_fmt = hw_pix_fmt_box.pix_fmt;
    if frame_format != hw_pix_fmt {
        return ConvertOutcome::Unsupported("デコード結果がHWサーフェスでない");
    }
    let hw_frames_ctx_ref = unsafe { (*ctx.dec_ctx).hw_frames_ctx };
    if hw_frames_ctx_ref.is_null() {
        return ConvertOutcome::Unsupported("hw_frames_ctx未設定(sw_format取得不能)");
    }
    let sw_format = unsafe {
        let frames_ctx = (*hw_frames_ctx_ref).data as *mut sys::AVHWFramesContext;
        (*frames_ctx).sw_format
    };
    let sw_format_i32 = unsafe { std::mem::transmute::<sys::AVPixelFormat, i32>(sw_format) };
    let is_direct_rgba = sw_format_i32 == av_pix_fmt_rgb0() || sw_format_i32 == av_pix_fmt_bgr0();

    let pts = unsafe {
        if (*av_frame).pts != sys::AV_NOPTS_VALUE {
            (*av_frame).pts
        } else {
            (*av_frame).pkt_dts
        }
    };
    let progressive = unsafe { (*av_frame).flags & sys::AV_FRAME_FLAG_INTERLACED == 0 };

    let (color_primaries, transfer_characteristics, matrix_coefficients, full_range) = unsafe {
        (
            map_color_primaries((*av_frame).color_primaries),
            map_transfer_characteristics((*av_frame).color_trc),
            map_matrix_coefficients((*av_frame).colorspace),
            is_full_range((*av_frame).color_range),
        )
    };

    #[cfg(unix)]
    let input = neo_media_transfer_vaapi::VaapiDecodedFrame {
        av_frame,
        src_hw_frames_ctx: hw_frames_ctx_ref,
        sw_format_i32,
        is_direct_rgba,
        coded_size: Size {
            width: ctx.width,
            height: ctx.height,
        },
        visible_rect: Rect {
            x: 0,
            y: 0,
            width: ctx.width,
            height: ctx.height,
        },
        color_primaries,
        transfer_characteristics,
        matrix_coefficients,
        full_range,
        pts,
        duration: 0,
        progressive,
    };

    #[cfg(target_os = "windows")]
    let input = {
        let _ = is_direct_rgba;
        let d3d11_texture =
            match unsafe { neo_media_transfer_d3d11::texture_from_av_frame(av_frame) } {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("[neoutl-video-decoder] D3D11Texture2D取得失敗: {e}");
                    return ConvertOutcome::Unsupported("D3D11Texture2D取得失敗");
                }
            };
        let subresource_index = unsafe { (*av_frame).data[1] as usize as u32 };
        neo_media_transfer_d3d11::D3d11DecodedFrame {
            av_frame,
            d3d11_texture,
            subresource_index,
            sw_format_i32,
            coded_size: Size {
                width: ctx.width,
                height: ctx.height,
            },
            visible_rect: Rect {
                x: 0,
                y: 0,
                width: ctx.width,
                height: ctx.height,
            },
            color_primaries,
            transfer_characteristics,
            matrix_coefficients,
            full_range,
            pts,
            duration: 0,
            progressive,
        }
    };

    let cache = gpu.cache.clone();
    let neo_frame =
        match gpu
            .backend
            .transfer(&input, &gpu.wgpu_device, &gpu.wgpu_queue, cache.as_ref())
        {
            Ok(f) => f,
            Err(neo_media_core::TransferError::PoolExhausted) => {
                if !frame_cache.evict_oldest() {
                    eprintln!(
                        "[neoutl-video-decoder] GPUフレーム転送失敗: PoolExhausted(退避対象なし)"
                    );
                    return ConvertOutcome::Unsupported("GPUフレーム転送(TransferBackend)失敗");
                }
                match gpu.backend.transfer(
                    &input,
                    &gpu.wgpu_device,
                    &gpu.wgpu_queue,
                    cache.as_ref(),
                ) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!(
                            "[neoutl-video-decoder] GPUフレーム転送失敗(退避後再試行も失敗): {e:?}"
                        );
                        return ConvertOutcome::Unsupported("GPUフレーム転送(TransferBackend)失敗");
                    }
                }
            }
            Err(e) => {
                eprintln!("[neoutl-video-decoder] GPUフレーム転送失敗: {e:?}");
                return ConvertOutcome::Unsupported("GPUフレーム転送(TransferBackend)失敗");
            }
        };

    #[cfg(unix)]
    let dst_pixel_format = neo_media_transfer_vaapi::dst_pixel_format_for(sw_format_i32);
    #[cfg(target_os = "windows")]
    let dst_pixel_format = neo_media_transfer_d3d11::dst_pixel_format_for(sw_format_i32);

    let owner_token =
        cache.owner_token_of(dst_pixel_format, ctx.width, ctx.height, &neo_frame.texture);
    let gpu_frame =
        GpuFrame::new_pooled(neo_frame.texture, ctx.width, ctx.height, cache, owner_token);
    ConvertOutcome::Gpu(VideoFrame(Arc::new(gpu_frame)))
}

fn convert_frame(
    ctx: &mut OpenContext,
    av_frame: *mut sys::AVFrame,
    frame_cache: &mut PooledFrameCache,
) -> Option<VideoFrame> {
    match try_convert_to_gpu(ctx, av_frame, frame_cache) {
        ConvertOutcome::Gpu(frame) => Some(frame),
        ConvertOutcome::Unsupported(reason) => {
            eprintln!(
                "[neoutl-video-decoder][非対応] GPUデコード経路失敗、フレームを破棄 理由={reason} size={}x{}",
                ctx.width, ctx.height
            );
            None
        }
    }
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
            match neo_media_support::probe(&path) {
                Ok(_) => {
                    eprintln!("[neoutl-video-decoder] open失敗: {e}");
                }
                Err(failure) => {
                    eprintln!(
                        "[neoutl-video-decoder][非対応] probe判定: {} ({failure:?}) open失敗: {e}",
                        failure.message()
                    );
                }
            }
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

    let mut frame_cache = PooledFrameCache::new(pooled_frame_cache_capacity(ctx.width, ctx.height));
    let mut gop_cache = GopCache::new(GOP_CACHE_CAPACITY);
    let mut last_decoded_frame: i64 = -1;

    const DEBOUNCE_WINDOW: Duration = Duration::from_millis(16);

    let (lock, cvar) = &*shared;
    loop {
        let target = {
            let mut guard = lock.lock().expect("mailbox mutex poisoned");
            loop {
                if guard.target_frame.is_some() {
                    break;
                }
                if guard.stopped {
                    return;
                }
                guard = cvar.wait(guard).expect("mailbox condvar poisoned");
            }
            loop {
                let before = guard.target_frame;
                let (g, timeout) = cvar
                    .wait_timeout(guard, DEBOUNCE_WINDOW)
                    .expect("mailbox condvar poisoned");
                guard = g;
                if guard.stopped {
                    return;
                }
                if timeout.timed_out() || guard.target_frame == before {
                    break;
                }
            }
            guard.target_frame.take().expect("target_frame must be set")
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

        let latest_requested = last_requested_frame.load(Ordering::Acquire);
        if latest_requested >= 0 && latest_requested != target {
            let mut guard = lock.lock().expect("mailbox mutex poisoned");
            if guard.target_frame.is_none() && !guard.stopped {
                eprintln!(
                    "[neoutl-video-decoder][診断][収束再投入] dispatched={target} latest_requested={latest_requested}"
                );
                guard.target_frame = Some(latest_requested);
                cvar.notify_one();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_task(
    ctx: &mut OpenContext,
    requested_target: i64,
    clip_key: &str,
    store: &Arc<VideoFrameStore>,
    frame_cache: &mut PooledFrameCache,
    gop_cache: &mut GopCache,
    last_decoded_frame: &mut i64,
    last_requested_frame: &AtomicI64,
) {
    if ctx.index.is_empty() {
        return;
    }
    let target = requested_target.clamp(0, ctx.index.len() - 1);

    if let Some(frame) = gop_cache.get(target, frame_cache) {
        store.set_frame(clip_key, target, frame.clone());
        ctx.last_good_frame = Some(frame);
        eprintln!(
            "[neoutl-video-decoder][診断][decode_task終了][gop_cache即応] requested_target={requested_target} target={target}"
        );
        return;
    }
    if let Some(frame) = frame_cache.get(target) {
        store.set_frame(clip_key, target, frame.clone());
        ctx.last_good_frame = Some(frame);
        eprintln!(
            "[neoutl-video-decoder][診断][decode_task終了][frame_cache即応] requested_target={requested_target} target={target}"
        );
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
        frame_indices: Vec::new(),
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
                eprintln!(
                    "[neoutl-video-decoder][診断] index_of_pts不一致 pts={pts} target={target} (このフレームは破棄される)"
                );
                continue;
            };
            *last_decoded_frame = decoded_index;

            if !frame_cache.contains(decoded_index) {
                match convert_frame(ctx, av_frame, frame_cache) {
                    Some(frame) => {
                        frame_cache.insert(decoded_index, frame.clone());
                        new_gop_block.frame_indices.push(decoded_index);
                        ctx.last_good_frame = Some(frame.clone());

                        if decoded_index == target && !target_dispatched {
                            store.set_frame(clip_key, decoded_index, frame);
                            target_dispatched = true;
                        }
                    }
                    None => {}
                }
            } else if decoded_index == target && !target_dispatched {
                if let Some(frame) = frame_cache.get(decoded_index) {
                    store.set_frame(clip_key, decoded_index, frame.clone());
                    ctx.last_good_frame = Some(frame);
                }
                target_dispatched = true;
            }

            if last_requested_frame.load(Ordering::Acquire) != requested_target {
                let superseded_by = last_requested_frame.load(Ordering::Acquire);
                eprintln!(
                    "[neoutl-video-decoder][診断][decode_task中断] requested_target={requested_target} \
target={target} last_decoded_frame={last_decoded_frame} superseded_by={superseded_by} \
decoded_frame_count={}",
                    new_gop_block.frame_indices.len(),
                    last_decoded_frame = *last_decoded_frame,
                );
                if !new_gop_block.frame_indices.is_empty() {
                    gop_cache.put(new_gop_block);
                }
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

    let decoded_count = new_gop_block.frame_indices.len();

    if !new_gop_block.frame_indices.is_empty() {
        gop_cache.put(new_gop_block);
    }

    {
        let boundary_lo = (target - 2).max(0);
        let boundary_hi = (target + 2).min(ctx.index.len() - 1);
        let mut boundary_dump = String::new();
        for i in boundary_lo..=boundary_hi {
            boundary_dump.push_str(&format!(
                "[{i}:pts={},key={}] ",
                ctx.index.pts_at(i),
                ctx.index.is_key_at(i)
            ));
        }
        eprintln!(
            "[neoutl-video-decoder][診断][decode_task終了] requested_target={requested_target} \
target={target} key_index={key_index} gop_end={gop_end} \
last_decoded_frame={last_decoded_frame} target_dispatched={target_dispatched} \
decoded_frame_count={decoded_count} boundary={boundary_dump}",
            last_decoded_frame = *last_decoded_frame,
        );
    }

    if !target_dispatched {
        if let Some(frame) = ctx.last_good_frame.clone() {
            eprintln!(
                "[neoutl-video-decoder][診断][近似フレーム配信] requested_target={requested_target} target={target}"
            );
            store.set_frame(clip_key, requested_target, frame);
        }
    }
}

fn averror_eagain() -> i32 {
    -(libc::EAGAIN as i32)
}

fn averror_eof() -> i32 {
    sys::AVERROR_EOF
}
