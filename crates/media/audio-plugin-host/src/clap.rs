use crate::{NeoPlugin, error::PluginError};
use clack_common::events::event_types::ParamValueEvent;
use clack_common::events::io::{EventBuffer, OutputEvents};
use clack_common::process::PluginAudioConfiguration;
use clack_common::utils::{ClapId, Cookie};
use clack_host::prelude::*;
use clack_host::process::{
    StartedPluginAudioProcessor, StoppedPluginAudioProcessor,
    audio_buffers::{AudioPortBuffer, AudioPortBufferType, AudioPorts, InputChannel},
};
use std::ffi::CStr;
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
    param_info: Vec<crate::vst3::PluginParamInfo>,
}

impl<'a> MainThreadHandler<'a> for NeoHostMainThread<'a> {
    fn initialized(&mut self, instance: InitializedPluginHandle<'a>) {
        self.param_info = read_param_info(&instance);
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
    param_info: Vec<crate::vst3::PluginParamInfo>,
    pending_params: std::collections::BTreeMap<u32, f64>,
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

        let mut instance = PluginInstance::<NeoHost>::new(
            |_shared| NeoHostShared::default(),
            |_shared| NeoHostMainThread {
                instance: None,
                param_info: Vec::new(),
            },
            &entry,
            plugin_id,
            &host_info,
        )
        .map_err(|e| PluginError::Clap(e.to_string()))?;

        let processor = instance
            .activate(
                |_shared, _main_thread| (),
                PluginAudioConfiguration {
                    sample_rate: 48_000.0,
                    min_frames_count: 1,
                    max_frames_count: 4096,
                },
            )
            .map_err(|e| PluginError::Clap(e.to_string()))?;
        let param_info = instance.access_handler(|main| main.param_info.clone());

        Ok(Self {
            instance,
            processor: Some(ProcessorState::Stopped(processor)),
            param_info,
            pending_params: std::collections::BTreeMap::new(),
        })
    }
}

impl NeoPlugin for ClapWrapper {
    fn start(&mut self) -> Result<(), PluginError> {
        let Some(ProcessorState::Stopped(processor)) = self.processor.take() else {
            return Ok(());
        };
        self.processor = Some(ProcessorState::Started(
            processor
                .start_processing()
                .map_err(|e| PluginError::Clap(e.to_string()))?,
        ));
        Ok(())
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

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], frames: usize) {
        let Some(ProcessorState::Started(processor)) = self.processor.as_mut() else {
            for output in outputs.iter_mut() {
                output.fill(0.0);
            }
            return;
        };
        let mut input_ports = AudioPorts::with_capacity(inputs.len(), 1);
        let mut input_buffers: Vec<Vec<f32>> = inputs
            .iter()
            .map(|channel| channel[..frames.min(channel.len())].to_vec())
            .collect();
        let input_channels = input_buffers
            .iter_mut()
            .map(|channel| InputChannel::variable(channel));
        let input = input_ports.with_input_buffers([AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only(input_channels),
            latency: 0,
        }]);
        let output_count = outputs.len();
        let output_channels = outputs.iter_mut().map(|channel| {
            let length = frames.min(channel.len());
            &mut channel[..length]
        });
        let mut output_ports = AudioPorts::with_capacity(output_count, 1);
        let mut output = output_ports.with_output_buffers([AudioPortBuffer {
            channels: AudioPortBufferType::f32_output_only(output_channels),
            latency: 0,
        }]);
        let mut input_event_buffer = EventBuffer::with_capacity(self.pending_params.len());
        for (&param_id, &value) in &self.pending_params {
            let event = ParamValueEvent::new(
                0,
                ClapId::new(param_id),
                clack_common::events::Pckn::match_all(),
                value,
                Cookie::empty(),
            );
            input_event_buffer.push(&event);
        }
        self.pending_params.clear();
        let input_events = input_event_buffer.as_input();
        let mut output_events = OutputEvents::void();
        let _ = processor.process(
            &input,
            &mut output,
            &input_events,
            &mut output_events,
            None,
            None,
        );
    }

    fn set_parameter(&mut self, id: u32, value: f64) -> Result<(), PluginError> {
        self.pending_params.insert(id, value);
        Ok(())
    }

    /// CLAPの生APIから取得したパラメータ定義のスナップショットを返す。
    fn param_info(&self) -> Vec<crate::vst3::PluginParamInfo> {
        self.param_info.clone()
    }
}

fn read_param_info(handle: &InitializedPluginHandle<'_>) -> Vec<crate::vst3::PluginParamInfo> {
    let raw_plugin = handle.as_raw();
    let Some(get_extension) = raw_plugin.get_extension else {
        return Vec::new();
    };
    let extension = unsafe {
        get_extension(
            raw_plugin as *const _,
            clap_sys::ext::params::CLAP_EXT_PARAMS.as_ptr(),
        )
    } as *const clap_sys::ext::params::clap_plugin_params;
    if extension.is_null() {
        return Vec::new();
    }
    let extension = unsafe { &*extension };
    let Some(count) = extension.count else {
        return Vec::new();
    };
    let Some(get_info) = extension.get_info else {
        return Vec::new();
    };
    let count = unsafe { count(raw_plugin as *const _) };
    (0..count)
        .filter_map(|index| {
            let mut info =
                std::mem::MaybeUninit::<clap_sys::ext::params::clap_param_info>::zeroed();
            let ok = unsafe { get_info(raw_plugin as *const _, index, info.as_mut_ptr()) };
            if !ok {
                return None;
            }
            let info = unsafe { info.assume_init() };
            let name = unsafe { CStr::from_ptr(info.name.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            Some(crate::vst3::PluginParamInfo {
                id: info.id,
                name,
                min: info.min_value,
                max: info.max_value,
                default: info.default_value,
                is_bypass: info.flags & clap_sys::ext::params::CLAP_PARAM_IS_BYPASS != 0,
            })
        })
        .collect()
}

impl Drop for ClapWrapper {
    fn drop(&mut self) {
        let Some(state) = self.processor.take() else {
            return;
        };
        let stopped = match state {
            ProcessorState::Started(started) => started.stop_processing(),
            ProcessorState::Stopped(stopped) => stopped,
            ProcessorState::Transitioning => return,
        };
        self.instance.deactivate(stopped);
    }
}
