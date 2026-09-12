use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::ffi::{NativeDeviceHandles, NativeTextureHandle};

const BACKEND_VULKAN: u8 = 0;
#[allow(dead_code)]
const BACKEND_METAL: u8 = 1;
#[allow(dead_code)]
const BACKEND_DX12: u8 = 2;

struct RenderContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    backend_tag: u8,
    target: Option<wgpu::Texture>,
}

static CONTEXTS: Mutex<Option<HashMap<usize, RenderContext>>> = Mutex::new(None);
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

fn contexts() -> std::sync::MutexGuard<'static, Option<HashMap<usize, RenderContext>>> {
    let mut guard = CONTEXTS.lock().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard
}

fn texture_descriptor(width: u32, height: u32) -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: Some("neoqtl-rhi-target"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    }
}

fn hal_texture_descriptor(width: u32, height: u32) -> wgpu::hal::TextureDescriptor<'static> {
    wgpu::hal::TextureDescriptor {
        label: Some("neoqtl-rhi-target-hal"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUses::COLOR_TARGET,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: Vec::new(),
    }
}

#[cfg(target_os = "linux")]
fn bind_vulkan(h: &NativeDeviceHandles) -> Option<(wgpu::Device, wgpu::Queue)> {
    use ash::vk::Handle;
    use wgpu::hal::api::Vulkan;
    use wgpu::hal::vulkan as hal_vk;

    let entry = unsafe { ash::Entry::load().ok()? };
    let raw_instance_handle = ash::vk::Instance::from_raw(h.instance);
    let raw_instance = unsafe { ash::Instance::load(entry.static_fn(), raw_instance_handle) };

    let instance = unsafe {
        hal_vk::Instance::from_raw(
            entry,
            raw_instance,
            0,
            0,
            None,
            Vec::new(),
            wgpu::InstanceFlags::empty(),
            wgpu::MemoryBudgetThresholds::default(),
            false,
            None,
        )
        .ok()?
    };

    let raw_physical_device = ash::vk::PhysicalDevice::from_raw(h.physical_device);
    let exposed = instance.expose_adapter(raw_physical_device)?;

    let enabled_extensions = exposed
        .adapter
        .required_device_extensions(wgpu::Features::empty());
    let raw_device_handle = ash::vk::Device::from_raw(h.device);
    let raw_device = unsafe {
        ash::Device::load(
            instance.shared_instance().raw_instance().fp_v1_0(),
            raw_device_handle,
        )
    };

    let open = unsafe {
        exposed
            .adapter
            .device_from_raw(
                raw_device,
                None,
                &enabled_extensions,
                wgpu::Features::empty(),
                &wgpu::Limits::default(),
                &wgpu::MemoryHints::default(),
                h.queue_family_index,
                0,
            )
            .ok()?
    };

    let wgpu_instance = unsafe { wgpu::Instance::from_hal::<Vulkan>(instance) };
    let adapter = unsafe { wgpu_instance.create_adapter_from_hal(exposed) };
    let (device, queue) = unsafe {
        adapter
            .create_device_from_hal(open, &wgpu::DeviceDescriptor::default())
            .ok()?
    };
    Some((device, queue))
}

#[cfg(target_os = "macos")]
fn bind_metal(h: &NativeDeviceHandles) -> Option<(wgpu::Device, wgpu::Queue)> {
    use objc::{msg_send, sel, sel_impl};
    use wgpu::hal::api::Metal;
    use wgpu::hal::metal as hal_mtl;

    let device_ptr = h.device as *mut objc::runtime::Object;
    let queue_ptr = h.queue as *mut objc::runtime::Object;
    let _: () = unsafe { msg_send![device_ptr, retain] };
    let _: () = unsafe { msg_send![queue_ptr, retain] };
    let raw_device = unsafe { metal::Device::from_ptr(device_ptr as *mut _) };
    let raw_queue = unsafe { metal::CommandQueue::from_ptr(queue_ptr as *mut _) };

    let exposed = hal_mtl::Adapter::new_external(raw_device)?;
    let open = unsafe {
        exposed
            .adapter
            .device_from_raw(raw_queue, &exposed.features, wgpu::Features::empty())
            .ok()?
    };
    let wgpu_instance = unsafe { wgpu::Instance::from_hal::<Metal>(hal_mtl::Instance::default()) };
    let adapter = unsafe { wgpu_instance.create_adapter_from_hal(exposed) };
    let (device, queue) = unsafe {
        adapter
            .create_device_from_hal(open, &wgpu::DeviceDescriptor::default())
            .ok()?
    };
    Some((device, queue))
}

