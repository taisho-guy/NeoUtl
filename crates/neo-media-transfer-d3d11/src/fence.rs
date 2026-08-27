use std::sync::atomic::{AtomicU64, Ordering};

use windows::Win32::Foundation::{CloseHandle, GENERIC_ALL, HANDLE};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device5, ID3D11DeviceContext4, ID3D11Fence};
use windows::Win32::Graphics::Direct3D12::{ID3D12Device, ID3D12Fence};
use windows::core::Interface;

pub struct CrossApiFence {
    d3d11_fence: ID3D11Fence,
    d3d12_fence: ID3D12Fence,
    counter: AtomicU64,
}

impl CrossApiFence {
    pub fn new(
        d3d11_device: &windows::Win32::Graphics::Direct3D11::ID3D11Device,
        d3d12_device: &ID3D12Device,
    ) -> Result<Self, String> {
        unsafe {
            let device5: ID3D11Device5 = d3d11_device.cast().map_err(|e| format!("{e}"))?;
            let mut d3d11_fence: Option<ID3D11Fence> = None;
            device5
                .CreateFence(
                    0,
                    windows::Win32::Graphics::Direct3D11::D3D11_FENCE_FLAG_SHARED,
                    &mut d3d11_fence,
                )
                .map_err(|e| format!("ID3D11Device5::CreateFence失敗: {e}"))?;
            let d3d11_fence = d3d11_fence
                .ok_or_else(|| "ID3D11Device5::CreateFence: フェンス未取得".to_owned())?;
            let shared_handle: HANDLE =
                d3d11_fence
                    .CreateSharedHandle(None, GENERIC_ALL.0, None)
                    .map_err(|e| format!("ID3D11Fence::CreateSharedHandle失敗: {e}"))?;
            let mut d3d12_fence: Option<ID3D12Fence> = None;
            d3d12_device
                .OpenSharedHandle(shared_handle, &mut d3d12_fence)
                .map_err(|e| format!("ID3D12Device::OpenSharedHandle(fence)失敗: {e}"))?;
            let d3d12_fence = d3d12_fence.ok_or_else(|| {
                "ID3D12Device::OpenSharedHandle(fence): フェンス未取得".to_owned()
            })?;
            let _ = CloseHandle(shared_handle);
            Ok(Self {
                d3d11_fence,
                d3d12_fence,
                counter: AtomicU64::new(0),
            })
        }
    }

    pub fn signal_after_d3d11_copy(&self, ctx4: &ID3D11DeviceContext4) -> Result<u64, String> {
        let value = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        unsafe {
            ctx4.Signal(&self.d3d11_fence, value)
                .map_err(|e| format!("ID3D11DeviceContext4::Signal失敗: {e}"))?;
        }
        Ok(value)
    }

    pub fn wait_on_d3d12_queue(
        &self,
        queue: &windows::Win32::Graphics::Direct3D12::ID3D12CommandQueue,
        value: u64,
    ) -> Result<(), String> {
        unsafe {
            queue
                .Wait(&self.d3d12_fence, value)
                .map_err(|e| format!("ID3D12CommandQueue::Wait失敗: {e}"))
        }
    }
}
