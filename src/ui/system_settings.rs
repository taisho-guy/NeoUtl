// src/ui/system_settings.rs
use crate::SystemSettingsWindow;
use crate::ecs::{EcsWorld, resources::SystemSettingsResource};
use std::sync::{Arc, Mutex};

pub fn setup(window: &SystemSettingsWindow, world_holder: Arc<Mutex<EcsWorld>>) {
    sync_from_resource(window, &world_holder.lock().unwrap().get_system_settings());

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
        });
    }

    {
        let wc = world_holder.clone();
        window.on_set_decode(move |decode_backend| {
            let mut world = wc.lock().unwrap();
            let mut s = world.get_system_settings();
            s.decode_backend = decode_backend;
            world.set_system_settings(s);
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
