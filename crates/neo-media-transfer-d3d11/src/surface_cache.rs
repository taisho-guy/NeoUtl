use windows::Win32::Foundation::{CloseHandle, GENERIC_ALL, HANDLE};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_RESOURCE_MISC_SHARED_NTHANDLE, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, ID3D11Device, ID3D11DeviceContext4, ID3D11Texture2D,
};
use windows::Win32::Graphics::Direct3D12::{ID3D12CommandQueue, ID3D12Device, ID3D12Resource};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;
use windows::Win32::Graphics::Dxgi::IDXGIResource1;
use windows::core::Interface;

use crate::fence::CrossApiFence;

const RING_SIZE: usize = 4;

struct RingEntry {
    d3d11_texture: ID3D11Texture2D,
    d3d12_resource: ID3D12Resource,
    fence_value: u64,
}

pub struct SurfaceCache {
    #[allow(dead_code)]
    d3d11_device: ID3D11Device,
    #[allow(dead_code)]
    d3d12_device: ID3D12Device,
    ctx4: ID3D11DeviceContext4,
    fence: CrossApiFence,
    format: DXGI_FORMAT,
    width: u32,
    height: u32,
    entries: Vec<RingEntry>,
    next: usize,
}

impl SurfaceCache {
    pub fn new(
        d3d11_device: ID3D11Device,
        d3d12_device: ID3D12Device,
        ctx4: ID3D11DeviceContext4,
        fence: CrossApiFence,
        format: DXGI_FORMAT,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let mut entries = Vec::with_capacity(RING_SIZE);
        for _ in 0..RING_SIZE {
            entries.push(Self::create_entry(
                &d3d11_device,
                &d3d12_device,
                format,
                width,
                height,
            )?);
        }
        Ok(Self {
            d3d11_device,
            d3d12_device,
            ctx4,
            fence,
            format,
            width,
            height,
            entries,
            next: 0,
        })
    }

    fn create_entry(
        d3d11_device: &ID3D11Device,
        d3d12_device: &ID3D12Device,
        format: DXGI_FORMAT,
        width: u32,
        height: u32,
    ) -> Result<RingEntry, String> {
        unsafe {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: format,
                SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0 as u32,
            };
            let mut texture: Option<ID3D11Texture2D> = None;
            d3d11_device
                .CreateTexture2D(&desc, None, Some(&mut texture))
                .map_err(|e| format!("CreateTexture2D(共有)失敗: {e}"))?;
            let texture = texture.ok_or_else(|| "共有テクスチャ未取得".to_owned())?;

            let dxgi_resource: IDXGIResource1 = texture.cast().map_err(|e| format!("{e}"))?;
            let handle: HANDLE = dxgi_resource
                .CreateSharedHandle(None, GENERIC_ALL.0, None)
                .map_err(|e| format!("CreateSharedHandle失敗: {e}"))?;
            let d3d12_resource: ID3D12Resource = {
                let mut resource: Option<ID3D12Resource> = None;
                d3d12_device
                    .OpenSharedHandle(handle, &mut resource)
                    .map_err(|e| format!("ID3D12Device::OpenSharedHandle失敗: {e}"))?;
                resource
                    .ok_or_else(|| "ID3D12Device::OpenSharedHandle: リソース未取得".to_owned())?
            };
            let _ = CloseHandle(handle);

            Ok(RingEntry {
                d3d11_texture: texture,
                d3d12_resource,
                fence_value: 0,
            })
        }
    }

    pub unsafe fn import(
        &mut self,
        src_array_texture: &ID3D11Texture2D,
        subresource_index: u32,
        wait_queue: &ID3D12CommandQueue,
    ) -> Result<ID3D12Resource, String> {
        unsafe {
            let slot = self.next;
            self.next = (self.next + 1) % self.entries.len();

            self.ctx4.CopySubresourceRegion(
                &self.entries[slot].d3d11_texture,
                0,
                0,
                0,
                0,
                src_array_texture,
                subresource_index,
                None,
            );
            let value = self.fence.signal_after_d3d11_copy(&self.ctx4)?;
            self.entries[slot].fence_value = value;
            self.fence.wait_on_d3d12_queue(wait_queue, value)?;

            Ok(self.entries[slot].d3d12_resource.clone())
        }
    }

    #[allow(dead_code)]
    pub fn format(&self) -> DXGI_FORMAT {
        self.format
    }

    #[allow(dead_code)]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
