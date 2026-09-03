use std::ffi::{CString, c_void};
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use ffmpeg_sys_next as sys;

use neo_media_cache::NeoMediaCache;
use neo_media_core::PixelFormat;

use crate::cache::{GopCache, GopCacheBlock, RamFrameCache, VramPromotionCache};
use crate::frame::{GpuFrame, PlaneBuffer, RamFrame, VideoFrame, VideoFrameStore};
use crate::index::{FrameIndex, build_index};

static SHARED_WGPU: OnceLock<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> = OnceLock::new();
static SHARED_CACHE: OnceLock<Arc<NeoMediaCache>> = OnceLock::new();
static SHARED_QUEUE_SUBMIT_LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();

pub fn shared_wgpu_submit_lock() -> Arc<Mutex<()>> {
    SHARED_QUEUE_SUBMIT_LOCK
        .get_or_init(|| Arc::new(Mutex::new(())))
        .clone()
}

fn rgba16f_frame_bytes(width: u32, height: u32) -> u64 {
    width as u64 * height as u64 * 8
}

pub fn set_shared_wgpu_device(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) {
    let budget_provider: Option<Arc<neo_media_cache::VramBudgetProvider>> = None;
    let ram_budget_provider: Option<Arc<neo_media_cache::RamBudgetProvider>> = None;
    let _ = SHARED_CACHE.set(Arc::new(NeoMediaCache::new(
        (*device).clone(),
        budget_provider,
        ram_budget_provider,
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
fn av_pix_fmt_rgb0() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_RGB0)
}
fn av_pix_fmt_bgr0() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_BGR0)
}
fn av_pix_fmt_rgba() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_RGBA)
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

fn av_color_meta_to_uniform(
    colorspace: sys::AVColorSpace,
    color_range: sys::AVColorRange,
) -> (u32, u32) {
    let color_matrix = match colorspace {
        sys::AVColorSpace::AVCOL_SPC_BT470BG | sys::AVColorSpace::AVCOL_SPC_SMPTE170M => 0,
        sys::AVColorSpace::AVCOL_SPC_BT2020_NCL | sys::AVColorSpace::AVCOL_SPC_BT2020_CL => 2,
        sys::AVColorSpace::AVCOL_SPC_BT709 => 1,
        _ => 1,
    };
    let range = match color_range {
        sys::AVColorRange::AVCOL_RANGE_JPEG => 1,
        _ => 0,
    };
    (color_matrix, range)
}

const AV_CODEC_CAP_SLICE_THREADS: i32 = 1 << 13;
const AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX: i32 = 0x01;
const FF_THREAD_FRAME: i32 = 1;
const FF_THREAD_SLICE: i32 = 2;
const SWS_BILINEAR: i32 = 2;
const OWN_CONCURRENT_HW_FRAME_HOLD: i32 = 1;

const GOP_CACHE_CAPACITY: usize = 3;
const FORWARD_DECODE_THRESHOLD: i64 = 120;
const RAM_FRAME_CACHE_MARGIN: usize = 2;
const VRAM_PROMOTION_CAPACITY: usize = 4;

fn ram_frame_cache_capacity(width: u32, height: u32) -> usize {
    shared_media_cache()
        .map(|cache| cache.effective_ram_capacity(rgba16f_frame_bytes(width, height)))
        .unwrap_or(neo_media_cache::RAM_MIN_CAPACITY)
        .saturating_sub(RAM_FRAME_CACHE_MARGIN)
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
                run_worker(WorkerSpawnRequest {
                    path,
                    clip_key,
                    store,
                    gpu_device,
                    gpu_queue,
                    shared: shared_thread,
                    is_ready: is_ready_thread,
                    last_requested_frame: last_requested_thread,
                    on_ready,
                });
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
) {
    unsafe {
        if hw_device_ctx.is_null() || !(*ctx).hw_frames_ctx.is_null() {
            return;
        }
        let mut frames_ref: *mut sys::AVBufferRef = ptr::null_mut();
        let ret = sys::avcodec_get_hw_frames_parameters(
            ctx,
            hw_device_ctx,
            std::mem::transmute::<i32, sys::AVPixelFormat>(hw_pix_fmt),
            &mut frames_ref,
        );
        if ret < 0 || frames_ref.is_null() {
            eprintln!(
                "[neoutl-video-decoder][診断] avcodec_get_hw_frames_parameters失敗 ret={ret}"
            );
            return;
        }
        let frames_ctx = (*frames_ref).data as *mut sys::AVHWFramesContext;
        (*frames_ctx).initial_pool_size += OWN_CONCURRENT_HW_FRAME_HOLD;
        if sys::av_hwframe_ctx_init(frames_ref) == 0 {
            (*ctx).hw_frames_ctx = sys::av_buffer_ref(frames_ref);
        } else {
            eprintln!("[neoutl-video-decoder][診断] av_hwframe_ctx_init失敗(動的算出プール)");
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
                    try_request_hw_frames_ctx(ctx, hw_box.hw_device_ctx, hw_pix_fmt);
                }
                return *p;
            }
            p = p.add(1);
        }
        *pixfmts
    }
}

