use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use ffmpeg_sys_next as sys;

pub(crate) struct PacketSlot {
    pub(crate) packet: *mut sys::AVPacket,
    pub(crate) eof: bool,
}

unsafe impl Send for PacketSlot {}

impl Drop for PacketSlot {
    fn drop(&mut self) {
        unsafe {
            if !self.packet.is_null() {
                sys::av_packet_free(&mut self.packet);
            }
        }
    }
}

pub(crate) struct PacketQueue {
    slot: Mutex<Option<PacketSlot>>,
    not_full: Condvar,
    not_empty: Condvar,
}

impl PacketQueue {
    pub(crate) fn new() -> Self {
        Self {
            slot: Mutex::new(None),
            not_full: Condvar::new(),
            not_empty: Condvar::new(),
        }
    }

    pub(crate) fn push_blocking(&self, item: PacketSlot, stop: &AtomicBool) -> bool {
        let mut guard = self.slot.lock().expect("packet queue mutex poisoned");
        loop {
            if stop.load(Ordering::Acquire) {
                return false;
            }
            if guard.is_none() {
                break;
            }
            let (g, timeout) = self
                .not_full
                .wait_timeout(guard, Duration::from_millis(50))
                .expect("packet queue condvar poisoned");
            guard = g;
            let _ = timeout;
        }
        *guard = Some(item);
        self.not_empty.notify_one();
        true
    }

    pub(crate) fn pop_blocking(&self, stop: &AtomicBool) -> Option<PacketSlot> {
        let mut guard = self.slot.lock().expect("packet queue mutex poisoned");
        loop {
            if let Some(item) = guard.take() {
                self.not_full.notify_one();
                return Some(item);
            }
            if stop.load(Ordering::Acquire) {
                return None;
            }
            let (g, timeout) = self
                .not_empty
                .wait_timeout(guard, Duration::from_millis(50))
                .expect("packet queue condvar poisoned");
            guard = g;
            let _ = timeout;
        }
    }

    pub(crate) fn flush(&self) {
        let mut guard = self.slot.lock().expect("packet queue mutex poisoned");
        *guard = None;
        self.not_full.notify_one();
    }
}

pub(crate) struct SeekLock(pub(crate) Mutex<()>);

impl SeekLock {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(()))
    }
}

pub(crate) struct SendPtr(pub(crate) *mut sys::AVFormatContext);
unsafe impl Send for SendPtr {}

pub(crate) fn packet_reader_loop(
    fmt_ctx: *mut sys::AVFormatContext,
    stream_index: i32,
    queue: Arc<PacketQueue>,
    seek_lock: Arc<SeekLock>,
    stop: Arc<AtomicBool>,
) {
    let fmt_ctx = SendPtr(fmt_ctx);
    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        let pkt = unsafe { sys::av_packet_alloc() };
        let read_ret = {
            let _guard = seek_lock.0.lock().expect("seek lock poisoned");
            unsafe { sys::av_read_frame(fmt_ctx.0, pkt) }
        };
        if read_ret < 0 {
            unsafe { sys::av_packet_free(&mut { pkt }) };
            let pushed = queue.push_blocking(
                PacketSlot {
                    packet: ptr::null_mut(),
                    eof: true,
                },
                &stop,
            );
            if !pushed {
                return;
            }
            continue;
        }
        if unsafe { (*pkt).stream_index } != stream_index {
            unsafe { sys::av_packet_free(&mut { pkt }) };
            continue;
        }
        let pushed = queue.push_blocking(
            PacketSlot {
                packet: pkt,
                eof: false,
            },
            &stop,
        );
        if !pushed {
            unsafe { sys::av_packet_free(&mut { pkt }) };
            return;
        }
    }
}
