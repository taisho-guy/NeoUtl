pub mod fields;

use crate::audio::{plugin_registry, plugin_settings};
use crate::ecs::{
    EcsWorld,
    resources::{AudioPluginSettingsResource, SystemSettingsResource},
};
use crate::localization::tr;
use crate::ui::ui_ext::{self, Density, UiExt, page_title};
use crate::update::{self, UpdateStatus};
use egui::{Context, Ui};
use egui_material_icons::{MaterialIcon, icons};
use elegance::{
    BuiltInTheme, Button, Indicator, IndicatorState, ProgressBar, SegmentedButton, Spinner, Switch,
    TextInput, ThemeSwitcher,
};
use fields::{choice_field, int_field, toggle_field};
use maolan_host_adapter::PluginCatalogEntry;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const CATEGORIES: [(&str, MaterialIcon); 8] = [
    ("一般", icons::ICON_SETTINGS),
    ("外観", icons::ICON_PALETTE),
    ("パフォーマンス", icons::ICON_SPEED),
    ("デコード", icons::ICON_MOVIE),
    ("タイムライン", icons::ICON_VIEW_TIMELINE),
    ("エクスポート", icons::ICON_UPLOAD),
    ("音声プラグイン", icons::ICON_EXTENSION),
    ("アップデート", icons::ICON_SYSTEM_UPDATE),
];

#[derive(Clone, Default)]
enum ScanStatus {
    #[default]
    Idle,
    Scanning,
    Done,
    Error(String),
}

fn category_label(index: usize) -> &'static str {
    CATEGORIES[index].0
}

fn hw_backend_display_name(id: &str) -> String {
    match id {
        "cuda" => "CUDA (NVIDIA)".to_owned(),
        "qsv" => "QSV (Intel)".to_owned(),
        "d3d11va" => "D3D11VA (Windows)".to_owned(),
        "d3d12va" => "D3D12VA (Windows)".to_owned(),
        "dxva2" => "DXVA2 (Windows)".to_owned(),
        "videotoolbox" => "VideoToolbox (macOS)".to_owned(),
        "vulkan" => "Vulkan".to_owned(),
        "opencl" => "OpenCL".to_owned(),
        "vdpau" => "VDPAU (Linux)".to_owned(),
        "amf" => "AMF (AMD)".to_owned(),
        "mediacodec" => "MediaCodec (Android)".to_owned(),
        "drm" => "DRM (Linux)".to_owned(),
        "vaapi" => "VAAPI (Linux)".to_owned(),
        other => other.to_owned(),
    }
}

fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent()
                .map(|d| d.join("settings").join("system-settings.npb"))
        })
        .unwrap_or_else(|| PathBuf::from("settings/system-settings.npb"))
}

fn save_to_disk(s: &SystemSettingsResource) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let encoded = crate::schema::encode_schema(s);
    std::fs::write(path, encoded)
}

pub(crate) fn load_from_disk() -> Option<SystemSettingsResource> {
    let bytes = std::fs::read(settings_path()).ok()?;
    crate::schema::decode_schema::<SystemSettingsResource>(&bytes).ok()
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
    hw_decode_extra_frames: i32,
    hw_device_type_priority: Vec<String>,
    default_snap: bool,
    magnetic_snap_range: i32,
    export_container: i32,
    export_codec: i32,

    check_update_on_startup: bool,
    update_status: Arc<Mutex<UpdateStatus>>,
    crash_reporting_enabled: bool,

    audio_plugin_settings: AudioPluginSettingsResource,
    new_scan_path: String,
    scan_status: Arc<Mutex<ScanStatus>>,

    compact_ui: bool,
    save_status: String,
}