#[cfg(target_os = "windows")]
fn bind_dx12(h: &NativeDeviceHandles) -> Option<(wgpu::Device, wgpu::Queue)> {
    use std::mem::ManuallyDrop;
    use wgpu::hal::api::Dx12;
    use wgpu::hal::dx12 as hal_dx12;
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D12::{ID3D12CommandQueue, ID3D12Device};

    let borrowed_device: ManuallyDrop<ID3D12Device> =
        ManuallyDrop::new(unsafe { Interface::from_raw(h.device as *mut core::ffi::c_void) });
    let borrowed_queue: ManuallyDrop<ID3D12CommandQueue> =
        ManuallyDrop::new(unsafe { Interface::from_raw(h.queue as *mut core::ffi::c_void) });
    let raw_device: ID3D12Device = (*borrowed_device).clone();
    let raw_queue: ID3D12CommandQueue = (*borrowed_queue).clone();

    let exposed = unsafe { hal_dx12::Adapter::from_raw_device(raw_device.clone())? };
    let open = unsafe {
        exposed
            .adapter
            .device_from_raw(
                raw_device,
                raw_queue,
                &exposed.features,
                wgpu::Features::empty(),
            )
            .ok()?
    };
    let wgpu_instance = unsafe { wgpu::Instance::from_hal::<Dx12>(hal_dx12::Instance::empty()) };
    let adapter = unsafe { wgpu_instance.create_adapter_from_hal(exposed) };
    let (device, queue) = unsafe {
        adapter
            .create_device_from_hal(open, &wgpu::DeviceDescriptor::default())
            .ok()?
    };
    Some((device, queue))
}

pub fn wgpu_renderer_create() -> usize {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn wgpu_renderer_bind_device(context_id: usize, handles: NativeDeviceHandles) {
    let bound = match handles.backend {
        #[cfg(target_os = "linux")]
        BACKEND_VULKAN => bind_vulkan(&handles),
        #[cfg(target_os = "macos")]
        BACKEND_METAL => bind_metal(&handles),
        #[cfg(target_os = "windows")]
        BACKEND_DX12 => bind_dx12(&handles),
        _ => None,
    };
    let Some((device, queue)) = bound else { return };
    contexts().as_mut().unwrap().insert(
        context_id,
        RenderContext {
            device,
            queue,
            backend_tag: handles.backend,
            target: None,
        },
    );
}

pub fn wgpu_renderer_bind_texture(context_id: usize, texture: NativeTextureHandle) {
    let mut guard = contexts();
    let Some(ctx) = guard.as_mut().unwrap().get_mut(&context_id) else {
        return;
    };
    let descriptor = texture_descriptor(texture.width.max(1) as u32, texture.height.max(1) as u32);
    let hal_descriptor =
        hal_texture_descriptor(texture.width.max(1) as u32, texture.height.max(1) as u32);

    let wrapped = match ctx.backend_tag {
        #[cfg(target_os = "linux")]
        BACKEND_VULKAN => unsafe {
            use ash::vk::Handle;
            let raw_image = ash::vk::Image::from_raw(texture.object);
            let hal_texture = ctx
                .device
                .as_hal::<wgpu::hal::api::Vulkan>()
                .map(|hal_device| {
                    hal_device.texture_from_raw(
                        raw_image,
                        &hal_descriptor,
                        Some(Box::new(|| {})),
                        wgpu::hal::vulkan::TextureMemory::External,
                    )
                });
            hal_texture.map(|tex| {
                ctx.device
                    .create_texture_from_hal::<wgpu::hal::api::Vulkan>(
                        tex,
                        &descriptor,
                        wgpu::TextureUses::COLOR_TARGET,
                    )
            })
        },
        #[cfg(target_os = "macos")]
        BACKEND_METAL => unsafe {
            use objc::{msg_send, sel, sel_impl};
            let texture_ptr = texture.object as *mut objc::runtime::Object;
            let _: () = msg_send![texture_ptr, retain];
            let raw = metal::Texture::from_ptr(texture_ptr as *mut _);
            let hal_texture = ctx
                .device
                .as_hal::<wgpu::hal::api::Metal>()
                .map(|hal_device| {
                    hal_device.texture_from_raw(
                        raw,
                        wgpu::TextureFormat::Rgba8Unorm,
                        wgpu::TextureDimension::D2,
                        descriptor.size,
                        1,
                        1,
                    )
                });
            hal_texture.map(|tex| {
                ctx.device.create_texture_from_hal::<wgpu::hal::api::Metal>(
                    tex,
                    &descriptor,
                    wgpu::TextureUses::COLOR_TARGET,
                )
            })
        },
        #[cfg(target_os = "windows")]
        BACKEND_DX12 => unsafe {
            use std::mem::ManuallyDrop;
            use windows::core::Interface;
            use windows::Win32::Graphics::Direct3D12::ID3D12Resource;
            let borrowed: ManuallyDrop<ID3D12Resource> =
                ManuallyDrop::new(Interface::from_raw(texture.object as *mut core::ffi::c_void));
            let raw: ID3D12Resource = (*borrowed).clone();
            let hal_texture = ctx
                .device
                .as_hal::<wgpu::hal::api::Dx12>()
                .map(|hal_device| hal_device.texture_from_raw(raw, &hal_descriptor));
            hal_texture.map(|tex| {
                ctx.device.create_texture_from_hal::<wgpu::hal::api::Dx12>(
                    tex,
                    &descriptor,
                    wgpu::TextureUses::COLOR_TARGET,
                )
            })
        },
        _ => None,
    };

    ctx.target = wrapped;
}

pub fn wgpu_renderer_render(context_id: usize) {
    let mut guard = contexts();
    let Some(ctx) = guard.as_mut().unwrap().get_mut(&context_id) else {
        return;
    };
    let Some(target) = ctx.target.as_ref() else {
        return;
    };

    let view = target.create_view(&Default::default());
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("neoqtl-rhi-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.2,
                        g: 0.02,
                        b: 0.8,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    ctx.queue.submit(Some(encoder.finish()));
}

pub fn wgpu_renderer_destroy(context_id: usize) {
    contexts().as_mut().unwrap().remove(&context_id);
}
