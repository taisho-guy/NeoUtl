pub mod fields;

use crate::ecs::{EcsWorld, resources::SystemSettingsResource};
use crate::localization::tr;
use crate::update::{self, UpdateStatus};
use egui::{Context, Ui};
use egui_material_icons::{MaterialIcon, icons};
use elegance::{BuiltInTheme, ThemeSwitcher};
use fields::{choice_field, int_field, toggle_field};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const CATEGORIES: [(&str, MaterialIcon); 7] = [
    ("一般", icons::ICON_SETTINGS),
    ("外観", icons::ICON_PALETTE),
    ("パフォーマンス", icons::ICON_SPEED),
    ("デコード", icons::ICON_MOVIE),
    ("タイムライン", icons::ICON_VIEW_TIMELINE),
    ("エクスポート", icons::ICON_UPLOAD),
    ("アップデート", icons::ICON_SYSTEM_UPDATE),
];

fn category_label(index: usize) -> &'static str {
    CATEGORIES[index].0
}

fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent()
                .map(|d| d.join("settings").join("system-settings.yaml"))
        })
        .unwrap_or_else(|| PathBuf::from("settings/system-settings.yaml"))
}

fn save_to_disk(s: &SystemSettingsResource) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let yaml = rust_yaml::to_string(s).map_err(std::io::Error::other)?;
    std::fs::write(path, yaml)
}

pub(crate) fn load_from_disk() -> Option<SystemSettingsResource> {
    let content = std::fs::read_to_string(settings_path()).ok()?;
    rust_yaml::from_str(&content).ok()
}

fn easing_engine_ids_and_names() -> (Vec<String>, Vec<String>) {
    let ids = crate::easings::loader::registry()
        .iter()
        .map(|e| e.id.clone())
        .collect();
    let names = crate::easings::loader::registry()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    (ids, names)
}

fn index_of(ids: &[String], id: &str) -> i32 {
    ids.iter().position(|i| i == id).map_or(0, |i| i as i32)
}

pub struct SystemSettingsWindow {
    pub open: bool,
    selected_category: i32,

    theme_choice: BuiltInTheme,
    easing_engine_ids: Vec<String>,
    easing_engine_names: Vec<String>,
    easing_engine_index: i32,

    autosave_enabled: bool,
    autosave_interval_sec: i32,
    ui_scale_percent: i32,
    worker_threads: i32,
    audio_max_block_size: i32,
    decode_backend: i32,
    default_snap: bool,
    magnetic_snap_range: i32,
    export_container: i32,
    export_codec: i32,

    check_update_on_startup: bool,
    update_status: Arc<Mutex<UpdateStatus>>,
    crash_reporting_enabled: bool,

    save_status: String,
}

impl SystemSettingsWindow {
    pub fn new(world_holder: &Arc<Mutex<EcsWorld>>) -> Self {
        if let Some(loaded) = load_from_disk() {
            world_holder.lock().unwrap().set_system_settings(loaded);
        }

        let (easing_engine_ids, easing_engine_names) = easing_engine_ids_and_names();
        let s = world_holder.lock().unwrap().get_system_settings();

        crate::media::runtime::set_worker_threads(s.worker_threads);
        crate::media::runtime::apply_decode_backend_env(s.decode_backend);
        crate::theme::restore(&s.theme_id);

        let update_status = Arc::new(Mutex::new(UpdateStatus::Idle));
        if s.check_update_on_startup {
            update::spawn_check(update_status.clone());
        }

        Self {
            open: false,
            selected_category: 0,
            theme_choice: crate::theme::current(),
            easing_engine_index: index_of(&easing_engine_ids, &s.easing_engine_id),
            easing_engine_ids,
            easing_engine_names,
            autosave_enabled: s.autosave_enabled,
            autosave_interval_sec: s.autosave_interval_sec,
            ui_scale_percent: s.ui_scale_percent,
            worker_threads: s.worker_threads,
            audio_max_block_size: s.audio_max_block_size,
            decode_backend: s.decode_backend,
            default_snap: s.default_snap,
            magnetic_snap_range: s.magnetic_snap_range,
            export_container: s.export_container,
            export_codec: s.export_codec,
            check_update_on_startup: s.check_update_on_startup,
            update_status,
            crash_reporting_enabled: s.crash_reporting_enabled,
            save_status: String::new(),
        }
    }

