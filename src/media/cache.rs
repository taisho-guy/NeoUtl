use super::loader;
use super::worker::DecodeWorker;
use super::{MediaKind, detect_kind};
use neoutl_media_api::{AudioBuffer, ImageSource, VideoSource};
use slint::wgpu_29::wgpu;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// UIスレッド側で生成したテクスチャのLRUキャッシュ。
/// デコードスレッドはCPUバイト列のみを返すため、UIスレッドが毎フレーム
/// create_texture + write_texture を行う。同一フレームの再描画時の再アップロードを
/// 抑制するため、変換済みテクスチャを容量付きで保持する。
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

/// UIスレッド側テクスチャLRUの容量。worker側リング(worker::RING_CAPACITY)と共有し、
/// config::DECODE_RING_CAPACITYを唯一の定義元とする。

struct VideoInstance {
    pending_decoder: Option<Box<dyn VideoSource>>,
    worker: Option<DecodeWorker>,
    texture_cache: TextureLru,
    /// 直近にframe_gpuで確定したフレーム番号。目的フレーム未準備時の代用元。
    last_index: Option<i64>,
    /// worker側で保持している最終エラー（次回フレーム返却用/デバッグ用）。
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
    /// spawn前の未使用デコーダ。最初にframe_atを呼んだインスタンスへ移譲する。
    /// 2つ目以降の同時インスタンスはopen_video_excludingで個別に新規オープンする
    /// （同一ファイルを複数のタイムラインクリップが同時参照する場合、GStreamer
    /// パイプラインは1本につき1つの再生ヘッドしか持てないため共有できない）。
    pending_decoder: Option<Box<dyn VideoSource>>,
    /// クリップインスタンス（呼び出し側が渡すkey。通常はECS上のObjectId）ごとの
    /// デコードセッション。同一ファイルの複数同時利用（同一ソースを2箇所の
    /// タイムラインクリップで使う等）間でシークヘッドが競合しないよう分離する。
    instances: HashMap<u64, VideoInstance>,
    /// 現在採用中のデコーダプラグインid（フォールバック判定・ログ用）。
    plugin_id: String,
    /// prefetch連続失敗により見限った（今後候補から除外する）プラグインidの集合。
    failed_plugins: HashSet<String>,
}

struct ImageEntry {
    decoder: Box<dyn ImageSource>,
    /// 画像は単一フレーム固定のため初回アップロード結果を恒久的に再利用する。
    texture: Option<wgpu::Texture>,
}

enum PathEntry {
    Video(VideoEntry),
    Image(ImageEntry),
    Audio(Arc<AudioBuffer>),
    /// open失敗を記憶し、毎フレームの再オープン試行を防ぐ。evict()で解除可能。
    Failed(String),
}

pub struct MediaCache {
    /// マップ操作（挿入・参照）のみを保護する短命ロック。デコード本体は
    /// 各パスのDecodeWorkerが専有スレッドで行うため、ここでは待機しない。
    entries: Mutex<HashMap<PathBuf, Arc<Mutex<PathEntry>>>>,
    redraw: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

fn ext_of(path: &Path) -> Result<String, String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| format!("拡張子なし: {}", path.display()))
}

