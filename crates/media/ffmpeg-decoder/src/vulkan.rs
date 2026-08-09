use std::os::raw::{c_int, c_uint, c_void};
use std::ptr;
use std::sync::Arc;

use ffmpeg_sys_next as sys;

const AV_HWDEVICE_TYPE_VULKAN: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN;
const AV_PIX_FMT_VULKAN: i32 = 152;

unsafe extern "C" {
    fn neoutl_vk_configure_device_ctx(
        av_hw_device_ctx: *mut sys::AVBufferRef,
        get_proc_addr: *mut c_void,
        instance: u64,
        phys_dev: u64,
        act_dev: u64,
        queue_family_index: c_uint,
    ) -> c_int;

    fn neoutl_vk_frame_query_image0(
        av_vk_frame: *mut c_void,
        out_image0: *mut u64,
        out_layout0: *mut c_int,
    ) -> c_int;
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

        let configure_ret = neoutl_vk_configure_device_ctx(
            av_hw_device_ctx,
            handles.get_instance_proc_addr as *mut c_void,
            handles.instance.as_raw(),
            handles.physical_device.as_raw(),
            handles.device.as_raw(),
            handles.queue_family_index,
        );
        if configure_ret < 0 {
            sys::av_buffer_unref(&mut { av_hw_device_ctx });
            return Err(format!(
                "neoutl_vk_configure_device_ctx失敗: ret={configure_ret}"
            ));
        }

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
        (*dst_frame).format = AV_PIX_FMT_VULKAN;
        (*dst_frame).hw_frames_ctx = sys::av_buffer_ref(derived_frames_ctx);
        if (*dst_frame).hw_frames_ctx.is_null() {
            sys::av_frame_free(&mut { dst_frame });
            return Err("hw_frames_ctx参照確保失敗".to_owned());
        }
        let map_flags = sys::AV_HWFRAME_MAP_READ as i32 as c_int;
        let map_ret = sys::av_hwframe_map(dst_frame, src_frame, map_flags);
        if map_ret < 0 {
            sys::av_frame_free(&mut { dst_frame });
            return Err(format!(
                "av_hwframe_map失敗(VAAPI→Vulkanゼロコピー導出) ret={map_ret}"
            ));
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
        let vk_frame_ptr = (*frame.av_frame).data[0] as *mut c_void;
        let mut image0: u64 = 0;
        let mut layout0: c_int = 0;
        let ret = neoutl_vk_frame_query_image0(vk_frame_ptr, &mut image0, &mut layout0);
        assert_eq!(ret, 0, "neoutl_vk_frame_query_image0失敗 ret={ret}");
        VkImageHandle {
            image: ash::vk::Image::from_raw(image0),
            layout: ash::vk::ImageLayout::from_raw(layout0),
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
    eprintln!("[neoutl-video-decoder][diag][vulkan] extract_vulkan_raw_handles開始");
    let handles = unsafe { extract_vulkan_raw_handles(device) }
        .ok_or_else(|| "Vulkan生ハンドル取得失敗(バックエンド非Vulkan)".to_owned())?;
    eprintln!("[neoutl-video-decoder][diag][vulkan] raw_handles取得済み");
    eprintln!("[neoutl-video-decoder][diag][vulkan] create_av_vulkan_device_ctx開始");
    let device_ctx = create_av_vulkan_device_ctx(&handles)?;
    eprintln!("[neoutl-video-decoder][diag][vulkan] create_av_vulkan_device_ctx成功");
    eprintln!("[neoutl-video-decoder][diag][vulkan] CopyEngine::new開始");
    let copy_engine = unsafe { CopyEngine::new(&handles, entry)? };
    eprintln!("[neoutl-video-decoder][diag][vulkan] CopyEngine::new成功");
    Ok(Arc::new(NeoutlVulkanContext {
        device_ctx,
        copy_engine,
    }))
}

pub struct SemiPlanarConvertEngine {
    device: ash::Device,
    queue: ash::vk::Queue,
    command_pool: ash::vk::CommandPool,
    command_buffer: ash::vk::CommandBuffer,
    fence: ash::vk::Fence,
    descriptor_set_layout: ash::vk::DescriptorSetLayout,
    pipeline_layout: ash::vk::PipelineLayout,
    pipeline: ash::vk::Pipeline,
    shader_module: ash::vk::ShaderModule,
    descriptor_pool: ash::vk::DescriptorPool,
    sampler: ash::vk::Sampler,
}

impl SemiPlanarConvertEngine {
    pub unsafe fn new(
        handles: &VulkanRawHandles,
        _entry: &ash::Entry,
        spirv_code: &[u8],
    ) -> Result<Self, String> {
        unsafe {
            eprintln!("[neoutl-video-decoder][diag][semi-planar] Instance::load開始");
            let instance = ash::Instance::load(
                &ash::StaticFn {
                    get_instance_proc_addr: handles.get_instance_proc_addr,
                },
                handles.instance,
            );
            eprintln!(
                "[neoutl-video-decoder][diag][semi-planar] Instance::load完了、Device::load開始"
            );
            let device = ash::Device::load(instance.fp_v1_0(), handles.device);
            eprintln!("[neoutl-video-decoder][diag][semi-planar] Device::load完了");

            let pool_info = ash::vk::CommandPoolCreateInfo::default()
                .queue_family_index(handles.queue_family_index)
                .flags(ash::vk::CommandPoolCreateFlags::TRANSIENT);
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_command_pool開始");
            let command_pool = device
                .create_command_pool(&pool_info, None)
                .map_err(|e| format!("create_command_pool失敗: {e}"))?;
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_command_pool完了");

            let alloc_info = ash::vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(ash::vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            eprintln!("[neoutl-video-decoder][diag][semi-planar] allocate_command_buffers開始");
            let command_buffer = device
                .allocate_command_buffers(&alloc_info)
                .map_err(|e| format!("allocate_command_buffers失敗: {e}"))?[0];
            eprintln!("[neoutl-video-decoder][diag][semi-planar] allocate_command_buffers完了");

            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_fence開始");
            let fence = device
                .create_fence(&ash::vk::FenceCreateInfo::default(), None)
                .map_err(|e| format!("create_fence失敗: {e}"))?;
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_fence完了");

            let sampler_info = ash::vk::SamplerCreateInfo::default()
                .mag_filter(ash::vk::Filter::LINEAR)
                .min_filter(ash::vk::Filter::LINEAR)
                .address_mode_u(ash::vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(ash::vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(ash::vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .mipmap_mode(ash::vk::SamplerMipmapMode::NEAREST);
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_sampler開始");
            let sampler = device
                .create_sampler(&sampler_info, None)
                .map_err(|e| format!("create_sampler失敗: {e}"))?;
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_sampler完了");

            let bindings = [
                ash::vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(ash::vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(ash::vk::ShaderStageFlags::COMPUTE),
                ash::vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(ash::vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(ash::vk::ShaderStageFlags::COMPUTE),
                ash::vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(ash::vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(ash::vk::ShaderStageFlags::COMPUTE),
                ash::vk::DescriptorSetLayoutBinding::default()
                    .binding(3)
                    .descriptor_type(ash::vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(ash::vk::ShaderStageFlags::COMPUTE),
            ];
            let layout_info = ash::vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_descriptor_set_layout開始");
            let descriptor_set_layout = device
                .create_descriptor_set_layout(&layout_info, None)
                .map_err(|e| format!("create_descriptor_set_layout失敗: {e}"))?;
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_descriptor_set_layout完了");

            let set_layouts = [descriptor_set_layout];
            let pipeline_layout_info =
                ash::vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_pipeline_layout開始");
            let pipeline_layout = device
                .create_pipeline_layout(&pipeline_layout_info, None)
                .map_err(|e| format!("create_pipeline_layout失敗: {e}"))?;
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_pipeline_layout完了");

            if spirv_code.len() % 4 != 0 {
                return Err("SPIR-Vバイト列長が4の倍数でない".to_owned());
            }
            let spirv_words: Vec<u32> = spirv_code
                .chunks_exact(4)
                .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            eprintln!(
                "[neoutl-video-decoder][diag][semi-planar] SPIR-Vワード数={} 先頭magic={:#010x}",
                spirv_words.len(),
                spirv_words.first().copied().unwrap_or(0)
            );
            let shader_info = ash::vk::ShaderModuleCreateInfo::default().code(&spirv_words);
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_shader_module開始");
            let shader_module = device
                .create_shader_module(&shader_info, None)
                .map_err(|e| format!("create_shader_module失敗: {e}"))?;
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_shader_module完了");

            let entry_name = c"main";
            let stage_info = ash::vk::PipelineShaderStageCreateInfo::default()
                .stage(ash::vk::ShaderStageFlags::COMPUTE)
                .module(shader_module)
                .name(entry_name);
            let pipeline_info = ash::vk::ComputePipelineCreateInfo::default()
                .stage(stage_info)
                .layout(pipeline_layout);
            eprintln!(
                "[neoutl-video-decoder][diag][semi-planar] create_compute_pipelines開始 pipeline_layout={pipeline_layout:?} shader_module={shader_module:?}"
            );
            let pipeline = device
                .create_compute_pipelines(ash::vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|(_, e)| format!("create_compute_pipelines失敗: {e}"))?[0];
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_compute_pipelines完了");

            let pool_sizes = [
                ash::vk::DescriptorPoolSize::default()
                    .ty(ash::vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(2),
                ash::vk::DescriptorPoolSize::default()
                    .ty(ash::vk::DescriptorType::SAMPLER)
                    .descriptor_count(1),
                ash::vk::DescriptorPoolSize::default()
                    .ty(ash::vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(1),
            ];
            let descriptor_pool_info = ash::vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&pool_sizes);
            eprintln!("[neoutl-video-decoder][diag][semi-planar] create_descriptor_pool開始");
            let descriptor_pool = device
                .create_descriptor_pool(&descriptor_pool_info, None)
                .map_err(|e| format!("create_descriptor_pool失敗: {e}"))?;
            eprintln!(
                "[neoutl-video-decoder][diag][semi-planar] create_descriptor_pool完了、Self構築"
            );

            Ok(Self {
                device,
                queue: handles.queue,
                command_pool,
                command_buffer,
                fence,
                descriptor_set_layout,
                pipeline_layout,
                pipeline,
                shader_module,
                descriptor_pool,
                sampler,
            })
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub unsafe fn convert(
        &self,
        src_image: ash::vk::Image,
        src_layout: ash::vk::ImageLayout,
        dst_image: ash::vk::Image,
        width: u32,
        height: u32,
        y_plane_format: ash::vk::Format,
        uv_plane_format: ash::vk::Format,
    ) -> Result<(), String> {
        unsafe {
            let y_view_info = ash::vk::ImageViewCreateInfo::default()
                .image(src_image)
                .view_type(ash::vk::ImageViewType::TYPE_2D)
                .format(y_plane_format)
                .subresource_range(
                    ash::vk::ImageSubresourceRange::default()
                        .aspect_mask(ash::vk::ImageAspectFlags::PLANE_0)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            let y_view = self
                .device
                .create_image_view(&y_view_info, None)
                .map_err(|e| format!("Yプレーンビュー生成失敗: {e}"))?;

            let uv_view_info = ash::vk::ImageViewCreateInfo::default()
                .image(src_image)
                .view_type(ash::vk::ImageViewType::TYPE_2D)
                .format(uv_plane_format)
                .subresource_range(
                    ash::vk::ImageSubresourceRange::default()
                        .aspect_mask(ash::vk::ImageAspectFlags::PLANE_1)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            let uv_view = self
                .device
                .create_image_view(&uv_view_info, None)
                .map_err(|e| format!("UVプレーンビュー生成失敗: {e}"))?;

            let dst_view_info = ash::vk::ImageViewCreateInfo::default()
                .image(dst_image)
                .view_type(ash::vk::ImageViewType::TYPE_2D)
                .format(ash::vk::Format::R8G8B8A8_UNORM)
                .subresource_range(
                    ash::vk::ImageSubresourceRange::default()
                        .aspect_mask(ash::vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            let dst_view = self
                .device
                .create_image_view(&dst_view_info, None)
                .map_err(|e| format!("出力ビュー生成失敗: {e}"))?;

            let set_layouts = [self.descriptor_set_layout];
            let alloc_info = ash::vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(self.descriptor_pool)
                .set_layouts(&set_layouts);
            let descriptor_set = self
                .device
                .allocate_descriptor_sets(&alloc_info)
                .map_err(|e| format!("allocate_descriptor_sets失敗: {e}"))?[0];

            let y_image_info = [ash::vk::DescriptorImageInfo::default()
                .image_view(y_view)
                .image_layout(ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let uv_image_info = [ash::vk::DescriptorImageInfo::default()
                .image_view(uv_view)
                .image_layout(ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let sampler_info = [ash::vk::DescriptorImageInfo::default().sampler(self.sampler)];
            let dst_image_info = [ash::vk::DescriptorImageInfo::default()
                .image_view(dst_view)
                .image_layout(ash::vk::ImageLayout::GENERAL)];

            let writes = [
                ash::vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(ash::vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&y_image_info),
                ash::vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(ash::vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&uv_image_info),
                ash::vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(2)
                    .descriptor_type(ash::vk::DescriptorType::SAMPLER)
                    .image_info(&sampler_info),
                ash::vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(3)
                    .descriptor_type(ash::vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&dst_image_info),
            ];
            self.device.update_descriptor_sets(&writes, &[]);

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

            let src_subresource = ash::vk::ImageSubresourceRange::default()
                .aspect_mask(
                    ash::vk::ImageAspectFlags::PLANE_0 | ash::vk::ImageAspectFlags::PLANE_1,
                )
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let src_barrier = ash::vk::ImageMemoryBarrier::default()
                .old_layout(src_layout)
                .new_layout(ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .image(src_image)
                .subresource_range(src_subresource)
                .dst_access_mask(ash::vk::AccessFlags::SHADER_READ);

            let dst_subresource = ash::vk::ImageSubresourceRange::default()
                .aspect_mask(ash::vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let dst_barrier_to_general = ash::vk::ImageMemoryBarrier::default()
                .old_layout(ash::vk::ImageLayout::UNDEFINED)
                .new_layout(ash::vk::ImageLayout::GENERAL)
                .src_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .image(dst_image)
                .subresource_range(dst_subresource)
                .dst_access_mask(ash::vk::AccessFlags::SHADER_WRITE);

            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                ash::vk::PipelineStageFlags::TOP_OF_PIPE,
                ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                ash::vk::DependencyFlags::empty(),
                &[],
                &[],
                &[src_barrier, dst_barrier_to_general],
            );

            self.device.cmd_bind_pipeline(
                self.command_buffer,
                ash::vk::PipelineBindPoint::COMPUTE,
                self.pipeline,
            );
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                ash::vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[descriptor_set],
                &[],
            );
            self.device.cmd_dispatch(
                self.command_buffer,
                width.div_ceil(8),
                height.div_ceil(8),
                1,
            );

            let dst_barrier_to_shader_read = ash::vk::ImageMemoryBarrier::default()
                .old_layout(ash::vk::ImageLayout::GENERAL)
                .new_layout(ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .image(dst_image)
                .subresource_range(dst_subresource)
                .src_access_mask(ash::vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(ash::vk::AccessFlags::SHADER_READ);
            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                ash::vk::PipelineStageFlags::FRAGMENT_SHADER,
                ash::vk::DependencyFlags::empty(),
                &[],
                &[],
                &[dst_barrier_to_shader_read],
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

            self.device.destroy_image_view(y_view, None);
            self.device.destroy_image_view(uv_view, None);
            self.device.destroy_image_view(dst_view, None);
            self.device
                .free_descriptor_sets(self.descriptor_pool, &[descriptor_set])
                .map_err(|e| format!("free_descriptor_sets失敗: {e}"))?;

            Ok(())
        }
    }
}

impl Drop for SemiPlanarConvertEngine {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_sampler(self.sampler, None);
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_shader_module(self.shader_module, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_command_pool(self.command_pool, None);
        }
    }
}

use ash::vk::Handle;