    fn persist(
        &self,
        world_holder: &Arc<Mutex<EcsWorld>>,
        mutate: impl FnOnce(&mut SystemSettingsResource),
    ) {
        let mut world = world_holder.lock().unwrap();
        let mut s = world.get_system_settings();
        mutate(&mut s);
        world.set_system_settings(s);
    }

    fn reload(&mut self, world_holder: &Arc<Mutex<EcsWorld>>) {
        let Some(loaded) = load_from_disk() else {
            self.save_status = t!("設定ファイルなし");
            return;
        };
        world_holder
            .lock()
            .unwrap()
            .set_system_settings(loaded.clone());
        crate::media::runtime::set_worker_threads(loaded.worker_threads);
        crate::media::runtime::apply_decode_backend_env(loaded.decode_backend);

        self.theme_choice = crate::theme::from_id(&loaded.theme_id);
        crate::theme::set(self.theme_choice);
        self.easing_engine_index = index_of(&self.easing_engine_ids, &loaded.easing_engine_id);
        self.autosave_enabled = loaded.autosave_enabled;
        self.autosave_interval_sec = loaded.autosave_interval_sec;
        self.ui_scale_percent = loaded.ui_scale_percent;
        self.worker_threads = loaded.worker_threads;
        self.audio_max_block_size = loaded.audio_max_block_size;
        self.decode_backend = loaded.decode_backend;
        self.default_snap = loaded.default_snap;
        self.magnetic_snap_range = loaded.magnetic_snap_range;
        self.export_container = loaded.export_container;
        self.export_codec = loaded.export_codec;
        self.check_update_on_startup = loaded.check_update_on_startup;
        self.crash_reporting_enabled = loaded.crash_reporting_enabled;
        self.save_status = t!("再読込完了");
    }