/// 拡張子に対応する動画デコーダプラグインをloader::decoder_priority順（同値はid昇順）で
/// 順次試行する。excluded_pluginsに
/// 含まれるidは連続失敗により見限られた候補のため試行対象から除外する。
/// 各プラグインのopen_videoが返すErrはハードウェア/ドライバ側の実行時制約
/// （例: Vulkan Video非対応環境でのgpuvideo-decoder）を含みうるため、
/// 1プラグインの失敗だけでは即座にopen_video全体を失敗とせず次候補へ移る。
/// 全候補が失敗（または除外済み）の場合のみ、各プラグインの理由を連結して返す。
/// 成功時は採用したプラグインidも合わせて返す（フォールバック判定用）。
fn open_video_excluding(
    path: &Path,
    excluded_plugins: &HashSet<String>,
) -> Result<(Box<dyn VideoSource>, String), String> {
    eprintln!("[media-cache] open_video開始: {}", path.display());
    let ext = ext_of(path)?;
    let candidates = loader::find_all_by_extension(&ext);
    if candidates.is_empty() {
        return Err(format!("動画デコーダ未登録: {}", path.display()));
    }

    let mut failures: Vec<String> = Vec::new();
    for plugin in candidates {
        if excluded_plugins.contains(&plugin.id) {
            eprintln!(
                "[media-cache] open_video候補除外（過去に連続失敗）: {} (plugin={})",
                path.display(),
                plugin.id
            );
            continue;
        }
        let Some(open_fn) = plugin.vtable.open_video else {
            continue;
        };
        match open_fn(path) {
            Ok(decoder) => {
                eprintln!(
                    "[media-cache] open_video成功: {} (plugin={})",
                    path.display(),
                    plugin.id
                );
                return Ok((decoder, plugin.id.clone()));
            }
            Err(err) => {
                eprintln!(
                    "[media-cache] open_videoフォールバック: {} (plugin={}) 理由={err}",
                    path.display(),
                    plugin.id
                );
                failures.push(format!("{}: {err}", plugin.id));
            }
        }
    }
    Err(format!(
        "全デコーダで開けませんでした: {} [{}]",
        path.display(),
        failures.join(" / ")
    ))
}

fn open_video(path: &Path) -> Result<(Box<dyn VideoSource>, String), String> {
    open_video_excluding(path, &HashSet::new())
}

fn open_image(path: &Path) -> Result<Box<dyn ImageSource>, String> {
    let ext = ext_of(path)?;
    let plugin = loader::find_by_extension(&ext)
        .ok_or_else(|| format!("画像デコーダ未登録: {}", path.display()))?;
    let open_fn = plugin
        .vtable
        .open_image
        .ok_or_else(|| format!("プラグイン{}はopen_image未実装", plugin.id))?;
    open_fn(path)
}

fn decode_audio(path: &Path) -> Result<AudioBuffer, String> {
    let ext = ext_of(path)?;
    let plugin = loader::find_by_extension(&ext)
        .ok_or_else(|| format!("音声デコーダ未登録: {}", path.display()))?;
    let decode_fn = plugin
        .vtable
        .decode_audio
        .ok_or_else(|| format!("プラグイン{}はdecode_audio未実装", plugin.id))?;
    decode_fn(path)
}

