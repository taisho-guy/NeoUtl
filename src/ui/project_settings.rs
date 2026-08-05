use crate::app_state::{self, SharedAppState};
use crate::localization::tr;
use crate::project;
use crate::ui::system_settings::fields::{choice_field, name_field};
use egui::{Context, Ui};

pub struct ProjectSettingsWindow {
    pub open: bool,
    project_name: String,
    audio_sample_rate: i32,
    audio_channels: i32,
}

impl ProjectSettingsWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            project_name: "Project".into(),
            audio_sample_rate: 48000,
            audio_channels: 2,
        }
    }

    /// アクティブプロジェクトの現在値を反映してダイアログを開く。
    pub fn open(&mut self, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let project = world.get_project();
        drop(world);

        self.project_name = project.name;
        self.audio_sample_rate = project.audio_sample_rate as i32;
        self.audio_channels = project.audio_channels as i32;
        self.open = true;
    }

    fn confirm(&mut self, state: &SharedAppState) {
        let name = self.project_name.clone();
        let sample_rate = self.audio_sample_rate.max(1) as u32;
        let channels = self.audio_channels.clamp(1, 8) as u32;

        let world_holder = app_state::active_world(state);
        app_state::snapshot_before_edit(state);
        let mut world = world_holder.lock().unwrap();
        let dir = world
            .get_project()
            .dir
            .unwrap_or_else(project::projects_dir);
        world.set_project_meta(name.clone(), dir);
        world.set_audio_format(sample_rate, channels);
        let _ = project::save_from_world(&world);
        drop(world);

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
        egui::CentralPanel::default().show(ui, |ui| {
            ui.group(|ui| {
                ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), t!("基本設定"));
                egui::Grid::new("project_settings_basic")
                    .num_columns(2)
                    .show(ui, |ui| {
                        name_field(ui, "プロジェクト名:", &mut self.project_name);
                    });
            });

            ui.group(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(0x8a, 0xab, 0xff),
                    t!("音声フォーマット"),
                );
                egui::Grid::new("project_settings_audio")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let sample_rate_options = [
                            "44100 Hz".to_string(),
                            "48000 Hz".to_string(),
                            "96000 Hz".to_string(),
                        ];
                        let mut sample_rate_index = match self.audio_sample_rate {
                            44100 => 0,
                            96000 => 2,
                            _ => 1,
                        };
                        if choice_field(
                            ui,
                            "サンプルレート:",
                            &sample_rate_options,
                            &mut sample_rate_index,
                        ) {
                            self.audio_sample_rate = match sample_rate_index {
                                0 => 44100,
                                2 => 96000,
                                _ => 48000,
                            };
                        }

                        let channel_options =
                            ["モノラル (1ch)".to_string(), "ステレオ (2ch)".to_string()];
                        let mut channel_index = if self.audio_channels == 1 { 0 } else { 1 };
                        if choice_field(ui, "チャンネル数:", &channel_options, &mut channel_index)
                        {
                            self.audio_channels = if channel_index == 0 { 1 } else { 2 };
                        }
                    });
            });

            ui.add_space(ui.available_height() - 32.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("OK")).clicked() {
                    confirmed = true;
                }
                if ui.button(t!("キャンセル")).clicked() {
                    close_requested = true;
                }
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
