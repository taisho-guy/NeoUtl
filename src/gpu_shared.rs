use egui_wgpu::wgpu;
use std::ffi::CStr;
use std::sync::Arc;

pub struct SharedGpu {
    pub instance: wgpu::Instance,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}

const EXTRA_DEVICE_EXTENSIONS: &[&CStr] = &[
    c"VK_EXT_queue_family_foreign",
    c"VK_KHR_external_semaphore",
    c"VK_KHR_external_semaphore_fd",
];

#[cfg(target_os = "linux")]
fn find_graphics_compute_queue_family(
    instance: &ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
) -> Result<u32, String> {
    let props = unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    let wanted = ash::vk::QueueFlags::GRAPHICS | ash::vk::QueueFlags::COMPUTE;
    props
        .iter()
        .enumerate()
        .find(|(_, p)| p.queue_flags.contains(wanted))
        .map(|(i, _)| i as u32)
        .ok_or_else(|| "GRAPHICS+COMPUTE対応キューファミリーが見つからない".to_owned())
}

#[cfg(target_os = "linux")]
fn filter_supported_extra_extensions(
    instance: &ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
) -> Result<Vec<&'static CStr>, String> {
    let supported = unsafe {
        instance
            .enumerate_device_extension_properties(physical_device)
            .map_err(|e| format!("enumerate_device_extension_properties失敗: {e}"))?
    };
    let supported_names: Vec<&CStr> = supported
        .iter()
        .map(|p| p.extension_name_as_c_str().unwrap_or(c""))
        .collect();
    let mut result = Vec::new();
    for ext in EXTRA_DEVICE_EXTENSIONS {
        let found = supported_names.iter().any(|s| *s == *ext);
        eprintln!("[gpu_shared][vulkan] 拡張確認 name={ext:?} 対応={found}");
        if found {
            result.push(*ext);
        }
    }
    Ok(result)
}

pub fn init_shared_gpu() -> Result<SharedGpu, Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });

        let wgpu_adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
            .map_err(|_| "Vulkanアダプタ取得失敗")?;

        let dma_buf_supported = wgpu_adapter
            .features()
            .contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF);
        eprintln!("[gpu_shared] VULKAN_EXTERNAL_MEMORY_DMA_BUFサポート={dma_buf_supported}");
        let wgpu_features = if dma_buf_supported {
            wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF
        } else {
            wgpu::Features::empty()
        };

        let (device, queue) = unsafe {
            let hal_adapter = wgpu_adapter
                .as_hal::<wgpu_hal::api::Vulkan>()
                .ok_or("wgpu AdapterがVulkanバックエンドでない")?;

            let raw_physical_device = hal_adapter.raw_physical_device();
            let shared_instance = hal_adapter.shared_instance();
            let raw_instance = shared_instance.raw_instance();

            let mut required_extensions = hal_adapter.required_device_extensions(wgpu_features);
            let extra = filter_supported_extra_extensions(raw_instance, raw_physical_device)?;
            required_extensions.extend(extra);
            let required_extensions_ptrs: Vec<*const std::os::raw::c_char> =
                required_extensions.iter().map(|e| e.as_ptr()).collect();

            let queue_family_index =
                find_graphics_compute_queue_family(raw_instance, raw_physical_device)?;
            let queue_priorities = [1.0f32];
            let queue_create_info = ash::vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&queue_priorities);
            let queue_create_infos = [queue_create_info];

            let device_create_info = ash::vk::DeviceCreateInfo::default()
                .queue_create_infos(&queue_create_infos)
                .enabled_extension_names(&required_extensions_ptrs);

            let mut physical_device_features =
                hal_adapter.physical_device_features(&required_extensions, wgpu_features);
            let device_create_info =
                physical_device_features.add_to_device_create(device_create_info);

            let raw_device = raw_instance
                .create_device(raw_physical_device, &device_create_info, None)
                .map_err(|e| format!("vkCreateDevice失敗: {e}"))?;

            eprintln!(
                "[gpu_shared][vulkan] VkDevice生成完了 有効化拡張数={} queue_family_index={queue_family_index}",
                required_extensions.len()
            );

            let open_device = hal_adapter
                .device_from_raw(
                    raw_device,
                    None,
                    &required_extensions,
                    wgpu_features,
                    &wgpu::Limits::default(),
                    &wgpu::MemoryHints::default(),
                    queue_family_index,
                    0,
                )
                .map_err(|e| format!("device_from_raw失敗: {e:?}"))?;

            wgpu_adapter
                .create_device_from_hal(
                    open_device,
                    &wgpu::DeviceDescriptor {
                        label: Some("neoutl-shared-device"),
                        required_features: wgpu_features,
                        required_limits: wgpu::Limits::default(),
                        ..Default::default()
                    },
                )
                .map_err(|e| format!("create_device_from_hal失敗: {e}"))?
        };

        crate::renderer::pipeline::install_device_lost_watcher(&device);

        let device = Arc::new(device);
        let queue = Arc::new(queue);
        neo_media_ffmpeg::set_shared_wgpu_device(device.clone(), queue.clone());

        return Ok(SharedGpu {
            instance,
            device,
            queue,
        });
    }

    #[cfg(not(target_os = "linux"))]
    {
        #[cfg(target_os = "macos")]
        let backends = wgpu::Backends::METAL;
        #[cfg(target_os = "windows")]
        let backends = wgpu::Backends::DX12;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|_| "adapter取得失敗")?;

        let mut limits = wgpu::Limits::default();
        limits.max_storage_buffers_per_shader_stage = 1;
        #[cfg(target_os = "macos")]
        let required_features = wgpu::Features::TEXTURE_FORMAT_NV12;
        #[cfg(target_os = "windows")]
        let required_features = wgpu::Features::empty();

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("neoutl-shared-device"),
                required_features,
                required_limits: limits,
                ..Default::default()
            }))?;

        crate::renderer::pipeline::install_device_lost_watcher(&device);

        let _ = adapter;
        Ok(SharedGpu {
            instance,
            device: Arc::new(device),
            queue: Arc::new(queue),
        })
    }
}