struct OpenContext {
    fmt_ctx: *mut sys::AVFormatContext,
    dec_ctx: *mut sys::AVCodecContext,
    stream_index: i32,
    fps: f64,
    width: u32,
    height: u32,
    index: FrameIndex,
    hw_device_ctx: *mut sys::AVBufferRef,
    hw_pix_fmt_box: Option<Box<HwPixFmtBox>>,
    gpu_device: Option<Arc<wgpu::Device>>,
    gpu_queue: Option<Arc<wgpu::Queue>>,
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

unsafe fn config_supports_sw_format(
    hw_device_ctx: *mut sys::AVBufferRef,
    config: *const sys::AVCodecHWConfig,
    stream_sw_format: sys::AVPixelFormat,
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

        let mut constraints = sys::av_hwdevice_get_hwframe_constraints(hw_device_ctx, ptr::null());
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

const HW_DEVICE_TYPE_PRIORITY: &[&str] =
    &["cuda", "vaapi", "d3d11va", "dxva2", "videotoolbox", "qsv"];

fn available_hw_device_types() -> Vec<sys::AVHWDeviceType> {
    let mut found: Vec<sys::AVHWDeviceType> = Vec::new();
    unsafe {
        let mut t = sys::av_hwdevice_iterate_types(sys::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE);
        while t != sys::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE {
            found.push(t);
            t = sys::av_hwdevice_iterate_types(t);
        }
    }
    let mut ordered: Vec<sys::AVHWDeviceType> = Vec::new();
    for name in HW_DEVICE_TYPE_PRIORITY {
        let c_name = CString::new(*name).expect("固定文字列のCString変換失敗");
        let device_type = unsafe { sys::av_hwdevice_find_type_by_name(c_name.as_ptr()) };
        if device_type != sys::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE && found.contains(&device_type)
        {
            ordered.push(device_type);
        }
    }
    for t in found {
        if !ordered.contains(&t) {
            ordered.push(t);
        }
    }
    ordered
}

fn first_drm_render_node() -> Option<CString> {
    let dir = std::fs::read_dir("/dev/dri").ok()?;
    let mut nodes: Vec<String> = dir
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("renderD"))
        .collect();
    nodes.sort();
    let name = nodes.into_iter().next()?;
    CString::new(format!("/dev/dri/{name}")).ok()
}

unsafe fn try_init_hw_device(
    codec: *const sys::AVCodec,
    stream_sw_format: sys::AVPixelFormat,
    gpu_device: &Option<Arc<wgpu::Device>>,
) -> Option<(*mut sys::AVBufferRef, i32)> {
    let _ = gpu_device;
    unsafe {
        let device_types = available_hw_device_types();
        eprintln!("[neoutl-video-decoder][diag] 検出HWデバイスタイプ={device_types:?}");
        eprintln!(
            "[neoutl-video-decoder][diag] env LIBVA_DRIVER_NAME={:?} WAYLAND_DISPLAY={:?} DISPLAY={:?}",
            std::env::var("LIBVA_DRIVER_NAME"),
            std::env::var("WAYLAND_DISPLAY"),
            std::env::var("DISPLAY")
        );
        let vaapi_render_node = first_drm_render_node();
        eprintln!(
            "[neoutl-video-decoder][diag] vaapi_render_node={:?}",
            vaapi_render_node.as_ref().map(|c| c.to_string_lossy())
        );

        for device_type in device_types {
            let mut i = 0;
            loop {
                let config = sys::avcodec_get_hw_config(codec, i);
                if config.is_null() {
                    break;
                }
                let methods = (*config).methods;
                let matches_method = (methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX) != 0;
                if matches_method && (*config).device_type == device_type {
                    let device_arg = if device_type == sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI {
                        vaapi_render_node
                            .as_ref()
                            .map_or(ptr::null(), |c| c.as_ptr())
                    } else {
                        ptr::null()
                    };
                    let mut hw_device_ctx: *mut sys::AVBufferRef = ptr::null_mut();
                    let ret = sys::av_hwdevice_ctx_create(
                        &mut hw_device_ctx,
                        device_type,
                        device_arg,
                        ptr::null_mut(),
                        0,
                    );
                    eprintln!(
                        "[neoutl-video-decoder][diag] av_hwdevice_ctx_create device_type={device_type:?} ret={ret}"
                    );
                    if ret == 0 {
                        let format_ok =
                            config_supports_sw_format(hw_device_ctx, config, stream_sw_format);
                        if format_ok {
                            return Some((
                                hw_device_ctx,
                                std::mem::transmute::<sys::AVPixelFormat, i32>((*config).pix_fmt),
                            ));
                        }
                        sys::av_buffer_unref(&mut hw_device_ctx);
                    }
                }
                i += 1;
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

        let gpu_device_owned = gpu_device.clone();
        let gpu_queue_owned = shared_wgpu_queue();

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
            fps: if fps > 0.0 { fps } else { 30.0 },
            width,
            height,
            index,
            hw_device_ctx,
            hw_pix_fmt_box,
            gpu_device: gpu_device_owned,
            gpu_queue: gpu_queue_owned,
            last_good_frame: None,
        })
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

fn copy_plane(data_ptr: *const u8, stride: i32, height: u32) -> PlaneBuffer {
    let stride = stride.max(0) as u32;
    let byte_len = (stride as usize) * (height as usize);
    let bytes: Arc<[u8]> = unsafe { Arc::from(std::slice::from_raw_parts(data_ptr, byte_len)) };
    PlaneBuffer { bytes, stride }
}

fn copy_plane_half_height(data_ptr: *const u8, stride: i32, luma_height: u32) -> PlaneBuffer {
    copy_plane(data_ptr, stride, luma_height.div_ceil(2))
}

fn convert_frame(ctx: &mut OpenContext, av_frame: *mut sys::AVFrame) -> Option<RamFrame> {
    unsafe {
        let hw_pix_fmt = ctx.hw_pix_fmt_box.as_ref().map(|b| b.pix_fmt);
        let is_hw_frame = hw_pix_fmt.is_some_and(|fmt| (*av_frame).format == fmt);

        let mut owned_sw_frame: *mut sys::AVFrame = ptr::null_mut();
        let src_frame: *mut sys::AVFrame = if is_hw_frame {
            let sw = sys::av_frame_alloc();
            if sw.is_null() || sys::av_hwframe_transfer_data(sw, av_frame, 0) < 0 {
                if !sw.is_null() {
                    sys::av_frame_free(&mut { sw });
                }
                eprintln!("[neoutl-video-decoder] av_hwframe_transfer_data失敗、フレームを破棄");
                return None;
            }
            owned_sw_frame = sw;
            sw
        } else {
            av_frame
        };

        let width = (*src_frame).width.max(0) as u32;
        let height = (*src_frame).height.max(0) as u32;
        if width == 0 || height == 0 {
            if !owned_sw_frame.is_null() {
                sys::av_frame_free(&mut owned_sw_frame);
            }
            return None;
        }
        let mut src_format = (*src_frame).format;
        if src_format == av_pix_fmt_yuvj420p() {
            src_format = av_pix_fmt_yuv420p();
        }

        let (color_matrix, color_range) =
            av_color_meta_to_uniform((*src_frame).colorspace, (*src_frame).color_range);

        let result = if src_format == av_pix_fmt_nv12() {
            let y = copy_plane((*src_frame).data[0], (*src_frame).linesize[0], height);
            let uv = copy_plane_half_height((*src_frame).data[1], (*src_frame).linesize[1], height);
            RamFrame::Nv12 {
                y,
                uv,
                width,
                height,
                color_matrix,
                color_range,
            }
        } else if src_format == av_pix_fmt_p010le() {
            let y = copy_plane((*src_frame).data[0], (*src_frame).linesize[0], height);
            let uv = copy_plane_half_height((*src_frame).data[1], (*src_frame).linesize[1], height);
            RamFrame::P010 {
                y,
                uv,
                width,
                height,
                color_matrix,
                color_range,
            }
        } else if src_format == av_pix_fmt_yuv420p() {
            let y = copy_plane((*src_frame).data[0], (*src_frame).linesize[0], height);
            let chroma_height = height.div_ceil(2);
            let u = copy_plane(
                (*src_frame).data[1],
                (*src_frame).linesize[1],
                chroma_height,
            );
            let v = copy_plane(
                (*src_frame).data[2],
                (*src_frame).linesize[2],
                chroma_height,
            );
            RamFrame::Yuv420p {
                y,
                u,
                v,
                width,
                height,
                color_matrix,
                color_range,
            }
        } else {
            let target_fmt = av_pix_fmt_rgba();
            let sws = sys::sws_getContext(
                width as i32,
                height as i32,
                std::mem::transmute::<i32, sys::AVPixelFormat>(src_format),
                width as i32,
                height as i32,
                std::mem::transmute::<i32, sys::AVPixelFormat>(target_fmt),
                SWS_BILINEAR,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            if sws.is_null() {
                if !owned_sw_frame.is_null() {
                    sys::av_frame_free(&mut owned_sw_frame);
                }
                eprintln!("[neoutl-video-decoder] sws_getContext失敗、フレームを破棄");
                return None;
            }
            let dst = sys::av_frame_alloc();
            (*dst).format = target_fmt;
            (*dst).width = width as i32;
            (*dst).height = height as i32;
            if sys::av_frame_get_buffer(dst, 32) != 0 {
                sys::sws_freeContext(sws);
                sys::av_frame_free(&mut { dst });
                if !owned_sw_frame.is_null() {
                    sys::av_frame_free(&mut owned_sw_frame);
                }
                eprintln!("[neoutl-video-decoder] av_frame_get_buffer失敗、フレームを破棄");
                return None;
            }
            sys::sws_scale(
                sws,
                (*src_frame).data.as_ptr() as *const *const u8,
                (*src_frame).linesize.as_ptr(),
                0,
                height as i32,
                (*dst).data.as_mut_ptr(),
                (*dst).linesize.as_mut_ptr(),
            );
            sys::sws_freeContext(sws);
            let plane = copy_plane((*dst).data[0], (*dst).linesize[0], height);
            let mut dst_mut = dst;
            sys::av_frame_free(&mut dst_mut);
            RamFrame::Rgba8 {
                plane,
                width,
                height,
            }
        };

        if !owned_sw_frame.is_null() {
            sys::av_frame_free(&mut owned_sw_frame);
        }

        Some(result)
    }
}

fn promote_to_vram(
    ram: &RamFrame,
    queue: &wgpu::Queue,
    cache: &Arc<NeoMediaCache>,
    vram_cache: &mut VramPromotionCache,
    frame_index: i64,
) -> Option<VideoFrame> {
    if let Some(frame) = vram_cache.get(frame_index) {
        return Some(frame);
    }

    if let RamFrame::Yuv420p {
        y,
        u,
        v,
        width,
        height,
        color_matrix,
        color_range,
    } = ram
    {
        let Some(device) = shared_wgpu_device() else {
            eprintln!("[neoutl-video-decoder][診断] YUV420P合成失敗: 共有wgpuデバイス未初期化");
            return None;
        };
        let texture = match crate::colorconv::composite_yuv420p_to_rgba(
            &device,
            queue,
            cache,
            &y.bytes,
            y.stride,
            &u.bytes,
            u.stride,
            &v.bytes,
            v.stride,
            *width,
            *height,
            *color_matrix,
            *color_range,
        ) {
            Ok(texture) => texture,
            Err(err) => {
                eprintln!(
                    "[neoutl-video-decoder][診断] YUV420P合成失敗 frame_index={frame_index} err={err}"
                );
                return None;
            }
        };
        let video_frame = VideoFrame(Arc::new(GpuFrame::new(
            texture,
            *width,
            *height,
            ram.color_meta(),
            cache.clone(),
            PixelFormat::Rgba8,
        )));
        vram_cache.put(frame_index, video_frame.clone());
        return Some(video_frame);
    }

    if let RamFrame::P010 {
        y,
        uv,
        width,
        height,
        color_matrix,
        color_range,
    } = ram
    {
        let Some(device) = shared_wgpu_device() else {
            eprintln!("[neoutl-video-decoder][診断] P010合成失敗: 共有wgpuデバイス未初期化");
            return None;
        };
        let texture = match crate::colorconv::composite_p010_to_rgba(
            &device,
            queue,
            cache,
            &y.bytes,
            y.stride,
            &uv.bytes,
            uv.stride,
            *width,
            *height,
            *color_matrix,
            *color_range,
        ) {
            Ok(texture) => texture,
            Err(err) => {
                eprintln!(
                    "[neoutl-video-decoder][診断] P010合成失敗 frame_index={frame_index} err={err}"
                );
                return None;
            }
        };
        let video_frame = VideoFrame(Arc::new(GpuFrame::new(
            texture,
            *width,
            *height,
            ram.color_meta(),
            cache.clone(),
            PixelFormat::Rgba8,
        )));
        vram_cache.put(frame_index, video_frame.clone());
        return Some(video_frame);
    }

    let format = ram.pixel_format();
    let width = ram.width();
    let height = ram.height();

    let texture = match cache.acquire_for_write_as(
        neo_media_cache::KIND_PLAYBACK,
        format,
        width,
        height,
    ) {
        Ok(texture) => texture,
        Err(err) => {
            eprintln!(
                "[neoutl-video-decoder][診断] VRAM層acquire失敗 frame_index={frame_index} err={err:?}"
            );
            return None;
        }
    };

    match ram {
        RamFrame::Nv12 {
            y,
            uv,
            width,
            height,
            ..
        } => {
            let chroma_height = height.div_ceil(2);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::Plane0,
                },
                &y.bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(y.stride),
                    rows_per_image: Some(*height),
                },
                wgpu::Extent3d {
                    width: *width,
                    height: *height,
                    depth_or_array_layers: 1,
                },
            );
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::Plane1,
                },
                &uv.bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(uv.stride),
                    rows_per_image: Some(chroma_height),
                },
                wgpu::Extent3d {
                    width: width.div_ceil(2),
                    height: chroma_height,
                    depth_or_array_layers: 1,
                },
            );
        }
        RamFrame::Rgba8 {
            plane,
            width,
            height,
        } => {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &plane.bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(plane.stride),
                    rows_per_image: Some(*height),
                },
                wgpu::Extent3d {
                    width: *width,
                    height: *height,
                    depth_or_array_layers: 1,
                },
            );
        }
        RamFrame::Yuv420p { .. } => unreachable!("Yuv420pは関数冒頭で早期returnされる"),
        RamFrame::P010 { .. } => unreachable!("P010は関数冒頭で早期returnされる"),
    }

    let submission_index = queue.submit(std::iter::empty());
    cache.mark_ready(format, width, height, &texture, submission_index);

    let video_frame = VideoFrame(Arc::new(GpuFrame::new(
        texture,
        width,
        height,
        ram.color_meta(),
        cache.clone(),
        format,
    )));
    vram_cache.put(frame_index, video_frame.clone());
    Some(video_frame)
}

