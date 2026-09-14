#![recursion_limit = "256"]

rust_i18n::i18n!("i18n");
extern crate rust_i18n;

macro_rules! t {
    ($($args:tt)*) => {
        rust_i18n::t!($($args)*).to_string()
    };
}

mod app_state;
mod audio;
mod config;
mod crash_report;
mod document;
mod easings;
mod ecs;
mod effects;
mod export;
mod ffi;
mod font_stack;
mod gpu_shared;
mod hot_reload;
mod localization;
mod objects;
mod project;
mod renderer;
mod rhi_bridge;
mod schema;
mod shortcuts;
pub mod system_settings;
mod theme;
pub mod ui;
mod update;

use cxx_qt_lib::{QByteArray, QGuiApplication, QQmlApplicationEngine, QUrl};

unsafe extern "C" {
    fn ensure_qml_aot_cache_registered();
}

fn configure_decode_runtime() {
    let sys = system_settings::load_from_disk().unwrap_or_default();
    neoutl_media_runtime::runtime::set_worker_threads(sys.worker_threads);
}

fn main() {
    unsafe {
        ensure_qml_aot_cache_registered();
    }
    localization::initialize();
    let crash_reporting_enabled = system_settings::load_from_disk()
        .unwrap_or_default()
        .crash_reporting_enabled;
    let _sentry_guard = crash_report::init(crash_reporting_enabled);
    let _ = project::begin_runtime_session();

    let (init_done_tx, init_done_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("neoqtl-init".into())
        .spawn(move || {
            configure_decode_runtime();
            objects::load_all(&objects::default_objects_dir());
            effects::load_all(
                &effects::default_effects_dir(),
                &effects::default_effects_lua_dir(),
            );
            neoutl_media_runtime::loader::load_all(
                &neoutl_media_runtime::loader::default_decoders_dir(),
            );
            easings::loader::load_all(&easings::loader::default_easings_dir());
            font_stack::preload_installed_fonts();
            match audio::plugin_settings::load_from_disk() {
                Some(saved) => {
                    audio::plugin_registry::init_from_cache(
                        saved.cached_catalog,
                        &saved.disabled_plugin_ids,
                    );
                }
                None => {
                    let default_dir = audio::plugin_registry::default_plugins_dir();
                    let paths = vec![default_dir];
                    let auto_detect_system = true;
                    let entries = audio::plugin_registry::rescan(&paths, auto_detect_system);
                    let saved = ecs::resources::AudioPluginSettingsResource {
                        scan_paths: paths
                            .iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect(),
                        disabled_plugin_ids: Vec::new(),
                        cached_catalog: entries,
                        auto_detect_system,
                    };
                    let _ = audio::plugin_settings::save_to_disk(&saved);
                }
            }
            let _ = init_done_tx.send(());
        })
        .expect("初期化スレッド起動失敗");

    ffi::register();

    let mut application = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    let url = QUrl::from_encoded(&QByteArray::from("qrc:/NeoQtl/src/ui/qml/AppRoot.qml"));
    engine.pin_mut().load(&url);

    let _ = init_done_rx.recv();

    application.pin_mut().exec();
    project::finish_runtime_session();
}
