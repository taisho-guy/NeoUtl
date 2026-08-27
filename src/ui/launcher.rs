use crate::project::{self, ProjectMeta};
use elegance::{Button, TextInput};

pub struct LauncherPanel {
    name: String,
    fps: u32,
    width: u32,
    height: u32,
    sample_rate: u32,
    channels: u32,
    status: String,
}

impl LauncherPanel {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            fps: 30,
            width: 1920,
            height: 1080,
            sample_rate: 48000,
            channels: 2,
            status: String::new(),
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<ProjectMeta> {
        let mut result = None;
        ui.heading(t!("NeoUtl - プロジェクト"));
        ui.separator();
        ui.columns(2, |columns| {
            columns[0].group(|ui| {
                ui.heading(t!("既存プロジェクト"));
                let projects = project::list_projects();
                if projects.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, t!("プロジェクトがありません"));
                }
                for item in projects {
                    if ui
                        .button(format!(
                            "{}\n{} × {}  {}fps",
                            item.name, item.width, item.height, item.fps
                        ))
                        .clicked()
                    {
                        result = Some(item);
                    }
                }
            });
            columns[1].group(|ui| {
                ui.heading(t!("新規プロジェクト"));
                ui.label(t!("名前"));
                ui.add(TextInput::new(&mut self.name));

                ui.label("fps");
                ui.add(
                    egui::DragValue::new(&mut self.fps)
                        .range(1..=240)
                        .suffix(" fps"),
                );

                ui.label(t!("幅"));
                ui.add(
                    egui::DragValue::new(&mut self.width)
                        .range(16..=7680)
                        .suffix(" px"),
                );

                ui.label(t!("高さ"));
                ui.add(
                    egui::DragValue::new(&mut self.height)
                        .range(16..=7680)
                        .suffix(" px"),
                );

                ui.label("Hz");
                ui.add(
                    egui::DragValue::new(&mut self.sample_rate)
                        .range(8000..=192000)
                        .suffix(" Hz"),
                );

                ui.label("ch");
                ui.add(
                    egui::DragValue::new(&mut self.channels)
                        .range(1..=8)
                        .suffix(" ch"),
                );

                if !self.status.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.status);
                }
                if ui.add(Button::new(t!("作成して開く"))).clicked() {
                    match project::create_project(
                        &self.name,
                        self.fps,
                        self.width,
                        self.height,
                        self.sample_rate,
                        self.channels,
                    ) {
                        Ok(meta) => {
                            self.status.clear();
                            result = Some(meta);
                        }
                        Err(err) => self.status = err.to_string(),
                    }
                }
            });
        });
        result
    }
}
