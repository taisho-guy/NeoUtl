// src/media/video.rs
use super::DecodedFrame;
use ffmpeg_next as ffmpeg;
use ffmpeg_next::software::scaling::{Context as ScalingContext, Flags as ScalingFlags};
use ffmpeg_next::util::frame::Video as VideoFrame;
use std::collections::{HashMap, VecDeque};
use std::path::Path;

struct IndexEntry {
    pts: i64,
    key: bool,
}

struct FrameCache {
    capacity_bytes: i64,
    used_bytes: i64,
    order: VecDeque<i64>,
    map: HashMap<i64, (DecodedFrame, i64)>,
}

impl FrameCache {
    fn new(capacity_bytes: i64) -> Self {
        Self {
            capacity_bytes,
            used_bytes: 0,
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }

    fn get(&mut self, display_index: i64) -> Option<DecodedFrame> {
        if !self.map.contains_key(&display_index) {
            return None;
        }
        self.order.retain(|&i| i != display_index);
        self.order.push_back(display_index);
        self.map.get(&display_index).map(|(f, _)| f.clone())
    }

    fn put(&mut self, display_index: i64, frame: DecodedFrame) {
        if self.map.contains_key(&display_index) {
            return;
        }
        let cost = (frame.width as i64) * (frame.height as i64) * 4;
        self.map.insert(display_index, (frame, cost));
        self.order.push_back(display_index);
        self.used_bytes += cost;
        while self.used_bytes > self.capacity_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some((_, cost)) = self.map.remove(&oldest) {
                self.used_bytes -= cost;
            }
        }
    }

    fn contains(&self, display_index: i64) -> bool {
        self.map.contains_key(&display_index)
    }
}

pub struct VideoDecoder {
    input: ffmpeg::format::context::Input,
    stream_index: usize,
    decoder: ffmpeg::decoder::Video,
    scaler: ScalingContext,
    fps: f64,
    width: u32,
    height: u32,
    index: Vec<IndexEntry>,
    cache: FrameCache,
    last_display_index: i64,
}

unsafe impl Send for VideoDecoder {}

const DEFAULT_CACHE_BYTES: i64 = 512 * 1024 * 1024;

impl VideoDecoder {
    pub fn open(path: &Path) -> Result<Self, ffmpeg::Error> {
        let mut input = ffmpeg::format::input(path)?;
        let stream = input
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or(ffmpeg::Error::StreamNotFound)?;
        let stream_index = stream.index();
        let fps_rational = stream.avg_frame_rate();
        let fps = fps_rational.numerator() as f64 / fps_rational.denominator().max(1) as f64;

        let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
        let mut decoder = context.decoder().video()?;
        let width = decoder.width();
        let height = decoder.height();

        let scaler = ScalingContext::get(
            decoder.format(),
            width,
            height,
            ffmpeg::format::Pixel::RGBA,
            width,
            height,
            ScalingFlags::BILINEAR,
        )?;

        let index = build_index(&mut input, stream_index, &mut decoder)?;
        input.seek(i64::MIN, ..)?;
        decoder.flush();

        Ok(Self {
            input,
            stream_index,
            decoder,
            scaler,
            fps: if fps > 0.0 { fps } else { 30.0 },
            width,
            height,
            index,
            cache: FrameCache::new(DEFAULT_CACHE_BYTES),
            last_display_index: -1,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn fps(&self) -> f64 {
        self.fps
    }

    pub fn total_frames(&self) -> i64 {
        self.index.len() as i64
    }

    pub fn frame_at(&mut self, frame_index: i64) -> Result<DecodedFrame, ffmpeg::Error> {
        let target_index = frame_index.clamp(0, self.total_frames() - 1);
        if let Some(frame) = self.cache.get(target_index) {
            self.last_display_index = target_index;
            return Ok(frame);
        }

        let need_seek = self.last_display_index < 0 || target_index <= self.last_display_index;
        if need_seek {
            let keyframe_index = self.preceding_keyframe(target_index);
            let seek_pts = self.index[keyframe_index as usize].pts;
            self.input.seek(seek_pts, ..seek_pts)?;
            self.decoder.flush();
            self.last_display_index = -1;
        }

        self.decode_until(target_index)
    }

    fn preceding_keyframe(&self, target_index: i64) -> i64 {
        for i in (0..=target_index).rev() {
            if self.index[i as usize].key {
                return i;
            }
        }
        0
    }

    fn decode_until(&mut self, target_index: i64) -> Result<DecodedFrame, ffmpeg::Error> {
        let mut decoded = VideoFrame::empty();
        let mut result: Option<DecodedFrame> = None;
        let stream_index = self.stream_index;
        for (stream, packet) in self.input.packets() {
            if stream.index() != stream_index {
                continue;
            }
            self.decoder.send_packet(&packet)?;
            while self.decoder.receive_frame(&mut decoded).is_ok() {
                let pts = decoded.pts().unwrap_or(0);
                let found = self.index.binary_search_by_key(&pts, |e| e.pts).ok();
                let Some(display_index) = found.map(|i| i as i64) else {
                    continue;
                };
                self.last_display_index = display_index;
                if self.cache.contains(display_index) {
                    if display_index == target_index {
                        result = self.cache.get(display_index);
                    }
                } else {
                    let rgba = convert_rgba(&mut self.scaler, self.width, self.height, &decoded)?;
                    if display_index == target_index {
                        result = Some(rgba.clone());
                    }
                    self.cache.put(display_index, rgba);
                }
                if display_index >= target_index && result.is_some() {
                    return Ok(result.unwrap());
                }
            }
        }
        result.ok_or(ffmpeg::Error::Eof)
    }
}

fn convert_rgba(
    scaler: &mut ScalingContext,
    width: u32,
    height: u32,
    frame: &VideoFrame,
) -> Result<DecodedFrame, ffmpeg::Error> {
    let mut rgba = VideoFrame::empty();
    scaler.run(frame, &mut rgba)?;
    let stride = rgba.stride(0);
    let width_px = width as usize;
    let height_px = height as usize;
    let data = rgba.data(0);
    let mut out = Vec::with_capacity(width_px * height_px * 4);
    for row in 0..height_px {
        let start = row * stride;
        out.extend_from_slice(&data[start..start + width_px * 4]);
    }
    Ok(DecodedFrame {
        width,
        height,
        rgba: out,
    })
}

fn build_index(
    input: &mut ffmpeg::format::context::Input,
    stream_index: usize,
    decoder: &mut ffmpeg::decoder::Video,
) -> Result<Vec<IndexEntry>, ffmpeg::Error> {
    let mut index = Vec::new();
    let mut decoded = VideoFrame::empty();
    for (stream, packet) in input.packets() {
        if stream.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet)?;
        while decoder.receive_frame(&mut decoded).is_ok() {
            index.push(IndexEntry {
                pts: decoded.pts().unwrap_or(0),
                key: decoded.is_key(),
            });
        }
    }
    decoder.send_eof()?;
    while decoder.receive_frame(&mut decoded).is_ok() {
        index.push(IndexEntry {
            pts: decoded.pts().unwrap_or(0),
            key: decoded.is_key(),
        });
    }
    index.sort_by_key(|e| e.pts);
    Ok(index)
}
