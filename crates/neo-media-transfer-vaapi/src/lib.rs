mod vulkan;

use std::ptr;
use std::sync::Arc;

use ffmpeg_sys_next as sys;
use neo_media_core::{
    ColorPrimaries, DecodedHwFrame, MatrixCoefficients, NeoFrame, NeoFramePool, PixelFormat, Rect,
    Size, SourceBackend, TransferBackend, TransferCharacteristics, TransferError,
};

pub use vulkan::{
    CopyEngine, DerivedVulkanFrame, NeoutlVulkanContext, NeoutlVulkanDeviceCtx,
    SemiPlanarConvertEngine, VkImageHandle, VulkanRawHandles, create_av_vulkan_device_ctx,
    create_derived_vulkan_frames_ctx, extract_vulkan_raw_handles, init_vulkan_context,
    transfer_to_vulkan_frame, vk_image_of,
};

pub fn is_sw_format_supported(sw_format_i32: i32, is_direct_rgba: bool) -> bool {
    pixel_format_of_sw_format(sw_format_i32, is_direct_rgba).is_some()
}

fn av_pix_fmt_as_i32(fmt: sys::AVPixelFormat) -> i32 {
    fmt as i32
}

fn semi_planar_view_formats(sw_format_i32: i32) -> Option<(ash::vk::Format, ash::vk::Format)> {
    if sw_format_i32 == av_pix_fmt_as_i32(sys::AVPixelFormat::AV_PIX_FMT_NV12) {
        Some((ash::vk::Format::R8_UNORM, ash::vk::Format::R8G8_UNORM))
    } else if sw_format_i32 == av_pix_fmt_as_i32(sys::AVPixelFormat::AV_PIX_FMT_P010LE)
        || sw_format_i32 == av_pix_fmt_as_i32(sys::AVPixelFormat::AV_PIX_FMT_P012LE)
        || sw_format_i32 == av_pix_fmt_as_i32(sys::AVPixelFormat::AV_PIX_FMT_P016LE)
    {
        Some((ash::vk::Format::R16_UNORM, ash::vk::Format::R16G16_UNORM))
    } else {
        None
    }
}

fn pixel_format_of_sw_format(sw_format_i32: i32, is_direct_rgba: bool) -> Option<PixelFormat> {
    if is_direct_rgba {
        return Some(PixelFormat::Rgba8);
    }
    if sw_format_i32 == av_pix_fmt_as_i32(sys::AVPixelFormat::AV_PIX_FMT_NV12) {
        Some(PixelFormat::Nv12)
    } else if sw_format_i32 == av_pix_fmt_as_i32(sys::AVPixelFormat::AV_PIX_FMT_P010LE) {
        Some(PixelFormat::P010)
    } else if sw_format_i32 == av_pix_fmt_as_i32(sys::AVPixelFormat::AV_PIX_FMT_P012LE) {
        Some(PixelFormat::P012)
    } else if sw_format_i32 == av_pix_fmt_as_i32(sys::AVPixelFormat::AV_PIX_FMT_P016LE) {
        Some(PixelFormat::P016)
    } else {
        None
    }
}

pub struct VaapiDecodedFrame {
    pub av_frame: *mut sys::AVFrame,
    pub src_hw_frames_ctx: *mut sys::AVBufferRef,
    pub sw_format_i32: i32,
    pub is_direct_rgba: bool,
    pub coded_size: Size,
    pub visible_rect: Rect,
    pub color_primaries: ColorPrimaries,
    pub transfer_characteristics: TransferCharacteristics,
    pub matrix_coefficients: MatrixCoefficients,
    pub full_range: bool,
    pub pts: i64,
    pub duration: i64,
    pub progressive: bool,
}

unsafe impl Send for VaapiDecodedFrame {}

impl DecodedHwFrame for VaapiDecodedFrame {
    fn pixel_format(&self) -> PixelFormat {
        pixel_format_of_sw_format(self.sw_format_i32, self.is_direct_rgba)
            .unwrap_or(PixelFormat::Rgba8)
    }

    fn coded_size(&self) -> Size {
        self.coded_size
    }

    fn visible_rect(&self) -> Rect {
        self.visible_rect
    }

    fn pts(&self) -> i64 {
        self.pts
    }

    fn duration(&self) -> i64 {
        self.duration
    }

    fn progressive(&self) -> bool {
        self.progressive
    }
}

static SEMI_PLANAR_SPIRV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/semi_planar_to_rgba.spv"));

pub struct VaapiTransferBackend {
    entry: ash::Entry,
    vulkan_ctx: Arc<NeoutlVulkanContext>,
    semi_planar_engine: Option<SemiPlanarConvertEngine>,
    derived_frames_ctx: *mut sys::AVBufferRef,
}

