use crate::ecs::EcsWorld;
use crate::ecs::audio_plugins::PluginInstanceRef;
use crate::ecs::components::{AudioParams, MediaSource};
use crate::ecs::systems::get_active_audio_system;
use crate::media;
use maolan_host_adapter::{
    HostError, PluginFormat, PluginHost, PluginParamInfo, default_binary_path,
};
use neoutl_media_api::AudioBuffer;
use rodio::Source;
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};
use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RING_CAPACITY_SECONDS: usize = 2;

const TARGET_BUFFER_SECONDS: f64 = 0.12;

const CONTINUITY_GAP_SECONDS: f64 = 0.25;

const DEFAULT_PLUGIN_BLOCK_SIZE: usize = 4096;

pub struct AudioMixer {
    output: Option<MixerDeviceSink>,
    ring: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
    channels: u16,
    decode_cache: HashMap<PathBuf, Arc<AudioBuffer>>,
    clip_phase: HashMap<usize, f64>,
    clip_last_tick: HashMap<usize, Instant>,
    plugin_instances: HashMap<u64, CachedPlugin>,
    plugin_host: PluginHost,
}

unsafe impl Send for AudioMixer {}

struct CachedPlugin {
    path: PathBuf,
    plugin_id: String,
    instance_id: Option<u32>,
}

impl AudioMixer {
    pub fn new(sample_rate: u32) -> Result<Self, String> {
        let channels: u16 = 2;
        let ring = Arc::new(Mutex::new(VecDeque::with_capacity(
            sample_rate as usize * channels as usize * RING_CAPACITY_SECONDS,
        )));
        let output = build_output(sample_rate, channels, ring.clone())?;
        let plugin_host = PluginHost::new(
            default_binary_path(),
            sample_rate as f64,
            DEFAULT_PLUGIN_BLOCK_SIZE,
        );

        Ok(Self {
            output: Some(output),
            ring,
            sample_rate,
            channels,
            decode_cache: HashMap::new(),
            clip_phase: HashMap::new(),
            clip_last_tick: HashMap::new(),
            plugin_instances: HashMap::new(),
            plugin_host,
        })
    }

