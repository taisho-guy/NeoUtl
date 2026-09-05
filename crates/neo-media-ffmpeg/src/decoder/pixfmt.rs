use ffmpeg_sys_next as sys;

pub(crate) fn pf(fmt: sys::AVPixelFormat) -> i32 {
    fmt as i32
}

pub(crate) fn av_pix_fmt_none() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_NONE)
}
pub(crate) fn av_pix_fmt_rgb0() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_RGB0)
}
pub(crate) fn av_pix_fmt_bgr0() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_BGR0)
}
pub(crate) fn av_pix_fmt_rgba() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_RGBA)
}
pub(crate) fn av_pix_fmt_nv12() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_NV12)
}
pub(crate) fn av_pix_fmt_p010le() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_P010LE)
}
pub(crate) fn av_pix_fmt_p012le() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_P012LE)
}
pub(crate) fn av_pix_fmt_p016le() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_P016LE)
}
pub(crate) fn av_pix_fmt_yuv420p10le() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_YUV420P10LE)
}
pub(crate) fn av_pix_fmt_yuv420p12le() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_YUV420P12LE)
}
pub(crate) fn av_pix_fmt_yuv420p() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_YUV420P)
}
pub(crate) fn av_pix_fmt_yuvj420p() -> i32 {
    pf(sys::AVPixelFormat::AV_PIX_FMT_YUVJ420P)
}

pub(crate) fn av_color_meta_to_uniform(
    colorspace: sys::AVColorSpace,
    color_range: sys::AVColorRange,
) -> (u32, u32) {
    let color_matrix = match colorspace {
        sys::AVColorSpace::AVCOL_SPC_BT470BG | sys::AVColorSpace::AVCOL_SPC_SMPTE170M => 0,
        sys::AVColorSpace::AVCOL_SPC_BT709 => 1,
        sys::AVColorSpace::AVCOL_SPC_BT2020_NCL | sys::AVColorSpace::AVCOL_SPC_BT2020_CL => 2,
        sys::AVColorSpace::AVCOL_SPC_RGB | sys::AVColorSpace::AVCOL_SPC_UNSPECIFIED => 1,
        other => {
            eprintln!(
                "[neoutl-video-decoder][診断] 未対応AVColorSpace={other:?} BT709へフォールバック"
            );
            1
        }
    };
    let range = match color_range {
        sys::AVColorRange::AVCOL_RANGE_JPEG => 1,
        sys::AVColorRange::AVCOL_RANGE_MPEG | sys::AVColorRange::AVCOL_RANGE_UNSPECIFIED => 0,
        other => {
            eprintln!(
                "[neoutl-video-decoder][診断] 未対応AVColorRange={other:?} MPEGへフォールバック"
            );
            0
        }
    };
    (color_matrix, range)
}