    pub fn show(&mut self, _ctx: &Context, ui: &mut Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        if !self.open {
            return;
        }

        egui::Panel::bottom("system_setting_footer")
            .frame(egui::Frame::default().inner_margin(4.0))
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), fields::field_height(ui)),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.label(&self.save_status);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(t!("保存")).clicked() {
                                let s = world_holder.lock().unwrap().get_system_settings();
                                self.save_status = match save_to_disk(&s) {
                                    Ok(()) => t!("保存完了"),
                                    Err(_) => t!("保存失敗"),
                                };
                            }
                            if ui.button(t!("再読込")).clicked() {
                                self.reload(world_holder);
                            }
                        });
                    },
                )
            });

        egui::Panel::left("system_settings_categories")
            .frame(
                egui::Frame::default()
                    .fill(ui.visuals().faint_bg_color)
                    .inner_margin(egui::Margin::symmetric(8,12)),
            )
            .show(ui, |ui| {
                ui.set_width(150.0);

                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    let widgets = &mut ui.style_mut().visuals.widgets;
                    widgets.inactive.bg_stroke = egui::Stroke::NONE;
                    widgets.hovered.bg_stroke = egui::Stroke::NONE;
                    widgets.active.bg_stroke = egui::Stroke::NONE;
                    widgets.hovered.expansion = 0.0;
                    widgets.active.expansion = 0.0;

                    for (i, (label, icon)) in CATEGORIES.iter().enumerate() {
                        self.category_item(ui, i as i32, label, icon);
                    }
                })
            });
        egui::Panel::top("system_setting_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(16, 12)))
            .show(ui, |ui| {
                ui.heading(tr(category_label(self.selected_category as usize)));
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::default().inner_margin(egui::Margin::same(16)))
            .show(ui, |ui| {
                egui::Grid::new("system_settings_page")
                    .num_columns(2)
                    .spacing([10.0, 10.0])
                    .show(ui, |ui| match self.selected_category {
                        0 => self.page_general(ui, world_holder),
                        1 => self.page_appearance(ui, world_holder),
                        2 => self.page_performance(ui, world_holder),
                        3 => self.page_decode(ui, world_holder),
                        4 => self.page_timeline_defaults(ui, world_holder),
                        5 => self.page_export(ui, world_holder),
                        _ => self.page_update(ui, world_holder),
                    });
            });
    }

    fn category_item(&mut self, ui: &mut Ui, index: i32, label: &str, icon: &MaterialIcon) {
        let active = index == self.selected_category;
        let is_update_category = index as usize == CATEGORIES.len() - 1;
        let has_update = is_update_category
            && matches!(
                *self.update_status.lock().unwrap(),
                UpdateStatus::Available(_)
            );
        let mark = if has_update { " ●" } else { "" };
        let text = format!("{}  {}{mark}", icon.codepoint, tr(label));

        if ui.selectable_label(active, text).clicked() {
            self.selected_category = index;
        }
    }

    fn page_general(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        let mut autosave_enabled = self.autosave_enabled;
        let mut autosave_interval_sec = self.autosave_interval_sec;
        let changed = toggle_field(ui, "自動保存を有効化", &mut autosave_enabled)
            | int_field(
                ui,
                "自動保存間隔（秒）",
                &mut autosave_interval_sec,
                10,
                3600,
            );
        if changed {
            self.autosave_enabled = autosave_enabled;
            self.autosave_interval_sec = autosave_interval_sec.clamp(10, 86_400);
            let (enabled, interval) = (self.autosave_enabled, self.autosave_interval_sec);
            self.persist(world_holder, |s| {
                s.autosave_enabled = enabled;
                s.autosave_interval_sec = interval;
            });
        }
    }

    fn page_appearance(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        ui.label(t!("テーマ"));
        let resp = ui.add(ThemeSwitcher::new(&mut self.theme_choice).auto_install(false));
        if resp.changed() {
            crate::theme::set(self.theme_choice);
            let id = crate::theme::id_of(self.theme_choice).to_string();
            self.persist(world_holder, |s| s.theme_id = id);
        }
        ui.end_row();

        let mut ui_scale_percent = self.ui_scale_percent;
        if int_field(ui, "UIスケール（%）", &mut ui_scale_percent, 50, 200) {
            self.ui_scale_percent = ui_scale_percent;
            let scale = self.ui_scale_percent;
            self.persist(world_holder, |s| s.ui_scale_percent = scale);
        }

        let mut easing_engine_index = self.easing_engine_index;
        if choice_field(
            ui,
            "イージングエンジン",
            &self.easing_engine_names,
            &mut easing_engine_index,
        ) {
            self.easing_engine_index = easing_engine_index;
            if let Some(id) = self
                .easing_engine_ids
                .get(easing_engine_index as usize)
                .cloned()
            {
                self.persist(world_holder, |s| s.easing_engine_id.clone_from(&id));
            }
        }
    }

    fn page_performance(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        let mut worker_threads = self.worker_threads;
        let mut audio_max_block_size = self.audio_max_block_size;
        let changed = int_field(
            ui,
            "ワーカースレッド数（0=自動）",
            &mut worker_threads,
            0,
            64,
        ) | int_field(
            ui,
            "オーディオ最大ブロックサイズ",
            &mut audio_max_block_size,
            64,
            16384,
        );
        if changed {
            self.worker_threads = worker_threads;
            self.audio_max_block_size = audio_max_block_size;
            let (threads, block) = (self.worker_threads, self.audio_max_block_size);
            self.persist(world_holder, |s| {
                s.worker_threads = threads;
                s.audio_max_block_size = block;
            });
            crate::media::runtime::set_worker_threads(threads);
        }
    }

    fn page_decode(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        let options = [
            "自動".to_string(),
            "GPU固定".to_string(),
            "CPU固定".to_string(),
        ];
        let mut decode_backend = self.decode_backend;
        if choice_field(
            ui,
            "映像デコードバックエンド",
            &options,
            &mut decode_backend,
        ) {
            self.decode_backend = decode_backend;
            self.persist(world_holder, |s| s.decode_backend = decode_backend);
            crate::media::runtime::apply_decode_backend_env(decode_backend);
        }
    }

    fn page_timeline_defaults(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        let mut default_snap = self.default_snap;
        let mut magnetic_snap_range = self.magnetic_snap_range;
        let changed = toggle_field(ui, "スナップを既定で有効化", &mut default_snap)
            | int_field(
                ui,
                "磁力スナップ範囲（px）",
                &mut magnetic_snap_range,
                0,
                100,
            );
        if changed {
            self.default_snap = default_snap;
            self.magnetic_snap_range = magnetic_snap_range;
            let (snap, range) = (self.default_snap, self.magnetic_snap_range);
            self.persist(world_holder, |s| {
                s.default_snap = snap;
                s.magnetic_snap_range = range;
            });
        }
    }

    fn page_export(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        let container_options = ["MP4".to_string(), "MOV".to_string(), "MKV".to_string()];
        let codec_options = ["H.264".to_string(), "HEVC".to_string(), "AV1".to_string()];

        let mut export_container = self.export_container;
        if choice_field(
            ui,
            "コンテナ形式",
            &container_options,
            &mut export_container,
        ) {
            self.export_container = export_container;
            self.persist(world_holder, |s| s.export_container = export_container);
        }

        let mut export_codec = self.export_codec;
        if choice_field(ui, "映像コーデック", &codec_options, &mut export_codec) {
            self.export_codec = export_codec;
            self.persist(world_holder, |s| s.export_codec = export_codec);
        }
    }

    fn page_update(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        let mut check_update_on_startup = self.check_update_on_startup;
        if toggle_field(
            ui,
            "起動時にアップデートを確認",
            &mut check_update_on_startup,
        ) {
            self.check_update_on_startup = check_update_on_startup;
            self.persist(world_holder, |s| {
                s.check_update_on_startup = check_update_on_startup;
            });
        }

        ui.label("");
        ui.end_row();

        let status = self.update_status.lock().unwrap().clone();
        match status {
            UpdateStatus::Idle => {
                ui.label(t!("未確認"));
                ui.end_row();
            }
            UpdateStatus::Checking => {
                ui.label(t!("確認中..."));
                ui.end_row();
            }
            UpdateStatus::UpToDate => {
                ui.label(t!("最新版です"));
                ui.end_row();
            }
            UpdateStatus::Available(info) => {
                ui.label(t!(
                    "新バージョン: %{arg0}",
                    arg0 = format!("{}", info.version)
                ));
                ui.end_row();
                ui.label(&info.notes);
                ui.end_row();
                if ui.button(t!("今すぐ更新")).clicked() {
                    update::spawn_apply(self.update_status.clone(), info.clone());
                }
                ui.end_row();
            }
            UpdateStatus::Downloading(fraction) => {
                ui.label(t!("ダウンロード中"));
                ui.add(egui::ProgressBar::new(fraction));
                ui.end_row();
            }
            UpdateStatus::Installed => {
                ui.label(t!("更新完了。再起動してください"));
                ui.end_row();
            }
            UpdateStatus::Error(err) => {
                ui.label(t!("エラー: %{arg0}", arg0 = format!("{err}")));
                ui.end_row();
            }
        }

        if ui.button(t!("今すぐ確認")).clicked() {
            update::spawn_check(self.update_status.clone());
        }
        ui.end_row();

        ui.separator();
        ui.end_row();

        let mut crash_reporting_enabled = self.crash_reporting_enabled;
        if toggle_field(
            ui,
            "エラー発生時に匿名の診断情報をGlitchTipへ送信",
            &mut crash_reporting_enabled,
        ) {
            self.crash_reporting_enabled = crash_reporting_enabled;
            self.persist(world_holder, |s| {
                s.crash_reporting_enabled = crash_reporting_enabled;
            });
        }
        ui.label(t!("変更は次回起動時から反映されます"));
        ui.end_row();
    }
}
