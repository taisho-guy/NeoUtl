use crate::project::{self, ProjectMeta};

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
        ui.heading("NeoUtl - プロジェクト");
        ui.separator();
        ui.columns(2, |columns| {
            columns[0].group(|ui| {
                ui.heading("既存プロジェクト");
                let projects = project::list_projects();
                if projects.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, "プロジェクトがありません");
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
                ui.heading("新規プロジェクト");
                ui.label("名前");
                ui.text_edit_singleline(&mut self.name);
                ui.add(
                    egui::DragValue::new(&mut self.fps)
                        .range(1..=240)
                        .prefix("fps: "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.width)
                        .range(16..=7680)
                        .prefix("幅: "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.height)
                        .range(16..=7680)
                        .prefix("高さ: "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.sample_rate)
                        .range(8000..=192000)
                        .prefix("Hz: "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.channels)
                        .range(1..=8)
                        .prefix("ch: "),
                );
                if !self.status.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.status);
                }
                if ui.button("作成して開く").clicked() {
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
