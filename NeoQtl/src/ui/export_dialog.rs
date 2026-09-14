use crate::app_state::SharedAppState;
use crate::export::{EncoderBackend, ExportCodec, ExportJob, ExportPreset};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ExportDialog {
    pub open: bool,
    pub presets: Vec<ExportPreset>,
    pub selected_preset: i32,
    pub preset_name: String,

    pub output_path: String,
    pub codec: i32,
    pub backend: i32,
    pub mkv_container: bool,
    pub average_bitrate_kbps: i32,
    pub max_bitrate_kbps: i32,
    pub start_frame: i32,
    pub end_frame: i32,
    pub total_frames: i32,

    pub progress: Arc<Mutex<(i32, i32)>>,
    pub status_text: String,
    pub status_is_error: bool,
    pub active_queue: Option<crate::export::RenderQueue>,
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
            progress: Arc::new(Mutex::new((0, 0))),
            status_text: String::new(),
            status_is_error: false,
            active_queue: None,
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

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn apply_preset(&mut self, index: usize) {
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

    pub fn save_preset(&mut self) {
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

    pub fn delete_preset(&mut self) {
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

    pub fn pick_output_path(&mut self) {
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

    pub fn start_export(&mut self, state: &SharedAppState) {
        if self.output_path.is_empty() {
            return;
        }

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
            cancel: None,
        };

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
        let pending = queue.pending_count();
        self.active_queue = Some(queue);
        self.status_text = format!("レンダーキューに追加しました({pending}件待機中)");
    }
}
