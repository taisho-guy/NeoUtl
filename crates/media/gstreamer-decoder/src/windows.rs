use ash::vk;
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_sys as gst_sys;
use std::os::raw::c_void;
use wgpu::hal::vulkan::Api as Vulkan;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Dxgi::IDXGIResource1;
use windows::core::Interface;

#[repr(C)]
struct GstD3D11Memory {
    mem: gst_sys::GstMemory,
    texture: *mut c_void,
}

fn extract_shared_handle(buffer: &gst::BufferRef) -> Result<HANDLE, String> {
    let memory = buffer.peek_memory(0);
    let raw: *const gst_sys::GstMemory = memory.as_ptr();
    let d3d11_mem = raw as *const GstD3D11Memory;
    let texture_ptr = unsafe { (*d3d11_mem).texture };
    if texture_ptr.is_null() {
        return Err("D3D11Texture2Dポインタ未取得".to_owned());
    }
    let texture: ID3D11Texture2D = unsafe { ID3D11Texture2D::from_raw_borrowed(&texture_ptr) }
        .ok_or("ID3D11Texture2D変換失敗")?
        .clone();
    let resource: IDXGIResource1 = texture.cast().map_err(|e| e.to_string())?;
    unsafe { resource.CreateSharedHandle(None, 0x10000000, None) }.map_err(|e| e.to_string())
}

pub unsafe fn import_frame(
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    buffer: &gst::BufferRef,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, String> {
    let shared_handle = extract_shared_handle(buffer)?;

    let hal_device = unsafe { device.as_hal::<Vulkan>() }.ok_or("Vulkanバックエンドではない")?;
    let raw_device = hal_device.raw_device();
    let raw_physical = hal_device.raw_physical_device();
    let instance = hal_device.shared_instance().raw_instance();

    let format = vk::Format::G8_B8R8_2PLANE_420_UNORM;
    let extent = vk::Extent3D {
        width,
        height,
        depth: 1,
    };

    let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE);
    let image_info = vk::ImageCreateInfo::default()
        .push_next(&mut external_info)
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(extent)
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let image = unsafe { raw_device.create_image(&image_info, None) }.map_err(|e| e.to_string())?;

    let win32_props_loader = ash::khr::external_memory_win32::Device::new(instance, raw_device);
    let mut win32_props = vk::MemoryWin32HandlePropertiesKHR::default();
    unsafe {
        win32_props_loader.get_memory_win32_handle_properties(
            vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE,
            shared_handle.0 as isize,
            &mut win32_props,
        )
    }
    .map_err(|e| e.to_string())?;

    let mem_requirements = unsafe { raw_device.get_image_memory_requirements(image) };
    let mem_properties = unsafe { instance.get_physical_device_memory_properties(raw_physical) };
    let type_bits = mem_requirements.memory_type_bits & win32_props.memory_type_bits;
    let memory_type_index = (0..mem_properties.memory_type_count)
        .find(|&i| (type_bits >> i) & 1 == 1)
        .ok_or("互換メモリタイプ未検出")?;

    let mut import_info = vk::ImportMemoryWin32HandleInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE)
        .handle(shared_handle.0 as isize);
    let alloc_info = vk::MemoryAllocateInfo::default()
        .push_next(&mut import_info)
        .allocation_size(mem_requirements.size)
        .memory_type_index(memory_type_index);
    let memory =
        unsafe { raw_device.allocate_memory(&alloc_info, None) }.map_err(|e| e.to_string())?;
    unsafe { raw_device.bind_image_memory(image, memory, 0) }.map_err(|e| e.to_string())?;

    let texture_desc = wgpu::hal::TextureDescriptor {
        label: Some("gst-d3d11-shared-frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::NV12,
        usage: wgpu::TextureUses::RESOURCE,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: vec![],
    };
    let hal_texture = unsafe {
        hal_device.texture_from_raw(
            image,
            &texture_desc,
            None,
            wgpu::hal::vulkan::TextureMemory::Dedicated(memory),
        )
    };

    let wgpu_desc = wgpu::TextureDescriptor {
        label: Some("gst-d3d11-shared-frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::NV12,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    Ok(unsafe { device.create_texture_from_hal::<Vulkan>(hal_texture, &wgpu_desc) })
}
