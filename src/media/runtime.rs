use crate::config::{DECODE_BACKEND_CPU_FIXED, DECODE_BACKEND_GPU_FIXED};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicI32, Ordering};
use tokio::runtime::Runtime;

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
        std::thread::available_parallelism().map_or(4, |n| n.get())
    }
}

pub fn set_worker_threads(worker_threads: i32) {
    WORKER_THREADS_SETTING.store(worker_threads, Ordering::Release);
}

pub fn handle() -> tokio::runtime::Handle {
    RUNTIME
        .get_or_init(|| {
            let threads = effective_thread_count();
            eprintln!(
                "{}",
                t!(
                    "[media-runtime] デコードスレッドプール起動: worker_threads=%{arg0}",
                    arg0 = format!("{}", threads)
                )
            );
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(threads)
                .thread_name("neoutl-decode")
                .build()
                .expect("デコードランタイム構築失敗")
        })
        .handle()
        .clone()
}

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
