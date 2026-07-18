// src/media/runtime.rs
// SystemSettingsResource::worker_threads / decode_backend を実処理へ反映する唯一の経路。
// - worker_threads: デコードワーカー(DecodeWorker)を実行するtokioマルチスレッドランタイムの
//   ワーカースレッド数として使う。0は論理コア数に追従する「自動」を意味する。
// - decode_backend: GStreamerのハードウェアデコーダ候補要素（gstreamer-decoder::
//   log_hardware_decoder_availabilityが列挙する候補と同一集合）のrankを
//   GST_PLUGIN_FEATURE_RANK環境変数経由で切り替える。main.rs::configure_gst_plugin_path()
//   が読むGST_INITはOnce実行のため、初回動画オープンより前に確定させる必要がある。
use crate::config::{DECODE_BACKEND_CPU_FIXED, DECODE_BACKEND_GPU_FIXED};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicI32, Ordering};
use tokio::runtime::Runtime;

/// gstreamer-decoder::log_hardware_decoder_availability()の候補配列と同一集合。
/// 候補要素の追加・削除がある場合は両ファイルを揃えて更新する。
const HW_DECODER_ELEMENTS: &[&str] = &[
    "vah264dec",
    "vah265dec",
    "vaapih264dec",
    "vaapih265dec",
    "v4l2h264dec",
    "v4l2h265dec",
    "nvh264dec",
    "nvh265dec",
    "d3d11h264dec",
    "d3d11h265dec",
];

static WORKER_THREADS_SETTING: AtomicI32 = AtomicI32::new(0);
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn effective_thread_count() -> usize {
    let configured = WORKER_THREADS_SETTING.load(Ordering::Acquire);
    if configured > 0 {
        configured as usize
    } else {
        std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(4)
    }
}

/// system-settings.yaml読込直後、及びSystemSettingsWindowでの変更確定時に呼ぶ。
/// 既にランタイムが構築済みの場合、tokioはワーカースレッド数を実行時再構成できないため
/// 反映は次回プロセス起動時からになる。
pub fn set_worker_threads(worker_threads: i32) {
    WORKER_THREADS_SETTING.store(worker_threads, Ordering::Release);
}

/// デコードワーカー用ランタイムを取得する（初回呼び出し時に遅延構築）。
pub fn handle() -> tokio::runtime::Handle {
    RUNTIME
        .get_or_init(|| {
            let threads = effective_thread_count();
            eprintln!("[media-runtime] デコードスレッドプール起動: worker_threads={threads}");
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(threads)
                .thread_name("neoutl-decode")
                .build()
                .expect("デコードランタイム構築失敗")
        })
        .handle()
        .clone()
}

/// decode_backend設定に対応するGST_PLUGIN_FEATURE_RANK断片を返す。
/// 空文字列は「自動（GStreamer既定rankのまま）」を意味する。
fn decode_backend_rank_rule(decode_backend: i32) -> String {
    if decode_backend == DECODE_BACKEND_GPU_FIXED {
        HW_DECODER_ELEMENTS
            .iter()
            .map(|name| format!("{name}:PRIMARY+100"))
            .collect::<Vec<_>>()
            .join(",")
    } else if decode_backend == DECODE_BACKEND_CPU_FIXED {
        HW_DECODER_ELEMENTS
            .iter()
            .map(|name| format!("{name}:NONE"))
            .collect::<Vec<_>>()
            .join(",")
    } else {
        String::new()
    }
}

/// GST_PLUGIN_FEATURE_RANKへdecode_backendの候補要素rankを合成反映する。
/// lv2/ladspa無効化ルール（main.rs::configure_gst_plugin_path由来）を保持したまま、
/// ハードウェアデコーダ候補のみ書き換える唯一の経路。
/// GStreamerはOnce初期化のため、初期化済み後の変更はこのプロセス内では反映されず、
/// 次回起動時から有効になる。
pub fn apply_decode_backend_env(decode_backend: i32) {
    let hw_rank_rule = decode_backend_rank_rule(decode_backend);
    let feature_rank = if hw_rank_rule.is_empty() {
        "lv2:NONE,ladspa:NONE".to_string()
    } else {
        format!("lv2:NONE,ladspa:NONE,{hw_rank_rule}")
    };
    unsafe {
        std::env::set_var("GST_PLUGIN_FEATURE_RANK", feature_rank);
    }
}
