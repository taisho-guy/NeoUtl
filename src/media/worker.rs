// src/media/worker.rs
// 1動画ソース = 1専用デコードスレッド。UIスレッドはrequest()で要求を出すのみで
// 待機しない。デコード結果はCPU側バイト列(FrameOutput)としてリングキャッシュへ書き込み、
// on_readyでUI再描画を要求する。GPUリソース操作は一切行わない（UIスレッドが行う）。
use neoutl_media_api::{FrameOutput, VideoSource};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

const PREFETCH_RADIUS: i64 = 8;
const RING_CAPACITY: usize = 32;
const STOP_SENTINEL: i64 = i64::MIN + 1;
const NONE_SENTINEL: i64 = i64::MIN;

struct Ring {
    map: HashMap<i64, FrameOutput>,
    order: VecDeque<i64>,
}

impl Ring {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&self, index: i64) -> Option<FrameOutput> {
        self.map.get(&index).cloned()
    }

    fn latest(&self) -> Option<FrameOutput> {
        self.order
            .back()
            .and_then(|index| self.map.get(index).cloned())
    }

    fn insert(&mut self, index: i64, frame: FrameOutput) {
        if !self.map.contains_key(&index) {
            self.order.push_back(index);
            if self.order.len() > RING_CAPACITY
                && let Some(evicted) = self.order.pop_front()
            {
                self.map.remove(&evicted);
            }
        }
        self.map.insert(index, frame);
    }
}

pub struct DecodeWorker {
    requested: Arc<AtomicI64>,
    signal: Arc<(Mutex<bool>, Condvar)>,
    ring: Arc<Mutex<Ring>>,
    thread: Option<JoinHandle<()>>,
}

impl DecodeWorker {
    /// device/queue を取らない。workerはデコードのみを行い、テクスチャ生成・
    /// アップロードは呼び出し元(UIスレッド)が行う。デコードスレッドから
    /// wgpu::Queueを操作するとSurface::present()との競合でデッドロックするため。
    pub fn spawn(mut decoder: Box<dyn VideoSource>, on_ready: Arc<dyn Fn() + Send + Sync>) -> Self {
        let requested = Arc::new(AtomicI64::new(NONE_SENTINEL));
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let ring = Arc::new(Mutex::new(Ring::new()));

        let requested_t = requested.clone();
        let signal_t = signal.clone();
        let ring_t = ring.clone();

        let thread = std::thread::spawn(move || {
            let mut served = NONE_SENTINEL;
            loop {
                let target = {
                    let (lock, cvar) = &*signal_t;
                    let mut pending = lock.lock().unwrap();
                    while !*pending {
                        pending = cvar.wait(pending).unwrap();
                    }
                    *pending = false;
                    requested_t.load(Ordering::Acquire)
                };
                if target == STOP_SENTINEL {
                    return;
                }
                if target == served {
                    continue;
                }
                let already_cached = ring_t.lock().unwrap().get(target).is_some();
                if !already_cached {
                    if let Ok(frame) = decoder.frame(target) {
                        ring_t.lock().unwrap().insert(target, frame);
                        served = target;
                        on_ready();
                    }
                } else {
                    served = target;
                }
                for offset in 1..=PREFETCH_RADIUS {
                    if requested_t.load(Ordering::Acquire) != target {
                        break;
                    }
                    let ahead = target + offset;
                    if ring_t.lock().unwrap().get(ahead).is_some() {
                        continue;
                    }
                    if let Ok(frame) = decoder.frame(ahead) {
                        ring_t.lock().unwrap().insert(ahead, frame);
                        on_ready();
                    }
                }
            }
        });

        Self {
            requested,
            signal,
            ring,
            thread: Some(thread),
        }
    }

    pub fn request(&self, frame_index: i64) {
        self.requested.store(frame_index, Ordering::Release);
        let (lock, cvar) = &*self.signal;
        *lock.lock().unwrap() = true;
        cvar.notify_one();
    }

    pub fn frame(&self, frame_index: i64) -> Option<FrameOutput> {
        self.ring.lock().unwrap().get(frame_index)
    }

    pub fn latest_available(&self) -> Option<FrameOutput> {
        self.ring.lock().unwrap().latest()
    }
}

impl Drop for DecodeWorker {
    fn drop(&mut self) {
        self.requested.store(STOP_SENTINEL, Ordering::Release);
        let (lock, cvar) = &*self.signal;
        *lock.lock().unwrap() = true;
        cvar.notify_one();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}
