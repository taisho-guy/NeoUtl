#[cfg(target_os = "linux")]
mod imp {
    use gpu_video::parameters::{
        ColorRange, ColorSpace, DecoderParameters, WgpuConverterParameters,
    };
    use gpu_video::{
        EncodedInputChunk, VulkanDevice as GpuVideoDevice, WgpuNv12ToRgbaConverter,
        WgpuTexturesDecoder,
    };
    use neoutl_media_api::{DEFAULT_DECODE_CACHE_BYTES, VideoSource};
    use std::collections::{HashMap, VecDeque};
    use std::fs::File;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};
    use symphonia::core::codecs::video::well_known::CODEC_ID_H264;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    const START_CODE: &[u8] = &[0, 0, 0, 1];

    fn create_crop_scratch_texture(
        device: &wgpu::Device,
        coded_width: u32,
        coded_height: u32,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpuvideo-decoder crop scratch (coded size)"),
            size: wgpu::Extent3d {
                width: coded_width,
                height: coded_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    const DECODE_LOOKAHEAD: i64 = 8;

    #[derive(Clone, Debug)]
    struct H264Config {
        nal_length_size: usize,
        sps: Vec<Vec<u8>>,
        pps: Vec<Vec<u8>>,
    }

    impl H264Config {
        fn inject_sps_pps(&self, out: &mut Vec<u8>) {
            for sps in &self.sps {
                out.extend_from_slice(START_CODE);
                out.extend_from_slice(sps);
            }
            for pps in &self.pps {
                out.extend_from_slice(START_CODE);
                out.extend_from_slice(pps);
            }
        }
    }

    fn parse_avcc_config(extra: &[u8]) -> Result<H264Config, String> {
        if extra.len() < 7 {
            return Err("avcC too short".to_string());
        }

        let nal_length_size = ((extra[4] & 0x03) + 1) as usize;
        let num_sps = (extra[5] & 0x1f) as usize;
        let mut off = 6;

        let mut sps = Vec::with_capacity(num_sps);
        for _ in 0..num_sps {
            if off + 2 > extra.len() {
                return Err("avcC truncated before SPS len".to_string());
            }
            let len = u16::from_be_bytes([extra[off], extra[off + 1]]) as usize;
            off += 2;
            if off + len > extra.len() {
                return Err("avcC truncated inside SPS".to_string());
            }
            sps.push(extra[off..off + len].to_vec());
            off += len;
        }

        if off >= extra.len() {
            return Err("avcC truncated before PPS count".to_string());
        }
        let num_pps = extra[off] as usize;
        off += 1;

        let mut pps = Vec::with_capacity(num_pps);
        for _ in 0..num_pps {
            if off + 2 > extra.len() {
                return Err("avcC truncated before PPS len".to_string());
            }
            let len = u16::from_be_bytes([extra[off], extra[off + 1]]) as usize;
            off += 2;
            if off + len > extra.len() {
                return Err("avcC truncated inside PPS".to_string());
            }
            pps.push(extra[off..off + len].to_vec());
            off += len;
        }

        if sps.is_empty() || pps.is_empty() {
            return Err("avcC missing SPS/PPS".to_string());
        }

        Ok(H264Config {
            nal_length_size,
            sps,
            pps,
        })
    }

    fn avcc_sample_to_annexb(
        cfg: &H264Config,
        sample_avcc: &[u8],
        inject_ps: bool,
    ) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(sample_avcc.len() + 256);

        if inject_ps {
            cfg.inject_sps_pps(&mut out);
        }

        let mut off = 0usize;
        while off + cfg.nal_length_size <= sample_avcc.len() {
            let len = match cfg.nal_length_size {
                1 => sample_avcc[off] as usize,
                2 => u16::from_be_bytes([sample_avcc[off], sample_avcc[off + 1]]) as usize,
                4 => u32::from_be_bytes([
                    sample_avcc[off],
                    sample_avcc[off + 1],
                    sample_avcc[off + 2],
                    sample_avcc[off + 3],
                ]) as usize,
                n => return Err(format!("unsupported nal_length_size={n}")),
            };
            off += cfg.nal_length_size;

            if len == 0 {
                continue;
            }
            if off + len > sample_avcc.len() {
                return Err(format!(
                    "AVCC sample truncated: off={} len={} total={}",
                    off,
                    len,
                    sample_avcc.len()
                ));
            }

            out.extend_from_slice(START_CODE);
            out.extend_from_slice(&sample_avcc[off..off + len]);
            off += len;
        }

        if out.is_empty() {
            return Err("empty AnnexB output".to_string());
        }
        Ok(out)
    }

    static GPU_DECODE_LOCK: Mutex<()> = Mutex::new(());
    const GPU_DECODE_LOCK_WAIT: Duration = Duration::from_millis(1500);
    const GPU_DECODE_LOCK_POLL: Duration = Duration::from_millis(5);

    fn acquire_gpu_decode_lock(
        frame_index: i64,
    ) -> Result<std::sync::MutexGuard<'static, ()>, String> {
        let deadline = Instant::now() + GPU_DECODE_LOCK_WAIT;
        loop {
            match GPU_DECODE_LOCK.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                    return Ok(poisoned.into_inner());
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "GPU_DECODE_LOCK取得タイムアウト (frame={frame_index}, wait={:?})",
                            GPU_DECODE_LOCK_WAIT
                        ));
                    }
                    thread::sleep(GPU_DECODE_LOCK_POLL);
                }
            }
        }
    }

    struct TextureCache {
        pool: Vec<wgpu::Texture>,
        free: VecDeque<usize>,
        map: HashMap<i64, usize>,
        order: VecDeque<i64>,
    }

    impl TextureCache {
        fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
            let cost = (width as i64) * (height as i64) * 4;
            let byte_budget_capacity = (DEFAULT_DECODE_CACHE_BYTES / cost.max(1)).max(1) as usize;
            let capacity = byte_budget_capacity.max((DECODE_LOOKAHEAD as usize) + 1);

            let pool: Vec<wgpu::Texture> = (0..capacity)
                .map(|_| {
                    device.create_texture(&wgpu::TextureDescriptor {
                        label: None,
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    })
                })
                .collect();

            eprintln!(
                "{}",
                t!(
                    "[gpuvideo][cache] pool allocated capacity=%{arg0} cost_per_slot=%{arg1} limit=%{arg2}",
                    arg0 = capacity,
                    arg1 = cost,
                    arg2 = byte_budget_capacity
                )
            );

            Self {
                pool,
                free: (0..capacity).collect(),
                map: HashMap::new(),
                order: VecDeque::new(),
            }
        }

        fn get(&mut self, index: i64) -> Option<wgpu::Texture> {
            let slot = *self.map.get(&index)?;
            self.order.retain(|&i| i != index);
            self.order.push_back(index);
            eprintln!(
                "{}",
                t!(
                    "[gpuvideo][cache] get hit index=%{arg0} slot=%{arg1} thread=%{arg2} entries=%{arg3}",
                    arg0 = index,
                    arg1 = slot,
                    arg2 = format!("{:?}", thread::current().id()),
                    arg3 = self.map.len()
                )
            );
            Some(self.pool[slot].clone())
        }

        fn acquire_for_write(&mut self, index: i64) -> wgpu::Texture {
            if let Some(&slot) = self.map.get(&index) {
                self.order.retain(|&i| i != index);
                self.order.push_back(index);
                return self.pool[slot].clone();
            }

            let slot = if let Some(s) = self.free.pop_front() {
                s
            } else {
                let oldest = self
                    .order
                    .pop_front()
                    .expect("capacity>=1のためプール枯渇時はorderが必ず非空");
                let s = self.map.remove(&oldest).expect("orderとmapは常に同期");
                eprintln!(
                    "{}",
                    t!(
                        "[gpuvideo][cache] evict index=%{arg0} slot=%{arg1} thread=%{arg2} entries=%{arg3}",
                        arg0 = oldest,
                        arg1 = s,
                        arg2 = format!("{:?}", thread::current().id()),
                        arg3 = self.map.len()
                    )
                );
                s
            };

            self.map.insert(index, slot);
            self.order.push_back(index);
            eprintln!(
                "{}",
                t!(
                    "[gpuvideo][cache] put index=%{arg0} slot=%{arg1} thread=%{arg2} entries=%{arg3}/%{arg4}",
                    arg0 = index,
                    arg1 = slot,
                    arg2 = format!("{:?}", thread::current().id()),
                    arg3 = self.map.len(),
                    arg4 = self.pool.len()
                )
            );
            self.pool[slot].clone()
        }
    }

    struct EncodedPacket {
        display_index: i64,
        pts: i64,
        data: Vec<u8>,
        is_sync: bool,
    }

    fn packet_is_sync(cfg: &H264Config, sample_avcc: &[u8]) -> bool {
        let mut off = 0usize;
        while off + cfg.nal_length_size <= sample_avcc.len() {
            let len = match cfg.nal_length_size {
                1 => sample_avcc[off] as usize,
                2 => u16::from_be_bytes([sample_avcc[off], sample_avcc[off + 1]]) as usize,
                4 => u32::from_be_bytes([
                    sample_avcc[off],
                    sample_avcc[off + 1],
                    sample_avcc[off + 2],
                    sample_avcc[off + 3],
                ]) as usize,
                _ => return false,
            };
            off += cfg.nal_length_size;
            if len == 0 || off + len > sample_avcc.len() {
                break;
            }
            let nal_type = sample_avcc[off] & 0x1f;
            if nal_type == 5 {
                return true;
            }
            off += len;
        }
        false
    }

    fn find_prev_sync(packets: &[EncodedPacket], idx: i64) -> i64 {
        let idx = idx.clamp(0, packets.len() as i64 - 1);
        for i in (0..=idx).rev() {
            if packets[i as usize].is_sync {
                return i;
            }
        }
        0
    }

    fn find_h264_track_id(demux: &dyn FormatReader) -> Option<u32> {
        demux.tracks().iter().find_map(|t| {
            let video = t.codec_params.as_ref()?.video()?;
            (video.codec == CODEC_ID_H264).then_some(t.id)
        })
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

    fn preload_packets(
        demux: &mut Box<dyn FormatReader>,
        track_id: u32,
        h264_cfg: &H264Config,
    ) -> Result<Vec<EncodedPacket>, String> {
        demux
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: symphonia::core::units::Time::default(),
                    track_id: Some(track_id),
                },
            )
            .map_err(|e| e.to_string())?;

        let mut packets = Vec::new();
        let mut display_index: i64 = 0;

        while let Some(packet) = demux.next_packet().map_err(|e| e.to_string())? {
            if packet.track_id != track_id {
                continue;
            }

            let pts_i64 = packet.pts.get();
            let data = packet.data.to_vec();
            let is_sync = packet_is_sync(h264_cfg, &data);

            packets.push(EncodedPacket {
                display_index,
                pts: pts_i64,
                data,
                is_sync,
            });

            display_index += 1;
        }

        Ok(packets)
    }

    pub struct GpuVideoDecoder {
        decoder: WgpuTexturesDecoder,
        converter: WgpuNv12ToRgbaConverter,
        width: u32,
        height: u32,

        crop_scratch: Option<(u32, u32, wgpu::Texture)>,

        fps: f64,
        total_frames: i64,

        packets: Vec<EncodedPacket>,
        cache: TextureCache,

        pending: VecDeque<EncodedPacket>,

        planned_gop_start: Option<i64>,

        h264_cfg: H264Config,

        device: Arc<GpuVideoDevice>,

        expected_next: Option<i64>,

        next_output_index: i64,

        reset_count: u64,
    }

    impl GpuVideoDecoder {
        pub fn open(path: &Path, device: &Arc<GpuVideoDevice>) -> Result<Self, String> {
            eprintln!(
                "{}",
                t!(
                    "[gpuvideo] open_video begin path=%{arg0}",
                    arg0 = path.display()
                )
            );

            let mut demux = probe(path)?;
            let track_id = find_h264_track_id(demux.as_ref()).ok_or("H.264トラック未検出")?;

            let track = demux
                .tracks()
                .iter()
                .find(|t| t.id == track_id)
                .ok_or_else(|| "H.264 track not found".to_string())?;

            let video_cp = track
                .codec_params
                .as_ref()
                .and_then(|cp| cp.video())
                .ok_or("codec_params未定義")?;

            let width: u32 = video_cp.width.ok_or("width未定義")?.into();
            let height: u32 = video_cp.height.ok_or("height未定義")?.into();

            let tb = track.time_base.ok_or("time_base未定義")?;
            let tb_numer = tb.numer.get() as f64;
            let tb_denom = tb.denom.get() as f64;

            let extra_data = video_cp
                .extra_data
                .iter()
                .find(|d| {
                    d.id == symphonia::core::codecs::video::well_known::extra_data::VIDEO_EXTRA_DATA_ID_AVC_DECODER_CONFIG
                })
                .map(|d| d.data.as_ref())
                .ok_or_else(|| "missing H.264 extra_data (AVCDecoderConfigurationRecord)".to_string())?;

            let h264_cfg = parse_avcc_config(extra_data)?;

            let packets = preload_packets(&mut demux, track_id, &h264_cfg)?;
            let total_frames = packets.len() as i64;

            let fps = if total_frames >= 2 {
                let first_pts = packets.first().map(|p| p.pts).unwrap_or(0);
                let last_pts = packets.last().map(|p| p.pts).unwrap_or(first_pts);

                let pts_span = (last_pts - first_pts) as f64;
                let span_seconds = pts_span * tb_numer / tb_denom;
                let frames = (total_frames as f64).max(1.0);
                if span_seconds > 1e-6 {
                    (frames - 1.0) / span_seconds
                } else {
                    30.0
                }
            } else {
                30.0
            };

            let decoder_init_started = Instant::now();
            eprintln!(
                "{}",
                t!(
                    "[gpuvideo][open] create_wgpu_textures_decoder_h264 begin path=%{arg0} thread=%{arg1}",
                    arg0 = path.display(),
                    arg1 = format!("{:?}", thread::current().id())
                )
            );
            let decoder = device
                .create_wgpu_textures_decoder_h264(DecoderParameters::default())
                .map_err(|e| {
                    let msg = format!("create_wgpu_textures_decoder_h264 failed: {e}");
                    eprintln!(
                        "{}",
                        t!(
                            "[gpuvideo] open_video failed path=%{arg0} reason=%{arg1}",
                            arg0 = path.display(),
                            arg1 = &msg
                        )
                    );
                    msg
                })?;
            eprintln!(
                "{}",
                t!(
                    "[gpuvideo][open] create_wgpu_textures_decoder_h264 end path=%{arg0} elapsed_ms=%{arg1} thread=%{arg2}",
                    arg0 = path.display(),
                    arg1 = decoder_init_started.elapsed().as_millis(),
                    arg2 = format!("{:?}", thread::current().id())
                )
            );

            let converter_init_started = Instant::now();
            let converter = WgpuNv12ToRgbaConverter::new(
                &device.wgpu_device(),
                WgpuConverterParameters {
                    color_space: ColorSpace::BT709,
                    color_range: ColorRange::Limited,
                },
            )
            .map_err(|e| {
                let msg = format!("WgpuNv12ToRgbaConverter::new failed: {e}");
                eprintln!(
                    "{}",
                    t!(
                        "[gpuvideo] open_video failed path=%{arg0} reason=%{arg1}",
                        arg0 = path.display(),
                        arg1 = &msg
                    )
                );
                msg
            })?;
            eprintln!(
                "{}",
                t!(
                    "[gpuvideo][open] converter init end path=%{arg0} elapsed_ms=%{arg1} thread=%{arg2}",
                    arg0 = path.display(),
                    arg1 = converter_init_started.elapsed().as_millis(),
                    arg2 = format!("{:?}", thread::current().id())
                )
            );

            eprintln!(
                "{}",
                t!(
                    "[gpuvideo] open_video ok path=%{arg0} codec=h264 %{arg1}x%{arg2} fps=%{arg3} frames=%{arg4}",
                    arg0 = path.display(),
                    arg1 = width,
                    arg2 = height,
                    arg3 = fps,
                    arg4 = total_frames
                )
            );

            Ok(Self {
                decoder,
                converter,
                width,
                height,
                crop_scratch: None,
                fps,
                total_frames,
                packets,
                cache: TextureCache::new(&device.wgpu_device(), width, height),
                pending: VecDeque::new(),
                planned_gop_start: None,
                h264_cfg,
                device: Arc::clone(device),
                expected_next: None,
                next_output_index: 0,
                reset_count: 0,
            })
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

        fn prefetch(&mut self, frame_index: i64) -> Result<(), String> {
            eprintln!(
                "{}",
                t!(
                    "[gpuvideo][prefetch] enter frame_index=%{arg0} thread=%{arg1} pending_len=%{arg2} pending_front=%{arg3} pending_back=%{arg4} cache_entries=%{arg5}",
                    arg0 = frame_index,
                    arg1 = format!("{:?}", thread::current().id()),
                    arg2 = self.pending.len(),
                    arg3 = format!("{:?}", self.pending.front().map(|p| p.display_index)),
                    arg4 = format!("{:?}", self.pending.back().map(|p| p.display_index)),
                    arg5 = self.cache.map.len()
                )
            );
            if self.cache.map.contains_key(&frame_index) {
                return Ok(());
            }

            if frame_index < 0 || (frame_index as usize) >= self.packets.len() {
                let msg = format!("prefetch EOF (frame={frame_index})");
                eprintln!("{}", t!("[gpuvideo] prefetch failed %{arg0}", arg0 = &msg));
                return Err(msg);
            }

            if self.pending.iter().any(|p| p.display_index == frame_index) {
                return Ok(());
            }

            let needed_sync = find_prev_sync(&self.packets, frame_index);
            let queue_end = (frame_index + DECODE_LOOKAHEAD).min(self.packets.len() as i64 - 1);
            let reset = self.planned_gop_start != Some(needed_sync);

            eprintln!(
                "{}",
                t!(
                    "[gpuvideo][prefetch] plan frame_index=%{arg0} needed_sync=%{arg1} queue_end=%{arg2} reset=%{arg3} thread=%{arg4}",
                    arg0 = frame_index,
                    arg1 = needed_sync,
                    arg2 = queue_end,
                    arg3 = reset,
                    arg4 = format!("{:?}", thread::current().id())
                )
            );

            if reset {
                self.pending.clear();
                for idx in needed_sync..=queue_end {
                    let p = &self.packets[idx as usize];
                    self.pending.push_back(EncodedPacket {
                        display_index: p.display_index,
                        pts: p.pts,
                        data: p.data.clone(),
                        is_sync: p.is_sync,
                    });
                }
                self.planned_gop_start = Some(needed_sync);
                return Ok(());
            }

            let start = self
                .pending
                .back()
                .map(|p| p.display_index + 1)
                .unwrap_or(needed_sync);
            if start > queue_end {
                return Ok(());
            }
            for idx in start..=queue_end {
                let p = &self.packets[idx as usize];
                self.pending.push_back(EncodedPacket {
                    display_index: p.display_index,
                    pts: p.pts,
                    data: p.data.clone(),
                    is_sync: p.is_sync,
                });
            }

            Ok(())
        }

        fn frame_gpu(
            &mut self,
            frame_index: i64,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
        ) -> Result<wgpu::Texture, String> {
            eprintln!(
                "{}",
                t!(
                    "[gpuvideo][frame_gpu] enter frame_index=%{arg0} thread=%{arg1} pending_len=%{arg2} expected_next=%{arg3} next_output_index=%{arg4} reset_count=%{arg5}",
                    arg0 = frame_index,
                    arg1 = format!("{:?}", thread::current().id()),
                    arg2 = self.pending.len(),
                    arg3 = format!("{:?}", self.expected_next),
                    arg4 = self.next_output_index,
                    arg5 = self.reset_count
                )
            );
            if let Some(cached) = self.cache.get(frame_index) {
                eprintln!(
                    "{}",
                    t!(
                        "[gpuvideo][frame_gpu] cache_hit frame_index=%{arg0} thread=%{arg1}",
                        arg0 = frame_index,
                        arg1 = format!("{:?}", thread::current().id())
                    )
                );
                return Ok(cached);
            }

            while let Some(packet) = self.pending.pop_front() {
                let discontinuous = self.expected_next != Some(packet.display_index);
                if discontinuous {
                    if !packet.is_sync {
                        eprintln!(
                            "{}",
                            t!(
                                "[gpuvideo] frame_gpu warning: non-sync packet at decode-run start display_index=%{arg0} frame_index=%{arg1}",
                                arg0 = packet.display_index,
                                arg1 = frame_index
                            )
                        );
                    }
                    self.reset_count += 1;
                    let reset_started = Instant::now();
                    eprintln!(
                        "{}",
                        t!(
                            "[gpuvideo][reset] begin #%{arg0} display_index=%{arg1} frame_index=%{arg2} thread=%{arg3}",
                            arg0 = self.reset_count,
                            arg1 = packet.display_index,
                            arg2 = frame_index,
                            arg3 = format!("{:?}", thread::current().id())
                        )
                    );
                    self.decoder = self
                        .device
                        .create_wgpu_textures_decoder_h264(DecoderParameters::default())
                        .map_err(|e| {
                            let msg = format!("decoder再生成失敗 (frame={frame_index}) err={e}");
                            eprintln!("{}", t!("[gpuvideo] frame_gpu failed %{arg0}", arg0 = &msg));
                            msg
                        })?;
                    eprintln!(
                        "{}",
                        t!(
                            "[gpuvideo][reset] end #%{arg0} elapsed_ms=%{arg1} thread=%{arg2}",
                            arg0 = self.reset_count,
                            arg1 = reset_started.elapsed().as_millis(),
                            arg2 = format!("{:?}", thread::current().id())
                        )
                    );
                    self.next_output_index = packet.display_index;
                }
                let inject_ps = discontinuous;
                let annexb = avcc_sample_to_annexb(&self.h264_cfg, &packet.data, inject_ps)?;
                self.expected_next = Some(packet.display_index + 1);

                eprintln!(
                    "{}",
                    t!(
                        "[gpuvideo] feed display_index=%{arg0} frame_index=%{arg1} is_sync=%{arg2} pts=%{arg3} avcc_len=%{arg4} annexb_len=%{arg5}",
                        arg0 = packet.display_index,
                        arg1 = frame_index,
                        arg2 = packet.is_sync,
                        arg3 = packet.pts,
                        arg4 = packet.data.len(),
                        arg5 = annexb.len()
                    )
                );

                let chunk = EncodedInputChunk {
                    data: &annexb,
                    pts: Some(
                        packet
                            .pts
                            .try_into()
                            .map_err(|_| "pts変換失敗".to_owned())?,
                    ),
                };

                let decode_started = Instant::now();
                eprintln!(
                    "{}",
                    t!(
                        "[gpuvideo][decode] call_begin display_index=%{arg0} frame_index=%{arg1} thread=%{arg2}",
                        arg0 = packet.display_index,
                        arg1 = frame_index,
                        arg2 = format!("{:?}", thread::current().id())
                    )
                );
                let frames = {
                    let _guard = acquire_gpu_decode_lock(frame_index).map_err(|e| {
                        eprintln!("{}", t!("[gpuvideo] frame_gpu failed %{arg0}", arg0 = &e));
                        e
                    })?;
                    self.decoder.decode(chunk)
                }
                .map_err(|e| {
                    let msg = format!("decoder.decode failed (frame={frame_index}) err={e}");
                    eprintln!("{}", t!("[gpuvideo] frame_gpu failed %{arg0}", arg0 = &msg));
                    msg
                })?;
                eprintln!(
                    "{}",
                    t!(
                        "[gpuvideo][decode] call_end display_index=%{arg0} frame_index=%{arg1} elapsed_ms=%{arg2} output_count=%{arg3} thread=%{arg4}",
                        arg0 = packet.display_index,
                        arg1 = frame_index,
                        arg2 = decode_started.elapsed().as_millis(),
                        arg3 = frames.len(),
                        arg4 = format!("{:?}", thread::current().id())
                    )
                );

                for frame in frames {
                    let display_index = self.next_output_index;
                    self.next_output_index += 1;

                    eprintln!(
                        "{}",
                        t!(
                            "[gpuvideo] output display_index=%{arg0} frame_index=%{arg1}",
                            arg0 = display_index,
                            arg1 = frame_index
                        )
                    );

                    let rgba = self.cache.acquire_for_write(display_index);

                    let bind_group =
                        self.converter
                            .create_input_bind_group(&frame)
                            .map_err(|e| {
                                let msg = format!(
                                    "create_input_bind_group failed (frame={frame_index}) err={e}"
                                );
                                eprintln!(
                                    "{}",
                                    t!("[gpuvideo] frame_gpu failed %{arg0}", arg0 = &msg)
                                );
                                msg
                            })?;

                    let physical_size = frame.data.size();
                    let physical_width = physical_size.width;
                    let physical_height = physical_size.height;

                    let convert_started = Instant::now();
                    let mut encoder =
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

                    if physical_width == self.width && physical_height == self.height {
                        let rgba_view = rgba.create_view(&wgpu::TextureViewDescriptor::default());
                        self.converter
                            .convert(&mut encoder, &bind_group, &rgba_view);
                    } else {
                        let needs_realloc = !matches!(
                            &self.crop_scratch,
                            Some((w, h, _)) if *w == physical_width && *h == physical_height
                        );
                        if needs_realloc {
                            eprintln!(
                                "{}",
                                t!(
                                    "[gpuvideo] crop scratch (re)allocate physical=%{arg0}x%{arg1} display=%{arg2}x%{arg3} frame_index=%{arg4}",
                                    arg0 = physical_width,
                                    arg1 = physical_height,
                                    arg2 = self.width,
                                    arg3 = self.height,
                                    arg4 = frame_index
                                )
                            );
                            self.crop_scratch = Some((
                                physical_width,
                                physical_height,
                                create_crop_scratch_texture(
                                    device,
                                    physical_width,
                                    physical_height,
                                ),
                            ));
                        }
                        let scratch = &self
                            .crop_scratch
                            .as_ref()
                            .expect("crop_scratchは直前に確保済み")
                            .2;
                        let scratch_view =
                            scratch.create_view(&wgpu::TextureViewDescriptor::default());
                        self.converter
                            .convert(&mut encoder, &bind_group, &scratch_view);
                        encoder.copy_texture_to_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: scratch,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            wgpu::TexelCopyTextureInfo {
                                texture: &rgba,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            wgpu::Extent3d {
                                width: self.width,
                                height: self.height,
                                depth_or_array_layers: 1,
                            },
                        );
                    }

                    queue.submit(Some(encoder.finish()));
                    eprintln!(
                        "{}",
                        t!(
                            "[gpuvideo][convert] display_index=%{arg0} frame_index=%{arg1} elapsed_ms=%{arg2} thread=%{arg3}",
                            arg0 = display_index,
                            arg1 = frame_index,
                            arg2 = convert_started.elapsed().as_millis(),
                            arg3 = format!("{:?}", thread::current().id())
                        )
                    );

                    if display_index == frame_index {
                        return Ok(rgba);
                    }
                }
            }

            let msg = format!(
                "対象フレーム未生成（デコード中。prefetchのlookahead={}不足の可能性）",
                DECODE_LOOKAHEAD
            );
            eprintln!(
                "{}",
                t!(
                    "[gpuvideo] frame_gpu failed frame=%{arg0} reason=%{arg1} thread=%{arg2} next_output_index=%{arg3} expected_next=%{arg4} reset_count=%{arg5}",
                    arg0 = frame_index,
                    arg1 = &msg,
                    arg2 = format!("{:?}", thread::current().id()),
                    arg3 = self.next_output_index,
                    arg4 = format!("{:?}", self.expected_next),
                    arg5 = self.reset_count
                )
            );
            Err(msg)
        }
    }

    static SHARED_DEVICE: std::sync::OnceLock<Arc<GpuVideoDevice>> = std::sync::OnceLock::new();

    pub fn set_shared_device(device: Arc<GpuVideoDevice>) {
        let _ = SHARED_DEVICE.set(device);
    }

    fn shared_device() -> Result<&'static Arc<GpuVideoDevice>, String> {
        SHARED_DEVICE.get().ok_or_else(|| {
            "gpu_video::Device未初期化（main.rs::set_shared_device未実行）".to_owned()
        })
    }

    use neoutl_media_api::{MediaKind, MediaMeta, MediaVTable};

    static EXTENSIONS: &[&str] = &["mp4", "mov", "mkv"];

    static META: MediaMeta = MediaMeta {
        id: "neoutl.media.gpuvideo",
        name: "GPU Video Decoder (H.264 zero-copy)",
        kind: MediaKind::Video,
        extensions_ptr: EXTENSIONS.as_ptr(),
        extensions_len: EXTENSIONS.len(),
    };

    pub fn meta() -> &'static MediaMeta {
        &META
    }

    fn open_video(path: &Path) -> Result<Box<dyn VideoSource>, String> {
        let device = shared_device()?;
        GpuVideoDecoder::open(path, device)
            .map(|d| Box::new(d) as Box<dyn VideoSource>)
            .map_err(|e| {
                let msg = format!("open_video failed path={} err={e}", path.display());
                eprintln!(
                    "{}",
                    t!("[gpuvideo] open_video failed %{arg0}", arg0 = &msg)
                );
                msg
            })
    }

    pub fn native_vtable() -> MediaVTable {
        MediaVTable {
            meta,
            open_video: Some(open_video),
            open_image: None,
            decode_audio: None,
        }
    }
}

#[cfg(target_os = "linux")]
pub use imp::*;

#[cfg(not(target_os = "linux"))]
pub mod macos_stub {
    use neoutl_media_api::{MediaKind, MediaMeta, MediaVTable};

    static EXTENSIONS: &[&str] = &[];

    static META: MediaMeta = MediaMeta {
        id: "neoutl.media.gpuvideo",
        name: "GPU Video Decoder (disabled: macOS未対応)",
        kind: MediaKind::Video,
        extensions_ptr: EXTENSIONS.as_ptr(),
        extensions_len: EXTENSIONS.len(),
    };

    pub fn meta() -> &'static MediaMeta {
        &META
    }

    pub fn native_vtable() -> MediaVTable {
        MediaVTable {
            meta,
            open_video: None,
            open_image: None,
            decode_audio: None,
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub use macos_stub::native_vtable;
rust_i18n::i18n!("../../i18n");
#[macro_use]
extern crate rust_i18n;
