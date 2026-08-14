use std::os::raw::c_int;

use ffmpeg_sys_next as sys;
use wgpu_hal as hal;

const AV_PIX_FMT_DRM_PRIME: i32 = 149;
const AV_DRM_MAX_PLANES: usize = 4;

pub struct DrmFrame {
    av_frame: *mut sys::AVFrame,
}

impl Drop for DrmFrame {
    fn drop(&mut self) {
        unsafe {
            sys::av_frame_free(&mut self.av_frame);
        }
    }
}

pub unsafe fn map_vaapi_to_drm(
    src_frame: *mut sys::AVFrame,
    drm_frames_ctx: *mut sys::AVBufferRef,
) -> Result<DrmFrame, String> {
    unsafe {
        let dst_frame = sys::av_frame_alloc();
        if dst_frame.is_null() {
            return Err("av_frame_alloc失敗".to_owned());
        }
        (*dst_frame).format = AV_PIX_FMT_DRM_PRIME;
        (*dst_frame).hw_frames_ctx = sys::av_buffer_ref(drm_frames_ctx);
        if (*dst_frame).hw_frames_ctx.is_null() {
            sys::av_frame_free(&mut { dst_frame });
            return Err("hw_frames_ctx参照確保失敗".to_owned());
        }

        let map_flags = sys::AV_HWFRAME_MAP_READ as i32 as c_int;
        let ret = sys::av_hwframe_map(dst_frame, src_frame, map_flags);
        if ret < 0 {
            sys::av_frame_free(&mut { dst_frame });
            return Err(format!("av_hwframe_map失敗(VAAPI→DRM_PRIME導出) ret={ret}"));
        }

        Ok(DrmFrame {
            av_frame: dst_frame,
        })
    }
}

struct PlaneImportInfo {
    fd: c_int,
    drm_format_modifier: u64,
    plane_layouts: Vec<ash::vk::SubresourceLayout>,
}

unsafe fn extract_plane_import_info(drm_frame: &DrmFrame) -> Result<PlaneImportInfo, String> {
    unsafe {
        let desc = (*drm_frame.av_frame).data[0] as *const sys::AVDRMFrameDescriptor;
        if desc.is_null() {
            return Err("AVDRMFrameDescriptor取得失敗(data[0]がNULL)".to_owned());
        }

        let nb_objects = (*desc).nb_objects as usize;
        if nb_objects == 0 {
            return Err("AVDRMFrameDescriptor.nb_objects=0".to_owned());
        }
        if nb_objects > 1 {
            return Err(format!(
                "未対応: 複数dma-bufオブジェクト(nb_objects={nb_objects})にまたがるフレーム。\
                 texture_from_dma_buf_fdは単一fdのみ対応"
            ));
        }

        let object = (*desc).objects[0];
        let fd = object.fd as c_int;
        let drm_format_modifier = object.format_modifier as u64;

        let nb_layers = (*desc).nb_layers as usize;
        if nb_layers == 0 {
            return Err("AVDRMFrameDescriptor.nb_layers=0".to_owned());
        }

        let mut plane_layouts = Vec::new();
        for layer_idx in 0..nb_layers {
            let layer = (*desc).layers[layer_idx];
            let nb_planes = (layer.nb_planes as usize).min(AV_DRM_MAX_PLANES);
            for plane_idx in 0..nb_planes {
                let plane = layer.planes[plane_idx];
                if plane.object_index != 0 {
                    return Err(format!(
                        "未対応: layer={layer_idx} plane={plane_idx}のobject_index={}が0以外",
                        plane.object_index
                    ));
                }
                plane_layouts.push(
                    ash::vk::SubresourceLayout::default()
                        .offset(plane.offset as u64)
                        .row_pitch(plane.pitch as u64),
                );
            }
        }

        Ok(PlaneImportInfo {
            fd,
            drm_format_modifier,
            plane_layouts,
        })
    }
}

pub unsafe fn import_drm_frame_as_texture(
    drm_frame: &DrmFrame,
    device: &wgpu::Device,
    hal_desc: &hal::TextureDescriptor,
    wgpu_desc: &wgpu::TextureDescriptor,
) -> Result<wgpu::Texture, String> {
    let info = unsafe { extract_plane_import_info(drm_frame) }?;

    let hal_texture = unsafe {
        let hal_device_guard = device
            .as_hal::<hal::api::Vulkan>()
            .ok_or_else(|| "wgpu::DeviceがVulkanバックエンドでない".to_owned())?;
        hal_device_guard
            .texture_from_dma_buf_fd(
                info.fd,
                info.drm_format_modifier,
                &info.plane_layouts,
                hal_desc,
            )
            .map_err(|e| format!("texture_from_dma_buf_fd失敗: {e:?}"))?
    };

    Ok(unsafe {
        device.create_texture_from_hal_borrowed::<hal::api::Vulkan>(hal_texture, wgpu_desc)
    })
}
