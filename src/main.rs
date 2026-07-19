mod app_state;
mod config;
mod document;
mod ecs;
mod effects;
mod media;
mod objects;
mod project;
mod renderer;
mod ui;

slint::include_modules!();

fn default_gst_plugin_dir() -> Option<std::path::PathBuf> {
    // linuxビルドでは未使用（システムプラグインパスをそのまま使うため）。
    #[allow(unused_variables)]
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

    #[cfg(target_os = "macos")]
    {
        let dir = exe_dir.join("../Resources/gstreamer-1.0");
        return dir.is_dir().then_some(dir);
    }

    #[cfg(target_os = "windows")]
    {
        let dir = exe_dir.join("lib/gstreamer-1.0");
        return dir.is_dir().then_some(dir);
    }

    #[cfg(target_os = "linux")]
    {
        return None;
    }
}

fn gst_registry_cache_path() -> Option<std::path::PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let dir = exe_dir.join("gst-registry");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("registry.bin"))
}

fn configure_gst_plugin_path() {
    let system_settings = ui::system_settings::load_from_disk().unwrap_or_default();
    media::runtime::set_worker_threads(system_settings.worker_threads);
    media::runtime::apply_decode_backend_env(system_settings.decode_backend);

    unsafe {
        // Linuxはディストリビューションパッケージのシステムプラグイン（va, v4l2codecs等）に
        // 依存するため、GST_PLUGIN_SYSTEM_PATH_1_0を空上書きしない。
        // Windows/macOSはバンドル配布のためシステムパス走査を無効化する。
        #[cfg(not(target_os = "linux"))]
        std::env::set_var("GST_PLUGIN_SYSTEM_PATH_1_0", "");

        if let Some(registry_path) = gst_registry_cache_path()
            && let Some(path_str) = registry_path.to_str()
        {
            std::env::set_var("GST_REGISTRY_1_0", path_str);
        }
    }

    let Some(dir) = default_gst_plugin_dir() else {
        return;
    };
    let Some(dir_str) = dir.to_str() else {
        eprintln!("[NeoUtl] GST_PLUGIN_PATHの解決に失敗（非UTF-8パス）: {dir:?}");
        return;
    };
    unsafe {
        std::env::set_var("GST_PLUGIN_PATH", dir_str);
    }
    eprintln!("[NeoUtl] GST_PLUGIN_PATH設定: {dir_str}");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use slint::ComponentHandle;

    configure_gst_plugin_path();

    objects::load_all(&objects::default_objects_dir());
    effects::load_all(&effects::default_effects_dir());
    media::loader::load_all(&media::loader::default_decoders_dir());

    // 単一デバイス構成: gpu-video側でVulkanInstance/Adapter/Deviceを生成し、
    // SlintのWGPUConfiguration::Manualへ注入する。これによりSlintのcompositing/present
    // とgpuvideo-decoderのデコード・変換が同一wgpu::Device上で動作し、
    // 生成済みテクスチャをslint::Image::try_from()で真のゼロコピー渡しできる。
    // gpu-video(Vulkan専用)はmacOSで使用不可のため、macOSは従来のAutomatic設定を用い、
    // gpuvideo-decoderプラグイン自体もmacOS向けビルドでは無効化スタブとなる
    // （crates/media/gpuvideo-decoder/src/lib.rs参照）。
    #[cfg(not(target_os = "macos"))]
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

        // gpuvideo-decoderプラグインのopen_videoは常時この共有インスタンスを参照する
        // （プラグイン内部でのVulkanInstance再生成を禁止するため、Phase0契約に基づく設定経路）。
        // MediaVTable自体はgpu_video型へ依存させないため、libloading経由の帯域外注入とする。
        media::loader::inject_gpuvideo_shared_device(
            &media::loader::default_decoders_dir(),
            &gpu_device,
        );

        slint::BackendSelector::new()
            .require_wgpu_29(slint::wgpu_29::WGPUConfiguration::Manual {
                instance: gpu_instance.wgpu_instance().clone(),
                adapter: gpu_device.wgpu_adapter().clone(),
                device: gpu_device.wgpu_device().clone(),
                queue: gpu_device.wgpu_queue().clone(),
            })
            .select()?;
    }
    #[cfg(target_os = "macos")]
    {
        let mut wgpu_settings = slint::wgpu_29::WGPUSettings::default();
        wgpu_settings.device_required_features |=
            slint::wgpu_29::wgpu::Features::TEXTURE_FORMAT_NV12;
        wgpu_settings.backends = slint::wgpu_29::wgpu::Backends::METAL;
        slint::BackendSelector::new()
            .require_wgpu_29(slint::wgpu_29::WGPUConfiguration::Automatic(wgpu_settings))
            .select()?;
    }

    let launcher = LauncherWindow::new()?;
    ui::install(&launcher);

    launcher.window().on_close_requested(|| {
        slint::quit_event_loop().ok();
        slint::CloseRequestResponse::HideWindow
    });

    launcher.show()?;
    slint::run_event_loop_until_quit()?;
    Ok(())
}
