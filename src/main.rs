// src/main.rs
mod app_state;
mod ecs;
mod media;
mod objects;
mod project;
mod renderer;
mod ui;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use slint::ComponentHandle;

    objects::load_all(&objects::default_objects_dir());

    slint::BackendSelector::new()
        .require_wgpu_29(slint::wgpu_29::WGPUConfiguration::default())
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
