use ash::vk;
use gstreamer as gst;
use gstreamer_allocators as gst_allocators;
use wgpu::hal::vulkan::Api as Vulkan;

fn extract_dmabuf_fd(buffer: &gst::BufferRef) -> Result<std::os::fd::RawFd, String> {
    let memory = buffer.peek_memory(0);
    let dmabuf = memory
        .downcast_memory_ref::<gst_allocators::DmaBufMemory>()
        .ok_or("DMABufメモリではない")?;
    Ok(dmabuf.fd())
}

pub unsafe fn import_frame(
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    buffer: &gst::BufferRef,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, String> {
    let fd = extract_dmabuf_fd(buffer)?;

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
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
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

    let fd_props_loader = ash::khr::external_memory_fd::Device::new(instance, raw_device);
    let mut fd_props = vk::MemoryFdPropertiesKHR::default();
    unsafe {
        fd_props_loader.get_memory_fd_properties(
            vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            fd,
            &mut fd_props,
        )
    }
    .map_err(|e| e.to_string())?;

    let mem_requirements = unsafe { raw_device.get_image_memory_requirements(image) };
    let mem_properties = unsafe { instance.get_physical_device_memory_properties(raw_physical) };
    let type_bits = mem_requirements.memory_type_bits & fd_props.memory_type_bits;
    let memory_type_index = (0..mem_properties.memory_type_count)
        .find(|&i| (type_bits >> i) & 1 == 1)
        .ok_or("互換メモリタイプ未検出")?;

    let mut import_info = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        .fd(fd);
    let alloc_info = vk::MemoryAllocateInfo::default()
        .push_next(&mut import_info)
        .allocation_size(mem_requirements.size)
        .memory_type_index(memory_type_index);
    let memory =
        unsafe { raw_device.allocate_memory(&alloc_info, None) }.map_err(|e| e.to_string())?;
    unsafe { raw_device.bind_image_memory(image, memory, 0) }.map_err(|e| e.to_string())?;

    let texture_desc = wgpu::hal::TextureDescriptor {
        label: Some("gst-dmabuf-frame"),
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
        label: Some("gst-dmabuf-frame"),
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
