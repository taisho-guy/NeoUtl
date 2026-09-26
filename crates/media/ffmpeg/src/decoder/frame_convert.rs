use std::ptr;
use std::sync::Arc;

use ffmpeg_sys_next as sys;

use neo_media_cache::NeoMediaCache;
use neo_media_core::PixelFormat;

use crate::colorconv::P0xxGpuResources;
use crate::frame::{ChannelOrder, GpuFrame, PlaneBuffer, RamFrame, VideoFrame};

use super::open::OpenContext;
use super::pixfmt::{
    av_color_meta_to_uniform, av_pix_fmt_bgr0, av_pix_fmt_nv12, av_pix_fmt_p010le,
    av_pix_fmt_p012le, av_pix_fmt_p016le, av_pix_fmt_rgb0, av_pix_fmt_rgba, av_pix_fmt_yuv420p,
    av_pix_fmt_yuvj420p,
};
use super::{shared_staging_pool, shared_wgpu_device};

const RGBA8_BYTES_PER_PIXEL: u32 = 4;

pub(crate) fn copy_plane(data_ptr: *const u8, stride: i32, height: u32) -> PlaneBuffer {
    let stride = stride.max(0) as u32;
    let byte_len = (stride as usize) * (height as usize);
    let bytes: Arc<[u8]> = unsafe { Arc::from(std::slice::from_raw_parts(data_ptr, byte_len)) };
    PlaneBuffer { bytes, stride }
}

pub(crate) fn copy_plane_half_height(
    data_ptr: *const u8,
    stride: i32,
    luma_height: u32,
) -> PlaneBuffer {
    copy_plane(data_ptr, stride, luma_height.div_ceil(2))
}

pub(crate) fn convert_frame(
    ctx: &mut OpenContext,
    av_frame: *mut sys::AVFrame,
) -> Option<RamFrame> {
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
            RamFrame::P0xx {
                y,
                uv,
                width,
                height,
                bit_depth: 8,
                color_matrix,
                color_range,
            }
        } else if src_format == av_pix_fmt_p010le()
            || src_format == av_pix_fmt_p012le()
            || src_format == av_pix_fmt_p016le()
        {
            let bit_depth: u32 = if src_format == av_pix_fmt_p010le() {
                10
            } else if src_format == av_pix_fmt_p012le() {
                12
            } else {
                16
            };
            let y = copy_plane((*src_frame).data[0], (*src_frame).linesize[0], height);
            let uv = copy_plane_half_height((*src_frame).data[1], (*src_frame).linesize[1], height);
            RamFrame::P0xx {
                y,
                uv,
                width,
                height,
                bit_depth,
                color_matrix,
                color_range,
            }
        } else if src_format == av_pix_fmt_yuv420p() {
            let chroma_height = height.div_ceil(2);
            let y = copy_plane((*src_frame).data[0], (*src_frame).linesize[0], height);
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
        } else if src_format == av_pix_fmt_rgba() || src_format == av_pix_fmt_rgb0() {
            let plane = copy_plane((*src_frame).data[0], (*src_frame).linesize[0], height);
            RamFrame::Rgba8 {
                plane,
                width,
                height,
                channel_order: ChannelOrder::Rgba,
            }
        } else if src_format == av_pix_fmt_bgr0() {
            let plane = copy_plane((*src_frame).data[0], (*src_frame).linesize[0], height);
            RamFrame::Rgba8 {
                plane,
                width,
                height,
                channel_order: ChannelOrder::Bgra,
            }
        } else {
            eprintln!(
                "[neoutl-video-decoder][診断] 未対応pixel_format={src_format} CPUフォールバック廃止のため破棄"
            );
            if !owned_sw_frame.is_null() {
                sys::av_frame_free(&mut owned_sw_frame);
            }
            return None;
        };

        if !owned_sw_frame.is_null() {
            sys::av_frame_free(&mut owned_sw_frame);
        }

        Some(result)
    }
}

