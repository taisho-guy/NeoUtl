// src/main.rs
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

mod config_format;
mod ecs;
mod media;
mod objects;
mod project;
mod renderer;
mod ui;

slint::include_modules!();

#[allow(dead_code)]
struct AppHandles {
    preview: PreviewWindow,
    timeline: TimelineWindow,
    props: PropertiesWindow,
    settings: SystemSettingsWindow,
}

fn build_main_app(meta: &project::ProjectMeta) -> Result<AppHandles, Box<dyn std::error::Error>> {
    let preview = PreviewWindow::new()?;
    let timeline = TimelineWindow::new()?;
    let props = PropertiesWindow::new()?;
    let settings = SystemSettingsWindow::new()?;

    let world_holder = Arc::new(Mutex::new(ecs::EcsWorld::new()));
    {
        let mut world = world_holder.lock().unwrap();
        world.set_project_meta(meta.name.clone(), meta.dir.clone());
        world.set_fps(meta.fps);
        world.set_resolution(meta.width, meta.height);
        world.set_audio_format(meta.audio_sample_rate, meta.audio_channels);
    }
    let engine_holder = Arc::new(Mutex::new(None::<renderer::RenderEngine>));

    let engine_setup = engine_holder.clone();
    let render_width = meta.width;
    let render_height = meta.height;
    preview
        .window()
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
                        render_width,
                        render_height,
                    ));
                }
            }
        })?;

    ui::setup_ui_callbacks(
        &preview,
        &timeline,
        &props,
        &settings,
        world_holder,
        engine_holder,
    );

    preview.set_fps(meta.fps as i32);
    preview.set_res_width(meta.width as i32);
    preview.set_res_height(meta.height as i32);

    preview.show()?;
    timeline.show()?;
    props.show()?;

    Ok(AppHandles {
        preview,
        timeline,
        props,
        settings,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    objects::load_all(&objects::default_objects_dir());

    slint::BackendSelector::new()
        .require_wgpu_29(slint::wgpu_29::WGPUConfiguration::default())
        .select()?;

    let launcher = LauncherWindow::new()?;
    let launcher_weak = launcher.as_weak();

    // ランチャーで作成・選択されたプロジェクトが確定するまで本体ウィンドウは生成しない。
    let app_handles: Rc<RefCell<Option<AppHandles>>> = Rc::new(RefCell::new(None));
    let handles_slot = app_handles.clone();

    let on_launch: Rc<dyn Fn(project::ProjectMeta)> =
        Rc::new(
            move |meta: project::ProjectMeta| match build_main_app(&meta) {
                Ok(handles) => {
                    *handles_slot.borrow_mut() = Some(handles);
                    if let Some(l) = launcher_weak.upgrade() {
                        let _ = l.hide();
                    }
                }
                Err(err) => {
                    eprintln!("[NeoUtl] プロジェクト起動失敗: {err}");
                }
            },
        );

    ui::launcher::setup(&launcher, on_launch);
    launcher.show()?;
    slint::run_event_loop()?;

    let _ = app_handles;
    Ok(())
}
