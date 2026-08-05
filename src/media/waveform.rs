//! 音声波形のマルチ解像度アセット。UI要素を生成せず、表示幅に応じた
//! min/maxピーク列だけを共有する。
use neoutl_media_api::AudioBuffer;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

#[derive(Clone, Copy, Debug, Default)]
pub struct Peak {
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Debug)]
pub struct WaveformAsset {
    pub path: PathBuf,
    pub levels: Vec<Arc<[Peak]>>,
}

static CACHE: OnceLock<RwLock<HashMap<PathBuf, Arc<WaveformAsset>>>> = OnceLock::new();

fn cache() -> &'static RwLock<HashMap<PathBuf, Arc<WaveformAsset>>> {
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 音声サンプルを最大512段階のマルチ解像度ピークへ変換する。
pub fn build(path: &Path, audio: &AudioBuffer) -> Arc<WaveformAsset> {
    let mut levels: Vec<Arc<[Peak]>> = Vec::new();
    let current = make_peaks(audio, 1);
    levels.push(Arc::from(current.into_boxed_slice()));
    while levels.last().is_some_and(|v| v.len() > 512) {
        let previous = levels.last().expect("waveform level");
        let mut next = Vec::with_capacity(previous.len().div_ceil(2));
        for pair in previous.chunks(2) {
            let mut peak = Peak {
                min: 1.0,
                max: -1.0,
            };
            for p in pair {
                peak.min = peak.min.min(p.min);
                peak.max = peak.max.max(p.max);
            }
            next.push(peak);
        }
        levels.push(Arc::from(next.into_boxed_slice()));
    }
    Arc::new(WaveformAsset {
        path: path.to_path_buf(),
        levels,
    })
}

pub fn get(path: &Path) -> Option<Arc<WaveformAsset>> {
    cache().read().ok()?.get(path).cloned()
}

pub fn insert(asset: Arc<WaveformAsset>) {
    if let Ok(mut cache) = cache().write() {
        cache.insert(asset.path.clone(), asset);
    }
}

pub fn level_for_columns(asset: &WaveformAsset, columns: usize) -> Arc<[Peak]> {
    let target = columns.max(1);
    asset
        .levels
        .iter()
        .find(|level| level.len() <= target * 2)
        .cloned()
        .unwrap_or_else(|| {
            asset
                .levels
                .last()
                .cloned()
                .unwrap_or_else(|| Arc::from([]))
        })
}

fn make_peaks(audio: &AudioBuffer, bucket: usize) -> Vec<Peak> {
    let channels = audio.channels.max(1) as usize;
    audio
        .samples
        .chunks(channels * bucket)
        .map(|samples| {
            let mut peak = Peak {
                min: 1.0,
                max: -1.0,
            };
            for sample in samples.chunks(channels) {
                let value = sample.iter().copied().sum::<f32>() / sample.len().max(1) as f32;
                peak.min = peak.min.min(value);
                peak.max = peak.max.max(value);
            }
            peak
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_bounded_levels() {
        let audio = AudioBuffer {
            sample_rate: 48_000,
            channels: 1,
            samples: (0..4096)
                .map(|i| if i % 2 == 0 { -0.5 } else { 0.5 })
                .collect(),
        };
        let asset = build(Path::new("test.wav"), &audio);
        assert!(asset.levels.last().unwrap().len() <= 512);
        assert_eq!(asset.levels[0][0].min, -0.5);
    }
}
