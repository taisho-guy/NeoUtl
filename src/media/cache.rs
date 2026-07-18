// src/media/cache.rs
use super::loader;
use super::worker::DecodeWorker;
use super::{MediaKind, detect_kind};
use neoutl_media_api::{AudioBuffer, FrameBytes, FrameOutput, ImageSource, VideoSource};
use slint::wgpu_29::wgpu;
use std::collections::{HashMap, VecDeque};
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

/// UIスレッド側テクスチャLRUの容量。worker側リング(worker.rs::RING_CAPACITY)と
/// 同じ32。デコードが先行して進む範囲をカバーし、毎フレームの再アップロードを抑制する。
const WORKER_RING_CAPACITY: usize = 32;

struct VideoEntry {
    width: u32,
    height: u32,
    fps: f64,
    total_frames: i64,
    /// spawn前の未使用デコーダ。frame_at初回呼び出し時にworkerへ移譲する。
    pending_decoder: Option<Box<dyn VideoSource>>,
    worker: Option<DecodeWorker>,
    /// UIスレッド側で生成したテクスチャのキャッシュ。
    /// workerから受け取ったCPUバイト列をテクスチャ化した結果を保持する。
    texture_cache: TextureLru,
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

fn open_video(path: &Path) -> Result<Box<dyn VideoSource>, String> {
    eprintln!("[media-cache] open_video開始: {}", path.display());
    let ext = ext_of(path)?;
    let plugin = loader::find_by_extension(&ext)
        .ok_or_else(|| format!("動画デコーダ未登録: {}", path.display()))?;
    let open_fn = plugin
        .vtable
        .open_video
        .ok_or_else(|| format!("プラグイン{}はopen_video未実装", plugin.id))?;
    let result = open_fn(path);
    match &result {
        Ok(_) => eprintln!(
            "[media-cache] open_video成功: {} (plugin={})",
            path.display(),
            plugin.id
        ),
        Err(e) => eprintln!("[media-cache] open_video失敗: {} 理由={e}", path.display()),
    }
    result
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

/// FrameOutput を wgpu::Texture へ実体化する。CPUバイト列の場合は create_texture +
/// write_texture でアップロードする。GPUテクスチャの場合はそのまま返す。
/// 必ずUIスレッドから呼ぶこと（wgpu::Queue操作を伴うため）。
fn materialize(frame: &FrameOutput, device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    match frame {
        FrameOutput::Gpu(texture) => texture.clone(),
        FrameOutput::Cpu(FrameBytes::Nv12 {
            bytes,
            width,
            height,
        }) => upload_nv12(device, queue, bytes, *width, *height),
        FrameOutput::Cpu(FrameBytes::Rgba8 {
            bytes,
            width,
            height,
        }) => upload_rgba8(device, queue, bytes, *width, *height),
    }
}

/// NV12バイト列(Y平面 + インターリーブUV平面)をNV12テクスチャへアップロードする。
/// 旧 gstreamer-decoder::import_frame のテクスチャ生成部をUIスレッド側へ移動したもの。
fn upload_nv12(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &[u8],
    width: u32,
    height: u32,
) -> wgpu::Texture {
    let y_plane_size = (width * height) as usize;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("video-nv12-frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::NV12,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::Plane0,
        },
        &data[0..y_plane_size],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::Plane1,
        },
        &data[y_plane_size..],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height / 2),
        },
        wgpu::Extent3d {
            width: width / 2,
            height: height / 2,
            depth_or_array_layers: 1,
        },
    );
    texture
}

/// RGBA8バイト列をRgba8Unormテクスチャへアップロードする。
/// 旧 ffmpeg-decoder::frame_texture のテクスチャ生成部をUIスレッド側へ移動したもの。
fn upload_rgba8(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &[u8],
    width: u32,
    height: u32,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("video-rgba8-frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture
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
                Ok(decoder) => PathEntry::Video(VideoEntry {
                    width: decoder.width(),
                    height: decoder.height(),
                    fps: decoder.fps(),
                    total_frames: decoder.total_frames(),
                    pending_decoder: Some(decoder),
                    worker: None,
                    texture_cache: TextureLru::new(WORKER_RING_CAPACITY),
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

    /// 完成テクスチャの即時返却のみを行う非ブロッキング呼び出し。
    /// 目的フレーム未完成時は直前に完成した最新フレームを返す。
    /// 一度も完成していない場合のみErrを返す（初回ロード直後の一瞬に限られる）。
    ///
    /// 本メソッドはUIスレッドから呼ばれることを前提とする。デコードスレッドは
    /// CPUバイト列(FrameOutput::Cpu)のみを返し、create_texture + write_texture は
    /// ここ（UIスレッド）で実行する。これにより Surface::present() と別スレッドの
    /// write_texture による wgpu SnatchLock デッドロックを回避する。
    pub fn frame_at(
        &self,
        path: &Path,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        let entry = self.entry(path);
        let mut guard = entry.lock().unwrap();
        match &mut *guard {
            PathEntry::Video(video) => {
                if video.worker.is_none() {
                    let decoder = video
                        .pending_decoder
                        .take()
                        .expect("workerがNoneの間pending_decoderは常時Some");
                    video.worker = Some(DecodeWorker::spawn(decoder, self.redraw_handle()));
                }
                let worker = video.worker.as_ref().unwrap();
                worker.request(frame_index);

                // UIスレッド側テクスチャLRUヒットなら即返却（再アップロード抑制）。
                if let Some(tex) = video.texture_cache.get(frame_index) {
                    return Ok(tex);
                }

                // 目的フレームのデコード結果を取得（非ブロッキング）。
                // まだ無ければ直近の完成フレームで代用し、表示の継続性を保つ。
                // 代用フレームはインデックスが一致しないためキャッシュしない。
                let exact = worker.frame(frame_index);
                let fallback = worker.latest_available();
                let (frame_output, is_exact) = match (exact, fallback) {
                    (Some(f), _) => (f, true),
                    (None, Some(f)) => (f, false),
                    (None, None) => return Err("デコード中".to_string()),
                };
                let tex = materialize(&frame_output, device, queue);
                if is_exact {
                    video.texture_cache.put(frame_index, tex.clone());
                }
                Ok(tex)
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
        let entry = self.entry(path);
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
        let entry = self.entry(path);
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
        let entry = self.entry(path);
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
        let entry = self.entry(path);
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
