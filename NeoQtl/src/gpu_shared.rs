use std::sync::Arc;

pub struct SharedGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}

impl SharedGpu {
    pub fn backend_kind(&self) -> neoutl_shared_abi::AcceleratorBackend {
        match self.adapter.get_info().backend {
            wgpu::Backend::Vulkan => neoutl_shared_abi::AcceleratorBackend::Vulkan,
            wgpu::Backend::Metal => neoutl_shared_abi::AcceleratorBackend::Metal,
            wgpu::Backend::Dx12 => neoutl_shared_abi::AcceleratorBackend::Dx12,
            _ => neoutl_shared_abi::AcceleratorBackend::Unknown,
        }
    }

    pub fn create_accelerator_handle(&self) -> neoutl_shared_abi::AcceleratorHandle {
        neoutl_shared_abi::AcceleratorHandle::new(
            self.backend_kind(),
            Arc::as_ptr(&self.device) as *const (),
            Arc::as_ptr(&self.queue) as *const (),
        )
    }

    pub fn broadcast_accelerator(&self) {
        let handle = self.create_accelerator_handle();
        crate::effects::broadcast_setup_accelerator(&handle);
        crate::objects::broadcast_setup_accelerator(&handle);
    }
}

pub fn locked_submit(
    queue: &wgpu::Queue,
    buffers: impl IntoIterator<Item = wgpu::CommandBuffer>,
) -> wgpu::SubmissionIndex {
    let lock = neo_media_ffmpeg::shared_wgpu_submit_lock();
    let wait_start = std::time::Instant::now();
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let waited = wait_start.elapsed();
    if waited > std::time::Duration::from_millis(5) {
        eprintln!("[gpu_shared][診断][submit_lock] 描画側待機={waited:?}(競合)");
    }
    queue.submit(buffers)
}

pub fn init_shared_gpu() -> Result<SharedGpu, Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .map_err(|e| format!("wgpu Adapter取得失敗: {e}"))?;

    let mut limits = wgpu::Limits::default();
    limits.max_storage_buffers_per_shader_stage = 1;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("neoutl-shared-device"),
        required_features: wgpu::Features::empty(),
        required_limits: limits,
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))?;

    crate::renderer::pipeline::install_device_lost_watcher(&device);

    let device = Arc::new(device);
    let queue = Arc::new(queue);
    neo_media_ffmpeg::set_shared_wgpu_device(device.clone(), queue.clone());

    let gpu = SharedGpu {
        instance,
        adapter,
        device,
        queue,
    };
    gpu.broadcast_accelerator();
    Ok(gpu)
}