struct WorkerSpawnRequest<F: FnOnce(VideoMeta) + Send + 'static> {
    path: std::path::PathBuf,
    clip_key: String,
    store: Arc<VideoFrameStore>,
    gpu_device: Option<Arc<wgpu::Device>>,
    gpu_queue: Option<Arc<wgpu::Queue>>,
    shared: Arc<(Mutex<Mailbox>, Condvar)>,
    is_ready: Arc<AtomicBool>,
    last_requested_frame: Arc<AtomicI64>,
    on_ready: F,
}

fn run_worker<F: FnOnce(VideoMeta) + Send + 'static>(req: WorkerSpawnRequest<F>) {
    let WorkerSpawnRequest {
        path,
        clip_key,
        store,
        gpu_device,
        gpu_queue: _gpu_queue,
        shared,
        is_ready,
        last_requested_frame,
        on_ready,
    } = req;
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

    let mut caches = DecodeCaches {
        ram_cache: RamFrameCache::new(ram_frame_cache_capacity(ctx.width, ctx.height)),
        vram_cache: VramPromotionCache::new(VRAM_PROMOTION_CAPACITY),
        gop_cache: GopCache::new(GOP_CACHE_CAPACITY),
        last_decoded_frame: -1,
    };

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
            &mut caches,
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

