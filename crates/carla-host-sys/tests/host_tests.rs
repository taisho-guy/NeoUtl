use carla_host_sys::{
    BinaryType, CarlaHost, EngineOption, EngineProcessMode, EngineTransportMode, PluginType,
    get_engine_driver_count, get_engine_driver_name,
};
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_driver_discovery() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let count = get_engine_driver_count();
    println!("Available engine driver count: {}", count);
    for i in 0..count {
        if let Some(name) = get_engine_driver_name(i) {
            println!("Driver {}: {}", i, name);
        }
    }
}

#[test]
fn test_host_creation_and_lifecycle() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let mut host = CarlaHost::new().expect("Failed to initialize CarlaHost");
    assert!(!host.raw_handle().is_null());

    assert!(!host.is_running());

    host.set_engine_option(
        EngineOption::ProcessMode,
        EngineProcessMode::ContinuousRack as i32,
        None,
    )
    .unwrap();
    host.set_engine_option(
        EngineOption::TransportMode,
        EngineTransportMode::Internal as i32,
        None,
    )
    .unwrap();

    host.init_engine("Dummy", "NeoUtlTestClient")
        .expect("Failed to init engine with Dummy driver");

    assert!(host.is_running());
    host.idle();

    println!("Engine buffer size: {}", host.buffer_size());
    println!("Engine sample rate: {}", host.sample_rate());

    host.transport_bpm(128.0);
    host.transport_relocate(44100);
    host.transport_play();
    let frame = host.current_transport_frame();
    println!("Current transport frame: {}", frame);

    let info = host.transport_info();
    println!("Transport info: {:?}", info);

    assert_eq!(host.plugin_count(), 0);

    match host.add_plugin(
        BinaryType::NATIVE,
        PluginType::Internal,
        None,
        Some("bypass"),
        Some("bypass"),
        0,
        0,
    ) {
        Ok(plugin_id) => {
            println!("Added bypass plugin with ID: {}", plugin_id);
            assert_eq!(host.plugin_count(), 1);

            if let Some(info) = host.plugin_info(plugin_id) {
                println!("Plugin info: name='{}', maker='{}'", info.name, info.maker);
            }

            let audio_ports = host.audio_port_count(plugin_id);
            println!(
                "Audio ports: ins={}, outs={}",
                audio_ports.ins, audio_ports.outs
            );

            let param_ports = host.parameter_count(plugin_id);
            println!(
                "Param count: ins={}, outs={}",
                param_ports.ins, param_ports.outs
            );

            host.remove_plugin(plugin_id)
                .expect("Failed to remove plugin");
            assert_eq!(host.plugin_count(), 0);
        }
        Err(e) => {
            println!("add_plugin note (internal plugin): {:?}", e);
        }
    }

    host.close_engine().expect("Failed to close engine");
    assert!(!host.is_running());
}

#[cfg(feature = "egui")]
#[test]
fn test_egui_embedded_ui() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let mut host = CarlaHost::new().expect("Failed to initialize CarlaHost");
    host.init_engine("Dummy", "NeoUtlEguiTest")
        .expect("Failed to init engine");

    let plugin_id = host
        .add_plugin(
            BinaryType::NATIVE,
            PluginType::Internal,
            None,
            Some("bypass"),
            Some("bypass"),
            0,
            0,
        )
        .expect("Failed to add plugin");

    let mut embedded_ui = carla_host_sys::EmbeddedPluginUi::new(plugin_id, "Bypass Plugin");
    assert_eq!(embedded_ui.plugin_id, plugin_id);
    assert!(!embedded_ui.is_embedded);
    assert!(!embedded_ui.is_floating_open);

    embedded_ui.show_floating_window(&host);
    assert!(embedded_ui.is_floating_open);

    embedded_ui.hide_floating_window(&host);
    assert!(!embedded_ui.is_floating_open);

    let ctx = egui::Context::default();
    let _ = ctx.run_ui(Default::default(), |ui| {
        embedded_ui.ui(ui, &host, None);
    });

    host.remove_plugin(plugin_id).unwrap();
    host.close_engine().unwrap();
}
