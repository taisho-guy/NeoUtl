use super::helpers::{
    ScanStatus, easing_engine_ids_and_names, index_of, load_from_disk, save_to_disk,
};
use crate::audio::{plugin_registry, plugin_settings};
use crate::ecs::{
    EcsWorld,
    resources::{AudioPluginSettingsResource, SystemSettingsResource},
};
use crate::update::UpdateStatus;
use std::sync::{Arc, Mutex};

pub struct SystemSettingsWindow {
    pub open: bool,
    pub selected_category: i32,

    pub easing_engine_ids: Vec<String>,
    pub easing_engine_names: Vec<String>,
    pub easing_engine_index: i32,

    pub autosave_enabled: bool,
    pub autosave_interval_sec: i32,
    pub ui_scale_percent: i32,
    pub worker_threads: i32,
    pub audio_max_block_size: i32,
    pub decode_backend: i32,
    pub hw_decode_extra_frames: i32,
    pub hw_device_type_priority: Vec<String>,
    pub default_snap: bool,
    pub magnetic_snap_range: i32,

    pub check_update_on_startup: bool,
    pub update_status: Arc<Mutex<UpdateStatus>>,
    pub crash_reporting_enabled: bool,

    pub audio_plugin_settings: AudioPluginSettingsResource,
    pub new_scan_path: String,
    pub(super) scan_status: Arc<Mutex<ScanStatus>>,

    pub save_status: String,
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
            crate::update::spawn_check(update_status.clone());
        }

        let audio_plugin_settings = plugin_settings::load_from_disk().unwrap_or_default();
        plugin_registry::set_disabled(&audio_plugin_settings.disabled_plugin_ids);

        Self {
            open: false,
            selected_category: 0,
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
            check_update_on_startup: s.check_update_on_startup,
            update_status,
            crash_reporting_enabled: s.crash_reporting_enabled,
            audio_plugin_settings,
            new_scan_path: String::new(),
            scan_status: Arc::new(Mutex::new(ScanStatus::Idle)),
            save_status: String::new(),
        }
    }

    pub fn save(&mut self, world_holder: &Arc<Mutex<EcsWorld>>) -> std::io::Result<()> {
        let easing_engine_id = self
            .easing_engine_ids
            .get(self.easing_engine_index as usize)
            .cloned()
            .unwrap_or_default();

        let s = SystemSettingsResource {
            theme_dark: true,
            theme_id: "slate".into(),
            easing_engine_id,
            autosave_enabled: self.autosave_enabled,
            autosave_interval_sec: self.autosave_interval_sec,
            ui_scale_percent: self.ui_scale_percent,
            worker_threads: self.worker_threads,
            audio_max_block_size: self.audio_max_block_size,
            decode_backend: self.decode_backend,
            hw_decode_extra_frames: self.hw_decode_extra_frames,
            hw_device_type_priority: self.hw_device_type_priority.clone(),
            default_snap: self.default_snap,
            magnetic_snap_range: self.magnetic_snap_range,
            check_update_on_startup: self.check_update_on_startup,
            crash_reporting_enabled: self.crash_reporting_enabled,
            max_group_chain_depth: 16,
        };

        save_to_disk(&s)?;
        world_holder.lock().unwrap().set_system_settings(s);
        self.save_status = "設定を保存しました".into();
        Ok(())
    }
}
