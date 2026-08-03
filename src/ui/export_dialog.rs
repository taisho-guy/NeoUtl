use crate::app_state::SharedAppState;
use crate::export::{EncoderBackend, ExportCodec, ExportJob, ExportPreset};
use egui::{Context, Ui};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct ExportDialog {
    pub open: bool,
    presets: Vec<ExportPreset>,
    selected_preset: i32,
    preset_name: String,

    output_path: String,
    codec: i32,
    backend: i32,
    mkv_container: bool,
    average_bitrate_kbps: i32,
    max_bitrate_kbps: i32,
    start_frame: i32,
    end_frame: i32,
    total_frames: i32,

    is_exporting: bool,
    progress: Arc<Mutex<(i32, i32)>>,
    status_text: String,
    status_is_error: bool,
    cancel: Option<Arc<AtomicBool>>,
}

impl ExportDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            presets: crate::export::load_export_presets(),
            selected_preset: -1,
            preset_name: String::new(),
            output_path: String::new(),
            codec: 0,
            backend: 0,
            mkv_container: false,
            average_bitrate_kbps: 8000,
            max_bitrate_kbps: 12000,
            start_frame: 0,
            end_frame: 0,
            total_frames: 0,
            is_exporting: false,
            progress: Arc::new(Mutex::new((0, 0))),
            status_text: String::new(),
            status_is_error: false,
            cancel: None,
        }
    }

    pub fn open(&mut self, state: &SharedAppState) {
        let total_frames = {
            let world_holder = crate::app_state::active_world(state);
            let world = world_holder.lock().unwrap();
            world.total_frames()
        };
        self.total_frames = total_frames;
        self.start_frame = 0;
        self.end_frame = total_frames;
        self.status_text.clear();
        self.presets = crate::export::load_export_presets();
        if let Some(first) = self.presets.first() {
            self.selected_preset = 0;
            self.preset_name = first.name.clone();
            self.apply_preset(0);
        } else {
            self.selected_preset = -1;
            self.preset_name.clear();
        }
        self.open = true;
    }

    fn apply_preset(&mut self, index: usize) {
        let Some(preset) = self.presets.get(index) else {
            return;
        };
        self.preset_name = preset.name.clone();
        self.codec = if preset.codec == ExportCodec::H264 {
            0
        } else {
            1
        };
        self.backend = match preset.backend {
            EncoderBackend::GpuVideo => 1,
            EncoderBackend::Gstreamer => 2,
            EncoderBackend::Auto => 0,
        };
        self.average_bitrate_kbps = (preset.average_bitrate / 1000) as i32;
        self.max_bitrate_kbps = (preset.max_bitrate / 1000) as i32;
        self.mkv_container = preset.container_ext.eq_ignore_ascii_case("mkv");
    }

    fn save_preset(&mut self) {
        let name = self.preset_name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let preset = ExportPreset {
            name: name.clone(),
            codec: if self.codec == 0 {
                ExportCodec::H264
            } else {
                ExportCodec::H265
            },
            backend: match self.backend {
                1 => EncoderBackend::GpuVideo,
                2 => EncoderBackend::Gstreamer,
                _ => EncoderBackend::Auto,
            },
            average_bitrate: self.average_bitrate_kbps.max(0) as u32 * 1000,
            max_bitrate: self.max_bitrate_kbps.max(0) as u32 * 1000,
            container_ext: if self.mkv_container {
                "mkv".into()
            } else {
                "mp4".into()
            },
        };
        if let Some(old) = self.presets.iter_mut().find(|p| p.name == name) {
            *old = preset;
        } else {
            self.presets.push(preset);
        }
        let _ = crate::export::save_export_presets(&self.presets);
        self.status_text = "プリセットを保存しました".into();
    }

    fn delete_preset(&mut self) {
        if self.selected_preset < 0 {
            return;
        }
        let index = self.selected_preset as usize;
        if index < self.presets.len() {
            self.presets.remove(index);
        }
        let _ = crate::export::save_export_presets(&self.presets);
        self.selected_preset = -1;
        self.preset_name.clear();
        self.status_text = "プリセットを削除しました".into();
    }

    fn pick_output_path(&mut self) {
        let mut picker = rfd::FileDialog::new();
        picker = if self.mkv_container {
            picker.add_filter("Matroska", &["mkv"])
        } else {
            picker.add_filter("MP4", &["mp4"])
        };
        if let Some(path) = picker.save_file() {
            self.output_path = path.to_string_lossy().into_owned();
        }
    }

    fn start_export(&mut self, state: &SharedAppState) {
        if self.output_path.is_empty() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());

        let progress = self.progress.clone();
        let job = ExportJob {
            output_path: self.output_path.clone().into(),
            codec: if self.codec == 0 {
                ExportCodec::H264
            } else {
                ExportCodec::H265
            },
            backend: match self.backend {
                1 => EncoderBackend::GpuVideo,
                2 => EncoderBackend::Gstreamer,
                _ => EncoderBackend::Auto,
            },
            average_bitrate: self.average_bitrate_kbps as u32 * 1000,
            max_bitrate: self.max_bitrate_kbps as u32 * 1000,
            start_frame: self.start_frame,
            end_frame: self.end_frame,
            progress: Some(Box::new(move |current, total| {
                *progress.lock().unwrap() = (current, total);
            })),
            cancel: Some(cancel),
        };

        self.is_exporting = true;
        self.status_is_error = false;
        self.status_text.clear();

        let project_dir = {
            let s = state.lock().unwrap();
            s.sessions[s.active].meta.dir.clone()
        };
        let queue = {
            let s = state.lock().unwrap();
            s.render_queue.clone()
        };
        queue.enqueue(job, project_dir);
        queue.start(state.clone());
        self.is_exporting = false;
        self.status_text = "レンダーキューに追加しました".into();
    }

    pub fn show(&mut self, _ctx: &Context, ui: &mut Ui, state: &SharedAppState) {
        if !self.open {
            return;
        }

        let (progress_current, progress_total) = *self.progress.lock().unwrap();

        let mut close_requested = false;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("書き出しプリセット");
            ui.horizontal(|ui| {
                let current = self
                    .presets
                    .get(self.selected_preset.max(0) as usize)
                    .map(|p| p.name.as_str())
                    .unwrap_or("(未選択)");
                if ui.button(current).clicked() && !self.presets.is_empty() {
                    let next = (self.selected_preset + 1).rem_euclid(self.presets.len() as i32);
                    self.selected_preset = next;
                    self.apply_preset(next as usize);
                }
                if ui.button("保存").clicked() {
                    self.save_preset();
                }
                ui.add_enabled(self.selected_preset >= 0, egui::Button::new("削除"))
                    .clicked()
                    .then(|| self.delete_preset());
            });
            ui.text_edit_singleline(&mut self.preset_name);

            ui.label("出力ファイル");
            ui.horizontal(|ui| {
                let display = if self.output_path.is_empty() {
                    "未選択"
                } else {
                    &self.output_path
                };
                ui.label(display);
                ui.add_enabled(!self.is_exporting, egui::Button::new("選択..."))
                    .clicked()
                    .then(|| self.pick_output_path());
            });

            ui.label("映像コーデック");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.codec, 0, "H.264");
                ui.selectable_value(&mut self.codec, 1, "H.265");
            });

            ui.label("エンコーダー");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.backend, 0, "自動(HW優先)");
                ui.selectable_value(&mut self.backend, 1, "GPU HW固定");
                ui.selectable_value(&mut self.backend, 2, "ソフトウェア");
            });

            ui.label("コンテナ");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mkv_container, false, "MP4 + AAC");
                ui.selectable_value(&mut self.mkv_container, true, "MKV + Opus");
            });

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("平均ビットレート(kbps)");
                    ui.add(
                        egui::DragValue::new(&mut self.average_bitrate_kbps).range(500..=200000),
                    );
                });
                ui.vertical(|ui| {
                    ui.label("最大ビットレート(kbps)");
                    ui.add(egui::DragValue::new(&mut self.max_bitrate_kbps).range(500..=200000));
                });
            });

            ui.horizontal(|ui| {
                let end_max = if self.total_frames > 0 {
                    self.total_frames - 1
                } else {
                    0
                };
                ui.vertical(|ui| {
                    ui.label("開始フレーム");
                    ui.add(egui::DragValue::new(&mut self.start_frame).range(0..=end_max));
                });
                ui.vertical(|ui| {
                    ui.label("終了フレーム");
                    ui.add(egui::DragValue::new(&mut self.end_frame).range(1..=self.total_frames));
                });
            });

            if self.is_exporting {
                ui.label(format!("書き出し中: {progress_current} / {progress_total}"));
                let fraction = if progress_total > 0 {
                    progress_current as f32 / progress_total as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(fraction));
            }

            if !self.status_text.is_empty() {
                let color = if self.status_is_error {
                    egui::Color32::from_rgb(0xd9, 0x4f, 0x4f)
                } else {
                    egui::Color32::from_rgb(0x4f, 0xd9, 0x7a)
                };
                ui.colored_label(color, &self.status_text);
            }

            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let start_enabled = !self.is_exporting && !self.output_path.is_empty();
                if ui
                    .add_enabled(start_enabled, egui::Button::new("書き出し"))
                    .clicked()
                {
                    self.start_export(state);
                }
                let close_label = if self.is_exporting {
                    "中止"
                } else {
                    "閉じる"
                };
                if ui.button(close_label).clicked() {
                    if self.is_exporting {
                        if let Some(c) = &self.cancel {
                            c.store(true, Ordering::Relaxed);
                        }
                    } else {
                        close_requested = true;
                    }
                }
            });
        });
        self.open = !close_requested;
    }
}
