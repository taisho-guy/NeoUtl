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
    use std::sync::Arc;
    use symphonia::core::codecs::video::well_known::CODEC_ID_H264;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    /// frame_gpu内キャッシュ。UIスレッド専有アクセスのため排他制御不要（Mutex除去）。
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
            while self.used_bytes > DEFAULT_DECODE_CACHE_BYTES {
                let Some(oldest) = self.order.pop_front() else {
                    break;
                };
                self.map.remove(&oldest);
                self.used_bytes -= cost;
            }
        }
    }

    /// open()時に demux を走査してメモリ化するH.264パケット。
    ///
    /// demux は Send 不可能な内部状態を持ちうるため、バイト列へ複製して所有権を保つ。
    struct EncodedPacket {
        display_index: i64,
        pts: i64,
        data: Vec<u8>,
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
    ) -> Result<Vec<EncodedPacket>, String> {
        // demuxを先頭に戻す
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

            // ptsは codec/コンテナによって missing などあり得るが、この設計では
            // pts の変換が失敗した場合は表示順indexで代用する（decode側の pts 必須性に合わせて調整）。
            let pts_i64 = packet.pts.get();
            let data = packet.data.to_vec();

            packets.push(EncodedPacket {
                display_index,
                pts: pts_i64,
                data,
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

        /// 全パケットをメモリ化したもの
        packets: Vec<EncodedPacket>,
        cache: TextureCache,

        /// prefetch が積む「まだ確定していない」パケット列（pendingはメモリ上のパケット参照）
        pending: VecDeque<EncodedPacket>,
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

            // open時に全走査済みなので total_frames は packets.len()
            // fps は元コード同様、必要なら track.time_base から推定。
            let tb = track.time_base.ok_or("time_base未定義")?;
            let tb_numer = tb.numer.get() as f64;
            let tb_denom = tb.denom.get() as f64;

            let packets = preload_packets(&mut demux, track_id)?;
            let total_frames = packets.len() as i64;

            let fps = if total_frames >= 2 {
                // pts差が極端に小さい/ゼロの場合は保守的に30fps。
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
                cache: TextureCache::new(),
                pending: VecDeque::new(),
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
        fn prefetch(&mut self, frame_index: i64) -> Result<(), String> {
            // 既にGPUキャッシュにあるなら不要
            if self.cache.map.contains_key(&frame_index) {
                return Ok(());
            }

            // pendingに対象display_indexが存在するなら不要
            let already_queued = self.pending.iter().any(|p| p.display_index == frame_index);
            if already_queued {
                return Ok(());
            }

            // frame_indexまでの必要分を pending に積む（demux不要）
            // display_index は open()時に 0..N-1 の連番で作っている前提。
            if frame_index < 0 || (frame_index as usize) >= self.packets.len() {
                let msg = format!("prefetch EOF (frame={frame_index})");
                eprintln!("[gpuvideo] prefetch failed {}", msg);
                return Err(msg);
            }

            // いま pending の末尾がどこまでかを見て、足りない分だけ積む
            // （pendingはdecodeで pop_front されるので、基本的に単調に進むが保険として末尾で管理）
            let pending_max = self.pending.back().map(|p| p.display_index).unwrap_or(-1);
            if pending_max >= frame_index {
                return Ok(());
            }

            let start = (pending_max + 1).max(0) as usize;
            let end = frame_index as usize;

            for idx in start..=end {
                // ここで data/pts/インデックスを複製せず Move するため clone は不要
                // ただし packets から移動すると壊れるので、Vecからは cloneして pendingへ複製する。
                // そのため EncodedPacket のサイズを考慮して必要に応じ最適化可能。
                let p = &self.packets[idx];
                self.pending.push_back(EncodedPacket {
                    display_index: p.display_index,
                    pts: p.pts,
                    data: p.data.clone(),
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
            if let Some(cached) = self.cache.get(frame_index) {
                return Ok(cached);
            }

            while let Some(packet) = self.pending.pop_front() {
                let chunk = EncodedInputChunk {
                    data: &packet.data,
                    pts: Some(
                        packet
                            .pts
                            .try_into()
                            .map_err(|_| "pts変換失敗".to_owned())?,
                    ),
                };

                let frames = self.decoder.decode(chunk).map_err(|e| {
                    let msg = format!("decoder.decode failed (frame={frame_index}) err={e}");
                    eprintln!("[gpuvideo] frame_gpu failed {}", msg);
                    msg
                })?;

                for frame in frames {
                    let rgba = device.create_texture(&wgpu::TextureDescriptor {
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

                    let mut encoder =
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                    self.converter
                        .convert(&mut encoder, &bind_group, &rgba_view);
                    queue.submit(Some(encoder.finish()));

                    let cost = (self.width as i64) * (self.height as i64) * 4;
                    self.cache.put(packet.display_index, rgba.clone(), cost);

                    if packet.display_index == frame_index {
                        return Ok(rgba);
                    }
                }
            }

            let msg = "対象フレーム未生成（prefetch未完了）".to_owned();
            eprintln!(
                "[gpuvideo] frame_gpu failed frame={} reason={}",
                frame_index, msg
            );
            Err(msg)
        }
    }

    // --- 共有GPUデバイス注入 ---
    // ホスト(main.rs)がSlint起動前にgpu_video::Deviceを一度だけ生成し、ここへ設定する。
    static SHARED_DEVICE: std::sync::OnceLock<Arc<GpuVideoDevice>> = std::sync::OnceLock::new();

    fn set_shared_device(device: Arc<GpuVideoDevice>) {
        let _ = SHARED_DEVICE.set(device);
    }

    fn shared_device() -> Result<&'static Arc<GpuVideoDevice>, String> {
        SHARED_DEVICE.get().ok_or_else(|| {
            "gpu_video::Device未初期化（main.rs::set_shared_device未実行）".to_owned()
        })
    }

    /// main.rsがMediaVTable経由でなく直接libloadingでこのシンボルを引く帯域外経路。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn neoutl_gpuvideo_inject_device(device: *mut GpuVideoDevice) {
        if !device.is_null() {
            let owned = unsafe { std::ptr::read(device) };
            set_shared_device(Arc::new(owned));
        }
    }

    pub const INJECT_DEVICE_SYMBOL: &[u8] = b"neoutl_gpuvideo_inject_device\0";

    // --- プラグインエントリ ---
    use neoutl_media_api::{EntryFn, MediaKind, MediaMeta, MediaVTable};

    static EXTENSIONS: &[&str] = &["mp4", "mov", "mkv"];

    static META: MediaMeta = MediaMeta {
        id: "neoutl.media.gpuvideo",
        name: "GPU Video Decoder (H.264 zero-copy)",
        kind: MediaKind::Video,
        extensions_ptr: EXTENSIONS.as_ptr(),
        extensions_len: EXTENSIONS.len(),
    };

    static VTABLE: std::sync::OnceLock<MediaVTable> = std::sync::OnceLock::new();

    fn meta() -> &'static MediaMeta {
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

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn neoutl_media_entry() -> *const MediaVTable {
        VTABLE.get_or_init(|| MediaVTable {
            meta,
            open_video: Some(open_video),
            open_image: None,
            decode_audio: None,
        })
    }

    const _: EntryFn = neoutl_media_entry;
}

#[cfg(not(target_os = "macos"))]
pub use imp::*;

/// macOS向け無効化スタブ。gpu-video(Vulkan)非対応のため実デコード機能を持たず、
/// MediaVTable登録もextensions_len=0のため実質何もマッチしない（安全側フォールバック）。
#[cfg(target_os = "macos")]
mod macos_stub {
    use neoutl_media_api::{EntryFn, MediaKind, MediaMeta, MediaVTable};

    static EXTENSIONS: &[&str] = &[];

    static META: MediaMeta = MediaMeta {
        id: "neoutl.media.gpuvideo",
        name: "GPU Video Decoder (disabled: macOS未対応)",
        kind: MediaKind::Video,
        extensions_ptr: EXTENSIONS.as_ptr(),
        extensions_len: EXTENSIONS.len(),
    };

    static VTABLE: std::sync::OnceLock<MediaVTable> = std::sync::OnceLock::new();

    fn meta() -> &'static MediaMeta {
        &META
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn neoutl_media_entry() -> *const MediaVTable {
        VTABLE.get_or_init(|| MediaVTable {
            meta,
            open_video: None,
            open_image: None,
            decode_audio: None,
        })
    }

    const _: EntryFn = neoutl_media_entry;
}
