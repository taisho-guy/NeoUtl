use std::ffi::c_void;
use std::os::raw::{c_int, c_uint};
use std::ptr;
use std::sync::Arc;

use ffmpeg_sys_next as sys;

const AV_HWDEVICE_TYPE_VULKAN: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN;
const AV_PIX_FMT_VULKAN: i32 = 152;
const FF_API_VULKAN_FIXED_QUEUES: u32 = 0;

#[repr(C)]
struct AVVulkanDeviceContext {
    get_proc_addr: *mut c_void,
    inst: usize,
    phys_dev: usize,
    act_dev: usize,
    device_features: [u8; 512],
    enabled_inst_extensions: *mut *const i8,
    nb_enabled_inst_extensions: c_int,
    enabled_dev_extensions: *mut *const i8,
    nb_enabled_dev_extensions: c_int,
    queue_family_index: c_int,
    nb_graphics_queues: c_int,
    queue_family_tx_index: c_int,
    nb_tx_queues: c_int,
    queue_family_comp_index: c_int,
    nb_comp_queues: c_int,
    queue_family_encode_index: c_int,
    nb_encode_queues: c_int,
    queue_family_decode_index: c_int,
    nb_decode_queues: c_int,
    alloc: *mut c_void,
    lock_queue: *mut c_void,
    unlock_queue: *mut c_void,
}

#[repr(C)]
struct AVVkFrame {
    img: [u64; 8],
    tiling: c_int,
    mem: [usize; 8],
    size: [u64; 8],
    flags: c_uint,
    sem: [u64; 8],
    sem_value: [u64; 8],
    layout: [c_int; 8],
    access: [u64; 8],
    queue_family: [c_int; 8],
    internal: *mut c_void,
}

pub struct VulkanRawHandles {
    pub instance: ash::vk::Instance,
    pub physical_device: ash::vk::PhysicalDevice,
    pub device: ash::vk::Device,
    pub queue: ash::vk::Queue,
    pub queue_family_index: u32,
    pub get_instance_proc_addr: ash::vk::PFN_vkGetInstanceProcAddr,
}

pub unsafe fn extract_vulkan_raw_handles(device: &wgpu::Device) -> Option<VulkanRawHandles> {
    unsafe {
        let hal_device = device.as_hal::<wgpu_hal::api::Vulkan>()?;
        let shared_instance = hal_device.shared_instance();
        let raw_instance = shared_instance.raw_instance();
        let raw_entry = shared_instance.entry();
        Some(VulkanRawHandles {
            instance: raw_instance.handle(),
            physical_device: hal_device.raw_physical_device(),
            device: hal_device.raw_device().handle(),
            queue: hal_device.raw_queue(),
            queue_family_index: hal_device.queue_family_index(),
            get_instance_proc_addr: raw_entry.static_fn().get_instance_proc_addr,
        })
    }
}

pub struct NeoutlVulkanDeviceCtx {
    pub av_hw_device_ctx: *mut sys::AVBufferRef,
}

unsafe impl Send for NeoutlVulkanDeviceCtx {}
unsafe impl Sync for NeoutlVulkanDeviceCtx {}

impl Drop for NeoutlVulkanDeviceCtx {
    fn drop(&mut self) {
        unsafe {
            if !self.av_hw_device_ctx.is_null() {
                sys::av_buffer_unref(&mut self.av_hw_device_ctx);
            }
        }
    }
}

pub fn create_av_vulkan_device_ctx(
    handles: &VulkanRawHandles,
) -> Result<NeoutlVulkanDeviceCtx, String> {
    unsafe {
        let av_hw_device_ctx = sys::av_hwdevice_ctx_alloc(AV_HWDEVICE_TYPE_VULKAN);
        if av_hw_device_ctx.is_null() {
            return Err("av_hwdevice_ctx_alloc失敗".to_owned());
        }

        let hwctx = (*av_hw_device_ctx).data as *mut sys::AVHWDeviceContext;
        let vk_ctx = (*hwctx).hwctx as *mut AVVulkanDeviceContext;

        (*vk_ctx).get_proc_addr = handles.get_instance_proc_addr as *mut c_void;
        (*vk_ctx).inst = handles.instance.as_raw() as usize;
        (*vk_ctx).phys_dev = handles.physical_device.as_raw() as usize;
        (*vk_ctx).act_dev = handles.device.as_raw() as usize;
        (*vk_ctx).queue_family_index = handles.queue_family_index as c_int;
        (*vk_ctx).nb_graphics_queues = 1;
        (*vk_ctx).queue_family_tx_index = handles.queue_family_index as c_int;
        (*vk_ctx).nb_tx_queues = 1;
        (*vk_ctx).queue_family_comp_index = handles.queue_family_index as c_int;
        (*vk_ctx).nb_comp_queues = 1;
        (*vk_ctx).queue_family_encode_index = -1;
        (*vk_ctx).nb_encode_queues = 0;
        (*vk_ctx).queue_family_decode_index = -1;
        (*vk_ctx).nb_decode_queues = 0;

        if sys::av_hwdevice_ctx_init(av_hw_device_ctx) < 0 {
            sys::av_buffer_unref(&mut { av_hw_device_ctx });
            return Err("av_hwdevice_ctx_init(Vulkan)失敗".to_owned());
        }

        Ok(NeoutlVulkanDeviceCtx { av_hw_device_ctx })
    }
}

