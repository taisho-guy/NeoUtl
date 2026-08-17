use super::loader;
use super::worker::DecodeWorker;
use super::{MediaKind, detect_kind};
use egui_wgpu::wgpu;
use neoutl_media_api::{AudioBuffer, ImageSource, VideoSource};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

struct TextureLru {
    map: HashMap<i64, wgpu::Texture>,
    order: VecDeque<i64>,
    capacity: usize,
}

impl TextureLru {
    fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    fn get(&self, index: i64) -> Option<wgpu::Texture> {
        self.map.get(&index).cloned()
    }

    fn put(&mut self, index: i64, texture: wgpu::Texture) {
        if self.map.contains_key(&index) {
            return;
        }
        self.map.insert(index, texture);
        self.order.push_back(index);
        while self.order.len() > self.capacity
            && let Some(evicted) = self.order.pop_front()
        {
            self.map.remove(&evicted);
        }
    }
}

struct VideoInstance {
    pending_decoder: Option<Box<dyn VideoSource>>,
    worker: Option<DecodeWorker>,
    texture_cache: TextureLru,
    last_index: Option<i64>,
    last_worker_error: Option<String>,
}

impl VideoInstance {
    fn new() -> Self {
        Self {
            pending_decoder: None,
            worker: None,
            texture_cache: TextureLru::new(super::worker::RING_CAPACITY),
            last_index: None,
            last_worker_error: None,
        }
    }
}

struct VideoEntry {
    generation: u64,
    width: u32,
    height: u32,
    fps: f64,
    total_frames: i64,
    pending_decoder: Option<Box<dyn VideoSource>>,
    instances: HashMap<u64, VideoInstance>,
    plugin_id: String,
    failed_plugins: HashSet<String>,
}

struct ImageEntry {
    decoder: Box<dyn ImageSource>,
    texture: Option<wgpu::Texture>,
}

enum PathEntry {
    Video(VideoEntry),
    Image(ImageEntry),
    Audio(Arc<AudioBuffer>),
    Failed(String),
}

pub struct MediaCache {
    entries: Mutex<HashMap<PathBuf, Arc<Mutex<PathEntry>>>>,
    redraw: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

fn ext_of(path: &Path) -> Result<String, String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| t!("拡張子なし: %{arg0}", arg0 = format!("{}", path.display())).to_string())
}

fn open_video_excluding(
    path: &Path,
    excluded_plugins: &HashSet<String>,
) -> Result<(Box<dyn VideoSource>, String), String> {
    eprintln!(
        "{}",
        t!(
            "[media-cache] open_video開始: %{arg0}",
            arg0 = format!("{}", path.display())
        )
    );
    let ext = ext_of(path)?;
    let candidates = loader::find_all_by_extension(&ext);
    if candidates.is_empty() {
        return Err(t!(
            "動画デコーダ未登録: %{arg0}",
            arg0 = format!("{}", path.display())
        )
        .to_string());
    }

    let mut failures: Vec<String> = Vec::new();
    for plugin in candidates {
        if excluded_plugins.contains(&plugin.id) {
            eprintln!(
                "{}",
                t!(
                    "[media-cache] open_video候補除外（過去に連続失敗）: %{arg0} (plugin=%{arg1})",
                    arg0 = format!("{}", path.display()),
                    arg1 = format!("{}", plugin.id)
                )
            );
            continue;
        }
        let Some(open_fn) = plugin.vtable.open_video else {
            continue;
        };
        match open_fn(path) {
            Ok(decoder) => {
                eprintln!(
                    "{}",
                    t!(
                        "[media-cache] open_video成功: %{arg0} (plugin=%{arg1})",
                        arg0 = format!("{}", path.display()),
                        arg1 = format!("{}", plugin.id)
                    )
                );
                return Ok((decoder, plugin.id.clone()));
            }
            Err(err) => {
                eprintln!(
                    "{}",
                    t!(
                        "[media-cache] open_videoフォールバック: %{arg0} (plugin=%{arg1}) 理由=%{arg2}",
                        arg0 = format!("{}", path.display()),
                        arg1 = format!("{}", plugin.id),
                        arg2 = format!("{err}")
                    )
                );
                failures.push(format!("{}: {err}", plugin.id));
            }
        }
    }
    Err(t!(
        "全デコーダで開けませんでした: %{arg0} [%{arg1}]",
        arg0 = format!("{}", path.display()),
        arg1 = format!("{}", failures.join(" / "))
    )
    .to_string())
}

fn open_video(path: &Path) -> Result<(Box<dyn VideoSource>, String), String> {
    open_video_excluding(path, &HashSet::new())
}

