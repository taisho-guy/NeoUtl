use std::collections::HashMap;
use std::os::raw::{c_int, c_uint, c_void};
use std::ptr;
use std::sync::{Arc, Mutex};

use ffmpeg_sys_next as sys;

const AV_HWDEVICE_TYPE_VULKAN: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN;

unsafe extern "C" {
    fn neoutl_vk_configure_device_ctx(
        av_hw_device_ctx: *mut sys::AVBufferRef,
        get_proc_addr: *mut c_void,
        instance: u64,
        phys_dev: u64,
        act_dev: u64,
        queue_family_index: c_uint,
        enabled_inst_extensions: *const *const std::os::raw::c_char,
        nb_enabled_inst_extensions: c_int,
        enabled_dev_extensions: *const *const std::os::raw::c_char,
        nb_enabled_dev_extensions: c_int,
    ) -> c_int;

    fn neoutl_vaapi_sync_surface(vaapi_frame: *mut sys::AVFrame) -> c_int;

    fn neoutl_vaapi_export_surface_drm(
        vaapi_frame: *mut sys::AVFrame,
        out_fourcc: *mut u32,
        out_width: *mut u32,
        out_height: *mut u32,
        out_fd: *mut c_int,
        out_drm_format_modifier: *mut u64,
        out_plane_count: *mut u32,
        out_plane_offset: *mut u32,
        out_plane_pitch: *mut u32,
        out_plane_format: *mut u32,
    ) -> c_int;
}

pub unsafe fn neoutl_vaapi_sync_surface_safe(vaapi_frame: *mut sys::AVFrame) -> c_int {
    unsafe { neoutl_vaapi_sync_surface(vaapi_frame) }
}

pub struct VulkanRawHandles {
    pub instance: ash::vk::Instance,
    pub physical_device: ash::vk::PhysicalDevice,
    pub device: ash::vk::Device,
    pub queue: ash::vk::Queue,
    pub queue_family_index: u32,
    pub get_instance_proc_addr: ash::vk::PFN_vkGetInstanceProcAddr,
    pub device_extensions: Vec<&'static std::ffi::CStr>,
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
            device_extensions: hal_device.enabled_device_extensions().to_vec(),
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

        let dev_ext_ptrs: Vec<*const std::os::raw::c_char> = handles
            .device_extensions
            .iter()
            .map(|name| name.as_ptr())
            .collect();

        let configure_ret = neoutl_vk_configure_device_ctx(
            av_hw_device_ctx,
            handles.get_instance_proc_addr as *mut c_void,
            handles.instance.as_raw(),
            handles.physical_device.as_raw(),
            handles.device.as_raw(),
            handles.queue_family_index,
            ptr::null(),
            0,
            dev_ext_ptrs.as_ptr(),
            dev_ext_ptrs.len() as c_int,
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

        log_enabled_vulkan_extensions(av_hw_device_ctx);

        Ok(NeoutlVulkanDeviceCtx { av_hw_device_ctx })
    }
}

unsafe fn log_enabled_vulkan_extensions(av_hw_device_ctx: *mut sys::AVBufferRef) {
    unsafe {
        let device_ctx = (*av_hw_device_ctx).data as *mut sys::AVHWDeviceContext;
        let vk_ctx = (*device_ctx).hwctx as *mut sys::AVVulkanDeviceContext;
        let names: Vec<String> = (0..(*vk_ctx).nb_enabled_dev_extensions)
            .map(|i| {
                let ptr = *(*vk_ctx).enabled_dev_extensions.offset(i as isize);
                std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
            })
            .collect();
        eprintln!(
            "[neoutl-video-decoder][diag][vulkan] 有効化デバイス拡張(count={}): {:?}",
            (*vk_ctx).nb_enabled_dev_extensions,
            names
        );
        let drm_related: Vec<&String> = names
            .iter()
            .filter(|n| n.contains("external_memory") || n.contains("drm_format_modifier"))
            .collect();
        eprintln!(
            "[neoutl-video-decoder][diag][vulkan] DRM/external_memory関連拡張: {:?}",
            drm_related
        );
    }
}

