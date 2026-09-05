use crate::app_state::{self, SharedAppState};
use crate::project;
use crate::ui::system_settings::fields::name_field;
use crate::ui::ui_ext::UiExt;
use egui::{Context, Ui};

pub struct ProjectSettingsWindow {
    pub open: bool,
    project_name: String,
    fps: u32,
    width: u32,
    height: u32,
    audio_sample_rate: u32,
    audio_channels: u32,
}

impl ProjectSettingsWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            project_name: "Project".into(),
            fps: 30,
            width: 1920,
            height: 1080,
            audio_sample_rate: 48000,
            audio_channels: 2,
        }
    }

    pub fn open(&mut self, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let project = world.get_project();
        drop(world);

        self.project_name = project.name;
        self.fps = project.fps;
        self.width = project.width;
        self.height = project.height;
        self.audio_sample_rate = project.audio_sample_rate;
        self.audio_channels = project.audio_channels;
        self.open = true;
    }

    fn confirm(&mut self, state: &SharedAppState) {
        let name = self.project_name.clone();
        let sample_rate = self.audio_sample_rate.max(1);
        let channels = self.audio_channels.clamp(1, 8);

        let world_holder = app_state::active_world(state);
        app_state::snapshot_before_edit(state);
        let mut world = world_holder.lock().unwrap();
        let dir = world
            .get_project()
            .dir
            .unwrap_or_else(project::projects_dir);
        world.set_project_meta(name.clone(), dir);
        world.set_fps(self.fps);
        world.set_resolution(self.width, self.height);
        world.set_audio_format(sample_rate, channels);
        let _ = project::save_from_world(&world);
        drop(world);
        app_state::active_audio_mixer(state)
            .lock()
            .unwrap()
            .set_sample_rate(sample_rate);

        {
            let mut s = state.lock().unwrap();
            let active = s.active;
            s.sessions[active].meta.name = name;
        }

        self.open = false;
    }

    pub fn show(&mut self, _ctx: &Context, ui: &mut Ui, state: &SharedAppState) -> bool {
        if !self.open {
            return false;
        }
        let mut confirmed = false;
        let mut close_requested = false;

        egui::Panel::bottom("project_setting_footer").show(ui, |ui| {
            ui.footer_bar(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t!("OK")).clicked() {
                        confirmed = true;
                    }
                    if ui.button(t!("キャンセル")).clicked() {
                        close_requested = true;
                    }
                })
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.page_content(|ui| {
                ui.section(t!("基本設定"), |ui| {
                    egui::Grid::new("project_settings_basic")
                        .num_columns(2)
                        .show(ui, |ui| {
                            name_field(ui, "プロジェクト名:", &mut self.project_name);
                        });
                });

                ui.section(t!("映像フォーマット"), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.fps)
                                .range(1..=240)
                                .suffix(" fps"),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.width)
                                .range(16..=7680)
                                .suffix(" px"),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.height)
                                .range(16..=7680)
                                .suffix(" px"),
                        );
                    });
                });

                ui.section(t!("音声フォーマット"), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.audio_channels)
                                .range(1..=8)
                                .suffix(" ch"),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.audio_sample_rate)
                                .range(8000..=192000)
                                .suffix(" Hz"),
                        );
                    });
                });
            });
        });

        if confirmed {
            self.confirm(state);
            return true;
        }
        self.open = !close_requested;
        false
    }
}