fn open_image(path: &Path) -> Result<Box<dyn ImageSource>, String> {
    let ext = ext_of(path)?;
    let plugin = loader::find_by_extension(&ext).ok_or_else(|| {
        t!(
            "画像デコーダ未登録: %{arg0}",
            arg0 = format!("{}", path.display())
        )
        .to_string()
    })?;
    let open_fn = plugin.vtable.open_image.ok_or_else(|| {
        t!(
            "プラグイン%{arg0}はopen_image未実装",
            arg0 = format!("{}", plugin.id)
        )
        .to_string()
    })?;
    open_fn(path)
}

fn decode_audio(path: &Path) -> Result<AudioBuffer, String> {
    let ext = ext_of(path)?;
    let plugin = loader::find_by_extension(&ext).ok_or_else(|| {
        t!(
            "音声デコーダ未登録: %{arg0}",
            arg0 = format!("{}", path.display())
        )
        .to_string()
    })?;
    let decode_fn = plugin.vtable.decode_audio.ok_or_else(|| {
        t!(
            "プラグイン%{arg0}はdecode_audio未実装",
            arg0 = format!("{}", plugin.id)
        )
        .to_string()
    })?;
    decode_fn(path)
}

impl MediaCache {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            redraw: Mutex::new(None),
        }
    }

    fn redraw_handle(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.redraw
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Arc::new(|| {}))
    }

    fn entry(&self, path: &Path) -> Arc<Mutex<PathEntry>> {
        {
            let map = self.entries.lock().unwrap();
            if let Some(existing) = map.get(path) {
                return existing.clone();
            }
        }
        eprintln!(
            "{}",
            t!(
                "[media-cache] 新規load: %{arg0}",
                arg0 = format!("{}", path.display())
            )
        );
        let built = match detect_kind(path) {
            None => {
                let err = t!(
                    "未対応の拡張子（対応デコーダプラグイン未検出）: %{arg0}",
                    arg0 = format!("{}", path.display())
                )
                .to_string();
                eprintln!("{}", t!("[media-cache] %{arg0}", arg0 = format!("{}", err)));
                PathEntry::Failed(err)
            }
            Some(MediaKind::Video) => match open_video(path) {
                Ok((decoder, plugin_id)) => PathEntry::Video(VideoEntry {
                    width: decoder.width(),
                    height: decoder.height(),
                    fps: decoder.fps(),
                    total_frames: decoder.total_frames(),
                    generation: 0,
                    pending_decoder: Some(decoder),
                    instances: HashMap::new(),
                    plugin_id,
                    failed_plugins: HashSet::new(),
                }),
                Err(err) => {
                    eprintln!(
                        "{}",
                        t!(
                            "[media-cache] load失敗: %{arg0} 理由=%{arg1}",
                            arg0 = format!("{}", path.display()),
                            arg1 = format!("{err}")
                        )
                    );
                    PathEntry::Failed(err)
                }
            },
            Some(MediaKind::Image) => match open_image(path) {
                Ok(decoder) => PathEntry::Image(ImageEntry {
                    decoder,
                    texture: None,
                }),
                Err(err) => {
                    eprintln!(
                        "{}",
                        t!(
                            "[media-cache] load失敗: %{arg0} 理由=%{arg1}",
                            arg0 = format!("{}", path.display()),
                            arg1 = format!("{err}")
                        )
                    );
                    PathEntry::Failed(err)
                }
            },
            Some(MediaKind::Audio) => match decode_audio(path) {
                Ok(buf) => PathEntry::Audio(Arc::new(buf)),
                Err(err) => {
                    eprintln!(
                        "{}",
                        t!(
                            "[media-cache] load失敗: %{arg0} 理由=%{arg1}",
                            arg0 = format!("{}", path.display()),
                            arg1 = format!("{err}")
                        )
                    );
                    PathEntry::Failed(err)
                }
            },
        };
        let arc = Arc::new(Mutex::new(built));
        self.entries
            .lock()
            .unwrap()
            .entry(path.to_path_buf())
            .or_insert(arc)
            .clone()
    }

    fn entry_existing(&self, path: &Path) -> Option<Arc<Mutex<PathEntry>>> {
        let map = self.entries.lock().unwrap();
        map.get(path).cloned()
    }

    pub fn schedule_prefetch_failure_with_reason(path: PathBuf, reason: String) {
        eprintln!(
            "{}",
            t!("[media-cache] schedule prefetch failure path=%{arg0} reason=%{arg1}")
        );
        super::runtime::handle().spawn_blocking(move || {
            crate::media::cache::global().handle_prefetch_failure_with_reason(&path, reason);
        });
    }

    pub fn handle_prefetch_failure_with_reason(&self, path: &Path, reason: String) {
        let entry = {
            let map = self.entries.lock().unwrap();
            let Some(existing) = map.get(path) else {
                return;
            };
            existing.clone()
        };

        let (failed_plugins, old_workers) = {
            let mut guard = entry.lock().unwrap();
            let PathEntry::Video(video) = &mut *guard else {
                return;
            };

            let is_watchdog_timeout = reason.contains("watchdog timeout");

            eprintln!(
                "{}",
                t!(
                    "[media-cache] prefetch failure path=%{arg0} plugin=%{arg1} gen=%{arg2} -> gen+1 旧worker/pending無効化 watchdog由来=%{arg3} reason=%{arg4}",
                    arg0 = format!("{}", path.display()),
                    arg1 = format!("{}", video.plugin_id),
                    arg2 = format!("{}", video.generation),
                    arg3 = format!("{}", is_watchdog_timeout),
                    arg4 = format!("{}", reason)
                )
            );

            if !is_watchdog_timeout {
                video.failed_plugins.insert(video.plugin_id.clone());
            }
            video.generation = video.generation.wrapping_add(1);

            let old_workers: Vec<DecodeWorker> = video
                .instances
                .values_mut()
                .filter_map(|inst| inst.worker.take())
                .collect();
            video.pending_decoder = None;
            for inst in video.instances.values_mut() {
                inst.pending_decoder = None;
                inst.texture_cache = TextureLru::new(super::worker::RING_CAPACITY);
                inst.last_index = None;
            }

            let failed_plugins = video.failed_plugins.clone();
            (failed_plugins, old_workers)
        };

        drop(old_workers);

        let result = open_video_excluding(path, &failed_plugins);

        let mut guard = entry.lock().unwrap();
        let PathEntry::Video(video) = &mut *guard else {
            return;
        };
        match result {
            Ok((decoder, plugin_id)) => {
                eprintln!(
                    "{}",
                    t!(
                        "[media-cache] fallback apply/open success path=%{arg0} plugin=%{arg1} gen=%{arg2} fps=%{arg3}"
                    )
                );
                video.width = decoder.width();
                video.height = decoder.height();
                video.fps = decoder.fps();
                video.total_frames = decoder.total_frames();
                video.plugin_id = plugin_id;
                video.pending_decoder = Some(decoder);
            }
            Err(err) => {
                eprintln!(
                    "{}",
                    t!(
                        "[media-cache] fallback apply/open failed path=%{arg0} reason=%{arg1}",
                        arg1 = format!("{}", err)
                    )
                );
                *guard = PathEntry::Failed(err);
            }
        }
        drop(guard);

        (self.redraw_handle())();
    }

    pub fn frame_at(
        &self,
        path: &Path,
        instance_key: u64,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        let entry = self.entry(path);
        let mut guard = entry.lock().unwrap();
        match &mut *guard {
            PathEntry::Video(video) => {
                let current_gen = video.generation;
                let failed_plugins = video.failed_plugins.clone();
                let spare_decoder = video.pending_decoder.take();
                let plugin_id = video.plugin_id.clone();

                let instance = video
                    .instances
                    .entry(instance_key)
                    .or_insert_with(VideoInstance::new);

                let worker_needs_refresh = match &instance.worker {
                    None => true,
                    Some(w) => w.generation() != current_gen,
                };
                if worker_needs_refresh {
                    instance.worker = None;
                    let decoder = if let Some(d) =
                        spare_decoder.or_else(|| instance.pending_decoder.take())
                    {
                        d
                    } else {
                        let (d, _) = open_video_excluding(path, &failed_plugins).map_err(|e| {
                            t!(
                                "追加インスタンス用デコーダを開けません: %{arg0} / plugin=%{arg1}",
                                arg0 = format!("{e}"),
                                arg1 = format!("{plugin_id}")
                            )
                            .to_string()
                        })?;
                        d
                    };
                    let fail_path = path.to_path_buf();
                    let generation = current_gen;

                    let redraw = self.redraw_handle();
                    let on_fail = Arc::new(move |reason: String| {
                        crate::media::cache::MediaCache::schedule_prefetch_failure_with_reason(
                            fail_path.clone(),
                            reason,
                        );
                    });

                    instance.worker = Some(DecodeWorker::spawn(
                        generation,
                        decoder,
                        Arc::new(device.clone()),
                        Arc::new(queue.clone()),
                        redraw,
                        on_fail,
                    ));
                    instance.last_index = None;
                } else if let Some(d) = spare_decoder {
                    video.pending_decoder = Some(d);
                }

                let worker = instance.worker.as_ref().unwrap();
                worker.request(frame_index);

                if let Some(tex) = instance.texture_cache.get(frame_index) {
                    return Ok(tex);
                }

                if let Some(tex) = worker.poll_texture(frame_index) {
                    instance.texture_cache.put(frame_index, tex.clone());
                    instance.last_index = Some(frame_index);
                    return Ok(tex);
                }

                if let Some(last) = instance.last_index {
                    if let Some(tex) = instance.texture_cache.get(last) {
                        return Ok(tex);
                    }
                    if let Some(tex) = worker.poll_texture(last) {
                        instance.texture_cache.put(last, tex.clone());
                        return Ok(tex);
                    }
                }

                if let Some(err) = worker.take_last_error() {
                    instance.last_worker_error = Some(err.clone());
                    return Err(format!("{err} / plugin={plugin_id}"));
                }

                Err("デコード中".to_string())
            }
            PathEntry::Image(image) => {
                if image.texture.is_none() {
                    image.texture = Some(image.decoder.texture(device, queue));
                }
                Ok(image.texture.clone().unwrap())
            }
            PathEntry::Audio(_) => Err(t!(
                "音声ファイルに映像フレームは存在しません: %{arg0}",
                arg0 = format!("{}", path.display())
            )
            .to_string()),
            PathEntry::Failed(err) => Err(err.clone()),
        }
    }

    pub fn dimensions(&self, path: &Path) -> Result<(u32, u32), String> {
        let entry = self.entry_existing(path).ok_or_else(|| {
            t!(
                "メディアがまだロードされていません: %{arg0}",
                arg0 = format!("{}", path.display())
            )
            .to_string()
        })?;
        let guard = entry.lock().unwrap();
        match &*guard {
            PathEntry::Video(video) => Ok((video.width, video.height)),
            PathEntry::Image(image) => Ok((image.decoder.width(), image.decoder.height())),
            PathEntry::Audio(_) => Err(t!(
                "音声ファイルに映像寸法は存在しません: %{arg0}",
                arg0 = format!("{}", path.display())
            )
            .to_string()),
            PathEntry::Failed(err) => Err(err.clone()),
        }
    }

    pub fn source_fps(&self, path: &Path) -> Result<f64, String> {
        let entry = self.entry_existing(path).ok_or_else(|| {
            t!(
                "メディアがまだロードされていません: %{arg0}",
                arg0 = format!("{}", path.display())
            )
            .to_string()
        })?;

        let guard = entry.lock().unwrap();

        match &*guard {
            PathEntry::Video(video) => Ok(video.fps),
            PathEntry::Image(_) => Ok(0.0),
            PathEntry::Audio(_) => Err(t!(
                "音声ファイルにFPSは存在しません: %{arg0}",
                arg0 = format!("{}", path.display())
            )
            .to_string()),
            PathEntry::Failed(err) => Err(err.clone()),
        }
    }

    #[allow(dead_code)]
    pub fn total_frames(&self, path: &Path) -> Result<i64, String> {
        let entry = self.entry_existing(path).ok_or_else(|| {
            t!(
                "メディアがまだロードされていません: %{arg0}",
                arg0 = format!("{}", path.display())
            )
            .to_string()
        })?;
        let guard = entry.lock().unwrap();
        match &*guard {
            PathEntry::Video(video) => Ok(video.total_frames),
            _ => Err(t!(
                "映像フレーム総数が存在しません: %{arg0}",
                arg0 = format!("{}", path.display())
            )
            .to_string()),
        }
    }

    pub fn audio(&self, path: &Path) -> Result<Arc<AudioBuffer>, String> {
        let entry = self.entry_existing(path).ok_or_else(|| {
            t!(
                "メディアがまだロードされていません: %{arg0}",
                arg0 = format!("{}", path.display())
            )
            .to_string()
        })?;
        let guard = entry.lock().unwrap();
        match &*guard {
            PathEntry::Audio(buffer) => Ok(buffer.clone()),
            PathEntry::Failed(err) => Err(err.clone()),
            _ => Err(t!(
                "音声トラックが見つかりません: %{arg0}",
                arg0 = format!("{}", path.display())
            )
            .to_string()),
        }
    }

    pub fn load_audio(&self, path: &Path) -> Result<Arc<AudioBuffer>, String> {
        let _ = self.entry(path);
        self.audio(path)
    }
}

static GLOBAL: OnceLock<MediaCache> = OnceLock::new();

pub fn global() -> &'static MediaCache {
    GLOBAL.get_or_init(MediaCache::new)
}
