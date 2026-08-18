mod vulkan;

use std::sync::{Arc, Mutex};

use ffmpeg_sys_next as sys;
use neo_media_core::{
    ColorPrimaries, DecodedHwFrame, MatrixCoefficients, NeoFrame, NeoFramePool, PixelFormat,
    PoolError, Rect, Size, SourceBackend, TransferBackend, TransferCharacteristics, TransferError,
};

pub use vulkan::{
    CopyEngine, NeoutlVulkanContext, NeoutlVulkanDeviceCtx, SemiPlanarConvertEngine,
    VkSurfaceCache, VulkanRawHandles, create_av_vulkan_device_ctx, extract_vulkan_raw_handles,
    init_vulkan_context, neoutl_vaapi_sync_surface_safe, query_vram_budget_bytes,
};

fn map_pool_error(err: PoolError) -> TransferError {
    match err {
        PoolError::Exhausted => TransferError::PoolExhausted,
        PoolError::UnsupportedFormat(format) => TransferError::UnsupportedFormat(format),
    }
}

pub fn is_sw_format_supported(sw_format_i32: i32, is_direct_rgba: bool) -> bool {
    pixel_format_of_sw_format(sw_format_i32, is_direct_rgba).is_some()
}

fn av_pix_fmt_as_i32(fmt: sys::AVPixelFormat) -> i32 {
    fmt as i32
}

pub fn dst_pixel_format_for(sw_format_i32: i32) -> PixelFormat {
    if semi_planar_view_formats(sw_format_i32).is_some() {
        PixelFormat::Rgba16Float
    } else {
        PixelFormat::Rgba8
    }
}

pub(crate) fn semi_planar_view_formats(
    sw_format_i32: i32,
) -> Option<(ash::vk::Format, ash::vk::Format)> {
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
    handles: VulkanRawHandles,
    vulkan_ctx: Arc<NeoutlVulkanContext>,
    semi_planar_engine: Option<SemiPlanarConvertEngine>,
    surface_cache: VkSurfaceCache,
}

unsafe impl Send for VaapiTransferBackend {}

impl VaapiTransferBackend {
    pub fn new(wgpu_device: &wgpu::Device, submit_lock: Arc<Mutex<()>>) -> Result<Self, String> {
        let entry =
            unsafe { ash::Entry::load() }.map_err(|e| format!("ash::Entry::load失敗: {e}"))?;
        let vulkan_ctx = init_vulkan_context(wgpu_device, &entry, submit_lock.clone())?;
        let handles = unsafe { extract_vulkan_raw_handles(wgpu_device) }
            .ok_or_else(|| "Vulkan生ハンドル取得失敗(semi_planar用)".to_owned())?;
        let semi_planar_engine = unsafe {
            SemiPlanarConvertEngine::new(&handles, &entry, SEMI_PLANAR_SPIRV, submit_lock)
        }
        .ok();
        Ok(Self {
            entry,
            handles,
            vulkan_ctx,
            semi_planar_engine,
            surface_cache: VkSurfaceCache::new(),
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

        let sync_ret = unsafe { neoutl_vaapi_sync_surface_safe(input.av_frame) };
        if sync_ret != 0 {
            return Err(TransferError::SyncFailed(format!(
                "neoutl_vaapi_sync_surface失敗 ret={sync_ret}"
            )));
        }

        let (src_image, src_layout) = unsafe {
            self.surface_cache.get_or_import(
                &self.handles,
                &self.vulkan_ctx.instance,
                &self.vulkan_ctx.device,
                input.av_frame,
                input.sw_format_i32,
                input.is_direct_rgba,
            )
        }
        .map_err(TransferError::SyncFailed)?;

        let width = input.visible_rect.width;
        let height = input.visible_rect.height;

        let dst_pixel_format = dst_pixel_format_for(input.sw_format_i32);
        let dst_vk_format = match dst_pixel_format {
            PixelFormat::Rgba16Float => ash::vk::Format::R16G16B16A16_SFLOAT,
            _ => ash::vk::Format::R8G8B8A8_UNORM,
        };

        let target_texture = pool
            .acquire(dst_pixel_format, width, height)
            .map_err(map_pool_error)?;

        macro_rules! release_and_return {
            ($err:expr) => {{
                pool.release(target_texture);
                return Err($err);
            }};
        }

        let dst_vk_image = unsafe {
            target_texture
                .as_hal::<wgpu_hal::api::Vulkan>()
                .map(|hal_texture| hal_texture.raw_handle())
        };
        let Some(dst_vk_image) = dst_vk_image else {
            release_and_return!(TransferError::CopyFailed(
                "wgpuテクスチャからVkImageハンドル取得失敗".to_owned(),
            ));
        };

        let new_layout_result = if let Some((y_format, uv_format)) = view_formats {
            let engine = match self.semi_planar_engine.as_ref() {
                Some(engine) => engine,
                None => release_and_return!(TransferError::SyncFailed(
                    "セミプラナー変換エンジン未初期化".to_owned()
                )),
            };
            unsafe {
                engine.convert(
                    src_image,
                    src_layout,
                    dst_vk_image,
                    width,
                    height,
                    y_format,
                    uv_format,
                    dst_vk_format,
                )
            }
        } else {
            unsafe {
                self.vulkan_ctx.copy_engine.copy_image(
                    src_image,
                    src_layout,
                    dst_vk_image,
                    width,
                    height,
                )
            }
        };
        let new_layout = match new_layout_result {
            Ok(layout) => layout,
            Err(e) => release_and_return!(TransferError::CopyFailed(e)),
        };
        self.surface_cache.update_layout(input.av_frame, new_layout);
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
