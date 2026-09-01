use std::sync::OnceLock;
use std::sync::atomic::{AtomicI32, Ordering};
use tokio::runtime::Runtime;

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
