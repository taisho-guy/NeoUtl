use std::collections::{HashMap, HashSet, VecDeque};

use crate::frame::VideoFrame;

pub struct FrameLruCache {
    max_cost: i64,
    used_cost: i64,
    order: VecDeque<i64>,
    map: HashMap<i64, (VideoFrame, i64)>,
}

impl FrameLruCache {
    pub fn new(max_cost: i64) -> Self {
        Self {
            max_cost,
            used_cost: 0,
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }

    pub fn set_max_cost(&mut self, max_cost: i64) {
        self.max_cost = max_cost;
        self.evict_until_within_budget();
    }

    pub fn get(&mut self, index: i64) -> Option<VideoFrame> {
        let frame = self.map.get(&index)?.0.clone();
        self.order.retain(|&i| i != index);
        self.order.push_back(index);
        Some(frame)
    }

    pub fn contains(&self, index: i64) -> bool {
        self.map.contains_key(&index)
    }

    pub fn insert(&mut self, index: i64, frame: VideoFrame) {
        if let Some((_, old_cost)) = self.map.remove(&index) {
            self.used_cost -= old_cost;
            self.order.retain(|&i| i != index);
        }
        let cost = frame.byte_cost();
        self.map.insert(index, (frame, cost));
        self.order.push_back(index);
        self.used_cost += cost;
        self.evict_until_within_budget();
    }

    fn evict_until_within_budget(&mut self) {
        while self.used_cost > self.max_cost {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some((_, cost)) = self.map.remove(&oldest) {
                self.used_cost -= cost;
            }
        }
    }
}

pub struct GopCacheBlock {
    pub keyframe_index: i64,
    pub start: i64,
    pub end: i64,
    pub frames: HashMap<i64, VideoFrame>,
}

pub struct GopCache {
    capacity: usize,
    blocks: VecDeque<GopCacheBlock>,
}

impl GopCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            blocks: VecDeque::new(),
        }
    }

    pub fn get(&mut self, frame_index: i64) -> Option<VideoFrame> {
        let pos = self.blocks.iter().position(|b| {
            frame_index >= b.start && frame_index <= b.end && b.frames.contains_key(&frame_index)
        })?;
        let block = self
            .blocks
            .remove(pos)
            .expect("posはiterで確認済みのため必ず存在する");
        let frame = block.frames.get(&frame_index).cloned();
        self.blocks.push_back(block);
        frame
    }

    pub fn put(&mut self, block: GopCacheBlock) {
        self.blocks
            .retain(|b| b.keyframe_index != block.keyframe_index);
        if self.blocks.len() >= self.capacity {
            self.blocks.pop_front();
        }
        self.blocks.push_back(block);
    }

    pub fn known_keyframes(&self) -> HashSet<i64> {
        self.blocks.iter().map(|b| b.keyframe_index).collect()
    }
}
