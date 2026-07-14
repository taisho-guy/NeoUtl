// src/main.rs
mod app_state;
mod document;
mod ecs;
mod effects;
mod media;
mod objects;
mod project;
mod renderer;
mod ui;

slint::include_modules!();

/// バンドル同梱GStreamerプラグインの実行時パスを解決する。
/// objects::default_objects_dir / effects::default_effects_dirと同一の
/// current_exe基準アルゴリズムに統一する。
///
/// - macOS: Contents/MacOS/NeoUtl から Contents/Resources/gstreamer-1.0 を参照
/// - Windows: NeoUtl.exe と同階層の lib/gstreamer-1.0 を参照
/// - Linux: 同梱せずシステム側gstreamer1.0-plugins-*に委ねるためNoneを返す
fn default_gst_plugin_dir() -> Option<std::path::PathBuf> {
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

/// GST_PLUGIN_PATHをバンドル同梱ディレクトリへ固定し、GST_PLUGIN_SYSTEM_PATH_1_0を
/// 空文字化することでホスト側にインストール済みのGStreamerプラグインとのバージョン
/// 不整合を排除する。gstreamer-decoderクレートの初回gst::init()呼び出しより前に
/// 実行する必要があるため、main()冒頭で行う。
fn configure_gst_plugin_path() {
    let Some(dir) = default_gst_plugin_dir() else {
        return;
    };
    let Some(dir_str) = dir.to_str() else {
        eprintln!("[NeoUtl] GST_PLUGIN_PATHの解決に失敗（非UTF-8パス）: {dir:?}");
        return;
    };
    unsafe {
        std::env::set_var("GST_PLUGIN_PATH", dir_str);
        std::env::set_var("GST_PLUGIN_SYSTEM_PATH_1_0", "");
    }
    eprintln!("[NeoUtl] GST_PLUGIN_PATH設定: {dir_str}");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use slint::ComponentHandle;

    configure_gst_plugin_path();

    objects::load_all(&objects::default_objects_dir());
    effects::load_all(&effects::default_effects_dir());

    let mut wgpu_settings = slint::wgpu_29::WGPUSettings::default();
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        wgpu_settings.backends = slint::wgpu_29::wgpu::Backends::VULKAN;
    }
    #[cfg(target_os = "macos")]
    {
        wgpu_settings.backends = slint::wgpu_29::wgpu::Backends::METAL;
    }
    slint::BackendSelector::new()
        .require_wgpu_29(slint::wgpu_29::WGPUConfiguration::Automatic(wgpu_settings))
        .select()?;

    let launcher = LauncherWindow::new()?;
    ui::install(&launcher);

    // メインウィンドウ（ランチャー）が閉じられたらプロセス全体を終了する。
    // 他ウィンドウ（プレビュー/タイムライン/設定等）はshow/hideのみで
    // イベントループを止めない。終了経路はここ一箇所に集約する。
    launcher.window().on_close_requested(|| {
        slint::quit_event_loop().ok();
        slint::CloseRequestResponse::HideWindow
    });

    launcher.show()?;
    slint::run_event_loop_until_quit()?;
    Ok(())
}
