pub mod fields;

use crate::ecs::{EcsWorld, resources::SystemSettingsResource};
use crate::theme;
use egui::{Color32, Context, Ui};
use fields::{choice_field, int_field, toggle_field};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const CATEGORIES: [(&str, &str); 6] = [
    ("一般", ""),
    ("外観", ""),
    ("パフォーマンス", ""),
    ("デコード", ""),
    ("タイムライン", ""),
    ("エクスポート", ""),
];

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

fn theme_ids_and_names() -> (Vec<String>, Vec<String>) {
    let ids = theme::registry()
        .iter()
        .map(|e| e.stable_id.clone())
        .collect();
    let names = theme::registry().iter().map(|e| e.name.clone()).collect();
    (ids, names)
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

/// "#RRGGBB" / "RRGGBB" 形式のみ受理する。不正値はNoneを返し呼び出し側は変更を諦める。
fn parse_hex_color(s: &str) -> Option<Color32> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

/// wallpaper_path未使用（空文字）で解決するため、壁紙連動テーマは既定色に留まる。
fn resolve_theme_background(id: &str) -> Option<Color32> {
    let entry = theme::by_stable_id(id)?;
    let wallpaper = std::ffi::CString::new("").unwrap();
    let ctx = neoutl_theme_api::ThemeContext {
        wallpaper_path: wallpaper.as_ptr(),
        unix_time_sec: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64),
    };
    let colors = theme::resolve(entry, &ctx);
    drop(wallpaper);
    colors.background.as_deref().and_then(parse_hex_color)
}

pub struct SystemSettingsWindow {
    pub open: bool,
    theme_background: Color32,
    selected_category: i32,

    theme_ids: Vec<String>,
    theme_names: Vec<String>,
    theme_index: i32,
    easing_engine_ids: Vec<String>,
    easing_engine_names: Vec<String>,
    easing_engine_index: i32,

    autosave_enabled: bool,
    autosave_interval_sec: i32,
    theme_dark: bool,
    ui_scale_percent: i32,
    worker_threads: i32,
    audio_max_block_size: i32,
    decode_backend: i32,
    default_snap: bool,
    magnetic_snap_range: i32,
    export_container: i32,
    export_codec: i32,

    save_status: String,
}

impl SystemSettingsWindow {
    pub fn new(world_holder: &Arc<Mutex<EcsWorld>>) -> Self {
        if let Some(loaded) = load_from_disk() {
            world_holder.lock().unwrap().set_system_settings(loaded);
        }

        let (theme_ids, theme_names) = theme_ids_and_names();
        let (easing_engine_ids, easing_engine_names) = easing_engine_ids_and_names();
        let s = world_holder.lock().unwrap().get_system_settings();

        crate::media::runtime::set_worker_threads(s.worker_threads);
        crate::media::runtime::apply_decode_backend_env(s.decode_backend);
        let theme_background =
            resolve_theme_background(&s.theme_id).unwrap_or(Color32::from_rgb(0x0e, 0x0e, 0x12));

        Self {
            open: false,
            theme_background,
            selected_category: 0,
            theme_index: index_of(&theme_ids, &s.theme_id),
            easing_engine_index: index_of(&easing_engine_ids, &s.easing_engine_id),
            theme_ids,
            theme_names,
            easing_engine_ids,
            easing_engine_names,
            autosave_enabled: s.autosave_enabled,
            autosave_interval_sec: s.autosave_interval_sec,
            theme_dark: s.theme_dark,
            ui_scale_percent: s.ui_scale_percent,
            worker_threads: s.worker_threads,
            audio_max_block_size: s.audio_max_block_size,
            decode_backend: s.decode_backend,
            default_snap: s.default_snap,
            magnetic_snap_range: s.magnetic_snap_range,
            export_container: s.export_container,
            export_codec: s.export_codec,
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
            self.save_status = "設定ファイルなし".into();
            return;
        };
        world_holder
            .lock()
            .unwrap()
            .set_system_settings(loaded.clone());
        crate::media::runtime::set_worker_threads(loaded.worker_threads);
        crate::media::runtime::apply_decode_backend_env(loaded.decode_backend);

        self.theme_index = index_of(&self.theme_ids, &loaded.theme_id);
        self.easing_engine_index = index_of(&self.easing_engine_ids, &loaded.easing_engine_id);
        self.autosave_enabled = loaded.autosave_enabled;
        self.autosave_interval_sec = loaded.autosave_interval_sec;
        self.theme_dark = loaded.theme_dark;
        self.ui_scale_percent = loaded.ui_scale_percent;
        self.worker_threads = loaded.worker_threads;
        self.audio_max_block_size = loaded.audio_max_block_size;
        self.decode_backend = loaded.decode_backend;
        self.default_snap = loaded.default_snap;
        self.magnetic_snap_range = loaded.magnetic_snap_range;
        self.export_container = loaded.export_container;
        self.export_codec = loaded.export_codec;
        self.theme_background = resolve_theme_background(&loaded.theme_id)
            .unwrap_or(Color32::from_rgb(0x0e, 0x0e, 0x12));
        self.save_status = "再読込完了".into();
    }

    pub fn show(&mut self, ctx: &Context, ui: &mut Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        if !self.open {
            return;
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&ctx.style_of(egui::Theme::Dark))
                    .fill(self.theme_background),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(160.0);
                        for (i, (label, icon)) in CATEGORIES.iter().enumerate() {
                            let i = i as i32;
                            let active = i == self.selected_category;
                            let text = format!("{icon}  {label}");
                            if ui.selectable_label(active, text).clicked() {
                                self.selected_category = i;
                            }
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::Grid::new("system_settings_page")
                            .num_columns(2)
                            .spacing([10.0, 10.0])
                            .show(ui, |ui| match self.selected_category {
                                0 => self.page_general(ui, world_holder),
                                1 => self.page_appearance(ui, world_holder),
                                2 => self.page_performance(ui, world_holder),
                                3 => self.page_decode(ui, world_holder),
                                4 => self.page_timeline_defaults(ui, world_holder),
                                _ => self.page_export(ui, world_holder),
                            });
                    });
                });

                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("保存").clicked() {
                        let s = world_holder.lock().unwrap().get_system_settings();
                        self.save_status = match save_to_disk(&s) {
                            Ok(()) => "保存完了".into(),
                            Err(_) => "保存失敗".into(),
                        };
                    }
                    if ui.button("再読込").clicked() {
                        self.reload(world_holder);
                    }
                    ui.label(&self.save_status);
                });
            });
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
        let mut theme_index = self.theme_index;
        if choice_field(ui, "テーマ", &self.theme_names, &mut theme_index) {
            self.theme_index = theme_index;
            if let Some(id) = self.theme_ids.get(theme_index as usize).cloned() {
                self.persist(world_holder, |s| s.theme_id.clone_from(&id));
                self.theme_background =
                    resolve_theme_background(&id).unwrap_or(self.theme_background);
            }
        }

        let mut theme_dark = self.theme_dark;
        let mut ui_scale_percent = self.ui_scale_percent;
        let changed = toggle_field(ui, "ダークテーマ", &mut theme_dark)
            | int_field(ui, "UIスケール（%）", &mut ui_scale_percent, 50, 200);
        if changed {
            self.theme_dark = theme_dark;
            self.ui_scale_percent = ui_scale_percent;
            let (dark, scale) = (self.theme_dark, self.ui_scale_percent);
            self.persist(world_holder, |s| {
                s.theme_dark = dark;
                s.ui_scale_percent = scale;
            });
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
}