    pub fn silent() -> Self {
        Self {
            output: None,
            ring: Arc::new(Mutex::new(VecDeque::new())),
            sample_rate: 48_000,
            channels: 2,
            decode_cache: HashMap::new(),
            clip_phase: HashMap::new(),
            clip_last_tick: HashMap::new(),
            plugin_instances: HashMap::new(),
            plugin_host: PluginHost::new(
                default_binary_path(),
                48_000.0,
                DEFAULT_PLUGIN_BLOCK_SIZE,
            ),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        if sample_rate == self.sample_rate {
            return;
        }
        self.sample_rate = sample_rate;
        self.plugin_host.set_sample_rate(sample_rate as f64);
        for (_, cached) in self.plugin_instances.drain() {
            if let Some(id) = cached.instance_id {
                let _ = self.plugin_host.remove_plugin(id);
            }
        }
        self.ring.lock().unwrap().clear();
        match build_output(sample_rate, self.channels, self.ring.clone()) {
            Ok(output) => self.output = Some(output),
            Err(err) => {
                eprintln!(
                    "{}",
                    t!(
                        "[NeoUtl] audio_mixer: 出力再構築失敗: %{arg0}",
                        arg0 = format!("{}", err)
                    )
                );
                self.output = None;
            }
        }
    }

    pub fn reset(&mut self) {
        self.clip_phase.clear();
        self.clip_last_tick.clear();
        self.ring.lock().unwrap().clear();
    }

    pub fn pause(&self) {
        self.ring.lock().unwrap().clear();
    }

    pub fn probe_plugin_param_info(
        &mut self,
        format: PluginFormat,
        path: &Path,
        plugin_id: &str,
    ) -> Vec<PluginParamInfo> {
        let spec = plugin_spec_for(path, plugin_id);
        let Some(spec) = spec else {
            return Vec::new();
        };
        match self.plugin_host.add_plugin(format, &spec) {
            Ok(id) => {
                let list = self.plugin_host.full_param_info_list(id);
                let _ = self.plugin_host.remove_plugin(id);
                list
            }
            Err(err) => {
                eprintln!(
                    "{}",
                    t!(
                        "[NeoUtl] audio_mixer: プラグイン情報取得失敗 spec=%{arg0}: %{arg1}",
                        arg0 = spec,
                        arg1 = format!("{}", err)
                    )
                );
                Vec::new()
            }
        }
    }

    pub fn play(&self) {}

    pub fn process_frame(&mut self, world: &EcsWorld, current_frame: i32, speed: f64) {
        if speed <= 0.0 {
            return;
        }
        if self.output.is_none() {
            return;
        }
        let now = Instant::now();

        let target_len =
            (TARGET_BUFFER_SECONDS * f64::from(self.sample_rate)) as usize * self.channels as usize;
        let current_len = self.ring.lock().unwrap().len();
        let needed_len = target_len.saturating_sub(current_len);
        let samples_per_tick = needed_len / self.channels as usize;
        if samples_per_tick == 0 {
            return;
        }
        let mut master = vec![0.0f32; samples_per_tick * self.channels as usize];

        let active_uids: HashSet<u64> = get_active_audio_system(world, current_frame)
            .iter()
            .flat_map(|entity| entity.plugin_chain.iter().map(|p| p.instance_uid))
            .filter(|uid| *uid != 0)
            .collect();
        self.evict_inactive_plugins(&active_uids);

        for entity in get_active_audio_system(world, current_frame) {
            if entity.audio.mute {
                continue;
            }
            self.mix_entity(&entity, now, speed, &mut master, samples_per_tick);
        }

        let mut ring = self.ring.lock().unwrap();
        let cap = ring.capacity();
        for sample in master {
            if ring.len() >= cap {
                ring.pop_front();
            }
            ring.push_back(sample);
        }
    }

    fn handle_plugin_failure(&self, spec: &str, err: &HostError) {
        handle_plugin_failure_impl(spec, err);
    }

    fn evict_inactive_plugins(&mut self, active_uids: &HashSet<u64>) {
        let stale_keys: Vec<u64> = self
            .plugin_instances
            .keys()
            .filter(|key| !active_uids.contains(key))
            .copied()
            .collect();
        for key in stale_keys {
            if let Some(cached) = self.plugin_instances.remove(&key) {
                if let Some(id) = cached.instance_id {
                    let _ = self.plugin_host.remove_plugin(id);
                }
            }
        }
    }

    fn mix_entity(
        &mut self,
        entity: &ActiveAudioEntity,
        now: Instant,
        speed: f64,
        master: &mut [f32],
        samples_per_tick: usize,
    ) {
        let Some(source) = &entity.media_source else {
            return;
        };
        let buffer = match self.decode_cache.get(&source.path) {
            Some(buf) => buf.clone(),
            None => match media::loader::decode_audio(&source.path) {
                Ok(buf) => {
                    let buf = Arc::new(buf);
                    self.decode_cache.insert(source.path.clone(), buf.clone());
                    buf
                }
                Err(err) => {
                    eprintln!(
                        "{}",
                        t!(
                            "[NeoUtl] audio_mixer: デコード失敗 %{arg0}: %{arg1}",
                            arg0 = format!("{}", source.path.display()),
                            arg1 = format!("{}", err)
                        )
                    );
                    return;
                }
            },
        };

        let continuous = self
            .clip_last_tick
            .get(&entity.id)
            .is_some_and(|last| (now - *last).as_secs_f64() < CONTINUITY_GAP_SECONDS);
        let mut phase = if continuous {
            *self.clip_phase.get(&entity.id).unwrap_or(&0.0)
        } else {
            entity.source_frame as f64 * f64::from(buffer.sample_rate) / entity.fps
        };

        let step = speed * f64::from(buffer.sample_rate) / f64::from(self.sample_rate);
        let channels = buffer.channels.max(1) as usize;
        let volume = entity.audio.volume;
        let pan = entity.audio.pan.clamp(-1.0, 1.0);
        let gain_l = volume * (1.0 - pan.max(0.0));
        let gain_r = volume * (1.0 + pan.min(0.0));

        let mut chan_l = vec![0.0f32; samples_per_tick];
        let mut chan_r = vec![0.0f32; samples_per_tick];

        for frame_idx in 0..samples_per_tick {
            let idx = phase as usize;
            if idx + 1 >= buffer.frame_count() {
                break;
            }
            let frac = phase.fract() as f32;
            let sample_l = lerp_sample(&buffer.samples, idx, channels, 0, frac);
            let sample_r = if channels > 1 {
                lerp_sample(&buffer.samples, idx, channels, 1, frac)
            } else {
                sample_l
            };
            chan_l[frame_idx] = sample_l * gain_l;
            chan_r[frame_idx] = sample_r * gain_r;
            phase += step;
        }

        self.apply_plugin_chain(entity.id, &entity.plugin_chain, &mut chan_l, &mut chan_r);

        for frame_idx in 0..samples_per_tick {
            master[frame_idx * 2] += chan_l[frame_idx];
            master[frame_idx * 2 + 1] += chan_r[frame_idx];
        }

        self.clip_phase.insert(entity.id, phase);
        self.clip_last_tick.insert(entity.id, now);
    }

    fn apply_plugin_chain(
        &mut self,
        entity_id: usize,
        chain: &[PluginInstanceRef],
        chan_l: &mut [f32],
        chan_r: &mut [f32],
    ) {
        let frames = chan_l.len();
        for instance_ref in chain {
            if instance_ref.bypass {
                continue;
            }
            let key = if instance_ref.instance_uid == 0 {
                entity_id as u64
            } else {
                instance_ref.instance_uid
            };
            let stale = self.plugin_instances.get(&key).is_none_or(|c| {
                c.path != instance_ref.path
                    || c.plugin_id != instance_ref.plugin_id
                    || c.instance_id.is_none()
            });
            if stale {
                if let Some(old_plugin) = self.plugin_instances.remove(&key) {
                    if let Some(id) = old_plugin.instance_id {
                        let _ = self.plugin_host.remove_plugin(id);
                    }
                }
                let instance_id = match plugin_spec_for(&instance_ref.path, &instance_ref.plugin_id)
                {
                    Some(spec) => match self.plugin_host.add_plugin(instance_ref.format, &spec) {
                        Ok(id) => Some(id),
                        Err(err) => {
                            if !matches!(err, maolan_host_adapter::HostError::Blocked(_)) {
                                self.handle_plugin_failure(&spec, &err);
                            }
                            eprintln!(
                                "{}",
                                t!(
                                    "[NeoUtl] audio_mixer: プラグイン生成失敗 spec=%{arg0}: %{arg1}",
                                    arg0 = spec,
                                    arg1 = format!("{}", err)
                                )
                            );
                            None
                        }
                    },
                    None => None,
                };
                self.plugin_instances.insert(
                    key,
                    CachedPlugin {
                        path: instance_ref.path.clone(),
                        plugin_id: instance_ref.plugin_id.clone(),
                        instance_id,
                    },
                );
            }

            let Some(cached) = self.plugin_instances.get(&key) else {
                continue;
            };
            let Some(instance_id) = cached.instance_id else {
                continue;
            };

            for (&param_id, &value) in &instance_ref.params {
                self.plugin_host
                    .set_parameter_value(instance_id, param_id, value as f32);
            }

            let mut out_l = vec![0.0f32; frames];
            let mut out_r = vec![0.0f32; frames];
            match self.plugin_host.process_stereo(
                instance_id,
                chan_l,
                chan_r,
                &mut out_l,
                &mut out_r,
                frames,
            ) {
                Ok(()) => {
                    chan_l.copy_from_slice(&out_l);
                    chan_r.copy_from_slice(&out_r);
                }
                Err(err) => {
                    if let Some(cached) = self.plugin_instances.get_mut(&key) {
                        cached.instance_id = None;
                    }
                    let spec = plugin_spec_for(&instance_ref.path, &instance_ref.plugin_id)
                        .unwrap_or_default();
                    self.handle_plugin_failure(&spec, &err);
                }
            }
        }
    }
}

fn handle_plugin_failure_impl(spec: &str, err: &HostError) {
    if spec.is_empty() {
        return;
    }
    let count = maolan_host_adapter::record_crash(spec);
    if count >= maolan_host_adapter::CRASH_THRESHOLD {
        maolan_host_adapter::block_plugin(spec, &format!("{}", err));
        eprintln!(
            "{}",
            t!(
                "[NeoUtl] audio_mixer: 連続失敗によりブロックリスト登録 spec=%{arg0} count=%{arg1}",
                arg0 = spec.to_string(),
                arg1 = count.to_string()
            )
        );
    }
}

fn plugin_spec_for(path: &Path, plugin_id: &str) -> Option<String> {
    if !plugin_id.is_empty() {
        Some(plugin_id.to_string())
    } else if !path.as_os_str().is_empty() {
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    }
}

fn lerp_sample(samples: &[f32], frame_idx: usize, channels: usize, ch: usize, frac: f32) -> f32 {
    let a = samples
        .get(frame_idx * channels + ch)
        .copied()
        .unwrap_or(0.0);
    let b = samples
        .get((frame_idx + 1) * channels + ch)
        .copied()
        .unwrap_or(a);
    a + (b - a) * frac
}

fn build_output(
    sample_rate: u32,
    channels: u16,
    ring: Arc<Mutex<VecDeque<f32>>>,
) -> Result<MixerDeviceSink, String> {
    let sample_rate = NonZero::new(sample_rate).ok_or("sample_rate must be nonzero")?;
    let channels = NonZero::new(channels).ok_or("channels must be nonzero")?;
    let device = DeviceSinkBuilder::from_default_device()
        .map_err(|e| e.to_string())?
        .with_channels(channels)
        .with_sample_rate(sample_rate)
        .open_stream()
        .map_err(|e| e.to_string())?;
    device.mixer().add(RingSource {
        ring,
        sample_rate,
        channels,
    });
    Ok(device)
}

struct RingSource {
    ring: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: NonZero<u32>,
    channels: NonZero<u16>,
}

impl Iterator for RingSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        Some(self.ring.lock().unwrap().pop_front().unwrap_or(0.0))
    }
}

impl Source for RingSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> NonZero<u16> {
        self.channels
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct ActiveAudioEntity {
    pub id: usize,
    pub audio: AudioParams,
    pub media_source: Option<MediaSource>,
    pub source_frame: i64,
    pub fps: f64,
    pub plugin_chain: Vec<PluginInstanceRef>,
}
