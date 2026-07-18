// src/ui/system_settings.rs
use crate::SystemSettingsWindow;
use crate::ecs::{EcsWorld, resources::SystemSettingsResource};
use slint::ComponentHandle;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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

pub fn setup(window: &SystemSettingsWindow, world_holder: Arc<Mutex<EcsWorld>>) {
    if let Some(loaded) = load_from_disk() {
        world_holder.lock().unwrap().set_system_settings(loaded);
    }

    let initial = world_holder.lock().unwrap().get_system_settings();
    crate::media::runtime::set_worker_threads(initial.worker_threads);
    crate::media::runtime::apply_decode_backend_env(initial.decode_backend);
    sync_from_resource(window, &initial);

    {
        let wc = world_holder.clone();
        window.on_set_general(move |autosave_enabled, autosave_interval_sec| {
            let mut world = wc.lock().unwrap();
            let mut s = world.get_system_settings();
            s.autosave_enabled = autosave_enabled;
            s.autosave_interval_sec = autosave_interval_sec;
            world.set_system_settings(s);
        });
    }

    {
        let wc = world_holder.clone();
        window.on_set_appearance(move |theme_dark, ui_scale_percent| {
            let mut world = wc.lock().unwrap();
            let mut s = world.get_system_settings();
            s.theme_dark = theme_dark;
            s.ui_scale_percent = ui_scale_percent;
            world.set_system_settings(s);
        });
    }

    {
        let wc = world_holder.clone();
        window.on_set_performance(move |worker_threads, audio_max_block_size| {
            let mut world = wc.lock().unwrap();
            let mut s = world.get_system_settings();
            s.worker_threads = worker_threads;
            s.audio_max_block_size = audio_max_block_size;
            world.set_system_settings(s);
            crate::media::runtime::set_worker_threads(worker_threads);
        });
    }

    {
        let wc = world_holder.clone();
        window.on_set_decode(move |decode_backend| {
            let mut world = wc.lock().unwrap();
            let mut s = world.get_system_settings();
            s.decode_backend = decode_backend;
            world.set_system_settings(s);
            crate::media::runtime::apply_decode_backend_env(decode_backend);
        });
    }

    {
        let wc = world_holder.clone();
        window.on_set_timeline_defaults(move |default_snap, magnetic_snap_range| {
            let mut world = wc.lock().unwrap();
            let mut s = world.get_system_settings();
            s.default_snap = default_snap;
            s.magnetic_snap_range = magnetic_snap_range;
            world.set_system_settings(s);
        });
    }

    {
        let wc = world_holder.clone();
        window.on_set_export(move |export_container, export_codec| {
            let mut world = wc.lock().unwrap();
            let mut s = world.get_system_settings();
            s.export_container = export_container;
            s.export_codec = export_codec;
            world.set_system_settings(s);
        });
    }

    {
        let wc = world_holder.clone();
        let weak = window.as_weak();
        window.on_save_settings(move || {
            let s = wc.lock().unwrap().get_system_settings();
            let result = save_to_disk(&s);
            if let Some(win) = weak.upgrade() {
                win.set_save_status(match result {
                    Ok(()) => "保存完了".into(),
                    Err(_) => "保存失敗".into(),
                });
            }
        });
    }

    {
        let wc = world_holder.clone();
        let weak = window.as_weak();
        window.on_reload_settings(move || {
            if let Some(loaded) = load_from_disk() {
                wc.lock().unwrap().set_system_settings(loaded.clone());
                crate::media::runtime::set_worker_threads(loaded.worker_threads);
                crate::media::runtime::apply_decode_backend_env(loaded.decode_backend);
                if let Some(win) = weak.upgrade() {
                    sync_from_resource(&win, &loaded);
                    win.set_save_status("再読込完了".into());
                }
            } else if let Some(win) = weak.upgrade() {
                win.set_save_status("設定ファイルなし".into());
            }
        });
    }
}

fn sync_from_resource(window: &SystemSettingsWindow, s: &SystemSettingsResource) {
    window.set_autosave_enabled(s.autosave_enabled);
    window.set_autosave_interval_sec(s.autosave_interval_sec);
    window.set_theme_dark(s.theme_dark);
    window.set_ui_scale_percent(s.ui_scale_percent);
    window.set_worker_threads(s.worker_threads);
    window.set_audio_max_block_size(s.audio_max_block_size);
    window.set_decode_backend(s.decode_backend);
    window.set_default_snap(s.default_snap);
    window.set_magnetic_snap_range(s.magnetic_snap_range);
    window.set_export_container(s.export_container);
    window.set_export_codec(s.export_codec);
}
