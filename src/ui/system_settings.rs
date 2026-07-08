// src/ui/system_settings.rs
use crate::SystemSettingsWindow;
use crate::ecs::{EcsWorld, resources::SystemSettingsResource};
use slint::ComponentHandle;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent()
                .map(|d| d.join("settings").join("system-settings.toml"))
        })
        .unwrap_or_else(|| PathBuf::from("settings/system-settings.toml"))
}

fn serialize(s: &SystemSettingsResource) -> String {
    format!(
        "autosave_enabled = {}\n\
         autosave_interval_sec = {}\n\
         theme_dark = {}\n\
         ui_scale_percent = {}\n\
         worker_threads = {}\n\
         audio_max_block_size = {}\n\
         decode_backend = {}\n\
         default_snap = {}\n\
         magnetic_snap_range = {}\n\
         export_container = {}\n\
         export_codec = {}\n",
        s.autosave_enabled,
        s.autosave_interval_sec,
        s.theme_dark,
        s.ui_scale_percent,
        s.worker_threads,
        s.audio_max_block_size,
        s.decode_backend,
        s.default_snap,
        s.magnetic_snap_range,
        s.export_container,
        s.export_codec,
    )
}

fn save_to_disk(s: &SystemSettingsResource) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serialize(s))
}

fn parse_toml_pairs(source: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(mut parser) = tree_sitter_language_pack::get_parser("toml") else {
        return map;
    };
    let Some(tree) = parser.parse(source) else {
        return map;
    };
    let bytes = source.as_bytes();
    let root = tree.root_node();
    let text_of = |node: &tree_sitter_language_pack::Node| -> String {
        let r = node.byte_range();
        String::from_utf8_lossy(&bytes[r.start..r.end])
            .trim()
            .to_string()
    };

    for i in 0..root.child_count() {
        let Some(node) = root.child(i as u32) else {
            continue;
        };
        if node.kind() != "pair" {
            continue;
        }
        let key_node = node.child_by_field_name("key");
        let value_node = node.child_by_field_name("value");
        if let (Some(k), Some(v)) = (key_node, value_node) {
            map.insert(text_of(&k), text_of(&v));
        }
    }
    map
}

fn load_from_disk() -> Option<SystemSettingsResource> {
    let content = std::fs::read_to_string(settings_path()).ok()?;
    let map = parse_toml_pairs(&content);
    let defaults = SystemSettingsResource::new();

    let get_bool =
        |key: &str, fallback: bool| map.get(key).map(|v| v == "true").unwrap_or(fallback);
    let get_int = |key: &str, fallback: i32| {
        map.get(key)
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(fallback)
    };

    Some(SystemSettingsResource {
        autosave_enabled: get_bool("autosave_enabled", defaults.autosave_enabled),
        autosave_interval_sec: get_int("autosave_interval_sec", defaults.autosave_interval_sec),
        theme_dark: get_bool("theme_dark", defaults.theme_dark),
        ui_scale_percent: get_int("ui_scale_percent", defaults.ui_scale_percent),
        worker_threads: get_int("worker_threads", defaults.worker_threads),
        audio_max_block_size: get_int("audio_max_block_size", defaults.audio_max_block_size),
        decode_backend: get_int("decode_backend", defaults.decode_backend),
        default_snap: get_bool("default_snap", defaults.default_snap),
        magnetic_snap_range: get_int("magnetic_snap_range", defaults.magnetic_snap_range),
        export_container: get_int("export_container", defaults.export_container),
        export_codec: get_int("export_codec", defaults.export_codec),
    })
}

pub fn setup(window: &SystemSettingsWindow, world_holder: Arc<Mutex<EcsWorld>>) {
    if let Some(loaded) = load_from_disk() {
        world_holder.lock().unwrap().set_system_settings(loaded);
    }

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
