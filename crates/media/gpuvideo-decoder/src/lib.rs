use gpu_video::parameters::{
    ColorRange, ColorSpace, DecoderParameters, VulkanAdapterDescriptor, VulkanDeviceDescriptor,
    WgpuConverterParameters,
};
use gpu_video::{EncodedInputChunk, VulkanInstance, WgpuNv12ToRgbaConverter, WgpuTexturesDecoder};
use neoutl_media_api::{FrameOutput, VideoSource};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::path::Path;
use symphonia::core::codecs::video::well_known::CODEC_ID_H264;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

const CACHE_BUDGET_BYTES: i64 = 512 * 1024 * 1024;

struct TextureCache {
    used_bytes: i64,
    order: VecDeque<i64>,
    map: HashMap<i64, wgpu::Texture>,
}

impl TextureCache {
    fn new() -> Self {
        Self {
            used_bytes: 0,
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }
    fn get(&mut self, index: i64) -> Option<wgpu::Texture> {
        if !self.map.contains_key(&index) {
            return None;
        }
        self.order.retain(|&i| i != index);
        self.order.push_back(index);
        self.map.get(&index).cloned()
    }
    fn put(&mut self, index: i64, texture: wgpu::Texture, cost: i64) {
        if self.map.contains_key(&index) {
            return;
        }
        self.map.insert(index, texture);
        self.order.push_back(index);
        self.used_bytes += cost;
        while self.used_bytes > CACHE_BUDGET_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.map.remove(&oldest);
            self.used_bytes -= cost;
        }
    }
}

fn find_h264_track_id(demux: &dyn FormatReader) -> Option<u32> {
    demux.tracks().iter().find_map(|t| {
        let video = t.codec_params.as_ref()?.video()?;
        (video.codec == CODEC_ID_H264).then_some(t.id)
    })
}

pub struct GpuVideoDecoder {
    demux: Box<dyn FormatReader>,
    track_id: u32,
    decoder: WgpuTexturesDecoder,
    converter: WgpuNv12ToRgbaConverter,
    width: u32,
    height: u32,
    fps: f64,
    total_frames: i64,
    display_index: HashMap<i64, i64>,
    cache: TextureCache,
    last_display_index: i64,
}

fn probe(path: &Path) -> Result<Box<dyn FormatReader>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())
}

fn build_index(demux: &mut Box<dyn FormatReader>, track_id: u32) -> Result<Vec<i64>, String> {
    let mut pts_list = Vec::new();
    loop {
        match demux.next_packet().map_err(|e| e.to_string())? {
            Some(packet) => {
                if packet.track_id == track_id {
                    pts_list.push(packet.pts.get());
                }
            }
            None => break,
        }
    }
    demux
        .seek(
            SeekMode::Coarse,
            SeekTo::Timestamp {
                ts: symphonia::core::units::Timestamp::new(0),
                track_id,
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(pts_list)
}

impl GpuVideoDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut demux = probe(path)?;
        let track_id = find_h264_track_id(demux.as_ref()).ok_or("H.264トラック未検出")?;

        let pts_list = build_index(&mut demux, track_id)?;
        let total_frames = pts_list.len() as i64;
        let display_index: HashMap<i64, i64> = pts_list
            .iter()
            .enumerate()
            .map(|(i, &pts)| (pts, i as i64))
            .collect();

        let track = demux.tracks().iter().find(|t| t.id == track_id).unwrap();
        let video_cp = track
            .codec_params
            .as_ref()
            .and_then(|cp| cp.video())
            .ok_or("codec_params未定義")?;
        let width = video_cp.width.ok_or("width未定義")?.into();
        let height = video_cp.height.ok_or("height未定義")?.into();
        let tb = track.time_base.ok_or("time_base未定義")?;
        let fps = if pts_list.len() >= 2 {
            let span = (pts_list[pts_list.len() - 1] - pts_list[0]) as f64 * tb.numer.get() as f64
                / tb.denom.get() as f64;
            (pts_list.len() as f64 - 1.0) / span.max(1e-6)
        } else {
            30.0
        };

        let instance = VulkanInstance::new().map_err(|e| e.to_string())?;
        let adapter = instance
            .create_adapter(&VulkanAdapterDescriptor {
                compatible_surface: None,
                ..Default::default()
            })
            .map_err(|e| e.to_string())?;
        let device = adapter
            .create_device(&VulkanDeviceDescriptor::default())
            .map_err(|e| e.to_string())?;
        let decoder = device
            .create_wgpu_textures_decoder_h264(DecoderParameters::default())
            .map_err(|e| e.to_string())?;

        let converter = WgpuNv12ToRgbaConverter::new(
            &device.wgpu_device(),
            WgpuConverterParameters {
                color_space: ColorSpace::BT709,
                color_range: ColorRange::Limited,
            },
        )
        .map_err(|e| e.to_string())?;

        Ok(Self {
            demux,
            track_id,
            decoder,
            converter,
            width,
            height,
            fps,
            total_frames,
            display_index,
            cache: TextureCache::new(),
            last_display_index: -1,
        })
    }

    /// Stage 2 でUIスレッド上のNV12→RGBA変換として復活させるまで保持。
    /// 現在 frame() は Err を返すため未使用。
    #[allow(dead_code)]
    fn decode_until(
        &mut self,
        target: i64,
        out_device: &wgpu::Device,
        out_queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        loop {
            let packet = match self.demux.next_packet().map_err(|e| e.to_string())? {
                Some(p) => p,
                None => return Err("EOF".to_owned()),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let Some(&idx) = self.display_index.get(&packet.pts.get()) else {
                continue;
            };
            self.last_display_index = idx;

            let chunk = EncodedInputChunk {
                data: &packet.data,
                pts: Some(packet.pts.get().try_into().unwrap()),
            };
            let frames = self.decoder.decode(chunk).map_err(|e| e.to_string())?;
            for frame in frames {
                let rgba = out_device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: self.width,
                        height: self.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let rgba_view = rgba.create_view(&wgpu::TextureViewDescriptor::default());
                let bind_group = self
                    .converter
                    .create_input_bind_group(&frame)
                    .map_err(|e| e.to_string())?;
                let mut encoder =
                    out_device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                self.converter
                    .convert(&mut encoder, &bind_group, &rgba_view);
                out_queue.submit(Some(encoder.finish()));

                let cost = (self.width as i64) * (self.height as i64) * 4;
                self.cache.put(idx, rgba.clone(), cost);
                if idx == target {
                    return Ok(rgba);
                }
            }
            if idx >= target {
                return self
                    .cache
                    .get(target)
                    .ok_or("対象フレーム未生成".to_owned());
            }
        }
    }
}

impl VideoSource for GpuVideoDecoder {
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
        self.total_frames
    }

    /// Stage 1: 一時的に無効化。
    /// gpuvideo の NV12→RGBA 変換はホストキュー(out_queue.submit)を必要とするため、
    /// デコードスレッドから呼ぶと Surface::present() との SnatchLock 競合で
    /// デッドロックする。変換をUIスレッドへ移行する Stage 2 で真ゼロコピーパスとして
    /// 復活させるまで、FrameOutput::Gpu 経路は封印する。
    fn frame(&mut self, _frame_index: i64) -> Result<FrameOutput, String> {
        Err("gpuvideo-decoder はデッドロック回避のため一時無効化されています".to_string())
    }
}
