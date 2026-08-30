use crate::project::{self, ProjectMeta};
use elegance::{Button, Card, TextInput};

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

    fn field(ui: &mut egui::Ui, label: impl Into<String>, add: impl FnOnce(&mut egui::Ui)) {
        let label = label.into();
        ui.horizontal(|ui| {
            ui.add_sized(
                [96.0, ui.spacing().interact_size.y],
                egui::Label::new(
                    egui::RichText::new(label)
                        .color(elegance::Theme::current(ui.ctx()).palette.text_muted),
                ),
            );
            add(ui);
        });
        ui.add_space(8.0);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<ProjectMeta> {
        let mut result = None;
        ui.add_space(4.0);
        ui.heading(t!("NeoUtl - プロジェクト"));
        ui.add_space(12.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.columns(2, |columns| {
                    Card::new()
                        .heading(t!("既存プロジェクト"))
                        .show(&mut columns[0], |ui| {
                            let projects = project::list_projects();
                            if projects.is_empty() {
                                ui.colored_label(
                                    elegance::Theme::current(ui.ctx()).palette.text_faint,
                                    t!("プロジェクトがありません"),
                                );
                            }
                            for item in projects {
                                if ui
                                    .add(
                                        Button::new(format!(
                                            "{}\n{} × {}  {}fps",
                                            item.name, item.width, item.height, item.fps
                                        ))
                                        .outline(),
                                    )
                                    .clicked()
                                {
                                    result = Some(item);
                                }
                                ui.add_space(6.0);
                            }
                        });

                    Card::new()
                        .heading(t!("新規プロジェクト"))
                        .show(&mut columns[1], |ui| {
                            ui.add(TextInput::new(&mut self.name).label(t!("名前")));
                            ui.add_space(8.0);

                            Self::field(ui, "fps", |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.fps)
                                        .range(1..=240)
                                        .suffix(" fps"),
                                );
                            });
                            Self::field(ui, t!("幅"), |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.width)
                                        .range(16..=7680)
                                        .suffix(" px"),
                                );
                            });
                            Self::field(ui, t!("高さ"), |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.height)
                                        .range(16..=7680)
                                        .suffix(" px"),
                                );
                            });
                            Self::field(ui, "Hz", |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.sample_rate)
                                        .range(8000..=192000)
                                        .suffix(" Hz"),
                                );
                            });
                            Self::field(ui, "ch", |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.channels)
                                        .range(1..=8)
                                        .suffix(" ch"),
                                );
                            });

                            if !self.status.is_empty() {
                                ui.colored_label(
                                    elegance::Theme::current(ui.ctx()).palette.danger,
                                    &self.status,
                                );
                                ui.add_space(6.0);
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
            });
        result
    }
}
