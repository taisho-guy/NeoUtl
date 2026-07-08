// src/ui/launcher.rs
use crate::LauncherWindow;
use crate::ProjectListItem;
use crate::project::{self, ProjectMeta};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::rc::Rc;

fn refresh_list(window: &LauncherWindow) {
    let items: Vec<ProjectListItem> = project::list_projects()
        .into_iter()
        .map(|p| ProjectListItem {
            name: p.name.into(),
            path: p.dir.to_string_lossy().to_string().into(),
            width: p.width as i32,
            height: p.height as i32,
            fps: p.fps as i32,
        })
        .collect();
    let model: Rc<dyn Model<Data = ProjectListItem>> = Rc::new(VecModel::from(items));
    window.set_projects(ModelRc::from(model));
}

pub fn setup(window: &LauncherWindow, on_launch: Rc<dyn Fn(ProjectMeta)>) {
    refresh_list(window);

    {
        let weak = window.as_weak();
        window.on_refresh(move || {
            if let Some(w) = weak.upgrade() {
                refresh_list(&w);
            }
        });
    }

    {
        let weak = window.as_weak();
        let on_launch = on_launch.clone();
        window.on_create_project(move |name, fps, width, height| {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                if let Some(w) = weak.upgrade() {
                    w.set_status_message("プロジェクト名を入力してください".into());
                }
                return;
            }
            let result = project::create_project(
                trimmed,
                fps.max(1) as u32,
                width.max(1) as u32,
                height.max(1) as u32,
            );
            match result {
                Ok(meta) => on_launch(meta),
                Err(err) => {
                    if let Some(w) = weak.upgrade() {
                        w.set_status_message(format!("作成失敗: {err}").into());
                    }
                }
            }
        });
    }

    {
        let weak = window.as_weak();
        let on_launch = on_launch.clone();
        window.on_open_project(move |path| {
            let dir = std::path::PathBuf::from(path.as_str());
            match project::load_project(&dir) {
                Some(meta) => on_launch(meta),
                None => {
                    if let Some(w) = weak.upgrade() {
                        w.set_status_message("プロジェクトを開けませんでした".into());
                    }
                }
            }
        });
    }
}