impl MediaCache {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            redraw: Mutex::new(None),
        }
    }

    /// デコード完了時のUI再描画要求を登録する。preview.rsのセットアップ時に一度だけ呼ぶ。
    pub fn set_redraw_callback(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.redraw.lock().unwrap() = Some(callback);
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
        eprintln!("[media-cache] 新規load: {}", path.display());
        let built = match detect_kind(path) {
            None => {
                let err = format!(
                    "未対応の拡張子（対応デコーダプラグイン未検出）: {}",
                    path.display()
                );
                eprintln!("[media-cache] {err}");
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
                    eprintln!("[media-cache] load失敗: {} 理由={err}", path.display());
                    PathEntry::Failed(err)
                }
            },
            Some(MediaKind::Image) => match open_image(path) {
                Ok(decoder) => PathEntry::Image(ImageEntry {
                    decoder,
                    texture: None,
                }),
                Err(err) => {
                    eprintln!("[media-cache] load失敗: {} 理由={err}", path.display());
                    PathEntry::Failed(err)
                }
            },
            Some(MediaKind::Audio) => match decode_audio(path) {
                Ok(buf) => PathEntry::Audio(Arc::new(buf)),
                Err(err) => {
                    eprintln!("[media-cache] load失敗: {} 理由={err}", path.display());
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

    /// 既存エントリのみ返す。新規ロードは行わない（entries mutex の待ちを避けるため）。
    fn entry_existing(&self, path: &Path) -> Option<Arc<Mutex<PathEntry>>> {
        let map = self.entries.lock().unwrap();
        map.get(path).cloned()
    }

    /// DecodeWorkerのon_fail経由で呼ばれる。prefetch連続失敗により現在のデコーダ
    /// プラグインを見限り、除外集合に加えた上で次点候補（拡張子重複時の後順位
    /// デコーダ、例: gpuvideo失敗時のgstreamer）へ再オープンする。
    /// 候補が尽きた場合はエントリをFailedへ遷移させ、以後の再試行を止める。
    /// worker自体は既にon_fail呼び出し直後に終了しているため、ここでは
    /// pending_decoder/workerを新しいものへ差し替えるのみでよい。
    pub fn handle_prefetch_failure(&self, path: &Path) {
        self.handle_prefetch_failure_with_reason(path, "prefetch連続失敗".to_string());
    }

    pub fn schedule_prefetch_failure_with_reason(&self, path: PathBuf, reason: String) {
        eprintln!(
            "[media-cache] schedule prefetch failure path={} reason={}",
            path.display(),
            reason
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

        let (generation_after, failed_plugins, old_workers) = {
            let mut guard = entry.lock().unwrap();
            let PathEntry::Video(video) = &mut *guard else {
                return;
            };

            eprintln!(
                "[media-cache] prefetch failure path={} plugin={} gen={} -> gen+1 旧worker/pending無効化 reason={}",
                path.display(),
                video.plugin_id,
                video.generation,
                reason
            );

            video.failed_plugins.insert(video.plugin_id.clone());
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

            let generation_after = video.generation;
            let failed_plugins = video.failed_plugins.clone();
            (generation_after, failed_plugins, old_workers)
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
                    "[media-cache] fallback apply/open success path={} plugin={} gen={} fps={}",
                    path.display(),
                    plugin_id,
                    generation_after,
                    decoder.fps()
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
                    "[media-cache] fallback apply/open failed path={} reason={err}",
                    path.display()
                );
                *guard = PathEntry::Failed(err);
            }
        }
        drop(guard);

        (self.redraw_handle())();
    }

    /// UIスレッド専用。目的フレームの準備完了を確認後、decoder.frame_gpu()を
    /// worker/cache間で共有するMutex越しに直接呼びテクスチャを取得する。
    /// これにより create_texture + write_texture もデコーダ実体もUIスレッド上で
    /// 完結し、Surface::present() との wgpu SnatchLock デッドロックを回避する。
    /// 未完成時は直前に完成した最新フレームで代用し、表示の継続性を保つ。
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
                    let decoder = match spare_decoder.or_else(|| instance.pending_decoder.take()) {
                        Some(d) => d,
                        None => {
                            let (d, _) = open_video_excluding(path, &failed_plugins)
                                .map_err(|e| format!("追加インスタンス用デコーダを開けません: {e} / plugin={plugin_id}"))?;
                            d
                        }
                    };
                    let fail_path = path.to_path_buf();
                    let generation = current_gen;

                    let redraw = self.redraw_handle();
                    let on_fail = Arc::new(move |reason: String| {
                        crate::media::cache::MediaCache::schedule_prefetch_failure_with_reason(
                            &crate::media::cache::global(),
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

                if worker.is_ready(frame_index) {
                    match worker.frame_gpu(frame_index) {
                        Ok(Some(tex)) => {
                            instance.texture_cache.put(frame_index, tex.clone());
                            instance.last_index = Some(frame_index);
                            return Ok(tex);
                        }
                        Ok(None) => {}
                        Err(err) => {
                            instance.last_worker_error = Some(err.clone());
                            return Err(format!("{} / plugin={}", err, plugin_id));
                        }
                    }
                }

                if let Some(last) = instance.last_index {
                    if let Some(tex) = instance.texture_cache.get(last) {
                        return Ok(tex);
                    }
                    if worker.is_ready(last) {
                        match worker.frame_gpu(last) {
                            Ok(Some(tex)) => {
                                instance.texture_cache.put(last, tex.clone());
                                return Ok(tex);
                            }
                            Ok(None) => {}
                            Err(err) => {
                                instance.last_worker_error = Some(err.clone());
                                return Err(format!("{} / plugin={}", err, plugin_id));
                            }
                        }
                    }
                }

                if let Some(err) = worker.take_last_error() {
                    instance.last_worker_error = Some(err.clone());
                    return Err(format!("{} / plugin={}", err, plugin_id));
                }

                Err("デコード中".to_string())
            }
            PathEntry::Image(image) => {
                if image.texture.is_none() {
                    image.texture = Some(image.decoder.texture(device, queue));
                }
                Ok(image.texture.clone().unwrap())
            }
            PathEntry::Audio(_) => Err(format!(
                "音声ファイルに映像フレームは存在しません: {}",
                path.display()
            )),
            PathEntry::Failed(err) => Err(err.clone()),
        }
    }

    /// ソース映像/画像のピクセル寸法を返す。open時点で確定済みの値のみ参照するため
    /// デコードスレッドの完了状況に依存しない。
    pub fn dimensions(&self, path: &Path) -> Result<(u32, u32), String> {
        let entry = self
            .entry_existing(path)
            .ok_or_else(|| format!("メディアがまだロードされていません: {}", path.display()))?;
        let guard = entry.lock().unwrap();
        match &*guard {
            PathEntry::Video(video) => Ok((video.width, video.height)),
            PathEntry::Image(image) => Ok((image.decoder.width(), image.decoder.height())),
            PathEntry::Audio(_) => Err(format!(
                "音声ファイルに映像寸法は存在しません: {}",
                path.display()
            )),
            PathEntry::Failed(err) => Err(err.clone()),
        }
    }

    /// ソース動画のフレームレート。プロジェクトFPSとの比率換算に用いる（画像/音声は不使用）。
    pub fn source_fps(&self, path: &Path) -> Result<f64, String> {
        let entry = self
            .entry_existing(path)
            .ok_or_else(|| format!("メディアがまだロードされていません: {}", path.display()))?;

        let guard = entry.lock().unwrap();

        match &*guard {
            PathEntry::Video(video) => Ok(video.fps),
            PathEntry::Image(_) => Ok(0.0),
            PathEntry::Audio(_) => Err(format!(
                "音声ファイルにFPSは存在しません: {}",
                path.display()
            )),
            PathEntry::Failed(err) => Err(err.clone()),
        }
    }

    #[allow(dead_code)]
    pub fn total_frames(&self, path: &Path) -> Result<i64, String> {
        let entry = self
            .entry_existing(path)
            .ok_or_else(|| format!("メディアがまだロードされていません: {}", path.display()))?;
        let guard = entry.lock().unwrap();
        match &*guard {
            PathEntry::Video(video) => Ok(video.total_frames),
            _ => Err(format!(
                "映像フレーム総数が存在しません: {}",
                path.display()
            )),
        }
    }

    pub fn audio(&self, path: &Path) -> Result<Arc<AudioBuffer>, String> {
        let entry = self
            .entry_existing(path)
            .ok_or_else(|| format!("メディアがまだロードされていません: {}", path.display()))?;
        let guard = entry.lock().unwrap();
        match &*guard {
            PathEntry::Audio(buffer) => Ok(buffer.clone()),
            PathEntry::Failed(err) => Err(err.clone()),
            _ => Err(format!("音声トラックが見つかりません: {}", path.display())),
        }
    }

    pub fn evict(&self, path: &Path) {
        self.entries.lock().unwrap().remove(path);
    }
}

static GLOBAL: OnceLock<MediaCache> = OnceLock::new();

pub fn global() -> &'static MediaCache {
    GLOBAL.get_or_init(MediaCache::new)
}
