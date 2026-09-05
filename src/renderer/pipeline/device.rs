use super::*;

static DEVICE_LOST: AtomicBool = AtomicBool::new(false);

pub fn is_device_lost() -> bool {
    DEVICE_LOST.load(Ordering::Relaxed)
}

fn mark_device_lost(reason: &str) {
    eprintln!(
        "{}",
        t!(
            "[NeoUtl] GPUデバイスロスト検知: %{arg0}",
            arg0 = format!("{reason}")
        )
    );
    DEVICE_LOST.store(true, Ordering::Relaxed);
}

pub fn install_device_lost_watcher(device: &wgpu::Device) {
    device.set_device_lost_callback(|reason, message| {
        mark_device_lost(&format!("{reason:?}: {message}"));
    });
}
