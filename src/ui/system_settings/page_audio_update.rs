use super::fields::{self, toggle_field};
use super::helpers::ScanStatus;
use super::window::SystemSettingsWindow;
use crate::app::update::{self, UpdateStatus};
use crate::audio::{plugin_registry, plugin_settings};
use crate::ecs::EcsWorld;
use crate::ecs::resources::AudioPluginSettingsResource;
use crate::ui::ui_ext::row_style;
use egui_taffy::{TuiBuilderLogic, tui};
use elegance::{Button, Indicator, IndicatorState, ProgressBar, Spinner, Switch, TextInput};
use maolan_host_adapter::PluginCatalogEntry;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

impl SystemSettingsWindow {
    pub(super) fn page_audio_plugins(&mut self, ui: &mut egui::Ui) {
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
                tui(ui, ui.id().with("scan_status_idle_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Indicator::new(IndicatorState::Off));
                        });
                        tui.ui(|ui| {
                            ui.label(t!("未走査"));
                        });
                    });
                ui.end_row();
            }
            ScanStatus::Scanning => {
                tui(ui, ui.id().with("scan_status_scanning_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Spinner::new());
                        });
                        tui.ui(|ui| {
                            ui.label(t!("走査中..."));
                        });
                    });
                ui.end_row();
            }
            ScanStatus::Done => {
                tui(ui, ui.id().with("scan_status_done_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Indicator::new(IndicatorState::On));
                        });
                        tui.ui(|ui| {
                            ui.label(t!("走査完了"));
                        });
                    });
                ui.end_row();
            }
            ScanStatus::Error(err) => {
                tui(ui, ui.id().with("scan_status_error_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Indicator::new(IndicatorState::Off));
                        });
                        tui.ui(|ui| {
                            ui.label(t!("エラー: %{arg0}", arg0 = format!("{err}")));
                        });
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

    pub(super) fn page_update(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
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
                tui(ui, ui.id().with("update_status_idle_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Indicator::new(IndicatorState::Off));
                        });
                        tui.ui(|ui| {
                            ui.label(t!("未確認"));
                        });
                    });
                ui.end_row();
            }
            UpdateStatus::Checking => {
                tui(ui, ui.id().with("update_status_checking_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Spinner::new());
                        });
                        tui.ui(|ui| {
                            ui.label(t!("確認中..."));
                        });
                    });
                ui.end_row();
            }
            UpdateStatus::UpToDate => {
                tui(ui, ui.id().with("update_status_uptodate_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Indicator::new(IndicatorState::On));
                        });
                        tui.ui(|ui| {
                            ui.label(t!("最新版です"));
                        });
                    });
                ui.end_row();
            }
            UpdateStatus::Available(info) => {
                tui(ui, ui.id().with("update_status_available_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Indicator::new(IndicatorState::Connecting));
                        });
                        tui.ui(|ui| {
                            ui.label(t!(
                                "新バージョン: %{arg0}",
                                arg0 = format!("{}", info.version)
                            ));
                        });
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
                tui(ui, ui.id().with("update_status_downloading_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Spinner::new());
                        });
                        tui.ui(|ui| {
                            ui.label(t!("ダウンロード中"));
                        });
                    });
                ui.add(ProgressBar::new(fraction));
                ui.end_row();
            }
            UpdateStatus::Installed => {
                tui(ui, ui.id().with("update_status_installed_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Indicator::new(IndicatorState::On));
                        });
                        tui.ui(|ui| {
                            ui.label(t!("更新完了。再起動してください"));
                        });
                    });
                ui.end_row();
            }
            UpdateStatus::Error(err) => {
                tui(ui, ui.id().with("update_status_error_row"))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.add(Indicator::new(IndicatorState::Off));
                        });
                        tui.ui(|ui| {
                            ui.label(t!("エラー: %{arg0}", arg0 = format!("{err}")));
                        });
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
