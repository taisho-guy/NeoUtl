use std::ffi::CString;
use std::path::Path;
use std::ptr;

use ffmpeg_sys_next as sys;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeFailure {
    ContainerOpenFailed,
    NoVideoTrack,
    CodecUnsupported,
    DeviceUnavailable,
    HwFramesCtxUnsupported,
    NeoFrameConversionUnsupported,
}

impl ProbeFailure {
    pub fn message(self) -> &'static str {
        match self {
            Self::ContainerOpenFailed => "ファイルを開けません(コンテナ形式非対応または破損)",
            Self::NoVideoTrack => "映像トラックが見つかりません",
            Self::CodecUnsupported => "非対応のコーデックです",
            Self::DeviceUnavailable => "GPUデコードデバイスが利用できません",
            Self::HwFramesCtxUnsupported => {
                "この映像形式はGPUデコード非対応です(色形式/ビット深度)"
            }
            Self::NeoFrameConversionUnsupported => "GPUフレーム変換が非対応の形式です",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProbeReport {
    pub width: u32,
    pub height: u32,
    pub codec_name: String,
    pub sw_format_i32: i32,
}

fn pf(fmt: sys::AVPixelFormat) -> i32 {
    fmt as i32
}

fn resolve_hw_sw_format(stream_sw_format: i32) -> Option<i32> {
    let nv12 = pf(sys::AVPixelFormat::AV_PIX_FMT_NV12);
    let yuv420p = pf(sys::AVPixelFormat::AV_PIX_FMT_YUV420P);
    let yuvj420p = pf(sys::AVPixelFormat::AV_PIX_FMT_YUVJ420P);
    let yuv420p10le = pf(sys::AVPixelFormat::AV_PIX_FMT_YUV420P10LE);
    let yuv420p12le = pf(sys::AVPixelFormat::AV_PIX_FMT_YUV420P12LE);
    let p010le = pf(sys::AVPixelFormat::AV_PIX_FMT_P010LE);
    let p012le = pf(sys::AVPixelFormat::AV_PIX_FMT_P012LE);
    let p016le = pf(sys::AVPixelFormat::AV_PIX_FMT_P016LE);
    let rgb0 = pf(sys::AVPixelFormat::AV_PIX_FMT_RGB0);
    let bgr0 = pf(sys::AVPixelFormat::AV_PIX_FMT_BGR0);

    if stream_sw_format == nv12 || stream_sw_format == yuv420p || stream_sw_format == yuvj420p {
        Some(p010le)
    } else if stream_sw_format == yuv420p10le {
        Some(p010le)
    } else if stream_sw_format == yuv420p12le {
        Some(p012le)
    } else if stream_sw_format == p010le || stream_sw_format == p012le || stream_sw_format == p016le
    {
        Some(stream_sw_format)
    } else if stream_sw_format == rgb0 || stream_sw_format == bgr0 {
        Some(stream_sw_format)
    } else {
        None
    }
}

pub fn probe(path: &Path) -> Result<ProbeReport, ProbeFailure> {
    unsafe {
        let mut fmt_ctx: *mut sys::AVFormatContext = ptr::null_mut();
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| ProbeFailure::ContainerOpenFailed)?;
        if sys::avformat_open_input(
            &mut fmt_ctx,
            c_path.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
        ) != 0
        {
            return Err(ProbeFailure::ContainerOpenFailed);
        }
        if sys::avformat_find_stream_info(fmt_ctx, ptr::null_mut()) < 0 {
            sys::avformat_close_input(&mut fmt_ctx);
            return Err(ProbeFailure::ContainerOpenFailed);
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
            return Err(ProbeFailure::NoVideoTrack);
        }
        let stream = *(*fmt_ctx).streams.add(stream_index as usize);
        let codecpar = (*stream).codecpar;
        let width = (*codecpar).width.max(0) as u32;
        let height = (*codecpar).height.max(0) as u32;
        let stream_sw_format = std::mem::transmute::<i32, sys::AVPixelFormat>((*codecpar).format);

        let codec = sys::avcodec_find_decoder((*codecpar).codec_id);
        if codec.is_null() {
            sys::avformat_close_input(&mut fmt_ctx);
            return Err(ProbeFailure::CodecUnsupported);
        }
        let codec_name = std::ffi::CStr::from_ptr((*codec).name)
            .to_string_lossy()
            .into_owned();

        let mut hw_device_ctx: *mut sys::AVBufferRef = ptr::null_mut();
        let ret = sys::av_hwdevice_ctx_create(
            &mut hw_device_ctx,
            sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            ptr::null(),
            ptr::null_mut(),
            0,
        );
        if ret < 0 || hw_device_ctx.is_null() {
            sys::avformat_close_input(&mut fmt_ctx);
            return Err(ProbeFailure::DeviceUnavailable);
        }

        let Some(sw_format) = resolve_hw_sw_format(std::mem::transmute::<sys::AVPixelFormat, i32>(
            stream_sw_format,
        )) else {
            sys::av_buffer_unref(&mut hw_device_ctx);
            sys::avformat_close_input(&mut fmt_ctx);
            return Err(ProbeFailure::HwFramesCtxUnsupported);
        };
        let frames_ref = sys::av_hwframe_ctx_alloc(hw_device_ctx);
        if frames_ref.is_null() {
            sys::av_buffer_unref(&mut hw_device_ctx);
            sys::avformat_close_input(&mut fmt_ctx);
            return Err(ProbeFailure::HwFramesCtxUnsupported);
        }
        let frames_ctx = (*frames_ref).data as *mut sys::AVHWFramesContext;
        (*frames_ctx).format = sys::AVPixelFormat::AV_PIX_FMT_VAAPI;
        (*frames_ctx).sw_format = std::mem::transmute::<i32, sys::AVPixelFormat>(sw_format);
        (*frames_ctx).width = width as i32;
        (*frames_ctx).height = height as i32;
        (*frames_ctx).initial_pool_size = 4;
        let init_ret = sys::av_hwframe_ctx_init(frames_ref);
        let mut frames_ref_mut = frames_ref;
        sys::av_buffer_unref(&mut frames_ref_mut);
        sys::av_buffer_unref(&mut hw_device_ctx);
        sys::avformat_close_input(&mut fmt_ctx);
        if init_ret != 0 {
            return Err(ProbeFailure::HwFramesCtxUnsupported);
        }

        let sw_format_i32 = std::mem::transmute::<sys::AVPixelFormat, i32>(stream_sw_format);

        Ok(ProbeReport {
            width,
            height,
            codec_name,
            sw_format_i32,
        })
    }
}
