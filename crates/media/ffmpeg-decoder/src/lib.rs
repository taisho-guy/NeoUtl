use ffmpeg_next as ffmpeg;
use ffmpeg_next::software::scaling::{Context as ScalingContext, Flags as ScalingFlags};
use ffmpeg_next::util::frame::Video as VideoFrame;
use neoutl_media_api::{DEFAULT_DECODE_CACHE_BYTES, VideoSource};

/// RGBA8バイト列をRgba8Unormテクスチャへアップロードする。cache.rs::materialize撤去に伴い
/// このCPU系decoderクレート内へ複製移動。
fn upload_rgba8(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &[u8],
    width: u32,
    height: u32,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("video-rgba8-frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture
}
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
    map: HashMap<i64, (Vec<u8>, i64)>,
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
    fn get(&mut self, index: i64) -> Option<Vec<u8>> {
        if !self.map.contains_key(&index) {
            return None;
        }
        self.order.retain(|&i| i != index);
        self.order.push_back(index);
        self.map.get(&index).map(|(f, _)| f.clone())
    }
    fn put(&mut self, index: i64, rgba: Vec<u8>) {
        if self.map.contains_key(&index) {
            return;
        }
        let cost = rgba.len() as i64;
        self.map.insert(index, (rgba, cost));
        self.order.push_back(index);
        self.used_bytes += cost;
        while self.used_bytes > self.capacity_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some((_, c)) = self.map.remove(&oldest) {
                self.used_bytes -= c;
            }
        }
    }
    fn contains(&self, index: i64) -> bool {
        self.map.contains_key(&index)
    }
}

pub struct FfmpegVideoDecoder {
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

unsafe impl Send for FfmpegVideoDecoder {}

impl FfmpegVideoDecoder {
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
            cache: FrameCache::new(DEFAULT_DECODE_CACHE_BYTES),
            last_display_index: -1,
        })
    }

    fn preceding_keyframe(&self, target_index: i64) -> i64 {
        for i in (0..=target_index).rev() {
            if self.index[i as usize].key {
                return i;
            }
        }
        0
    }

    fn decode_until(&mut self, target_index: i64) -> Result<Vec<u8>, String> {
        let mut decoded = VideoFrame::empty();
        let mut result: Option<Vec<u8>> = None;
        let stream_index = self.stream_index;
        for (stream, packet) in self.input.packets() {
            if stream.index() != stream_index {
                continue;
            }
            self.decoder
                .send_packet(&packet)
                .map_err(|e| e.to_string())?;
            while self.decoder.receive_frame(&mut decoded).is_ok() {
                let pts = decoded.pts().unwrap_or(0);
                let Some(display_index) = self
                    .index
                    .binary_search_by_key(&pts, |e| e.pts)
                    .ok()
                    .map(|i| i as i64)
                else {
                    continue;
                };
                self.last_display_index = display_index;
                if self.cache.contains(display_index) {
                    if display_index == target_index {
                        result = self.cache.get(display_index);
                    }
                } else {
                    let rgba = convert_rgba(&mut self.scaler, self.width, self.height, &decoded)
                        .map_err(|e| e.to_string())?;
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
        result.ok_or_else(|| "EOF".to_owned())
    }

    fn rgba_at(&mut self, frame_index: i64) -> Result<Vec<u8>, String> {
        let target = frame_index.clamp(0, self.total_frames() - 1);
        if let Some(f) = self.cache.get(target) {
            self.last_display_index = target;
            return Ok(f);
        }
        let need_seek = self.last_display_index < 0 || target <= self.last_display_index;
        if need_seek {
            let key = self.preceding_keyframe(target);
            let seek_pts = self.index[key as usize].pts;
            self.input
                .seek(seek_pts, ..seek_pts)
                .map_err(|e| e.to_string())?;
            self.decoder.flush();
            self.last_display_index = -1;
        }
        self.decode_until(target)
    }
}

impl VideoSource for FfmpegVideoDecoder {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn fps(&self) -> f64 {
        self.fps
    }
    fn total_frames(&self) -> i64 {
        self.index.len() as i64
    }

    /// バックグラウンドスレッド専用。デコード・カラースペース変換までを完了し
    /// 内部FrameCacheへ蓄積する。GPU操作なし。
    fn prefetch(&mut self, frame_index: i64) -> Result<(), String> {
        self.rgba_at(frame_index)?;
        Ok(())
    }

    /// UIスレッド専用。prefetch済みRGBA8バイト列をテクスチャへアップロードする。
    fn frame_gpu(
        &mut self,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        let rgba = self.rgba_at(frame_index)?;
        Ok(upload_rgba8(device, queue, &rgba, self.width, self.height))
    }
}

fn convert_rgba(
    scaler: &mut ScalingContext,
    width: u32,
    height: u32,
    frame: &VideoFrame,
) -> Result<Vec<u8>, ffmpeg::Error> {
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
    Ok(out)
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

pub fn decode_audio(path: &Path) -> Result<neoutl_media_api::AudioBuffer, String> {
    use ffmpeg_next::software::resampling::Context as ResamplingContext;
    use ffmpeg_next::util::format::sample::{Sample, Type as SampleType};
    use ffmpeg_next::util::frame::Audio as AudioFrame;

    let mut input = ffmpeg::format::input(path).map_err(|e| e.to_string())?;
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or(ffmpeg::Error::StreamNotFound)
        .map_err(|e| e.to_string())?;
    let stream_index = stream.index();

    let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| e.to_string())?;
    let mut decoder = context.decoder().audio().map_err(|e| e.to_string())?;

    let out_rate = decoder.rate();
    let out_channels = decoder.channels();
    let out_layout = decoder.channel_layout();

    let mut resampler = ResamplingContext::get(
        decoder.format(),
        decoder.channel_layout(),
        decoder.rate(),
        Sample::F32(SampleType::Packed),
        out_layout,
        out_rate,
    )
    .map_err(|e| e.to_string())?;

    let mut samples: Vec<f32> = Vec::new();
    let mut decoded = AudioFrame::empty();
    let mut resampled = AudioFrame::empty();

    for (stream, packet) in input.packets() {
        if stream.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet).map_err(|e| e.to_string())?;
        while decoder.receive_frame(&mut decoded).is_ok() {
            resampler
                .run(&decoded, &mut resampled)
                .map_err(|e| e.to_string())?;
            append_planar_f32(&resampled, out_channels, &mut samples);
        }
    }
    decoder.send_eof().map_err(|e| e.to_string())?;
    while decoder.receive_frame(&mut decoded).is_ok() {
        resampler
            .run(&decoded, &mut resampled)
            .map_err(|e| e.to_string())?;
        append_planar_f32(&resampled, out_channels, &mut samples);
    }

    Ok(neoutl_media_api::AudioBuffer {
        sample_rate: out_rate,
        channels: out_channels,
        samples,
    })
}

fn append_planar_f32(frame: &ffmpeg_next::util::frame::Audio, channels: u16, out: &mut Vec<f32>) {
    let data = frame.data(0);
    let sample_count = frame.samples() * channels as usize;
    let bytes = &data[..sample_count * 4];
    out.extend(
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
    );
}
