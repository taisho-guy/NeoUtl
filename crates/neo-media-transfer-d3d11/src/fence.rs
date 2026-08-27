use std::sync::atomic::{AtomicU64, Ordering};

use ash::vk;
use windows::Win32::Foundation::{CloseHandle, GENERIC_ALL, HANDLE};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device5, ID3D11DeviceContext4, ID3D11Fence};
use windows::core::Interface;

pub struct CrossApiFence {
    d3d11_fence: ID3D11Fence,
    vk_semaphore: vk::Semaphore,
    external_semaphore_win32: ash::khr::external_semaphore_win32::Device,
    counter: AtomicU64,
}

impl CrossApiFence {
    pub fn new(
        d3d11_device: &windows::Win32::Graphics::Direct3D11::ID3D11Device,
        handles: &crate::vulkan_convert::VulkanRawHandles,
    ) -> Result<Self, String> {
        let vk_instance = &handles.instance;
        let vk_device = &handles.device;
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

            let mut timeline_info = vk::SemaphoreTypeCreateInfo::default()
                .semaphore_type(vk::SemaphoreType::TIMELINE)
                .initial_value(0);
            let mut export_info = vk::ExportSemaphoreCreateInfo::default()
                .handle_types(vk::ExternalSemaphoreHandleTypeFlags::D3D12_FENCE);
            let semaphore_info = vk::SemaphoreCreateInfo::default()
                .push_next(&mut timeline_info)
                .push_next(&mut export_info);
            let vk_semaphore = vk_device
                .create_semaphore(&semaphore_info, None)
                .map_err(|e| format!("vkCreateSemaphore失敗: {e}"))?;

            let external_semaphore_win32 =
                ash::khr::external_semaphore_win32::Device::new(vk_instance, vk_device);

            let import_info = vk::ImportSemaphoreWin32HandleInfoKHR::default()
                .semaphore(vk_semaphore)
                .handle_type(vk::ExternalSemaphoreHandleTypeFlags::D3D12_FENCE)
                .handle(shared_handle.0 as isize);
            external_semaphore_win32
                .import_semaphore_win32_handle(&import_info)
                .map_err(|e| format!("vkImportSemaphoreWin32HandleKHR失敗: {e}"))?;

            let _ = CloseHandle(shared_handle);

            Ok(Self {
                d3d11_fence,
                vk_semaphore,
                external_semaphore_win32,
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

    pub fn vk_semaphore(&self) -> vk::Semaphore {
        self.vk_semaphore
    }

    pub unsafe fn destroy(&self, vk_device: &ash::Device) {
        unsafe {
            vk_device.destroy_semaphore(self.vk_semaphore, None);
        }
        let _ = &self.external_semaphore_win32;
    }
}
