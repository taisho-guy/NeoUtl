use super::fields::{choice_field, int_field};
use super::helpers::hw_backend_display_name;
use super::window::SystemSettingsWindow;
use crate::ecs::EcsWorld;
use crate::infra::localization::tr;
use std::sync::{Arc, Mutex};

impl SystemSettingsWindow {
    pub(super) fn page_performance(
        &mut self,
        ui: &mut egui::Ui,
        world_holder: &Arc<Mutex<EcsWorld>>,
    ) {
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

    pub(super) fn page_decode(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        debug_assert_eq!(crate::project::config::DECODE_BACKEND_AUTO, 0);
        debug_assert_eq!(crate::project::config::DECODE_BACKEND_GPU_FIXED, 1);
        debug_assert_eq!(crate::project::config::DECODE_BACKEND_CPU_FIXED, 2);
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
            crate::project::config::HW_DECODE_EXTRA_FRAMES_MIN,
            crate::project::config::HW_DECODE_EXTRA_FRAMES_MAX,
        ) {
            self.hw_decode_extra_frames = hw_decode_extra_frames;
            self.persist(world_holder, |s| {
                s.hw_decode_extra_frames = hw_decode_extra_frames
            });
            neo_media_ffmpeg::set_hw_decode_extra_frames(hw_decode_extra_frames);
        }
    }

    pub(super) fn page_decode_wide(
        &mut self,
        ui: &mut egui::Ui,
        world_holder: &Arc<Mutex<EcsWorld>>,
    ) {
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
}
