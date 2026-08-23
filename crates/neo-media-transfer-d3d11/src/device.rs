use std::ptr;

use ffmpeg_sys_next as sys;
use windows::Win32::Foundation::{HMODULE, LUID};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
    D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, IDXGIFactory6,
};
use windows::core::Interface;

const AV_HWDEVICE_TYPE_D3D11VA: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA;

pub struct NeoutlD3d11DeviceCtx {
    pub av_hw_device_ctx: *mut sys::AVBufferRef,
    pub d3d11_device: ID3D11Device,
}

unsafe impl Send for NeoutlD3d11DeviceCtx {}
unsafe impl Sync for NeoutlD3d11DeviceCtx {}

impl Drop for NeoutlD3d11DeviceCtx {
    fn drop(&mut self) {
        unsafe {
            if !self.av_hw_device_ctx.is_null() {
                sys::av_buffer_unref(&mut self.av_hw_device_ctx);
            }
        }
    }
}

fn adapter_by_luid(luid: LUID) -> Result<windows::Win32::Graphics::Dxgi::IDXGIAdapter1, String> {
    unsafe {
        let factory: IDXGIFactory6 =
            CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).map_err(|e| format!("{e}"))?;
        factory
            .EnumAdapterByLuid(luid)
            .map_err(|e| format!("EnumAdapterByLuid失敗: {e}"))
    }
}

pub fn create_d3d11_device_on_luid(luid: LUID) -> Result<ID3D11Device, String> {
    unsafe {
        let adapter = adapter_by_luid(luid)?;
        let mut device: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
        .map_err(|e| format!("D3D11CreateDevice失敗: {e}"))?;
        device.ok_or_else(|| "D3D11CreateDevice: デバイス未取得".to_owned())
    }
}

pub fn create_av_d3d11va_device_ctx(
    d3d11_device: &ID3D11Device,
) -> Result<NeoutlD3d11DeviceCtx, String> {
    unsafe {
        let av_hw_device_ctx = sys::av_hwdevice_ctx_alloc(AV_HWDEVICE_TYPE_D3D11VA);
        if av_hw_device_ctx.is_null() {
            return Err("av_hwdevice_ctx_alloc失敗".to_owned());
        }

        let device_ctx = (*av_hw_device_ctx).data as *mut sys::AVHWDeviceContext;
        let hwctx = (*device_ctx).hwctx as *mut sys::AVD3D11VADeviceContext;
        (*hwctx).device = d3d11_device.as_raw() as *mut sys::ID3D11Device;

        if sys::av_hwdevice_ctx_init(av_hw_device_ctx) < 0 {
            sys::av_buffer_unref(&mut { av_hw_device_ctx });
            return Err("av_hwdevice_ctx_init(D3D11VA)失敗".to_owned());
        }

        let _ = ptr::null::<()>();
        Ok(NeoutlD3d11DeviceCtx {
            av_hw_device_ctx,
            d3d11_device: d3d11_device.clone(),
        })
    }
}
