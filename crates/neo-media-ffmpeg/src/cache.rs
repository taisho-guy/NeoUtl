use std::collections::{HashMap, HashSet, VecDeque};

use crate::frame::VideoFrame;

pub struct PooledFrameCache {
    capacity: usize,
    order: VecDeque<i64>,
    map: HashMap<i64, VideoFrame>,
}

impl PooledFrameCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }

    pub fn get(&mut self, index: i64) -> Option<VideoFrame> {
        let frame = self.map.get(&index)?.clone();
        self.order.retain(|&i| i != index);
        self.order.push_back(index);
        Some(frame)
    }

    pub fn contains(&self, index: i64) -> bool {
        self.map.contains_key(&index)
    }

    pub fn insert(&mut self, index: i64, frame: VideoFrame) {
        if self.map.remove(&index).is_some() {
            self.order.retain(|&i| i != index);
        }
        self.map.insert(index, frame);
        self.order.push_back(index);
        self.evict_until_within_budget();
    }

    fn evict_until_within_budget(&mut self) {
        while self.map.len() > self.capacity {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.map.remove(&oldest);
        }
    }

    pub fn evict_oldest(&mut self) -> bool {
        let Some(oldest) = self.order.pop_front() else {
            return false;
        };
        self.map.remove(&oldest).is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

pub struct GopCacheBlock {
    pub keyframe_index: i64,
    pub start: i64,
    pub end: i64,
    pub frame_indices: Vec<i64>,
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

    pub fn get(&mut self, frame_index: i64, frames: &mut PooledFrameCache) -> Option<VideoFrame> {
        let pos = self.blocks.iter().position(|b| {
            frame_index >= b.start && frame_index <= b.end && b.frame_indices.contains(&frame_index)
        })?;
        let block = self
            .blocks
            .remove(pos)
            .expect("posはiterで確認済みのため必ず存在する");
        let frame = frames.get(frame_index);
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
