use gst::prelude::*;
use gstreamer as gst;
use gstreamer_gl as gst_gl;
use objc2_core_video::CVPixelBuffer;
use objc2_io_surface::IOSurfaceRef;
use wgpu::hal::metal::Api as Metal;

fn extract_iosurface(buffer: &gst::BufferRef) -> Result<*mut IOSurfaceRef, String> {
    let memory = buffer.peek_memory(0);
    let gl_memory = memory
        .downcast_memory_ref::<gst_gl::GLMemory>()
        .ok_or("GLメモリではない")?;
    let pixel_buffer: *mut CVPixelBuffer = gl_memory.texture_target_cv_pixel_buffer();
    if pixel_buffer.is_null() {
        return Err("CVPixelBuffer未取得".to_owned());
    }
    let surface = unsafe { objc2_core_video::CVPixelBufferGetIOSurface(pixel_buffer) };
    if surface.is_null() {
        return Err("IOSurface未取得".to_owned());
    }
    Ok(surface)
}

pub unsafe fn import_frame(
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    buffer: &gst::BufferRef,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, String> {
    let io_surface = extract_iosurface(buffer)?;

    let hal_device = unsafe { device.as_hal::<Metal>() };
    let raw_device = hal_device.raw_device();

    let descriptor = metal::TextureDescriptor::new();
    descriptor.set_pixel_format(metal::MTLPixelFormat::YCBCR8_420_2P);
    descriptor.set_width(width as u64);
    descriptor.set_height(height as u64);
    descriptor.set_storage_mode(metal::MTLStorageMode::Shared);
    descriptor.set_usage(metal::MTLTextureUsage::ShaderRead);

    let raw_texture =
        raw_device
            .lock()
            .new_texture_with_iosurface(&descriptor, io_surface as *mut _, 0);

    let texture_desc = wgpu::hal::TextureDescriptor {
        label: Some("gst-iosurface-frame"),
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
    let hal_texture = unsafe { hal_device.texture_from_raw(raw_texture, &texture_desc, None) };

    let wgpu_desc = wgpu::TextureDescriptor {
        label: Some("gst-iosurface-frame"),
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
    Ok(unsafe { device.create_texture_from_hal::<Metal>(hal_texture, &wgpu_desc) })
}
