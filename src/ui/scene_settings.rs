use crate::app_state::{self, SharedAppState};
use crate::ecs::SceneSettings;
use crate::project;
use crate::ui::system_settings::fields::{
    choice_field, float_field, int_field, name_field, toggle_field,
};
use egui::{Context, Ui};

pub struct SceneSettingsWindow {
    pub open: bool,
    is_creation_mode: bool,
    target_scene_id: i32,

    scene_name: String,
    scene_width: i32,
    scene_height: i32,
    scene_fps: f32,

    enable_snap: bool,
    magnetic_snap_range: i32,
    grid_mode: i32,
    grid_bpm: f32,
    grid_offset: f32,
    grid_interval: i32,
    grid_subdivision: i32,
}

impl SceneSettingsWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            is_creation_mode: true,
            target_scene_id: -1,
            scene_name: "Scene".into(),
            scene_width: 1920,
            scene_height: 1080,
            scene_fps: 30.0,
            enable_snap: true,
            magnetic_snap_range: 10,
            grid_mode: 0,
            grid_bpm: 120.0,
            grid_offset: 0.0,
            grid_interval: 10,
            grid_subdivision: 4,
        }
    }

    pub fn open_for_create(&mut self, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let project = world.get_project();
        let count = world.scenes().len();
        drop(world);

        let settings_holder = app_state::settings_world(state);
        let system_settings = settings_holder.lock().unwrap().get_system_settings();
        let defaults = crate::ecs::resources::SceneMeta::new_with_defaults(
            -1,
            "",
            system_settings.default_snap,
            system_settings.magnetic_snap_range,
        );

        self.is_creation_mode = true;
        self.target_scene_id = -1;
        self.scene_name = format!("Scene {}", count + 1);
        self.scene_width = project.width as i32;
        self.scene_height = project.height as i32;
        self.scene_fps = project.fps as f32;
        self.enable_snap = defaults.enable_snap;
        self.magnetic_snap_range = defaults.magnetic_snap_range;
        self.grid_mode = defaults.grid_mode;
        self.grid_bpm = defaults.grid_bpm;
        self.grid_offset = defaults.grid_offset;
        self.grid_interval = defaults.grid_interval;
        self.grid_subdivision = defaults.grid_subdivision;
        self.open = true;
    }

    pub fn open_for_edit(&mut self, state: &SharedAppState, scene_id: i32) {
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let Some(s) = world.get_scene(scene_id) else {
            return;
        };
        drop(world);

        self.is_creation_mode = false;
        self.target_scene_id = scene_id;
        self.scene_name = s.name;
        self.scene_width = s.width as i32;
        self.scene_height = s.height as i32;
        self.scene_fps = s.fps as f32;
        self.enable_snap = s.enable_snap;
        self.magnetic_snap_range = s.magnetic_snap_range;
        self.grid_mode = s.grid_mode;
        self.grid_bpm = s.grid_bpm;
        self.grid_offset = s.grid_offset;
        self.grid_interval = s.grid_interval;
        self.grid_subdivision = s.grid_subdivision;
        self.open = true;
    }

    fn confirm(&mut self, state: &SharedAppState) {
        let settings = SceneSettings {
            name: self.scene_name.clone(),
            width: self.scene_width.max(1) as u32,
            height: self.scene_height.max(1) as u32,
            fps: self.scene_fps.max(1.0) as u32,
            grid_mode: self.grid_mode,
            grid_bpm: self.grid_bpm,
            grid_offset: self.grid_offset,
            grid_interval: self.grid_interval,
            grid_subdivision: self.grid_subdivision,
            enable_snap: self.enable_snap,
            magnetic_snap_range: self.magnetic_snap_range,
        };

        let world_holder = app_state::active_world(state);
        app_state::snapshot_before_edit(state);
        let mut world = world_holder.lock().unwrap();

        let scene_id = if self.is_creation_mode {
            let id = world.add_scene(settings.name.clone());
            world.switch_scene(id);
            id
        } else {
            self.target_scene_id
        };
        world.update_scene_settings(scene_id, settings);
        let _ = project::save_from_world(&world);
        drop(world);

        self.open = false;
    }

    pub fn show(&mut self, ctx: &Context, ui: &mut Ui, state: &SharedAppState) -> bool {
        if !self.open {
            return false;
        }
        let title = if self.is_creation_mode {
            t!("新規シーン作成")
        } else {
            t!("シーン設定")
        };
        let section_color = egui::Color32::from_rgb(0x8a, 0xab, 0xff);

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        let mut confirmed = false;
        let mut close_requested = false;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.group(|ui| {
                ui.colored_label(section_color, t!("基本設定"));
                egui::Grid::new("scene_settings_basic")
                    .num_columns(2)
                    .show(ui, |ui| {
                        name_field(ui, "シーン名:", &mut self.scene_name);
                        int_field(ui, "幅:", &mut self.scene_width, 1, 8000);
                        int_field(ui, "高さ:", &mut self.scene_height, 1, 8000);
                        float_field(ui, "FPS:", &mut self.scene_fps);
                    });
            });

            ui.group(|ui| {
                ui.colored_label(section_color, t!("編集とスナップ"));
                egui::Grid::new("scene_settings_snap")
                    .num_columns(2)
                    .show(ui, |ui| {
                        toggle_field(ui, "スナップを有効にする", &mut self.enable_snap);
                        int_field(
                            ui,
                            "磁力スナップ範囲:",
                            &mut self.magnetic_snap_range,
                            1,
                            100,
                        );
                        let grid_mode_options = [
                            "自動 (秒/フレーム)".to_string(),
                            "BPM (音楽)".to_string(),
                            "フレーム数固定".to_string(),
                        ];
                        choice_field(
                            ui,
                            "グリッドモード:",
                            &grid_mode_options,
                            &mut self.grid_mode,
                        );
                    });
            });

            if self.grid_mode == 1 {
                ui.group(|ui| {
                    ui.colored_label(section_color, t!("BPM設定"));
                    egui::Grid::new("scene_settings_bpm")
                        .num_columns(2)
                        .show(ui, |ui| {
                            float_field(ui, "BPM:", &mut self.grid_bpm);
                            int_field(ui, "拍子 (分割数):", &mut self.grid_subdivision, 1, 32);
                            float_field(ui, "オフセット (秒):", &mut self.grid_offset);
                        });
                });
            }

            if self.grid_mode == 2 {
                ui.group(|ui| {
                    ui.colored_label(section_color, t!("フレーム設定"));
                    egui::Grid::new("scene_settings_frame")
                        .num_columns(2)
                        .show(ui, |ui| {
                            int_field(ui, "間隔 (Frames):", &mut self.grid_interval, 1, 1000);
                        });
                });
            }

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