struct DecodeCaches {
    ram_cache: RamFrameCache,
    vram_cache: VramPromotionCache,
    gop_cache: GopCache,
    last_decoded_frame: i64,
}

fn decode_task(
    ctx: &mut OpenContext,
    requested_target: i64,
    clip_key: &str,
    store: &Arc<VideoFrameStore>,
    caches: &mut DecodeCaches,
    last_requested_frame: &AtomicI64,
) {
    let DecodeCaches {
        ram_cache,
        vram_cache,
        gop_cache,
        last_decoded_frame,
    } = caches;
    if ctx.index.is_empty() {
        return;
    }
    let target = requested_target.clamp(0, ctx.index.len() - 1);
    let media_cache = shared_media_cache();

    if let Some(frame) = vram_cache.get(target) {
        store.set_frame(clip_key, target, frame.clone());
        ctx.last_good_frame = Some(frame);
        eprintln!(
            "[neoutl-video-decoder][診断][decode_task終了][vram_cache即応] requested_target={requested_target} target={target}"
        );
        return;
    }
    if let Some(ram_frame) = gop_cache.get(target, ram_cache) {
        if let (Some(_), Some(queue), Some(cache)) = (
            ctx.gpu_device.as_ref(),
            ctx.gpu_queue.as_ref(),
            media_cache.as_ref(),
        ) {
            if let Some(frame) = promote_to_vram(&ram_frame, queue, cache, vram_cache, target) {
                store.set_frame(clip_key, target, frame.clone());
                ctx.last_good_frame = Some(frame);
                eprintln!(
                    "[neoutl-video-decoder][診断][decode_task終了][gop_cache即応] requested_target={requested_target} target={target}"
                );
                return;
            }
        }
    }
    if let Some(ram_frame) = ram_cache.get(target) {
        if let (Some(_), Some(queue), Some(cache)) = (
            ctx.gpu_device.as_ref(),
            ctx.gpu_queue.as_ref(),
            media_cache.as_ref(),
        ) {
            if let Some(frame) = promote_to_vram(&ram_frame, queue, cache, vram_cache, target) {
                store.set_frame(clip_key, target, frame.clone());
                ctx.last_good_frame = Some(frame);
                eprintln!(
                    "[neoutl-video-decoder][診断][decode_task終了][ram_cache即応] requested_target={requested_target} target={target}"
                );
                return;
            }
        }
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

            if !ram_cache.contains(decoded_index) {
                match convert_frame(ctx, av_frame) {
                    Some(ram_frame) => {
                        ram_cache.insert(decoded_index, ram_frame.clone());
                        new_gop_block.frame_indices.push(decoded_index);

                        if decoded_index == target && !target_dispatched {
                            if let (Some(_), Some(queue), Some(cache)) = (
                                ctx.gpu_device.as_ref(),
                                ctx.gpu_queue.as_ref(),
                                media_cache.as_ref(),
                            ) {
                                if let Some(frame) = promote_to_vram(
                                    &ram_frame,
                                    queue,
                                    cache,
                                    vram_cache,
                                    decoded_index,
                                ) {
                                    ctx.last_good_frame = Some(frame.clone());
                                    store.set_frame(clip_key, decoded_index, frame);
                                    target_dispatched = true;
                                }
                            } else {
                                eprintln!(
                                    "[neoutl-video-decoder][非対応] wgpuデバイス未取得、昇格スキップ"
                                );
                            }
                        }
                    }
                    None => {}
                }
            } else if decoded_index == target && !target_dispatched {
                if let Some(ram_frame) = ram_cache.get(decoded_index) {
                    if let (Some(_), Some(queue), Some(cache)) = (
                        ctx.gpu_device.as_ref(),
                        ctx.gpu_queue.as_ref(),
                        media_cache.as_ref(),
                    ) {
                        if let Some(frame) =
                            promote_to_vram(&ram_frame, queue, cache, vram_cache, decoded_index)
                        {
                            ctx.last_good_frame = Some(frame.clone());
                            store.set_frame(clip_key, decoded_index, frame);
                        }
                    }
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
