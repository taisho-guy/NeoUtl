#![allow(non_camel_case_types)]

/// デコーダが返す1フレーム分のデータ。
/// GPU リソース(wgpu::Texture)の生成・書き込みは原則としてUIスレッドが行う。
/// デコードスレッドがwgpu::Queueを操作するとSurface::present()とのSnatchLock
/// 競合でデッドロックするため、デコード結果はCPU側バイト列で返すのが基本。
/// ただしゼロコピーを達成できるデコーダに限りGpuバリアントで生成済みテクスチャを
/// 直接返す経路も許容する（この場合、submit等のQueue操作はUIスレッドで行う責務を持つ）。
#[derive(Clone, Debug)]
pub enum FrameOutput {
    /// CPU側バイト列。呼び出し元(UIスレッド)が create_texture + write_texture でアップロードする。
    Cpu(FrameBytes),
    /// 既にGPU上に生成済みのテクスチャ（ゼロコピーパス）。
    Gpu(wgpu::Texture),
}

/// CPU経由で受け渡すフレームのピクセルレイアウト。
/// NV12はY平面(w*h)とインターリーブUV平面(w*h/2)の2プレーン構成。
#[derive(Clone, Debug)]
pub enum FrameBytes {
    Nv12 {
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    Rgba8 {
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
}

pub trait VideoSource: Send {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn fps(&self) -> f64;
    fn total_frames(&self) -> i64;
    /// デコード結果を返す。本メソッドはGPUリソースを一切操作せず、
    /// 呼び出し元(UIスレッド)がテクスチャ生成・アップロードを行う前提。
    fn frame(&mut self, frame_index: i64) -> Result<FrameOutput, String>;
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
