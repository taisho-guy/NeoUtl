use egui_wgpu::wgpu;
use std::sync::Arc;

/// winit surface・egui_wgpu::Renderer・RenderEngineの3者へ配る単一のDevice/Queue。
pub struct SharedGpu {
    pub instance: wgpu::Instance,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}

pub fn init_shared_gpu() -> Result<SharedGpu, Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    {
        let gpu_instance =
            gpu_video::VulkanInstance::new().map_err(|e| format!("VulkanInstance生成失敗: {e}"))?;
        let gpu_adapter = gpu_instance
            .create_adapter(&gpu_video::parameters::VulkanAdapterDescriptor {
                compatible_surface: None,
                ..Default::default()
            })
            .map_err(|e| format!("Vulkanアダプタ生成失敗: {e}"))?;
        let gpu_device = gpu_adapter
            .create_device(&gpu_video::parameters::VulkanDeviceDescriptor::default())
            .map_err(|e| format!("Vulkanデバイス生成失敗: {e}"))?;

        let instance = gpu_instance.wgpu_instance().clone();
        let adapter = gpu_device.wgpu_adapter().clone();
        let device = gpu_device.wgpu_device().clone();
        let queue = gpu_device.wgpu_queue().clone();

        crate::media::loader::inject_gpuvideo_shared_device(gpu_device.clone());
        neoutl_media_gpuvideo_encoder::set_shared_device(gpu_device);
        crate::renderer::pipeline::install_device_lost_watcher(&device);

        let _ = adapter;
        return Ok(SharedGpu {
            instance,
            device: Arc::new(device),
            queue: Arc::new(queue),
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