impl SystemSettingsWindow {
    pub fn new(world_holder: &Arc<Mutex<EcsWorld>>) -> Self {
        if let Some(loaded) = load_from_disk() {
            world_holder.lock().unwrap().set_system_settings(loaded);
        }

        let (easing_engine_ids, easing_engine_names) = easing_engine_ids_and_names();
        let s = world_holder.lock().unwrap().get_system_settings();

        neoutl_media_runtime::runtime::set_worker_threads(s.worker_threads);
        neo_media_ffmpeg::set_hw_decode_extra_frames(s.hw_decode_extra_frames);
        neo_media_ffmpeg::set_hw_device_type_priority(s.hw_device_type_priority.clone());
        crate::theme::restore(&s.theme_id);

        let update_status = Arc::new(Mutex::new(UpdateStatus::Idle));
        if s.check_update_on_startup {
            update::spawn_check(update_status.clone());
        }

        let audio_plugin_settings = plugin_settings::load_from_disk().unwrap_or_default();
        plugin_registry::set_disabled(&audio_plugin_settings.disabled_plugin_ids);

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
            hw_decode_extra_frames: s.hw_decode_extra_frames,
            hw_device_type_priority: s.hw_device_type_priority.clone(),
            default_snap: s.default_snap,
            magnetic_snap_range: s.magnetic_snap_range,
            export_container: s.export_container,
            export_codec: s.export_codec,
            check_update_on_startup: s.check_update_on_startup,
            update_status,
            crash_reporting_enabled: s.crash_reporting_enabled,
            audio_plugin_settings,
            new_scan_path: String::new(),
            scan_status: Arc::new(Mutex::new(ScanStatus::Idle)),
            compact_ui: ui_ext::density() == Density::Compact,
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

    fn persistable_audio_plugin_settings(&self) -> AudioPluginSettingsResource {
        AudioPluginSettingsResource {
            cached_catalog: plugin_registry::get_all_unfiltered(),
            ..self.audio_plugin_settings.clone()
        }
    }

    fn persist_audio_plugin_settings(&self) {
        let _ = plugin_settings::save_to_disk(&self.persistable_audio_plugin_settings());
        plugin_registry::set_disabled(&self.audio_plugin_settings.disabled_plugin_ids);
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
        neoutl_media_runtime::runtime::set_worker_threads(loaded.worker_threads);
        neo_media_ffmpeg::set_hw_decode_extra_frames(loaded.hw_decode_extra_frames);
        neo_media_ffmpeg::set_hw_device_type_priority(loaded.hw_device_type_priority.clone());
        self.hw_device_type_priority = loaded.hw_device_type_priority.clone();

        self.theme_choice = crate::theme::from_id(&loaded.theme_id);
        crate::theme::set(self.theme_choice);
        self.easing_engine_index = index_of(&self.easing_engine_ids, &loaded.easing_engine_id);
        self.autosave_enabled = loaded.autosave_enabled;
        self.autosave_interval_sec = loaded.autosave_interval_sec;
        self.ui_scale_percent = loaded.ui_scale_percent;
        self.worker_threads = loaded.worker_threads;
        self.audio_max_block_size = loaded.audio_max_block_size;
        self.decode_backend = loaded.decode_backend;
        self.hw_decode_extra_frames = loaded.hw_decode_extra_frames;
        self.default_snap = loaded.default_snap;
        self.magnetic_snap_range = loaded.magnetic_snap_range;
        self.export_container = loaded.export_container;
        self.export_codec = loaded.export_codec;
        self.check_update_on_startup = loaded.check_update_on_startup;
        self.crash_reporting_enabled = loaded.crash_reporting_enabled;

        if let Some(loaded_plugins) = plugin_settings::load_from_disk() {
            self.audio_plugin_settings = loaded_plugins;
            plugin_registry::set_disabled(&self.audio_plugin_settings.disabled_plugin_ids);
        }

        self.save_status = t!("再読込完了");
    }

    pub fn show(&mut self, _ctx: &Context, ui: &mut Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        if !self.open {
            return;
        }

        egui::Panel::bottom("system_setting_footer").show(ui, |ui| {
            ui.footer_bar(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), fields::field_height(ui)),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.label(&self.save_status);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(Button::new(t!("保存"))).clicked() {
                                let s = world_holder.lock().unwrap().get_system_settings();
                                let plugin_save = plugin_settings::save_to_disk(
                                    &self.persistable_audio_plugin_settings(),
                                );
                                self.save_status = match (save_to_disk(&s), plugin_save) {
                                    (Ok(()), Ok(())) => t!("保存完了"),
                                    _ => t!("保存失敗"),
                                };
                            }
                            if ui.add(Button::new(t!("再読込")).outline()).clicked() {
                                self.reload(world_holder);
                            }
                        });
                    },
                )
            })
        });

        egui::Panel::left("system_settings_categories").show(ui, |ui| {
            ui.sidebar(|ui| {
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
            })
        });
        egui::Panel::top("system_setting_header").show(ui, |ui| {
            ui.header_bar(|ui| {
                ui.heading(page_title(tr(category_label(
                    self.selected_category as usize,
                ))));
            })
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.page_content(|ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
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
                                6 => self.page_audio_plugins(ui),
                                _ => self.page_update(ui, world_holder),
                            });
                        if self.selected_category == 3 {
                            self.page_decode_wide(ui, world_holder);
                        }
                    });
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

        let mut on = active;
        if ui.add(SegmentedButton::new(&mut on, text)).changed() && on {
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

        let mut compact_ui = self.compact_ui;
        if toggle_field(ui, "コンパクト表示", &mut compact_ui) {
            self.compact_ui = compact_ui;
            ui_ext::set_density(if compact_ui {
                Density::Compact
            } else {
                Density::Comfortable
            });
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
            neoutl_media_runtime::runtime::set_worker_threads(threads);
        }
    }

    fn page_decode(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        debug_assert_eq!(crate::config::DECODE_BACKEND_AUTO, 0);
        debug_assert_eq!(crate::config::DECODE_BACKEND_GPU_FIXED, 1);
        debug_assert_eq!(crate::config::DECODE_BACKEND_CPU_FIXED, 2);
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
        }

        let mut hw_decode_extra_frames = self.hw_decode_extra_frames;
        if int_field(
            ui,
            "HWデコードサーフェス予備数",
            &mut hw_decode_extra_frames,
            crate::config::HW_DECODE_EXTRA_FRAMES_MIN,
            crate::config::HW_DECODE_EXTRA_FRAMES_MAX,
        ) {
            self.hw_decode_extra_frames = hw_decode_extra_frames;
            self.persist(world_holder, |s| {
                s.hw_decode_extra_frames = hw_decode_extra_frames
            });
            neo_media_ffmpeg::set_hw_decode_extra_frames(hw_decode_extra_frames);
        }
    }

    fn page_decode_wide(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        ui.separator();
        ui.add_space(8.0);
        ui.label(tr("HWデコードバックエンド優先順"));
        ui.add_space(4.0);

        let mut priority = self.hw_device_type_priority.clone();
        let mut rows: Vec<elegance::SortableItem> = priority
            .iter()
            .map(|id| elegance::SortableItem::new(id.clone(), hw_backend_display_name(id)))
            .collect();

        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                elegance::SortableList::new("hw_device_type_priority", &mut rows).show(ui);
            });

        priority = rows.into_iter().map(|row| row.id).collect();
        if priority != self.hw_device_type_priority {
            self.hw_device_type_priority = priority.clone();
            self.persist(world_holder, |s| {
                s.hw_device_type_priority = priority.clone()
            });
            neo_media_ffmpeg::set_hw_device_type_priority(priority);
        }

        ui.add_space(8.0);
        if ui.button(t!("既定順に戻す")).clicked() {
            let defaults = neo_media_ffmpeg::default_hw_device_type_priority();
            self.hw_device_type_priority = defaults.clone();
            self.persist(world_holder, |s| {
                s.hw_device_type_priority = defaults.clone()
            });
            neo_media_ffmpeg::set_hw_device_type_priority(defaults);
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

    fn page_audio_plugins(&mut self, ui: &mut egui::Ui) {
        let mut auto_detect_system = self.audio_plugin_settings.auto_detect_system;
        if toggle_field(
            ui,
            "システムにインストールされたプラグインを自動で検知する",
            &mut auto_detect_system,
        ) {
            self.audio_plugin_settings.auto_detect_system = auto_detect_system;
            self.persist_audio_plugin_settings();
        }

        ui.label(t!("走査パス"));
        ui.end_row();

        let mut remove_index: Option<usize> = None;
        for (i, path) in self.audio_plugin_settings.scan_paths.iter().enumerate() {
            ui.label(path);
            if ui.add(Button::new(t!("削除")).outline()).clicked() {
                remove_index = Some(i);
            }
            ui.end_row();
        }
        if let Some(i) = remove_index {
            self.audio_plugin_settings.scan_paths.remove(i);
            self.persist_audio_plugin_settings();
        }

        ui.add_sized(
            egui::vec2(ui.available_width(), fields::field_height(ui)),
            TextInput::new(&mut self.new_scan_path),
        );
        if ui.add(Button::new(t!("パスを追加"))).clicked() && !self.new_scan_path.is_empty() {
            self.audio_plugin_settings
                .scan_paths
                .push(std::mem::take(&mut self.new_scan_path));
            self.persist_audio_plugin_settings();
        }
        ui.end_row();

        ui.separator();
        ui.end_row();

        let status = self.scan_status.lock().unwrap().clone();
        match &status {
            ScanStatus::Idle => {
                ui.horizontal(|ui| {
                    ui.add(Indicator::new(IndicatorState::Off));
                    ui.label(t!("未走査"));
                });
                ui.end_row();
            }
            ScanStatus::Scanning => {
                ui.horizontal(|ui| {
                    ui.add(Spinner::new());
                    ui.label(t!("走査中..."));
                });
                ui.end_row();
            }
            ScanStatus::Done => {
                ui.horizontal(|ui| {
                    ui.add(Indicator::new(IndicatorState::On));
                    ui.label(t!("走査完了"));
                });
                ui.end_row();
            }
            ScanStatus::Error(err) => {
                ui.horizontal(|ui| {
                    ui.add(Indicator::new(IndicatorState::Off));
                    ui.label(t!("エラー: %{arg0}", arg0 = format!("{err}")));
                });
                ui.end_row();
            }
        }

        if ui.add(Button::new(t!("プラグインを再走査"))).clicked() {
            let paths: Vec<PathBuf> = self
                .audio_plugin_settings
                .scan_paths
                .iter()
                .map(PathBuf::from)
                .collect();
            let scan_status = self.scan_status.clone();
            *scan_status.lock().unwrap() = ScanStatus::Scanning;
            let disabled_ids = self.audio_plugin_settings.disabled_plugin_ids.clone();
            let auto_detect_system = self.audio_plugin_settings.auto_detect_system;
            std::thread::spawn(move || {
                let entries = plugin_registry::rescan(&paths, auto_detect_system);
                plugin_registry::set_disabled(&disabled_ids);
                let saved = AudioPluginSettingsResource {
                    scan_paths: paths
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect(),
                    disabled_plugin_ids: disabled_ids,
                    cached_catalog: entries,
                    auto_detect_system,
                };
                *scan_status.lock().unwrap() = match plugin_settings::save_to_disk(&saved) {
                    Ok(()) => ScanStatus::Done,
                    Err(err) => ScanStatus::Error(format!("{err}")),
                };
            });
        }
        ui.end_row();

        ui.separator();
        ui.end_row();

        ui.label(t!("検出済みプラグイン"));
        ui.end_row();

        let catalog: Vec<PluginCatalogEntry> = plugin_registry::get_all_unfiltered();
        let mut toggled: Option<(String, bool)> = None;
        for entry in &catalog {
            let mut enabled = !plugin_registry::is_disabled(&entry.plugin_id);
            let label = format!("{} ({:?})", entry.name, entry.format);
            ui.label(&label);
            if ui.add(Switch::new(&mut enabled, "")).changed() {
                toggled = Some((entry.plugin_id.clone(), !enabled));
            }
            ui.end_row();
        }
        if let Some((plugin_id, disabled)) = toggled {
            let ids = &mut self.audio_plugin_settings.disabled_plugin_ids;
            if disabled {
                if !ids.contains(&plugin_id) {
                    ids.push(plugin_id);
                }
            } else {
                ids.retain(|id| id != &plugin_id);
            }
            self.persist_audio_plugin_settings();
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
                ui.horizontal(|ui| {
                    ui.add(Indicator::new(IndicatorState::Off));
                    ui.label(t!("未確認"));
                });
                ui.end_row();
            }
            UpdateStatus::Checking => {
                ui.horizontal(|ui| {
                    ui.add(Spinner::new());
                    ui.label(t!("確認中..."));
                });
                ui.end_row();
            }
            UpdateStatus::UpToDate => {
                ui.horizontal(|ui| {
                    ui.add(Indicator::new(IndicatorState::On));
                    ui.label(t!("最新版です"));
                });
                ui.end_row();
            }
            UpdateStatus::Available(info) => {
                ui.horizontal(|ui| {
                    ui.add(Indicator::new(IndicatorState::Connecting));
                    ui.label(t!(
                        "新バージョン: %{arg0}",
                        arg0 = format!("{}", info.version)
                    ));
                });
                ui.end_row();
                ui.label(&info.notes);
                ui.end_row();
                if ui.add(Button::new(t!("今すぐ更新"))).clicked() {
                    update::spawn_apply(self.update_status.clone(), info.clone());
                }
                ui.end_row();
            }
            UpdateStatus::Downloading(fraction) => {
                ui.horizontal(|ui| {
                    ui.add(Spinner::new());
                    ui.label(t!("ダウンロード中"));
                });
                ui.add(ProgressBar::new(fraction));
                ui.end_row();
            }
            UpdateStatus::Installed => {
                ui.horizontal(|ui| {
                    ui.add(Indicator::new(IndicatorState::On));
                    ui.label(t!("更新完了。再起動してください"));
                });
                ui.end_row();
            }
            UpdateStatus::Error(err) => {
                ui.horizontal(|ui| {
                    ui.add(Indicator::new(IndicatorState::Off));
                    ui.label(t!("エラー: %{arg0}", arg0 = format!("{err}")));
                });
                ui.end_row();
            }
        }

        if ui.add(Button::new(t!("今すぐ確認")).outline()).clicked() {
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
