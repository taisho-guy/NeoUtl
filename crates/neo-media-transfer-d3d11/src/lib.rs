mod device;
mod dx12;
mod fence;
mod surface_cache;

use ffmpeg_sys_next as sys;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_NV12, DXGI_FORMAT_P010, DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R8G8_UNORM,
    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R16_UNORM, DXGI_FORMAT_R16G16_UNORM,
    DXGI_FORMAT_R16G16B16A16_FLOAT,
};
use windows::core::Interface;

use neo_media_core::{
    ColorPrimaries, DecodedHwFrame, MatrixCoefficients, NeoFrame, NeoFramePool, PixelFormat,
    PoolError, Rect, Size, SourceBackend, TransferBackend, TransferCharacteristics, TransferError,
};

pub use device::{NeoutlD3d11DeviceCtx, create_av_d3d11va_device_ctx, create_d3d11_device_on_luid};
pub use dx12::{ColorTags, Dx12RawHandles, SemiPlanarConvertEngine, extract_dx12_raw_handles};

static SEMI_PLANAR_DXIL: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/semi_planar_to_rgba.dxil"));

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
    let handles = unsafe { extract_dx12_raw_handles(wgpu_device) }
        .ok_or_else(|| "wgpu DX12生ハンドル取得失敗".to_owned())?;
    unsafe { Ok(handles.device.GetAdapterLuid()) }
}

pub fn query_vram_budget_bytes(wgpu_device: &wgpu::Device) -> Option<u64> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
        DXGI_QUERY_VIDEO_MEMORY_INFO, IDXGIAdapter3, IDXGIFactory4,
    };
    let luid = dx12_adapter_luid(wgpu_device).ok()?;
    unsafe {
        let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).ok()?;
        let adapter: IDXGIAdapter3 = factory.EnumAdapterByLuid(luid).ok()?;
        let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
        adapter
            .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info)
            .ok()?;
        Some(info.Budget)
    }
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
    handles: Dx12RawHandles,
    engine: SemiPlanarConvertEngine,
    surface_cache_nv12: Option<surface_cache::SurfaceCache>,
    surface_cache_p010: Option<surface_cache::SurfaceCache>,
    d3d11_device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
    ctx4: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext4,
    coded_size: Size,
}

unsafe impl Send for D3d11TransferBackend {}

impl D3d11TransferBackend {
    pub fn new(
        wgpu_device: &wgpu::Device,
        d3d11_device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
        coded_size: Size,
    ) -> Result<Self, String> {
        let handles = unsafe { extract_dx12_raw_handles(wgpu_device) }
            .ok_or_else(|| "wgpu DX12生ハンドル取得失敗".to_owned())?;
        let engine = unsafe { SemiPlanarConvertEngine::new(&handles, SEMI_PLANAR_DXIL)? };
        let ctx4: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext4 = unsafe {
            let ctx = d3d11_device
                .GetImmediateContext()
                .map_err(|e| format!("ID3D11Device::GetImmediateContext失敗: {e}"))?;
            ctx.cast().map_err(|e| format!("{e}"))?
        };
        Ok(Self {
            handles,
            engine,
            surface_cache_nv12: None,
            surface_cache_p010: None,
            d3d11_device,
            ctx4,
            coded_size,
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
            let fence_for_cache =
                fence::CrossApiFence::new(&self.d3d11_device, &self.handles.device)?;
            *slot = Some(surface_cache::SurfaceCache::new(
                self.d3d11_device.clone(),
                self.handles.device.clone(),
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

        let d3d12_queue = self.handles.queue.clone();
        let cache = self.cache_for(src_fmt).map_err(TransferError::SyncFailed)?;

        let shared_resource =
            unsafe { cache.import(&input.d3d11_texture, input.subresource_index, &d3d12_queue) }
                .map_err(TransferError::SyncFailed)?;

        let width = input.visible_rect.width;
        let height = input.visible_rect.height;
        let dst_pixel_format = dst_pixel_format_for(input.sw_format_i32);
        let dst_dxgi_format = match dst_pixel_format {
            PixelFormat::Rgba16Float => DXGI_FORMAT_R16G16B16A16_FLOAT,
            _ => DXGI_FORMAT_R8G8B8A8_UNORM,
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

        let dst_resource: Option<ID3D12Resource> = unsafe {
            target_texture
                .as_hal::<wgpu_hal::api::Dx12>()
                .map(|hal_texture| hal_texture.raw_resource().clone())
        };
        let Some(dst_resource) = dst_resource else {
            release_and_return!(TransferError::CopyFailed(
                "wgpuテクスチャからID3D12Resource取得失敗".to_owned()
            ));
        };

        let convert_result = unsafe {
            self.engine.convert(dx12::ConvertRequest {
                src_resource: &shared_resource,
                y_format: y_fmt,
                uv_format: uv_fmt,
                dst_resource: &dst_resource,
                dst_format: dst_dxgi_format,
                width,
                height,
                tags: color_tags_of(input),
            })
        };
        if let Err(e) = convert_result {
            release_and_return!(TransferError::CopyFailed(e));
        }

        let target_texture = match unsafe { pool.finalize_write(device, target_texture) } {
            Ok(texture) => texture,
            Err(e) => return Err(map_pool_error(e)),
        };

        let _ = &self.handles;

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