unsafe impl Send for VaapiTransferBackend {}

impl Drop for VaapiTransferBackend {
    fn drop(&mut self) {
        unsafe {
            if !self.derived_frames_ctx.is_null() {
                sys::av_buffer_unref(&mut self.derived_frames_ctx);
            }
        }
    }
}

impl VaapiTransferBackend {
    pub fn new(wgpu_device: &wgpu::Device) -> Result<Self, String> {
        let entry =
            unsafe { ash::Entry::load() }.map_err(|e| format!("ash::Entry::load失敗: {e}"))?;
        let vulkan_ctx = init_vulkan_context(wgpu_device, &entry)?;
        let handles = unsafe { extract_vulkan_raw_handles(wgpu_device) }
            .ok_or_else(|| "Vulkan生ハンドル取得失敗(semi_planar用)".to_owned())?;
        let semi_planar_engine =
            unsafe { SemiPlanarConvertEngine::new(&handles, &entry, SEMI_PLANAR_SPIRV) }.ok();
        Ok(Self {
            entry,
            vulkan_ctx,
            semi_planar_engine,
            derived_frames_ctx: ptr::null_mut(),
        })
    }
}

impl TransferBackend for VaapiTransferBackend {
    type Input = VaapiDecodedFrame;

    fn source_backend(&self) -> SourceBackend {
        SourceBackend::Vaapi
    }

    fn transfer(
        &mut self,
        input: &Self::Input,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        pool: &dyn NeoFramePool,
    ) -> Result<NeoFrame, TransferError> {
        let view_formats = semi_planar_view_formats(input.sw_format_i32);
        if !input.is_direct_rgba && view_formats.is_none() {
            return Err(TransferError::UnsupportedFormat(
                pixel_format_of_sw_format(input.sw_format_i32, false).unwrap_or(PixelFormat::Rgba8),
            ));
        }
        if view_formats.is_some() && self.semi_planar_engine.is_none() {
            return Err(TransferError::SyncFailed(
                "セミプラナー変換エンジン未初期化".to_owned(),
            ));
        }

        if self.derived_frames_ctx.is_null() {
            self.derived_frames_ctx = create_derived_vulkan_frames_ctx(
                input.src_hw_frames_ctx,
                &self.vulkan_ctx.device_ctx,
            )
            .map_err(TransferError::SyncFailed)?;
        }

        let derived = transfer_to_vulkan_frame(input.av_frame, self.derived_frames_ctx)
            .map_err(TransferError::SyncFailed)?;
        unsafe {
            self.vulkan_ctx
                .copy_engine
                .device_wait_idle()
                .map_err(TransferError::SyncFailed)?;
        }
        let src_image = unsafe { vk_image_of(&derived) };

        let width = input.visible_rect.width;
        let height = input.visible_rect.height;

        let target_texture = pool
            .acquire(PixelFormat::Rgba8, width, height)
            .map_err(|_| TransferError::PoolExhausted)?;

        let dst_vk_image = unsafe {
            target_texture
                .as_hal::<wgpu_hal::api::Vulkan>()
                .map(|hal_texture| hal_texture.raw_handle())
        };
        let Some(dst_vk_image) = dst_vk_image else {
            return Err(TransferError::CopyFailed(
                "wgpuテクスチャからVkImageハンドル取得失敗".to_owned(),
            ));
        };

        let convert_result = if let Some((y_format, uv_format)) = view_formats {
            let engine = self.semi_planar_engine.as_ref().ok_or_else(|| {
                TransferError::SyncFailed("セミプラナー変換エンジン未初期化".to_owned())
            })?;
            unsafe {
                engine.convert(
                    src_image.image,
                    src_image.layout,
                    dst_vk_image,
                    width,
                    height,
                    y_format,
                    uv_format,
                )
            }
        } else {
            unsafe {
                self.vulkan_ctx
                    .copy_engine
                    .copy_image(src_image, dst_vk_image, width, height)
            }
        };
        convert_result.map_err(TransferError::CopyFailed)?;
        let _ = &self.entry;

        Ok(NeoFrame {
            texture: target_texture,
            width,
            height,
            coded_size: input.coded_size,
            visible_rect: input.visible_rect,
            color_primaries: input.color_primaries,
            transfer_characteristics: input.transfer_characteristics,
            matrix_coefficients: input.matrix_coefficients,
            full_range: input.full_range,
            chroma_siting: neo_media_core::ChromaSiting::Unknown,
            pts: input.pts,
            duration: input.duration,
            progressive: input.progressive,
            source_backend: SourceBackend::Vaapi,
        })
    }
}