pub(crate) fn compose_output_frame(
    ram: &RamFrame,
    queue: &wgpu::Queue,
    cache: &Arc<NeoMediaCache>,
    p0xx_resources: &mut Option<P0xxGpuResources>,
) -> Option<VideoFrame> {
    match ram {
        RamFrame::P0xx {
            y,
            uv,
            width,
            height,
            bit_depth,
            color_matrix,
            color_range,
        } => {
            let Some(device) = shared_wgpu_device() else {
                eprintln!("[neoutl-video-decoder][診断] P0xx合成失敗: 共有wgpuデバイス未初期化");
                return None;
            };
            let Some(pool) = shared_staging_pool() else {
                eprintln!("[neoutl-video-decoder][診断] P0xx合成失敗: StagingPool未初期化");
                return None;
            };
            let resources = p0xx_resources.get_or_insert_with(|| P0xxGpuResources::new(&device));
            let texture = match crate::colorconv::composite_p0xx_to_rgba(
                resources,
                &device,
                queue,
                &pool,
                cache,
                crate::colorconv::P0xxInput {
                    y: &y.bytes,
                    y_stride: y.stride,
                    uv: &uv.bytes,
                    uv_stride: uv.stride,
                    width: *width,
                    height: *height,
                    bit_depth: *bit_depth,
                    color_matrix: *color_matrix,
                    color_range: *color_range,
                },
            ) {
                Ok(texture) => texture,
                Err(err) => {
                    eprintln!("[neoutl-video-decoder][診断] P0xx合成失敗 err={err}");
                    return None;
                }
            };
            Some(VideoFrame(Arc::new(GpuFrame::new(
                texture,
                *width,
                *height,
                ram.color_meta(),
                cache.clone(),
                PixelFormat::Rgba8,
            ))))
        }
        RamFrame::Yuv420p {
            y,
            u,
            v,
            width,
            height,
            color_matrix,
            color_range,
        } => {
            let Some(device) = shared_wgpu_device() else {
                eprintln!("[neoutl-video-decoder][診断] Yuv420p合成失敗: 共有wgpuデバイス未初期化");
                return None;
            };
            let Some(pool) = shared_staging_pool() else {
                eprintln!("[neoutl-video-decoder][診断] Yuv420p合成失敗: StagingPool未初期化");
                return None;
            };
            let resources = p0xx_resources.get_or_insert_with(|| P0xxGpuResources::new(&device));
            let texture = match crate::colorconv::composite_yuv420p_to_rgba(
                resources,
                &device,
                queue,
                &pool,
                cache,
                crate::colorconv::Yuv420Input {
                    y: &y.bytes,
                    y_stride: y.stride,
                    u: &u.bytes,
                    u_stride: u.stride,
                    v: &v.bytes,
                    v_stride: v.stride,
                    width: *width,
                    height: *height,
                    color_matrix: *color_matrix,
                    color_range: *color_range,
                },
            ) {
                Ok(texture) => texture,
                Err(err) => {
                    eprintln!("[neoutl-video-decoder][診断] Yuv420p合成失敗 err={err}");
                    return None;
                }
            };
            Some(VideoFrame(Arc::new(GpuFrame::new(
                texture,
                *width,
                *height,
                ram.color_meta(),
                cache.clone(),
                PixelFormat::Rgba8,
            ))))
        }
        RamFrame::Rgba8 {
            plane,
            width,
            height,
            channel_order,
        } => {
            let Some(pool) = shared_staging_pool() else {
                eprintln!("[neoutl-video-decoder][診断] Rgba8転送失敗: StagingPool未初期化");
                return None;
            };
            let texture = match cache.acquire_for_write_as(
                neo_media_cache::KIND_PLAYBACK,
                PixelFormat::Rgba8,
                *width,
                *height,
            ) {
                Ok(texture) => texture,
                Err(err) => {
                    eprintln!("[neoutl-video-decoder][診断] VRAM acquire失敗 err={err:?}");
                    return None;
                }
            };
            let upload_bytes: std::borrow::Cow<[u8]> = match channel_order {
                ChannelOrder::Rgba => std::borrow::Cow::Borrowed(&plane.bytes),
                ChannelOrder::Bgra => {
                    let mut swapped = plane.bytes.to_vec();
                    for px in swapped.as_chunks_mut::<4>().0 {
                        px.swap(0, 2);
                    }
                    std::borrow::Cow::Owned(swapped)
                }
            };
            let submission_index = pool.upload_plane(
                queue,
                &texture,
                *width,
                *height,
                *width * RGBA8_BYTES_PER_PIXEL,
                upload_bytes.as_ref(),
                plane.stride,
            );
            cache.mark_ready(
                PixelFormat::Rgba8,
                *width,
                *height,
                &texture,
                submission_index,
            );
            Some(VideoFrame(Arc::new(GpuFrame::new(
                texture,
                *width,
                *height,
                ram.color_meta(),
                cache.clone(),
                PixelFormat::Rgba8,
            ))))
        }
    }
}