pub struct CachedVkSurface {
    device: ash::Device,
    pub image: ash::vk::Image,
    memory: ash::vk::DeviceMemory,
    pub layout: ash::vk::ImageLayout,
}

impl Drop for CachedVkSurface {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

fn vk_format_for_surface(
    sw_format_i32: i32,
    is_direct_rgba: bool,
) -> Result<ash::vk::Format, String> {
    use ash::vk::Format;

    fn as_i32(fmt: sys::AVPixelFormat) -> i32 {
        fmt as i32
    }

    if is_direct_rgba {
        return Ok(Format::B8G8R8A8_UNORM);
    }
    if sw_format_i32 == as_i32(sys::AVPixelFormat::AV_PIX_FMT_NV12) {
        Ok(Format::G8_B8R8_2PLANE_420_UNORM)
    } else if sw_format_i32 == as_i32(sys::AVPixelFormat::AV_PIX_FMT_P010LE)
        || sw_format_i32 == as_i32(sys::AVPixelFormat::AV_PIX_FMT_P012LE)
        || sw_format_i32 == as_i32(sys::AVPixelFormat::AV_PIX_FMT_P016LE)
    {
        Ok(Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16)
    } else {
        Err(format!("未対応sw_format: {sw_format_i32}"))
    }
}

unsafe fn find_dma_buf_memory_type(
    instance: &ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
    memory_type_bits: u32,
) -> Result<u32, String> {
    unsafe {
        let props = instance.get_physical_device_memory_properties(physical_device);
        for i in 0..props.memory_type_count {
            if memory_type_bits & (1 << i) != 0 {
                return Ok(i);
            }
        }
        Err("dma-buf importに適合するVkMemoryTypeが見つからない".to_owned())
    }
}

pub unsafe fn import_surface_once(
    handles: &VulkanRawHandles,
    instance: &ash::Instance,
    device: &ash::Device,
    src_frame: *mut sys::AVFrame,
    sw_format_i32: i32,
    is_direct_rgba: bool,
) -> Result<CachedVkSurface, String> {
    unsafe {
        let mut fourcc: u32 = 0;
        let mut width: u32 = 0;
        let mut height: u32 = 0;
        let mut fd: c_int = -1;
        let mut modifier: u64 = 0;
        let mut plane_count: u32 = 0;
        let mut plane_offset = [0u32; 4];
        let mut plane_pitch = [0u32; 4];
        let mut plane_format: u32 = 0;

        let export_ret = neoutl_vaapi_export_surface_drm(
            src_frame,
            &mut fourcc,
            &mut width,
            &mut height,
            &mut fd,
            &mut modifier,
            &mut plane_count,
            plane_offset.as_mut_ptr(),
            plane_pitch.as_mut_ptr(),
            &mut plane_format,
        );
        if export_ret != 0 {
            return Err(format!(
                "neoutl_vaapi_export_surface_drm失敗 ret={export_ret}"
            ));
        }
        let _ = fourcc;
        let _ = plane_format;

        let format = vk_format_for_surface(sw_format_i32, is_direct_rgba).map_err(|e| {
            if fd >= 0 {
                libc_close(fd);
            }
            e
        })?;

        let mut plane_layouts = Vec::with_capacity(plane_count as usize);
        for i in 0..plane_count as usize {
            plane_layouts.push(
                ash::vk::SubresourceLayout::default()
                    .offset(plane_offset[i] as u64)
                    .row_pitch(plane_pitch[i] as u64),
            );
        }

        let mut modifier_info = ash::vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(modifier)
            .plane_layouts(&plane_layouts);

        let mut external_memory_info = ash::vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(ash::vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

        let image_info = ash::vk::ImageCreateInfo::default()
            .image_type(ash::vk::ImageType::TYPE_2D)
            .format(format)
            .extent(ash::vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(ash::vk::SampleCountFlags::TYPE_1)
            .tiling(ash::vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(ash::vk::ImageUsageFlags::TRANSFER_SRC | ash::vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(ash::vk::SharingMode::EXCLUSIVE)
            .initial_layout(ash::vk::ImageLayout::UNDEFINED)
            .push_next(&mut modifier_info)
            .push_next(&mut external_memory_info);

        let image = device.create_image(&image_info, None).map_err(|e| {
            libc_close(fd);
            format!("create_image(DRM import)失敗: {e}")
        })?;

        let mem_req = device.get_image_memory_requirements(image);

        let mut fd_props = ash::vk::MemoryFdPropertiesKHR::default();
        let ext_fd_fn = ash::khr::external_memory_fd::Device::new(instance, device);
        ext_fd_fn
            .get_memory_fd_properties(
                ash::vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                fd,
                &mut fd_props,
            )
            .map_err(|e| {
                device.destroy_image(image, None);
                libc_close(fd);
                format!("vkGetMemoryFdPropertiesKHR失敗: {e}")
            })?;

        let memory_type_index = find_dma_buf_memory_type(
            instance,
            handles.physical_device,
            mem_req.memory_type_bits & fd_props.memory_type_bits,
        )
        .map_err(|e| {
            device.destroy_image(image, None);
            libc_close(fd);
            e
        })?;

        let mut import_info = ash::vk::ImportMemoryFdInfoKHR::default()
            .handle_type(ash::vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(fd);
        let mut dedicated_info = ash::vk::MemoryDedicatedAllocateInfo::default().image(image);
        let alloc_info = ash::vk::MemoryAllocateInfo::default()
            .allocation_size(mem_req.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut dedicated_info)
            .push_next(&mut import_info);

        let memory = device.allocate_memory(&alloc_info, None).map_err(|e| {
            device.destroy_image(image, None);
            libc_close(fd);
            format!("dma-buf importのallocate_memory失敗: {e}")
        })?;

        device.bind_image_memory(image, memory, 0).map_err(|e| {
            device.destroy_image(image, None);
            device.free_memory(memory, None);
            format!("bind_image_memory失敗: {e}")
        })?;

        Ok(CachedVkSurface {
            device: device.clone(),
            image,
            memory,
            layout: ash::vk::ImageLayout::UNDEFINED,
        })
    }
}

unsafe fn libc_close(fd: c_int) {
    unsafe extern "C" {
        fn close(fd: c_int) -> c_int;
    }
    if fd >= 0 {
        unsafe {
            close(fd);
        }
    }
}

pub struct VkSurfaceCache {
    entries: HashMap<u32, CachedVkSurface>,
}

impl VkSurfaceCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub unsafe fn get_or_import(
        &mut self,
        handles: &VulkanRawHandles,
        instance: &ash::Instance,
        device: &ash::Device,
        src_frame: *mut sys::AVFrame,
        sw_format_i32: i32,
        is_direct_rgba: bool,
    ) -> Result<(ash::vk::Image, ash::vk::ImageLayout), String> {
        unsafe {
            let surface_id = (*src_frame).data[3] as usize as u32;
            if let Some(cached) = self.entries.get(&surface_id) {
                return Ok((cached.image, cached.layout));
            }
            let cached = import_surface_once(
                handles,
                instance,
                device,
                src_frame,
                sw_format_i32,
                is_direct_rgba,
            )?;
            let handle = (cached.image, cached.layout);
            self.entries.insert(surface_id, cached);
            Ok(handle)
        }
    }

    pub fn update_layout(
        &mut self,
        src_frame: *mut sys::AVFrame,
        new_layout: ash::vk::ImageLayout,
    ) {
        let surface_id = unsafe { (*src_frame).data[3] as usize as u32 };
        if let Some(cached) = self.entries.get_mut(&surface_id) {
            cached.layout = new_layout;
        }
    }
}

impl Default for VkSurfaceCache {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CopyEngine {
    device: ash::Device,
    queue: ash::vk::Queue,
    command_pool: ash::vk::CommandPool,
    command_buffer: ash::vk::CommandBuffer,
    fence: ash::vk::Fence,
    submit_lock: Arc<Mutex<()>>,
}

impl CopyEngine {
    pub unsafe fn new(
        handles: &VulkanRawHandles,
        entry: &ash::Entry,
        submit_lock: Arc<Mutex<()>>,
    ) -> Result<Self, String> {
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
                submit_lock,
            })
        }
    }

    pub unsafe fn copy_image(
        &self,
        src_image: ash::vk::Image,
        src_layout: ash::vk::ImageLayout,
        dst: ash::vk::Image,
        width: u32,
        height: u32,
    ) -> Result<ash::vk::ImageLayout, String> {
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

            let src_barrier_acquire = ash::vk::ImageMemoryBarrier::default()
                .old_layout(src_layout)
                .new_layout(ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .src_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .image(src_image)
                .subresource_range(subresource)
                .dst_access_mask(ash::vk::AccessFlags::TRANSFER_READ);

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
                &[src_barrier_acquire, dst_barrier_to_transfer],
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
                src_image,
                ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
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

            let new_src_layout = ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
            let src_barrier_keep = ash::vk::ImageMemoryBarrier::default()
                .old_layout(ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(new_src_layout)
                .src_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .image(src_image)
                .subresource_range(subresource)
                .src_access_mask(ash::vk::AccessFlags::TRANSFER_READ);

            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                ash::vk::PipelineStageFlags::TRANSFER,
                ash::vk::PipelineStageFlags::FRAGMENT_SHADER
                    | ash::vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                ash::vk::DependencyFlags::empty(),
                &[],
                &[],
                &[dst_barrier_to_shader, src_barrier_keep],
            );

            self.device
                .end_command_buffer(self.command_buffer)
                .map_err(|e| format!("end_command_buffer失敗: {e}"))?;

            self.device
                .reset_fences(&[self.fence])
                .map_err(|e| format!("reset_fences失敗: {e}"))?;

            let command_buffers = [self.command_buffer];
            let submit_info = ash::vk::SubmitInfo::default().command_buffers(&command_buffers);
            {
                let wait_start = std::time::Instant::now();
                let _guard = self
                    .submit_lock
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let waited = wait_start.elapsed();
                if waited > std::time::Duration::from_millis(5) {
                    eprintln!(
                        "[neoutl-video-decoder][診断][submit_lock] CopyEngine待機={waited:?}(競合)"
                    );
                }
                self.device
                    .queue_submit(self.queue, &[submit_info], self.fence)
                    .map_err(|e| format!("queue_submit失敗: {e}"))?;
            }

            self.device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .map_err(|e| format!("wait_for_fences失敗: {e}"))?;

            Ok(new_src_layout)
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
    pub instance: ash::Instance,
    pub device: ash::Device,
}

pub fn init_vulkan_context(
    device_wgpu: &wgpu::Device,
    entry: &ash::Entry,
    submit_lock: Arc<Mutex<()>>,
) -> Result<Arc<NeoutlVulkanContext>, String> {
    eprintln!("[neoutl-video-decoder][diag][vulkan] extract_vulkan_raw_handles開始");
    let handles = unsafe { extract_vulkan_raw_handles(device_wgpu) }
        .ok_or_else(|| "Vulkan生ハンドル取得失敗(バックエンド非Vulkan)".to_owned())?;
    eprintln!("[neoutl-video-decoder][diag][vulkan] raw_handles取得済み");
    eprintln!("[neoutl-video-decoder][diag][vulkan] create_av_vulkan_device_ctx開始");
    let device_ctx = create_av_vulkan_device_ctx(&handles)?;
    eprintln!("[neoutl-video-decoder][diag][vulkan] create_av_vulkan_device_ctx成功");
    eprintln!("[neoutl-video-decoder][diag][vulkan] CopyEngine::new開始");
    let copy_engine = unsafe { CopyEngine::new(&handles, entry, submit_lock)? };
    eprintln!("[neoutl-video-decoder][diag][vulkan] CopyEngine::new成功");

    let instance = unsafe {
        ash::Instance::load(
            &ash::StaticFn {
                get_instance_proc_addr: handles.get_instance_proc_addr,
            },
            handles.instance,
        )
    };
    let device = unsafe { ash::Device::load(instance.fp_v1_0(), handles.device) };

    Ok(Arc::new(NeoutlVulkanContext {
        device_ctx,
        copy_engine,
        instance,
        device,
    }))
}

pub struct SemiPlanarConvertEngine {
    device: ash::Device,
    queue: ash::vk::Queue,
    queue_family_index: u32,
    command_pool: ash::vk::CommandPool,
    command_buffer: ash::vk::CommandBuffer,
    fence: ash::vk::Fence,
    descriptor_set_layout: ash::vk::DescriptorSetLayout,
    pipeline_layout: ash::vk::PipelineLayout,
    pipeline: ash::vk::Pipeline,
    shader_module: ash::vk::ShaderModule,
    descriptor_pool: ash::vk::DescriptorPool,
    sampler: ash::vk::Sampler,
    submit_lock: Arc<Mutex<()>>,
}

impl SemiPlanarConvertEngine {
    pub unsafe fn new(
        handles: &VulkanRawHandles,
        _entry: &ash::Entry,
        spirv_code: &[u8],
        submit_lock: Arc<Mutex<()>>,
    ) -> Result<Self, String> {
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

            let fence = device
                .create_fence(&ash::vk::FenceCreateInfo::default(), None)
                .map_err(|e| format!("create_fence失敗: {e}"))?;

            let sampler_info = ash::vk::SamplerCreateInfo::default()
                .mag_filter(ash::vk::Filter::LINEAR)
                .min_filter(ash::vk::Filter::LINEAR)
                .address_mode_u(ash::vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(ash::vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(ash::vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .mipmap_mode(ash::vk::SamplerMipmapMode::NEAREST);
            let sampler = device
                .create_sampler(&sampler_info, None)
                .map_err(|e| format!("create_sampler失敗: {e}"))?;

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
            let descriptor_set_layout = device
                .create_descriptor_set_layout(&layout_info, None)
                .map_err(|e| format!("create_descriptor_set_layout失敗: {e}"))?;

            let set_layouts = [descriptor_set_layout];
            let pipeline_layout_info =
                ash::vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
            let pipeline_layout = device
                .create_pipeline_layout(&pipeline_layout_info, None)
                .map_err(|e| format!("create_pipeline_layout失敗: {e}"))?;

            if spirv_code.len() % 4 != 0 {
                return Err("SPIR-Vバイト列長が4の倍数でない".to_owned());
            }
            let spirv_words: Vec<u32> = spirv_code
                .chunks_exact(4)
                .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let shader_info = ash::vk::ShaderModuleCreateInfo::default().code(&spirv_words);
            let shader_module = device
                .create_shader_module(&shader_info, None)
                .map_err(|e| format!("create_shader_module失敗: {e}"))?;

            let entry_name = c"main";
            let stage_info = ash::vk::PipelineShaderStageCreateInfo::default()
                .stage(ash::vk::ShaderStageFlags::COMPUTE)
                .module(shader_module)
                .name(entry_name);
            let pipeline_info = ash::vk::ComputePipelineCreateInfo::default()
                .stage(stage_info)
                .layout(pipeline_layout);
            let pipeline = device
                .create_compute_pipelines(ash::vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|(_, e)| format!("create_compute_pipelines失敗: {e}"))?[0];

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
                .flags(ash::vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
                .pool_sizes(&pool_sizes);
            let descriptor_pool = device
                .create_descriptor_pool(&descriptor_pool_info, None)
                .map_err(|e| format!("create_descriptor_pool失敗: {e}"))?;

            Ok(Self {
                device,
                queue: handles.queue,
                queue_family_index: handles.queue_family_index,
                command_pool,
                command_buffer,
                fence,
                descriptor_set_layout,
                pipeline_layout,
                pipeline,
                shader_module,
                descriptor_pool,
                sampler,
                submit_lock,
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
    ) -> Result<ash::vk::ImageLayout, String> {
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
            let new_src_layout = ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
            let src_barrier_keep = ash::vk::ImageMemoryBarrier::default()
                .old_layout(ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(new_src_layout)
                .src_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(ash::vk::QUEUE_FAMILY_IGNORED)
                .image(src_image)
                .subresource_range(src_subresource)
                .src_access_mask(ash::vk::AccessFlags::SHADER_READ);
            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                ash::vk::PipelineStageFlags::FRAGMENT_SHADER
                    | ash::vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                ash::vk::DependencyFlags::empty(),
                &[],
                &[],
                &[dst_barrier_to_shader_read, src_barrier_keep],
            );

            self.device
                .end_command_buffer(self.command_buffer)
                .map_err(|e| format!("end_command_buffer失敗: {e}"))?;
            self.device
                .reset_fences(&[self.fence])
                .map_err(|e| format!("reset_fences失敗: {e}"))?;

            let command_buffers = [self.command_buffer];
            let submit_info = ash::vk::SubmitInfo::default().command_buffers(&command_buffers);
            {
                let wait_start = std::time::Instant::now();
                let _guard = self
                    .submit_lock
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let waited = wait_start.elapsed();
                if waited > std::time::Duration::from_millis(5) {
                    eprintln!(
                        "[neoutl-video-decoder][診断][submit_lock] SemiPlanarConvertEngine待機={waited:?}(競合)"
                    );
                }
                self.device
                    .queue_submit(self.queue, &[submit_info], self.fence)
                    .map_err(|e| format!("queue_submit失敗: {e}"))?;
            }
            self.device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .map_err(|e| format!("wait_for_fences失敗: {e}"))?;

            self.device.destroy_image_view(y_view, None);
            self.device.destroy_image_view(uv_view, None);
            self.device.destroy_image_view(dst_view, None);
            self.device
                .free_descriptor_sets(self.descriptor_pool, &[descriptor_set])
                .map_err(|e| format!("free_descriptor_sets失敗: {e}"))?;

            let _ = self.queue_family_index;
            Ok(new_src_layout)
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

const VRAM_BUDGET_SAFETY_HEAP_INDEX_LIMIT: u32 = 16;

unsafe fn sum_device_local_heaps(
    instance: &ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
) -> u64 {
    unsafe {
        let props = instance.get_physical_device_memory_properties(physical_device);
        props
            .memory_heaps
            .iter()
            .take(
                props
                    .memory_heap_count
                    .min(VRAM_BUDGET_SAFETY_HEAP_INDEX_LIMIT) as usize,
            )
            .filter(|heap| heap.flags.contains(ash::vk::MemoryHeapFlags::DEVICE_LOCAL))
            .map(|heap| heap.size)
            .sum()
    }
}

unsafe fn sum_device_local_heap_budgets(
    instance: &ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
) -> u64 {
    unsafe {
        let mem_props = instance.get_physical_device_memory_properties(physical_device);
        let mut budget_props = ash::vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
        let mut mem_props2 =
            ash::vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget_props);
        instance.get_physical_device_memory_properties2(physical_device, &mut mem_props2);
        (0..mem_props
            .memory_heap_count
            .min(VRAM_BUDGET_SAFETY_HEAP_INDEX_LIMIT) as usize)
            .filter(|&i| {
                mem_props.memory_heaps[i]
                    .flags
                    .contains(ash::vk::MemoryHeapFlags::DEVICE_LOCAL)
            })
            .map(|i| budget_props.heap_budget[i])
            .sum()
    }
}

pub unsafe fn query_vram_budget_bytes(device: &wgpu::Device) -> Option<u64> {
    unsafe {
        let handles = extract_vulkan_raw_handles(device)?;
        let instance = ash::Instance::load(
            &ash::StaticFn {
                get_instance_proc_addr: handles.get_instance_proc_addr,
            },
            handles.instance,
        );
        let budget_ext_enabled = handles
            .device_extensions
            .iter()
            .any(|ext| ext.to_bytes() == ash::ext::memory_budget::NAME.to_bytes());
        let bytes = if budget_ext_enabled {
            sum_device_local_heap_budgets(&instance, handles.physical_device)
        } else {
            sum_device_local_heaps(&instance, handles.physical_device)
        };
        Some(bytes)
    }
}
