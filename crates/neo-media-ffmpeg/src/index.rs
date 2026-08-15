use ffmpeg_sys_next as sys;

pub struct FrameIndexEntry {
    pub pts: i64,
    pub dts: i64,
    pub is_key: bool,
}

pub struct FrameIndex {
    pub entries: Vec<FrameIndexEntry>,
    pub prev_keyframe: Vec<i64>,
    pub gop_end: Vec<i64>,
}

impl FrameIndex {
    pub fn len(&self) -> i64 {
        self.entries.len() as i64
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn pts_at(&self, index: i64) -> i64 {
        self.entries[index as usize].pts
    }

    pub fn is_key_at(&self, index: i64) -> bool {
        self.entries[index as usize].is_key
    }

    pub fn index_of_pts(&self, pts: i64) -> Option<i64> {
        if self.entries.is_empty() {
            return None;
        }
        if let Ok(i) = self.entries.binary_search_by_key(&pts, |e| e.pts) {
            return Some(i as i64);
        }

        let idx = self.entries.partition_point(|e| e.pts < pts);
        let mut best: Option<(usize, i64)> = None;
        for cand in [idx.checked_sub(1), Some(idx)] {
            let Some(i) = cand else { continue };
            if i >= self.entries.len() {
                continue;
            }
            let diff = (self.entries[i].pts - pts).abs();
            if best.is_none_or(|(_, d)| diff < d) {
                best = Some((i, diff));
            }
        }

        best.and_then(|(i, diff)| {
            let neighbor_gap = if i + 1 < self.entries.len() {
                (self.entries[i + 1].pts - self.entries[i].pts).abs()
            } else if i > 0 {
                (self.entries[i].pts - self.entries[i - 1].pts).abs()
            } else {
                i64::MAX
            };
            (diff.saturating_mul(2) <= neighbor_gap).then_some(i as i64)
        })
    }

    pub fn preceding_keyframe(&self, target: i64) -> i64 {
        self.prev_keyframe[target as usize]
    }

    pub fn gop_end_of(&self, target: i64) -> i64 {
        self.gop_end[self.preceding_keyframe(target) as usize]
    }

    pub fn index_from_seconds(&self, seconds: f64, time_base: (i32, i32)) -> i64 {
        if self.is_empty() {
            return 0;
        }
        let tb = time_base.0 as f64 / time_base.1.max(1) as f64;
        if tb <= 0.0 {
            return 0;
        }
        let target_pts = (seconds / tb).round() as i64;
        let idx = self.entries.partition_point(|e| e.pts < target_pts) as i64;
        if idx <= 0 {
            return 0;
        }
        if idx >= self.len() {
            return self.len() - 1;
        }
        let a = self.entries[(idx - 1) as usize].pts;
        let b = self.entries[idx as usize].pts;
        if (target_pts - a).abs() <= (b - target_pts).abs() {
            idx - 1
        } else {
            idx
        }
    }
}

pub unsafe fn build_index(fmt_ctx: *mut sys::AVFormatContext, stream_index: i32) -> FrameIndex {
    let mut entries = Vec::new();
    let pkt = unsafe { sys::av_packet_alloc() };
    loop {
        let ret = unsafe { sys::av_read_frame(fmt_ctx, pkt) };
        if ret < 0 {
            break;
        }
        unsafe {
            if (*pkt).stream_index == stream_index {
                entries.push(FrameIndexEntry {
                    pts: (*pkt).pts,
                    dts: (*pkt).dts,
                    is_key: ((*pkt).flags & sys::AV_PKT_FLAG_KEY) != 0,
                });
            }
            sys::av_packet_unref(pkt);
        }
    }
    unsafe { sys::av_packet_free(&mut { pkt }) };

    entries.sort_by_key(|e| {
        if e.pts != sys::AV_NOPTS_VALUE {
            e.pts
        } else {
            e.dts
        }
    });

    let n = entries.len();
    let mut prev_keyframe = vec![0i64; n];
    let mut last_key = 0i64;
    for i in 0..n {
        if entries[i].is_key {
            last_key = i as i64;
        }
        prev_keyframe[i] = last_key;
    }

    let mut gop_end = vec![0i64; n.max(1)];
    let mut end = n as i64 - 1;
    for i in (0..n).rev() {
        gop_end[i] = end;
        if i > 0 && entries[i].is_key {
            end = i as i64 - 1;
        }
    }

    if entries.len() >= 2 {
        let typical_delta = {
            let mut deltas: Vec<i64> = entries
                .windows(2)
                .map(|w| w[1].pts - w[0].pts)
                .filter(|d| *d > 0)
                .collect();
            deltas.sort_unstable();
            deltas.get(deltas.len() / 2).copied().unwrap_or(1)
        };
        for i in 1..entries.len() {
            let delta = entries[i].pts - entries[i - 1].pts;
            if delta <= 0 || delta > typical_delta.saturating_mul(3) {
                eprintln!(
                    "[neoutl-video-decoder][診断][index] PTS不連続検出 index={i} \
prev_index={prev} prev_pts={prev_pts} prev_is_key={prev_key} \
curr_pts={curr_pts} curr_is_key={curr_key} delta={delta} typical_delta={typical_delta}",
                    prev = i - 1,
                    prev_pts = entries[i - 1].pts,
                    prev_key = entries[i - 1].is_key,
                    curr_pts = entries[i].pts,
                    curr_key = entries[i].is_key,
                );
            }
        }
    }

    FrameIndex {
        entries,
        prev_keyframe,
        gop_end,
    }
}
