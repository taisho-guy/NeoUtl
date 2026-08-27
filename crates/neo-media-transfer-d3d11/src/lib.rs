mod device;
mod fence;
mod surface_cache;
mod vulkan_convert;

use ffmpeg_sys_next as sys;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_NV12, DXGI_FORMAT_P010, DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R8G8_UNORM,
    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R16_UNORM, DXGI_FORMAT_R16G16_UNORM,
    DXGI_FORMAT_R16G16B16A16_FLOAT,
};

use neo_media_core::{
    ColorPrimaries, DecodedHwFrame, MatrixCoefficients, NeoFrame, NeoFramePool, PixelFormat,
    PoolError, Rect, Size, SourceBackend, TransferBackend, TransferCharacteristics, TransferError,
};

pub use device::{NeoutlD3d11DeviceCtx, create_av_d3d11va_device_ctx, create_d3d11_device_on_luid};
pub use vulkan_convert::{
    ColorTags, ConvertRequest, SemiPlanarConvertEngine, VulkanRawHandles,
    extract_vulkan_raw_handles,
};

pub struct D3d11DecodedFrame {
    pub av_frame: *mut sys::AVFrame,
    pub d3d11_texture: ID3D11Texture2D,
    pub subresource_index: u32,
    pub sw_format_i32: i32,
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

unsafe impl Send for D3d11DecodedFrame {}

fn av_pix_fmt_as_i32(fmt: sys::AVPixelFormat) -> i32 {
    fmt as i32
}

fn plane_view_formats(
    sw_format_i32: i32,
) -> Option<(DXGI_FORMAT, DXGI_FORMAT, DXGI_FORMAT, PixelFormat)> {
    if sw_format_i32 == av_pix_fmt_as_i32(sys::AVPixelFormat::AV_PIX_FMT_NV12) {
        Some((
            DXGI_FORMAT_R8_UNORM,
            DXGI_FORMAT_R8G8_UNORM,
            DXGI_FORMAT_NV12,
            PixelFormat::Nv12,
        ))
    } else if sw_format_i32 == av_pix_fmt_as_i32(sys::AVPixelFormat::AV_PIX_FMT_P010LE) {
        Some((
            DXGI_FORMAT_R16_UNORM,
            DXGI_FORMAT_R16G16_UNORM,
            DXGI_FORMAT_P010,
            PixelFormat::P010,
        ))
    } else {
        None
    }
}

fn dxgi_plane_format_to_vk(format: DXGI_FORMAT) -> ash::vk::Format {
    match format {
        DXGI_FORMAT_R8_UNORM => ash::vk::Format::R8_UNORM,
        DXGI_FORMAT_R8G8_UNORM => ash::vk::Format::R8G8_UNORM,
        DXGI_FORMAT_R16_UNORM => ash::vk::Format::R16_UNORM,
        DXGI_FORMAT_R16G16_UNORM => ash::vk::Format::R16G16_UNORM,
        _ => unreachable!("plane_view_formatsが返す値はここに列挙した4種のみ"),
    }
}

pub unsafe fn device_from_raw(
    raw: *mut sys::ID3D11Device,
) -> windows::Win32::Graphics::Direct3D11::ID3D11Device {
    unsafe {
        let raw = raw as *mut core::ffi::c_void;
        windows::core::Interface::from_raw_borrowed(&raw)
            .map(|r: &windows::Win32::Graphics::Direct3D11::ID3D11Device| r.clone())
            .expect("AVD3D11VADeviceContext.deviceがnull")
    }
}

pub unsafe fn texture_from_av_frame(
    av_frame: *mut sys::AVFrame,
) -> Result<ID3D11Texture2D, String> {
    unsafe {
        let raw = (*av_frame).data[0] as *mut sys::ID3D11Texture2D;
        if raw.is_null() {
            return Err("AVFrame.data[0]がnull".to_owned());
        }
        let raw = raw as *mut core::ffi::c_void;
        windows::core::Interface::from_raw_borrowed(&raw)
            .map(|r: &ID3D11Texture2D| r.clone())
            .ok_or_else(|| "ID3D11Texture2D復元失敗".to_owned())
    }
}

pub fn dx12_adapter_luid(
    wgpu_device: &wgpu::Device,
) -> Result<windows::Win32::Foundation::LUID, String> {
    let luid_bytes = vulkan_convert::dx12_adapter_luid(wgpu_device)
        .ok_or_else(|| "VkPhysicalDeviceIDProperties.deviceLUID未取得".to_owned())?;
    Ok(windows::Win32::Foundation::LUID {
        LowPart: u32::from_ne_bytes(luid_bytes[0..4].try_into().unwrap()),
        HighPart: i32::from_ne_bytes(luid_bytes[4..8].try_into().unwrap()),
    })
}

pub fn query_vram_budget_bytes(wgpu_device: &wgpu::Device) -> Option<u64> {
    vulkan_convert::query_vram_budget_bytes(wgpu_device)
}

pub fn is_sw_format_supported(sw_format_i32: i32) -> bool {
    plane_view_formats(sw_format_i32).is_some()
}

pub fn dst_pixel_format_for(sw_format_i32: i32) -> PixelFormat {
    match plane_view_formats(sw_format_i32) {
        Some((_, _, _, PixelFormat::Nv12)) => PixelFormat::Rgba8,
        _ => PixelFormat::Rgba16Float,
    }
}

fn color_tags_of(input: &D3d11DecodedFrame) -> ColorTags {
    ColorTags {
        matrix_coefficients: match input.matrix_coefficients {
            MatrixCoefficients::Bt2020Ncl => 1,
            MatrixCoefficients::Smpte170m => 2,
            MatrixCoefficients::Bt709 | MatrixCoefficients::Unknown => 0,
        },
        transfer_characteristics: match input.transfer_characteristics {
            TransferCharacteristics::Smpte2084 => 1,
            TransferCharacteristics::AribStdB67 => 2,
            TransferCharacteristics::Bt709 | TransferCharacteristics::Unknown => 0,
        },
        color_primaries: match input.color_primaries {
            ColorPrimaries::Bt2020 => 1,
            ColorPrimaries::Bt709 | ColorPrimaries::Smpte170m | ColorPrimaries::Unknown => 0,
        },
        full_range: input.full_range as u32,
    }
}

fn map_pool_error(err: PoolError) -> TransferError {
    match err {
        PoolError::Exhausted => TransferError::PoolExhausted,
        PoolError::UnsupportedFormat(format) => TransferError::UnsupportedFormat(format),
    }
}

impl DecodedHwFrame for D3d11DecodedFrame {
    fn pixel_format(&self) -> PixelFormat {
        plane_view_formats(self.sw_format_i32)
            .map(|(_, _, _, fmt)| fmt)
            .unwrap_or(PixelFormat::Nv12)
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

pub struct D3d11TransferBackend {
    handles: VulkanRawHandles,
    engine: SemiPlanarConvertEngine,
    surface_cache_nv12: Option<surface_cache::SurfaceCache>,
    surface_cache_p010: Option<surface_cache::SurfaceCache>,
    d3d11_device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
    ctx4: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext4,
    coded_size: Size,
    fence_counter: u64,
}

unsafe impl Send for D3d11TransferBackend {}

impl D3d11TransferBackend {
    pub fn new(
        wgpu_device: &wgpu::Device,
        d3d11_device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
        coded_size: Size,
    ) -> Result<Self, String> {
        let handles = unsafe { extract_vulkan_raw_handles(wgpu_device) }
            .ok_or_else(|| "wgpu Vulkan生ハンドル取得失敗".to_owned())?;
        let engine =
            SemiPlanarConvertEngine::new(handles.device.clone(), handles.queue_family_index)?;
        let ctx4: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext4 = unsafe {
            let ctx = d3d11_device
                .GetImmediateContext()
                .map_err(|e| format!("ID3D11Device::GetImmediateContext失敗: {e}"))?;
            windows::core::Interface::cast(&ctx).map_err(|e| format!("{e}"))?
        };
        Ok(Self {
            handles,
            engine,
            surface_cache_nv12: None,
            surface_cache_p010: None,
            d3d11_device,
            ctx4,
            coded_size,
            fence_counter: 0,
        })
    }

    fn cache_for(
        &mut self,
        format: DXGI_FORMAT,
    ) -> Result<&mut surface_cache::SurfaceCache, String> {
        let slot = if format == DXGI_FORMAT_NV12 {
            &mut self.surface_cache_nv12
        } else {
            &mut self.surface_cache_p010
        };
        let stale = slot.as_ref().is_some_and(|cache| {
            cache.format() != format
                || cache.size() != (self.coded_size.width, self.coded_size.height)
        });
        if slot.is_none() || stale {
            let fence_for_cache = fence::CrossApiFence::new(&self.d3d11_device, &self.handles)?;
            *slot = Some(surface_cache::SurfaceCache::new(
                self.d3d11_device.clone(),
                &self.handles,
                self.ctx4.clone(),
                fence_for_cache,
                format,
                self.coded_size.width,
                self.coded_size.height,
            )?);
        }
        Ok(slot.as_mut().unwrap())
    }
}

impl TransferBackend for D3d11TransferBackend {
    type Input = D3d11DecodedFrame;

    fn source_backend(&self) -> SourceBackend {
        SourceBackend::D3d11va
    }

    fn transfer(
        &mut self,
        input: &Self::Input,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        pool: &dyn NeoFramePool,
    ) -> Result<NeoFrame, TransferError> {
        let Some((y_fmt, uv_fmt, src_fmt, _)) = plane_view_formats(input.sw_format_i32) else {
            return Err(TransferError::UnsupportedFormat(PixelFormat::Nv12));
        };

        let vk_queue = self.handles.queue;
        let cache = self.cache_for(src_fmt).map_err(TransferError::SyncFailed)?;

        let shared = unsafe { cache.import(&input.d3d11_texture, input.subresource_index) }
            .map_err(TransferError::SyncFailed)?;
        let fence_semaphore = cache.fence().vk_semaphore();

        let width = input.visible_rect.width;
        let height = input.visible_rect.height;
        let dst_pixel_format = dst_pixel_format_for(input.sw_format_i32);
        let dst_dxgi_format = match dst_pixel_format {
            PixelFormat::Rgba16Float => DXGI_FORMAT_R16G16B16A16_FLOAT,
            _ => DXGI_FORMAT_R8G8B8A8_UNORM,
        };
        let dst_vk_format = match dst_dxgi_format {
            DXGI_FORMAT_R16G16B16A16_FLOAT => ash::vk::Format::R16G16B16A16_SFLOAT,
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

        let dst_image: Option<ash::vk::Image> = unsafe {
            target_texture
                .as_hal::<wgpu_hal::api::Vulkan>()
                .map(|hal_texture| hal_texture.raw_handle())
        };
        let Some(dst_image) = dst_image else {
            release_and_return!(TransferError::CopyFailed(
                "wgpuテクスチャからVkImage取得失敗".to_owned()
            ));
        };

        let convert_result = self.engine.convert(ConvertRequest {
            src_image: shared.vk_image,
            y_format: dxgi_plane_format_to_vk(y_fmt),
            uv_format: dxgi_plane_format_to_vk(uv_fmt),
            dst_image,
            dst_format: dst_vk_format,
            width,
            height,
            queue: vk_queue,
            fence_semaphore,
            wait_value: shared.wait_value,
            signal_value: shared.signal_value,
            tags: color_tags_of(input),
        });
        if let Err(e) = convert_result {
            release_and_return!(TransferError::CopyFailed(e));
        }

        let target_texture = match unsafe { pool.finalize_write(device, target_texture) } {
            Ok(texture) => texture,
            Err(e) => return Err(map_pool_error(e)),
        };

        self.fence_counter = self.fence_counter.wrapping_add(1);

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
            source_backend: SourceBackend::D3d11va,
        })
    }
}
