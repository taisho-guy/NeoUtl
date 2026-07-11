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

    launcher.show()?;
    slint::run_event_loop()?;
    Ok(())
}
