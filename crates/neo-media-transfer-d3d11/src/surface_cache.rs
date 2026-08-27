use ash::vk;
use windows::Win32::Foundation::{CloseHandle, GENERIC_ALL, HANDLE};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_RESOURCE_MISC_SHARED_NTHANDLE, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, ID3D11Device, ID3D11DeviceContext4, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_NV12, DXGI_FORMAT_P010};
use windows::Win32::Graphics::Dxgi::IDXGIResource1;
use windows::core::Interface;

use crate::fence::CrossApiFence;
use crate::vulkan_convert::VulkanRawHandles;

const RING_SIZE: usize = 4;

fn dxgi_video_format_to_vk(format: DXGI_FORMAT) -> Result<vk::Format, String> {
    match format {
        DXGI_FORMAT_NV12 => Ok(vk::Format::G8_B8R8_2PLANE_420_UNORM),
        DXGI_FORMAT_P010 => Ok(vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16),
        other => Err(format!("未対応DXGI_FORMAT(動画面): {other:?}")),
    }
}

struct RingEntry {
    d3d11_texture: ID3D11Texture2D,
    vk_image: vk::Image,
    vk_memory: vk::DeviceMemory,
}

pub struct SharedResource {
    pub vk_image: vk::Image,
    pub wait_value: u64,
    pub signal_value: u64,
}

pub struct SurfaceCache {
    vk_device: ash::Device,
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
        handles: &VulkanRawHandles,
        ctx4: ID3D11DeviceContext4,
        fence: CrossApiFence,
        format: DXGI_FORMAT,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let vk_format = dxgi_video_format_to_vk(format)?;
        let external_memory_win32 =
            ash::khr::external_memory_win32::Device::new(&handles.instance, &handles.device);
        let mut entries = Vec::with_capacity(RING_SIZE);
        for _ in 0..RING_SIZE {
            entries.push(Self::create_entry(
                &d3d11_device,
                &handles.device,
                &external_memory_win32,
                &handles.instance,
                handles.physical_device,
                format,
                vk_format,
                width,
                height,
            )?);
        }
        Ok(Self {
            vk_device: handles.device.clone(),
            ctx4,
            fence,
            format,
            width,
            height,
            entries,
            next: 0,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn create_entry(
        d3d11_device: &ID3D11Device,
        vk_device: &ash::Device,
        external_memory_win32: &ash::khr::external_memory_win32::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        format: DXGI_FORMAT,
        vk_format: vk::Format,
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

            let mut external_image_info = vk::ExternalMemoryImageCreateInfo::default()
                .handle_types(vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE);
            let image_info = vk::ImageCreateInfo::default()
                .push_next(&mut external_image_info)
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk_format)
                .extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let vk_image = vk_device
                .create_image(&image_info, None)
                .map_err(|e| format!("vkCreateImage失敗: {e}"))?;

            let mut mem_props = vk::MemoryWin32HandlePropertiesKHR::default();
            external_memory_win32
                .get_memory_win32_handle_properties(
                    vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE,
                    handle.0 as isize,
                    &mut mem_props,
                )
                .map_err(|e| format!("vkGetMemoryWin32HandlePropertiesKHR失敗: {e}"))?;

            let requirements = vk_device.get_image_memory_requirements(vk_image);
            let memory_type_index = select_memory_type(
                instance,
                physical_device,
                requirements.memory_type_bits & mem_props.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?;

            let mut import_info = vk::ImportMemoryWin32HandleInfoKHR::default()
                .handle_type(vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE)
                .handle(handle.0 as isize);
            let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(vk_image);
            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type_index)
                .push_next(&mut dedicated_info)
                .push_next(&mut import_info);
            let vk_memory = vk_device
                .allocate_memory(&alloc_info, None)
                .map_err(|e| format!("vkAllocateMemory(インポート)失敗: {e}"))?;
            vk_device
                .bind_image_memory(vk_image, vk_memory, 0)
                .map_err(|e| format!("vkBindImageMemory失敗: {e}"))?;

            let _ = CloseHandle(handle);

            Ok(RingEntry {
                d3d11_texture: texture,
                vk_image,
                vk_memory,
            })
        }
    }

    pub fn format(&self) -> DXGI_FORMAT {
        self.format
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub unsafe fn import(
        &mut self,
        src_texture: &ID3D11Texture2D,
        subresource_index: u32,
    ) -> Result<SharedResource, String> {
        unsafe {
            let idx = self.next;
            self.next = (self.next + 1) % self.entries.len();
            let entry = &self.entries[idx];

            self.ctx4.CopySubresourceRegion(
                &entry.d3d11_texture,
                0,
                0,
                0,
                0,
                src_texture,
                subresource_index,
                None,
            );

            let signal_value = self.fence.signal_after_d3d11_copy(&self.ctx4)?;

            Ok(SharedResource {
                vk_image: entry.vk_image,
                wait_value: signal_value.saturating_sub(1),
                signal_value,
            })
        }
    }

    pub fn fence(&self) -> &CrossApiFence {
        &self.fence
    }
}

fn select_memory_type(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    type_bits: u32,
    required_properties: vk::MemoryPropertyFlags,
) -> Result<u32, String> {
    let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    for i in 0..mem_props.memory_type_count {
        let bit_match = (type_bits & (1 << i)) != 0;
        let prop_match = mem_props.memory_types[i as usize]
            .property_flags
            .contains(required_properties);
        if bit_match && prop_match {
            return Ok(i);
        }
    }
    Err("適合メモリタイプ未検出".to_owned())
}

impl Drop for SurfaceCache {
    fn drop(&mut self) {
        unsafe {
            for entry in &self.entries {
                self.vk_device.destroy_image(entry.vk_image, None);
                self.vk_device.free_memory(entry.vk_memory, None);
            }
        }
    }
}
