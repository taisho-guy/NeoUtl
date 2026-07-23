/// プロジェクト既定値（ProjectResource::new）
pub const PROJECT_DEFAULT_WIDTH: u32 = 1920;
pub const PROJECT_DEFAULT_HEIGHT: u32 = 1080;
pub const PROJECT_DEFAULT_FPS: u32 = 30;
pub const PROJECT_DEFAULT_AUDIO_SAMPLE_RATE: u32 = 48_000;
pub const PROJECT_DEFAULT_AUDIO_CHANNELS: u32 = 2;

/// タイムライン既定値
pub const DEFAULT_LAYER_COUNT: usize = 128;
pub const DEFAULT_TOTAL_FRAMES: i32 = 300;

/// シーングリッド既定値（SceneMeta::new）
pub const SCENE_DEFAULT_GRID_INTERVAL: i32 = 10;
pub const SCENE_DEFAULT_GRID_SUBDIVISION: i32 = 4;
pub const SCENE_DEFAULT_GRID_BPM: f32 = 120.0;
pub const SCENE_DEFAULT_GRID_OFFSET: f32 = 0.0;
pub const SCENE_DEFAULT_ENABLE_SNAP: bool = true;
pub const SCENE_DEFAULT_MAGNETIC_SNAP_RANGE: i32 = 10;

/// システム設定既定値（SystemSettingsResource::new）
pub const SYSTEM_DEFAULT_AUTOSAVE_ENABLED: bool = true;
pub const SYSTEM_DEFAULT_AUTOSAVE_INTERVAL_SEC: i32 = 300;
pub const SYSTEM_DEFAULT_THEME_DARK: bool = true;
/// 未選択時は内蔵の明暗2値（theme_dark）へフォールバックする
pub const SYSTEM_DEFAULT_THEME_ID: &str = "";
pub const SYSTEM_DEFAULT_UI_SCALE_PERCENT: i32 = 100;
/// 0 = 自動（論理コア数に追従）
pub const SYSTEM_DEFAULT_WORKER_THREADS: i32 = 0;
pub const SYSTEM_DEFAULT_AUDIO_MAX_BLOCK_SIZE: i32 = 4096;
pub const SYSTEM_DEFAULT_DECODE_BACKEND: i32 = DECODE_BACKEND_AUTO;
pub const SYSTEM_DEFAULT_DEFAULT_SNAP: bool = true;
pub const SYSTEM_DEFAULT_MAGNETIC_SNAP_RANGE: i32 = 10;
pub const SYSTEM_DEFAULT_EXPORT_CONTAINER: i32 = 0;
pub const SYSTEM_DEFAULT_EXPORT_CODEC: i32 = 0;

/// decode_backend列挙値（SystemSettingsResource::decode_backend）
pub const DECODE_BACKEND_AUTO: i32 = 0;
pub const DECODE_BACKEND_GPU_FIXED: i32 = 1;
pub const DECODE_BACKEND_CPU_FIXED: i32 = 2;

/// Undo/Redo保持段数（app_state::History）
pub const UNDO_HISTORY_LIMIT: usize = 100;

/// デコードワーカーのリングキャッシュ容量（フレーム数）。
/// media/worker.rs（デコード結果本体）とmedia/cache.rs（UIスレッド側テクスチャ）で共有する。
/// neoutl_media_api::VIDEO_TEXTURE_POOL_CAPACITY（CPU系デコーダプラグインの固定テクスチャ
/// プール枚数）と同一値を保つ必要があるため、後者を唯一の定義元として直接参照する。
pub const DECODE_RING_CAPACITY: usize = neoutl_media_api::VIDEO_TEXTURE_POOL_CAPACITY;
/// 先読み対象フレーム数
pub const DECODE_PREFETCH_RADIUS: i64 = 8;
/// RING_CAPACITYがPREFETCH_RADIUS*2以下だと、先読み中のフレームが表示側の
/// 消費より先にLRUで破棄され、is_ready判定が恒常的にfalseとなり再デコードが
/// 繰り返される（media/worker.rs::Ring::mark_ready参照）。両者の関係が
/// ビルド時に破綻しないよう固定する。
const _: () = assert!(DECODE_RING_CAPACITY as i64 > DECODE_PREFETCH_RADIUS * 2);
/// prefetch()の連続失敗許容回数。超過時、当該デコーダプラグインを除外し
/// 次点候補（拡張子重複時の後順位デコーダ、例: gstreamer）へフォールバックする。
/// media/worker.rs（計上）とmedia/cache.rs（除外集合管理・再オープン）で共有する。
pub const DECODE_PREFETCH_FAIL_THRESHOLD: i64 = 30;
/// frame_gpu()単発呼び出し（GPU decode()実行含む）の監視タイムアウト（ミリ秒）。
/// gpu-videoクレート内部（vulkan_decoder.rs: wait_for(..., u64::MAX)）がVulkan側で
/// 無期限停止する既知障害に対する回収不能検知用。超過時はdecoderを放棄し世代を切り替える。
pub const DECODE_WATCHDOG_TIMEOUT_MS: u64 = 5_000;

/// レンダリング上限（renderer/pipeline.rs）
pub const MAX_SCENE_OBJECTS: u64 = 512;
pub const UNIFORM_STRIDE_BYTES: u64 = 256;
pub const MAX_EFFECT_UNIFORM_BYTES: u64 = 128;
pub const MEDIA_UNIFORM_BYTES: u64 = 80;

/// 再生速度可変範囲（%）
pub const PLAYBACK_SPEED_MIN_PERCENT: i32 = 10;
pub const PLAYBACK_SPEED_MAX_PERCENT: i32 = 400;