pub struct DerivedVulkanFrame {
    av_frame: *mut sys::AVFrame,
}

unsafe impl Send for DerivedVulkanFrame {}

impl Drop for DerivedVulkanFrame {
    fn drop(&mut self) {
        unsafe {
            if !self.av_frame.is_null() {
                sys::av_frame_free(&mut self.av_frame);
            }
        }
    }
}

pub fn create_derived_vulkan_frames_ctx(
    src_hw_frames_ctx: *mut sys::AVBufferRef,
    vulkan_device_ctx: &NeoutlVulkanDeviceCtx,
) -> Result<*mut sys::AVBufferRef, String> {
    unsafe {
        let mut derived: *mut sys::AVBufferRef = ptr::null_mut();
        let ret = sys::av_hwframe_ctx_create_derived(
            &mut derived,
            std::mem::transmute::<i32, sys::AVPixelFormat>(AV_PIX_FMT_VULKAN),
            vulkan_device_ctx.av_hw_device_ctx,
            src_hw_frames_ctx,
            0,
        );
        if ret < 0 {
            return Err(format!("av_hwframe_ctx_create_derived失敗: {ret}"));
        }
        Ok(derived)
    }
}

pub fn transfer_to_vulkan_frame(
    src_frame: *mut sys::AVFrame,
    derived_frames_ctx: *mut sys::AVBufferRef,
) -> Result<DerivedVulkanFrame, String> {
    unsafe {
        let dst_frame = sys::av_frame_alloc();
        if dst_frame.is_null() {
            return Err("av_frame_alloc失敗".to_owned());
        }
        if sys::av_hwframe_get_buffer(derived_frames_ctx, dst_frame, 0) < 0 {
            sys::av_frame_free(&mut { dst_frame });
            return Err("av_hwframe_get_buffer失敗".to_owned());
        }
        if sys::av_hwframe_transfer_data(dst_frame, src_frame, 0) < 0 {
            sys::av_frame_free(&mut { dst_frame });
            return Err("av_hwframe_transfer_data(Vulkan導出)失敗".to_owned());
        }
        Ok(DerivedVulkanFrame {
            av_frame: dst_frame,
        })
    }
}

pub struct VkImageHandle {
    pub image: ash::vk::Image,
    pub layout: ash::vk::ImageLayout,
}

pub unsafe fn vk_image_of(frame: &DerivedVulkanFrame) -> VkImageHandle {
    unsafe {
        let vk_frame = (*frame.av_frame).data[0] as *mut AVVkFrame;
        VkImageHandle {
            image: ash::vk::Image::from_raw((*vk_frame).img[0]),
            layout: ash::vk::ImageLayout::from_raw((*vk_frame).layout[0]),
        }
    }
}

pub struct CopyEngine {
    device: ash::Device,
    queue: ash::vk::Queue,
    command_pool: ash::vk::CommandPool,
    command_buffer: ash::vk::CommandBuffer,
    fence: ash::vk::Fence,
}

