#![allow(non_camel_case_types)]

/// デコードフレームキャッシュの既定バイト予算。デコーダプラグイン（ffmpeg-decoder,
/// gpuvideo-decoder等）間で個別定義せず、この値を唯一の定義元として参照する。
pub const DEFAULT_DECODE_CACHE_BYTES: i64 = 512 * 1024 * 1024;

/// CPU系デコーダ（NV12バイト列 → wgpu::Texture変換を自前実装するプラグイン、
/// 例: gstreamer-decoder）が内部で保持する固定テクスチャプールの枚数。
/// ホスト側 media/cache.rs::TextureLru および media/worker.rs::RING_CAPACITY
/// （config::DECODE_RING_CAPACITY由来）と同一値を用いること。
/// プール容量 < LRU容量だと、LRUが「まだキャッシュ済」と誤認したテクスチャハンドルの
/// 実体がプールのローテーションにより既に上書き済みとなり、古いフレーム番号で
/// 新しいフレームの映像が表示される（stale handle aliasing）不具合を招く。
/// この値を両者の唯一の定義元とし、host側では
/// `config::DECODE_RING_CAPACITY = neoutl_media_api::VIDEO_TEXTURE_POOL_CAPACITY` と
/// 直接参照させることで値の乖離を構造的に防ぐ。
pub const VIDEO_TEXTURE_POOL_CAPACITY: usize = 32;

/// VideoSourceの2メソッド契約。
/// prefetch: バックグラウンドスレッドが呼ぶ。パケット読出し・内部キュー蓄積のみ実行し、
/// GPUリソース(Device/Queue)を一切操作しない。
/// frame_gpu: UIスレッドが呼ぶ。蓄積済みパケットのデコード・変換・テクスチャ生成
/// (create_texture + write_texture)まで完了し、wgpu::Textureを直接返す。
/// UIスレッド以外からのframe_gpu呼び出しはSurface::present()とのSnatchLock競合により
/// デッドロックしうるため禁止する。
pub trait VideoSource: Send {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn fps(&self) -> f64;
    fn total_frames(&self) -> i64;
    /// frame_indexのパケットを先読みし内部キューへ蓄積する。GPU操作なし。
    /// バックグラウンドスレッド専用。
    fn prefetch(&mut self, frame_index: i64) -> Result<(), String>;
    /// frame_indexのフレームをデコード・GPUアップロードし、生成済みテクスチャを返す。
    /// UIスレッド専用。呼び出し前提としてframe_indexのprefetch完了を要求する。
    fn frame_gpu(
        &mut self,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String>;
}

pub trait ImageSource: Send {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn texture(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture;
}

pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }

    pub fn range(&self, start_sample: usize, sample_count: usize) -> &[f32] {
        let channels = self.channels.max(1) as usize;
        let start = start_sample
            .saturating_mul(channels)
            .min(self.samples.len());
        let end = (start_sample + sample_count)
            .saturating_mul(channels)
            .min(self.samples.len());
        &self.samples[start..end]
    }
}

/// デコーダプラグインが対応するメディア種別。1プラグイン=1種別固定とし、
/// 動画・音声を同時に扱う複合プラグイン（例: ffmpeg系フルデマルチプレクサ）は
/// 種別ごとに別プラグインとして分割登録すること（責務分離・拡張子解決の一意性維持のため）。
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MediaKind {
    Video = 0,
    Image = 1,
    Audio = 2,
}

/// デコーダプラグインの静的メタデータ。objects/effectsのMeta規約
/// （'static参照 + 生ポインタ配列）に統一する。
#[repr(C)]
pub struct MediaMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: MediaKind,
    /// 小文字・ドット無し拡張子の一覧（例: "mp4"）。ホストはdetect_kind時に
    /// 全プラグインのこの一覧を走査し、一致した最初のプラグイン（id昇順）へ委譲する。
    pub extensions_ptr: *const &'static str,
    pub extensions_len: usize,
}
unsafe impl Send for MediaMeta {}
unsafe impl Sync for MediaMeta {}

/// デコーダプラグインの関数テーブル。
/// entry関数（neoutl_media_entry、extern "C"）のみがdylib境界を越える対象であり、
/// 本体各フィールドはホストと同一Cargo.lock・同一rustcで一括ビルドされる前提の
/// 素のRust関数ポインタとする（objects/effectsのRenderContext拡張点と同一のABI前提）。
/// kindに応じた1フィールドのみSomeとなる。
pub struct MediaVTable {
    pub meta: fn() -> &'static MediaMeta,
    pub open_video: Option<fn(path: &std::path::Path) -> Result<Box<dyn VideoSource>, String>>,
    pub open_image: Option<fn(path: &std::path::Path) -> Result<Box<dyn ImageSource>, String>>,
    pub decode_audio: Option<fn(path: &std::path::Path) -> Result<AudioBuffer, String>>,
}

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_media_entry\0";
pub type EntryFn = unsafe extern "C" fn() -> *const MediaVTable;
