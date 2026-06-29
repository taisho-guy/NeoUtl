// src/main.rs
use slint::ComponentHandle;
use std::sync::{Arc, Mutex};

mod ecs;
mod objects;
mod renderer;
mod ui;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    objects::load_all(&objects::default_objects_dir());

    slint::BackendSelector::new()
        .require_wgpu_29(slint::wgpu_29::WGPUConfiguration::default())
        .select()?;

    let app = MainWindow::new()?;
    let world_holder = Arc::new(Mutex::new(ecs::EcsWorld::new()));
    let engine_holder = Arc::new(Mutex::new(None));

    let engine_setup = engine_holder.clone();
    app.window()
        .set_rendering_notifier(move |state, graphics_api| {
            if let (
                slint::RenderingState::RenderingSetup,
                slint::GraphicsAPI::WGPU29 { device, queue, .. },
            ) = (state, graphics_api)
            {
                let mut lock = engine_setup.lock().unwrap();
                if lock.is_none() {
                    *lock = Some(renderer::RenderEngine::new(
                        device.clone(),
                        queue.clone(),
                        1920,
                        1080,
                    ));
                }
            }
        })?;

    ui::setup_ui_callbacks(&app, world_holder, engine_holder);
    app.run()?;
    Ok(())
}