impl CopyEngine {
    pub unsafe fn new(handles: &VulkanRawHandles, entry: &ash::Entry) -> Result<Self, String> {
        unsafe {
            let instance = ash::Instance::load(
                &ash::StaticFn {
                    get_instance_proc_addr: handles.get_instance_proc_addr,
                },
                handles.instance,
            );
            let device = ash::Device::load(instance.fp_v1_0(), handles.device);

            let pool_info = ash::vk::CommandPoolCreateInfo::default()
                .queue_family_index(handles.queue_family_index)
                .flags(ash::vk::CommandPoolCreateFlags::TRANSIENT);
            let command_pool = device
                .create_command_pool(&pool_info, None)
                .map_err(|e| format!("create_command_pool失敗: {e}"))?;

            let alloc_info = ash::vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(ash::vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let command_buffer = device
                .allocate_command_buffers(&alloc_info)
                .map_err(|e| format!("allocate_command_buffers失敗: {e}"))?[0];

            let fence_info = ash::vk::FenceCreateInfo::default();
            let fence = device
                .create_fence(&fence_info, None)
                .map_err(|e| format!("create_fence失敗: {e}"))?;

            let _ = entry;
            Ok(Self {
                device,
                queue: handles.queue,
                command_pool,
                command_buffer,
                fence,
            })
        }
    }

    pub unsafe fn copy_image(
        &self,
        src: VkImageHandle,
        dst: ash::vk::Image,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        unsafe {
            self.device
                .reset_command_buffer(
                    self.command_buffer,
                    ash::vk::CommandBufferResetFlags::empty(),
                )
                .map_err(|e| format!("reset_command_buffer失敗: {e}"))?;

            let begin_info = ash::vk::CommandBufferBeginInfo::default()
                .flags(ash::vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(self.command_buffer, &begin_info)
                .map_err(|e| format!("begin_command_buffer失敗: {e}"))?;

            let subresource = ash::vk::ImageSubresourceRange::default()
                .aspect_mask(ash::vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);

            let dst_barrier_to_transfer = ash::vk::ImageMemoryBarrier::default()
                .old_layout(ash::vk::ImageLayout::UNDEFINED)
                .new_layout(ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .image(dst)
                .subresource_range(subresource)
                .dst_access_mask(ash::vk::AccessFlags::TRANSFER_WRITE);

            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                ash::vk::PipelineStageFlags::TOP_OF_PIPE,
                ash::vk::PipelineStageFlags::TRANSFER,
                ash::vk::DependencyFlags::empty(),
                &[],
                &[],
                &[dst_barrier_to_transfer],
            );

            let subresource_layers = ash::vk::ImageSubresourceLayers::default()
                .aspect_mask(ash::vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1);

            let region = ash::vk::ImageCopy::default()
                .src_subresource(subresource_layers)
                .src_offset(ash::vk::Offset3D::default())
                .dst_subresource(subresource_layers)
                .dst_offset(ash::vk::Offset3D::default())
                .extent(ash::vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                });

            self.device.cmd_copy_image(
                self.command_buffer,
                src.image,
                src.layout,
                dst,
                ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );

            let dst_barrier_to_shader = ash::vk::ImageMemoryBarrier::default()
                .old_layout(ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .image(dst)
                .subresource_range(subresource)
                .src_access_mask(ash::vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(ash::vk::AccessFlags::SHADER_READ);

            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                ash::vk::PipelineStageFlags::TRANSFER,
                ash::vk::PipelineStageFlags::FRAGMENT_SHADER,
                ash::vk::DependencyFlags::empty(),
                &[],
                &[],
                &[dst_barrier_to_shader],
            );

            self.device
                .end_command_buffer(self.command_buffer)
                .map_err(|e| format!("end_command_buffer失敗: {e}"))?;

            self.device
                .reset_fences(&[self.fence])
                .map_err(|e| format!("reset_fences失敗: {e}"))?;

            let command_buffers = [self.command_buffer];
            let submit_info = ash::vk::SubmitInfo::default().command_buffers(&command_buffers);
            self.device
                .queue_submit(self.queue, &[submit_info], self.fence)
                .map_err(|e| format!("queue_submit失敗: {e}"))?;

            self.device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .map_err(|e| format!("wait_for_fences失敗: {e}"))?;

            Ok(())
        }
    }
}

impl Drop for CopyEngine {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.command_pool, None);
        }
    }
}

pub struct NeoutlVulkanContext {
    pub device_ctx: NeoutlVulkanDeviceCtx,
    pub copy_engine: CopyEngine,
}

pub fn init_vulkan_context(
    device: &wgpu::Device,
    entry: &ash::Entry,
) -> Result<Arc<NeoutlVulkanContext>, String> {
    let handles = unsafe { extract_vulkan_raw_handles(device) }
        .ok_or_else(|| "Vulkan生ハンドル取得失敗(バックエンド非Vulkan)".to_owned())?;
    let device_ctx = create_av_vulkan_device_ctx(&handles)?;
    let copy_engine = unsafe { CopyEngine::new(&handles, entry)? };
    Ok(Arc::new(NeoutlVulkanContext {
        device_ctx,
        copy_engine,
    }))
}

use ash::vk::Handle;
