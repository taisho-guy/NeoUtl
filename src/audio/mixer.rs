use crate::ecs::EcsWorld;
use crate::ecs::components::{AudioParams, MediaSource};
use crate::ecs::systems::get_active_audio_system;
use crate::media;
use neoutl_media_api::AudioBuffer;
use rodio::Source;
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};
use std::collections::{HashMap, VecDeque};
use std::num::NonZero;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// リングバッファの上限秒数。出力側の消費停止・遅延時の無制限蓄積を防ぐ。
const RING_CAPACITY_SECONDS: usize = 2;

/// 1tick分のdt上限。フレーム描画停滞・デバッガ中断等による長時間ギャップを
/// そのままサンプル数へ反映すると巨大バーストが発生するためクランプする。
const MAX_TICK_SECONDS: f64 = 0.1;

/// process_frame呼び出し間隔がこれを超えたクリップは非連続（新規再生開始）とみなし、
/// sourceフレーム位置から位相を再計算する。ビデオフレーム番号ではなく実時間で判定するため、
/// 1ビデオフレーム未満の間隔で複数回process_frameが呼ばれる場合（tick > フレームレート）でも
/// 誤って非連続判定されない。
const CONTINUITY_GAP_SECONDS: f64 = 0.25;

/// AviQtl::Engine::AudioMixer対応物。デコード済みAudioBufferをボリューム/パン/ミュート
/// 適用の上で加算合成し、rodio経由で出力デバイスへ書き込む。
/// デバイス側の要求サンプルフォーマット（f32/i16/u16等）への変換はrodioが担う。
pub struct AudioMixer {
    output: Option<AudioOutput>,
    ring: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
    channels: u16,
    /// 直前process_frame呼び出し時刻。dt算出に使用し、pause()/reset()でNoneへ戻す。
    last_tick: Option<Instant>,
    decode_cache: HashMap<PathBuf, Arc<AudioBuffer>>,
    clip_phase: HashMap<usize, f64>,
    clip_last_tick: HashMap<usize, Instant>,
}

/// deviceはdrop時に出力を停止するため、AudioMixerと同じ生存期間で保持する。
struct AudioOutput {
    device: MixerDeviceSink,
}

impl AudioMixer {
    pub fn new(sample_rate: u32) -> Result<Self, String> {
        let channels: u16 = 2;
        let ring = Arc::new(Mutex::new(VecDeque::with_capacity(
            sample_rate as usize * channels as usize * RING_CAPACITY_SECONDS,
        )));
        let output = build_output(sample_rate, channels, ring.clone())?;
        Ok(Self {
            output: Some(output),
            ring,
            sample_rate,
            channels,
            last_tick: None,
            decode_cache: HashMap::new(),
            clip_phase: HashMap::new(),
            clip_last_tick: HashMap::new(),
        })
    }

    /// 出力デバイス非搭載環境（CI・ヘッドレス実行）向け。出力なしで動作し、
    /// process_frameは合成のみ実行しringは無音のまま滞留・破棄される。
    pub fn silent() -> Self {
        Self {
            output: None,
            ring: Arc::new(Mutex::new(VecDeque::new())),
            sample_rate: 48_000,
            channels: 2,
            last_tick: None,
            decode_cache: HashMap::new(),
            clip_phase: HashMap::new(),
            clip_last_tick: HashMap::new(),
        }
    }

    /// プロジェクトのサンプルレート変更時。出力を再構築する。
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        if sample_rate == self.sample_rate {
            return;
        }
        self.sample_rate = sample_rate;
        self.ring.lock().unwrap().clear();
        self.last_tick = None;
        match build_output(sample_rate, self.channels, self.ring.clone()) {
            Ok(output) => self.output = Some(output),
            Err(err) => {
                eprintln!("[NeoUtl] audio_mixer: 出力再構築失敗: {err}");
                self.output = None;
            }
        }
    }

    /// シーク発生時。連続再生前提の位相・直前tick記録・リングバッファを破棄する。
    pub fn reset(&mut self) {
        self.clip_phase.clear();
        self.clip_last_tick.clear();
        self.last_tick = None;
        self.ring.lock().unwrap().clear();
    }

    /// 即座に無音化する。dt起点もクリアし、再開時の巨大dt流入を防ぐ。
    pub fn pause(&self) {
        self.ring.lock().unwrap().clear();
    }

    pub fn play(&self) {}

    /// 直前process_frame呼び出しからの実経過時間分のサンプルを合成しringへ積む。
    /// ビデオフレーム更新の有無に依存せず、呼び出される度に実時間ベースで音声を生成するため、
    /// タイマー周期とfpsが非整数比でもring枯渇（プツプツ音）が生じない。
    /// current_frameは合成対象クリップの検索窓（get_active_audio_system）にのみ用いる。
    pub fn process_frame(&mut self, world: &EcsWorld, current_frame: i32, speed: f64) {
        if speed <= 0.0 {
            self.last_tick = None;
            return;
        }
        let now = Instant::now();
        let dt = match self.last_tick {
            Some(prev) => (now - prev).as_secs_f64().min(MAX_TICK_SECONDS),
            None => 1.0 / 60.0,
        };
        self.last_tick = Some(now);

        let samples_per_tick = (dt * f64::from(self.sample_rate)).round() as usize;
        if samples_per_tick == 0 {
            return;
        }
        let mut master = vec![0.0f32; samples_per_tick * self.channels as usize];

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
                        "[NeoUtl] audio_mixer: デコード失敗 {}: {err}",
                        source.path.display()
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
            master[frame_idx * 2] += sample_l * gain_l;
            master[frame_idx * 2 + 1] += sample_r * gain_r;
            phase += step;
        }

        self.clip_phase.insert(entity.id, phase);
        self.clip_last_tick.insert(entity.id, now);
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
) -> Result<AudioOutput, String> {
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
    Ok(AudioOutput { device })
}

/// AudioMixer::process_frameが積んだサンプルをpopして返す無限長Source。
/// 枯渇時（未再生時・pause直後）は無音（0.0）を返しストリームを継続させる
/// （AviQtl側QAudioSinkのpush駆動と対称）。play/pause状態を持たず、ring内容のみに従うため、
/// スクラブ時のprocess_frame単発呼び出しもそのまま音として出力される。
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

/// get_active_audio_systemが返すエンティティ表現。
pub struct ActiveAudioEntity {
    pub id: usize,
    pub audio: AudioParams,
    pub media_source: Option<MediaSource>,
    pub source_frame: i64,
    pub fps: f64,
}
