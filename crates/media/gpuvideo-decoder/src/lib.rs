//! H.264ゼロコピー動画デコーダプラグイン。VulkanInstance(gpu-video crate)へ依存するため
//! Vulkan非対応のmacOSでは無効化する。macOSではGStreamer/ffmpeg経路(CPUアップロード)へ
//! 自動的にフォールバックする（media/loader.rsのid昇順拡張子解決による）。

#[cfg(not(target_os = "macos"))]
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

    /// decoder内部バッファリングにより「packet Nをfeedしても即座にframe Nが
    /// 出力されるとは限らない」ため、対象frame_indexより先読みしてpendingへ
    /// 積んでおく必要のあるフレーム数。値はgpu-video側の内部バッファ段数に
    /// 対する保守的な余裕。
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

    /// gpu-video crateのGPU decodeパス(wgpu::Device::as_hal内部リソースガード)は、
    /// 同一wgpu::Deviceに対する複数スレッドからの同時呼び出しに対して排他制御されていない。
    /// 共有デバイス構成下で複数GpuVideoDecoderインスタンスが並行してdecode()を実行すると、
    /// as_hal()がリソース競合によりNoneを返しunwrapでpanicする(gpu-video側の既知の制約)。
    /// decode()呼び出しのみを直列化する（NV12→RGBA変換・テクスチャ確保・queue.submitは
    /// 通常のwgpu操作でありwgpu自体が内部同期するため、この範囲には含めない）。
    ///
    /// このMutexは全GpuVideoDecoderインスタンス間で共有される単一の静的ロックであり、
    /// いずれか1インスタンスのdecode()呼び出しがgpu-video crate内部で無期限停止すると、
    /// 保持スレッドはロックを永久に解放しない。worker.rs側は各frame_gpu()呼び出しを
    /// 新規スレッドへ委譲するため、この状態で後続の全インスタンスがロック取得待ちの
    /// スレッドを新規に生成し続け、いずれも解放されずに滞留する
    /// （デコーダースレッド無限生成の直接要因）。lock()による無期限ブロックを禁止し、
    /// 上限時間内のtry_lockポーリングへ置き換えることで、スレッドが最終的に必ず
    /// 終了する（成功またはタイムアウトエラー）ことを保証する。
    static GPU_DECODE_LOCK: Mutex<()> = Mutex::new(());
    const GPU_DECODE_LOCK_WAIT: Duration = Duration::from_millis(1500);
    const GPU_DECODE_LOCK_POLL: Duration = Duration::from_millis(5);

    /// GPU_DECODE_LOCKを上限時間内で取得する。取得不能時はErrを返し、
    /// 呼び出し元スレッドを解放させる（無期限park禁止）。
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

    /// RGBA変換先テクスチャの固定プール。
    ///
    /// 旧実装は出力フレームごとにdevice.create_texture()を新規発行しており、
    /// 頻繁なVkImage確保・解放がGPUアロケータの断片化・スループット低下要因となっていた。
    /// open()時にDEFAULT_DECODE_CACHE_BYTES / costで算出した固定枚数を一括確保し、
    /// 以降はスロットの再割当のみで運用する（確保コスト自体は当初のバイト予算LRUと同一挙動）。
    ///
    /// 呼び出し元スレッドを混同すると自明でない競合を生むため、
    /// get/acquire_for_writeの双方でthread::current().id()を記録し、実際に単一スレッドから
    /// しか呼ばれていないか検証可能にする。
    struct TextureCache {
        pool: Vec<wgpu::Texture>,
        free: VecDeque<usize>,
        map: HashMap<i64, usize>,
        order: VecDeque<i64>,
        capacity: usize,
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
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                })
                .collect();

            eprintln!(
                "[gpuvideo][cache] pool allocated capacity={} cost_per_slot={} limit={}",
                capacity, cost, DEFAULT_DECODE_CACHE_BYTES
            );

            Self {
                pool,
                free: (0..capacity).collect(),
                map: HashMap::new(),
                order: VecDeque::new(),
                capacity,
            }
        }

        fn get(&mut self, index: i64) -> Option<wgpu::Texture> {
            let slot = *self.map.get(&index)?;
            self.order.retain(|&i| i != index);
            self.order.push_back(index);
            eprintln!(
                "[gpuvideo][cache] get hit index={} slot={} thread={:?} entries={}",
                index,
                slot,
                thread::current().id(),
                self.map.len()
            );
            Some(self.pool[slot].clone())
        }

        /// indexの書き込み先テクスチャを確保する。既存スロットがあればそれを返し、
        /// 無ければ空きスロット、それも無ければ最古indexのスロットを回収し再割当する。
        /// 呼び出し側はこのテクスチャへNV12→RGBA変換結果を書き込む。
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
                    "[gpuvideo][cache] evict index={} slot={} thread={:?} entries={}",
                    oldest,
                    s,
                    thread::current().id(),
                    self.map.len()
                );
                s
            };

            self.map.insert(index, slot);
            self.order.push_back(index);
            eprintln!(
                "[gpuvideo][cache] put index={} slot={} thread={:?} entries={}/{}",
                index,
                slot,
                thread::current().id(),
                self.map.len(),
                self.capacity
            );
            self.pool[slot].clone()
        }
    }

    /// open()時に demux を走査してメモリ化するH.264パケット。
    ///
    /// demux は Send 不可能な内部状態を持ちうるため、バイト列へ複製して所有権を保つ。
    struct EncodedPacket {
        display_index: i64,
        pts: i64,
        data: Vec<u8>,
        is_sync: bool,
    }

    /// AVCC sample内のNALユニットを走査し、IDR(NALタイプ5)の有無でsync sample判定する。
    /// symphonia Packetはコンテナ非依存でsync flagを保証しないため、ビットストリーム側の
    /// NALタイプで自前判定する（コンテナのstss解析より単純かつ本デコーダの用途で十分）。
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

    /// packets中でidx以下の最も近いsync sampleのdisplay_indexを返す。無ければ0。
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

    /// open()時に全パケットをメモリ化する。
    /// 目的: prefetch/frame_gpu から demux を触らず、逐次demux崩壊を回避する。
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

        loop {
            let packet = match demux.next_packet().map_err(|e| e.to_string())? {
                Some(p) => p,
                None => break,
            };

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
        track_id: u32,
        decoder: WgpuTexturesDecoder,
        converter: WgpuNv12ToRgbaConverter,
        width: u32,
        height: u32,
        fps: f64,
        total_frames: i64,

        /// 全パケットをメモリ化したもの（AVCC sample bytes）
        packets: Vec<EncodedPacket>,
        cache: TextureCache,

        /// prefetch が積む「まだ確定していない」パケット列（pendingはメモリ上のパケット参照）
        pending: VecDeque<EncodedPacket>,

        /// pendingが現在カバーしているGOP先頭のdisplay_index。
        /// pending.front()はframe_gpuの消費により前進するため、reset要否の判定に
        /// pending.front()そのものを使うと「同一GOP内で単に消費が進んだだけ」を
        /// 「GOP境界超過・シーク」と誤認する。この専用フィールドはprefetchが実際に
        /// pendingを（reset経由で）組み直した時にのみ更新し、消費による前進では
        /// 変化しない。
        planned_gop_start: Option<i64>,

        /// avcC-derived H.264 config for AnnexB conversion
        h264_cfg: H264Config,

        /// decoder再生成用。openで注入された共有デバイスをそのまま保持する。
        device: Arc<GpuVideoDevice>,

        /// frame_gpuが最後にデコーダへ供給したpacketの次に来るべきdisplay_index。
        /// pop_frontしたpacketのdisplay_indexがこれと一致しない場合、連続性が
        /// 途切れている（シーク発生）とみなしdecoderを再生成しSPS/PPSを再注入する。
        expected_next: Option<i64>,

        /// decoderが実際に出力したフレームへ割り当てる次のdisplay_index。
        ///
        /// feedしたpacket.display_indexをそのまま出力フレームのcacheキーに使うと、
        /// decoder内部バッファリング（1 packet feedが即1 frame出力とは限らない）により
        /// 出力とfeed順がずれてcacheキーが実際の表示順と食い違う。出力が確定した順に
        /// この専用カウンタを進めてcacheキーとすることで、feed側のインデックスと
        /// 出力側のインデックスを分離する。decoder再生成時は再生成の起点となる
        /// sync sampleのdisplay_indexへ合わせてリセットする。
        next_output_index: i64,

        /// create_wgpu_textures_decoder_h264を呼んだ累計回数。
        /// discontinuous判定の誤爆でdecoder再生成・GPUリソース確保が
        /// 想定外に高頻度発生していないかログで確認するためのカウンタ。
        reset_count: u64,
    }

    impl GpuVideoDecoder {
        /// deviceはホスト（Slint/gpu-video Manual注入）が生成した共有インスタンスを渡す。
        /// 本関数内でVulkanInstance/Adapter/Deviceを新規生成しない
        /// （単一デバイス構成をホスト全体で維持するため）。
        pub fn open(path: &Path, device: &Arc<GpuVideoDevice>) -> Result<Self, String> {
            eprintln!("[gpuvideo] open_video begin path={}", path.display());

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

            let width = video_cp.width.ok_or("width未定義")?.into();
            let height = video_cp.height.ok_or("height未定義")?.into();

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
                "[gpuvideo][open] create_wgpu_textures_decoder_h264 begin path={} thread={:?}",
                path.display(),
                thread::current().id()
            );
            let decoder = device
                .create_wgpu_textures_decoder_h264(DecoderParameters::default())
                .map_err(|e| {
                    let msg = format!("create_wgpu_textures_decoder_h264 failed: {e}");
                    eprintln!(
                        "[gpuvideo] open_video failed path={} reason={}",
                        path.display(),
                        msg
                    );
                    msg
                })?;
            eprintln!(
                "[gpuvideo][open] create_wgpu_textures_decoder_h264 end path={} elapsed_ms={} thread={:?}",
                path.display(),
                decoder_init_started.elapsed().as_millis(),
                thread::current().id()
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
                    "[gpuvideo] open_video failed path={} reason={}",
                    path.display(),
                    msg
                );
                msg
            })?;
            eprintln!(
                "[gpuvideo][open] converter init end path={} elapsed_ms={} thread={:?}",
                path.display(),
                converter_init_started.elapsed().as_millis(),
                thread::current().id()
            );

            eprintln!(
                "[gpuvideo] open_video ok path={} codec=h264 {}x{} fps={} frames={}",
                path.display(),
                width,
                height,
                fps,
                total_frames
            );

            Ok(Self {
                track_id,
                decoder,
                converter,
                width,
                height,
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

        /// バックグラウンドスレッド専用。demux は触らず、メモリ上の pending/pkts のみ操作する。
        ///
        /// pending は常に「直前sync sampleから連続するAVCC sample列」を保つ。
        /// frame_indexが属するGOPの起点（needed_sync）とpending先頭が食い違う場合、
        /// pendingを全消去して needed_sync からframe_indexまでを積み直す
        /// （順再生の継続・GOP跨ぎ・逆シークの3ケースを同一ロジックで処理する）。
        fn prefetch(&mut self, frame_index: i64) -> Result<(), String> {
            eprintln!(
                "[gpuvideo][prefetch] enter frame_index={} thread={:?} pending_len={} pending_front={:?} pending_back={:?} cache_entries={}",
                frame_index,
                thread::current().id(),
                self.pending.len(),
                self.pending.front().map(|p| p.display_index),
                self.pending.back().map(|p| p.display_index),
                self.cache.map.len()
            );
            if self.cache.map.contains_key(&frame_index) {
                return Ok(());
            }

            if frame_index < 0 || (frame_index as usize) >= self.packets.len() {
                let msg = format!("prefetch EOF (frame={frame_index})");
                eprintln!("[gpuvideo] prefetch failed {}", msg);
                return Err(msg);
            }

            if self.pending.iter().any(|p| p.display_index == frame_index) {
                return Ok(());
            }

            let needed_sync = find_prev_sync(&self.packets, frame_index);
            let queue_end = (frame_index + DECODE_LOOKAHEAD).min(self.packets.len() as i64 - 1);
            let reset = self.planned_gop_start != Some(needed_sync);

            eprintln!(
                "[gpuvideo][prefetch] plan frame_index={} needed_sync={} queue_end={} reset={} thread={:?}",
                frame_index,
                needed_sync,
                queue_end,
                reset,
                thread::current().id()
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

        /// workerスレッド専用。蓄積済みパケットをデコード・変換し確定テクスチャを返す。
        fn frame_gpu(
            &mut self,
            frame_index: i64,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
        ) -> Result<wgpu::Texture, String> {
            eprintln!(
                "[gpuvideo][frame_gpu] enter frame_index={} thread={:?} pending_len={} expected_next={:?} next_output_index={} reset_count={}",
                frame_index,
                thread::current().id(),
                self.pending.len(),
                self.expected_next,
                self.next_output_index,
                self.reset_count
            );
            if let Some(cached) = self.cache.get(frame_index) {
                eprintln!(
                    "[gpuvideo][frame_gpu] cache_hit frame_index={} thread={:?}",
                    frame_index,
                    thread::current().id()
                );
                return Ok(cached);
            }

            while let Some(packet) = self.pending.pop_front() {
                let discontinuous = self.expected_next != Some(packet.display_index);
                if discontinuous {
                    if !packet.is_sync {
                        eprintln!(
                            "[gpuvideo] frame_gpu warning: non-sync packet at decode-run start display_index={} frame_index={}",
                            packet.display_index, frame_index
                        );
                    }
                    self.reset_count += 1;
                    let reset_started = Instant::now();
                    eprintln!(
                        "[gpuvideo][reset] begin #{} display_index={} frame_index={} thread={:?}",
                        self.reset_count,
                        packet.display_index,
                        frame_index,
                        thread::current().id()
                    );
                    self.decoder = self
                        .device
                        .create_wgpu_textures_decoder_h264(DecoderParameters::default())
                        .map_err(|e| {
                            let msg = format!("decoder再生成失敗 (frame={frame_index}) err={e}");
                            eprintln!("[gpuvideo] frame_gpu failed {}", msg);
                            msg
                        })?;
                    eprintln!(
                        "[gpuvideo][reset] end #{} elapsed_ms={} thread={:?}",
                        self.reset_count,
                        reset_started.elapsed().as_millis(),
                        thread::current().id()
                    );
                    self.next_output_index = packet.display_index;
                }
                let inject_ps = discontinuous;
                let annexb = avcc_sample_to_annexb(&self.h264_cfg, &packet.data, inject_ps)?;
                self.expected_next = Some(packet.display_index + 1);

                eprintln!(
                    "[gpuvideo] feed display_index={} frame_index={} is_sync={} pts={} avcc_len={} annexb_len={}",
                    packet.display_index,
                    frame_index,
                    packet.is_sync,
                    packet.pts,
                    packet.data.len(),
                    annexb.len()
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
                    "[gpuvideo][decode] call_begin display_index={} frame_index={} thread={:?}",
                    packet.display_index,
                    frame_index,
                    thread::current().id()
                );
                let frames = {
                    let _guard = acquire_gpu_decode_lock(frame_index).map_err(|e| {
                        eprintln!("[gpuvideo] frame_gpu failed {}", e);
                        e
                    })?;
                    self.decoder.decode(chunk)
                }
                .map_err(|e| {
                    let msg = format!("decoder.decode failed (frame={frame_index}) err={e}");
                    eprintln!("[gpuvideo] frame_gpu failed {}", msg);
                    msg
                })?;
                eprintln!(
                    "[gpuvideo][decode] call_end display_index={} frame_index={} elapsed_ms={} output_count={} thread={:?}",
                    packet.display_index,
                    frame_index,
                    decode_started.elapsed().as_millis(),
                    frames.len(),
                    thread::current().id()
                );

                for frame in frames {
                    let display_index = self.next_output_index;
                    self.next_output_index += 1;

                    eprintln!(
                        "[gpuvideo] output display_index={} frame_index={}",
                        display_index, frame_index
                    );

                    let rgba = self.cache.acquire_for_write(display_index);
                    let rgba_view = rgba.create_view(&wgpu::TextureViewDescriptor::default());

                    let bind_group =
                        self.converter
                            .create_input_bind_group(&frame)
                            .map_err(|e| {
                                let msg = format!(
                                    "create_input_bind_group failed (frame={frame_index}) err={e}"
                                );
                                eprintln!("[gpuvideo] frame_gpu failed {}", msg);
                                msg
                            })?;

                    let convert_started = Instant::now();
                    let mut encoder =
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                    self.converter
                        .convert(&mut encoder, &bind_group, &rgba_view);
                    queue.submit(Some(encoder.finish()));
                    eprintln!(
                        "[gpuvideo][convert] display_index={} frame_index={} elapsed_ms={} thread={:?}",
                        display_index,
                        frame_index,
                        convert_started.elapsed().as_millis(),
                        thread::current().id()
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
                "[gpuvideo] frame_gpu failed frame={} reason={} thread={:?} next_output_index={} expected_next={:?} reset_count={}",
                frame_index,
                msg,
                thread::current().id(),
                self.next_output_index,
                self.expected_next,
                self.reset_count
            );
            Err(msg)
        }
    }

    static SHARED_DEVICE: std::sync::OnceLock<Arc<GpuVideoDevice>> = std::sync::OnceLock::new();

    /// main.rsが起動時に一度だけ呼ぶ。本体プロセス内で直接呼ばれる素のRust関数呼び出しであり、
    /// dylib境界（libloading + extern "C" + 生ポインタ受け渡し）を経由しない。
    /// これによりVulkanDevice/wgpu::Deviceの内部レイアウトは常に単一コンパイル単位内で
    /// 一貫し、ABI不一致によるas_hal()のNone化を構造的に排除する。
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
                eprintln!("[gpuvideo] open_video failed {}", msg);
                msg
            })
    }

    /// src/media/loader.rsのネイティブプラグインレジストリへ直接登録するためのVTable生成。
    /// dylibロード（libloading::Library::get）を経由せず、本体バイナリと同一コンパイル単位の
    /// 関数ポインタをそのまま束ねるだけの操作。
    pub fn native_vtable() -> MediaVTable {
        MediaVTable {
            meta,
            open_video: Some(open_video),
            open_image: None,
            decode_audio: None,
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub use imp::*;

/// macOS向け無効化スタブ。gpu-video(Vulkan)非対応のため実デコード機能を持たず、
/// MediaVTable登録もextensions_len=0のため実質何もマッチしない（安全側フォールバック）。
#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
pub use macos_stub::native_vtable;
