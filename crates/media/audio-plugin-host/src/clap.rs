use crate::{NeoPlugin, error::PluginError};
use clack_host::prelude::*;
use clack_host::process::{StartedPluginAudioProcessor, StoppedPluginAudioProcessor};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// スレッド安全共有状態。プラグインからのリスタート・処理要求を保持する。
#[derive(Default)]
struct NeoHostShared {
    restart_requested: AtomicBool,
    process_requested: AtomicBool,
    callback_requested: AtomicBool,
}

impl<'a> SharedHandler<'a> for NeoHostShared {
    fn initializing(&self, _instance: InitializingPluginHandle<'a>) {}
    fn request_restart(&self) {
        self.restart_requested.store(true, Ordering::SeqCst);
    }
    fn request_process(&self) {
        self.process_requested.store(true, Ordering::SeqCst);
    }
    fn request_callback(&self) {
        self.callback_requested.store(true, Ordering::SeqCst);
    }
}

/// メインスレッド専有状態。初期化済みハンドルを保持する。
struct NeoHostMainThread<'a> {
    instance: Option<InitializedPluginHandle<'a>>,
}

impl<'a> MainThreadHandler<'a> for NeoHostMainThread<'a> {
    fn initialized(&mut self, instance: InitializedPluginHandle<'a>) {
        self.instance = Some(instance);
    }
}

struct NeoHost;

impl HostHandlers for NeoHost {
    type Shared<'a> = NeoHostShared;
    type MainThread<'a> = NeoHostMainThread<'a>;
    type AudioProcessor<'a> = ();

    fn declare_extensions(_builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {}
}

/// 状態遷移: Stopped(activate直後) → Started(start_processing) → process()を反復
/// → Stopped(stop_processing) → PluginInstance::deactivate。
/// `activate`呼び出し（`PluginInstance`側）と`InputAudioBuffers`/`OutputAudioBuffers`/
/// `InputEvents`/`OutputEvents`の構築方法は未確認のため、`start`/`process`は保留。
enum ProcessorState<H: clack_host::host::HostHandlers> {
    Stopped(StoppedPluginAudioProcessor<H>),
    Started(StartedPluginAudioProcessor<H>),
    Transitioning,
}

pub struct ClapWrapper {
    instance: PluginInstance<NeoHost>,
    processor: Option<ProcessorState<NeoHost>>,
}

unsafe impl Send for ClapWrapper {}

impl ClapWrapper {
    pub fn load(path: &Path, plugin_id: &std::ffi::CStr) -> Result<Self, PluginError> {
        let host_info = HostInfo::new(
            "NeoUtl",
            "taisho-guy",
            "https://neoutl.taisho-guy.org",
            "0.1.0",
        )
        .map_err(|e| PluginError::Clap(e.to_string()))?;

        let entry =
            unsafe {
                PluginEntry::load(path.to_str().ok_or_else(|| {
                    PluginError::Clap(format!("non-UTF8 path: {}", path.display()))
                })?)
            }
            .map_err(|e| PluginError::Clap(e.to_string()))?;

        let instance = PluginInstance::<NeoHost>::new(
            |_shared| NeoHostShared::default(),
            |_shared| NeoHostMainThread { instance: None },
            &entry,
            plugin_id,
            &host_info,
        )
        .map_err(|e| PluginError::Clap(e.to_string()))?;

        Ok(Self {
            instance,
            processor: None,
        })
    }
}

impl NeoPlugin for ClapWrapper {
    fn start(&mut self) -> Result<(), PluginError> {
        Err(PluginError::Clap(
            "activate 未実装（plugin/instance.rs API未確認）".to_string(),
        ))
    }

    fn stop(&mut self) -> Result<(), PluginError> {
        match self.processor.take() {
            Some(ProcessorState::Started(started)) => {
                self.processor = Some(ProcessorState::Stopped(started.stop_processing()));
                Ok(())
            }
            Some(state) => {
                self.processor = Some(state);
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn process(&mut self, _inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _frames: usize) {}

    fn set_parameter(&mut self, _id: u32, _value: f64) -> Result<(), PluginError> {
        Err(PluginError::Clap(
            "clack-extensions params 未統合".to_string(),
        ))
    }

    /// clack-extensions paramsエクステンション未導入のため常に空。
    /// マイルストーン6以降、params拡張導入時にVst3Wrapper::param_infoと同型で実装する。
    fn param_info(&self) -> Vec<crate::vst3::PluginParamInfo> {
        Vec::new()
    }
}
