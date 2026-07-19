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

    /// prefetchが蓄積する未デコードパケット。demuxはSend不可能な内部状態を持ちうるため
    /// バイト列へ複製し所有権を保つ。
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
        pending: VecDeque<EncodedPacket>,
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
        /// deviceはホスト（Slint/gpu-video Manual注入）が生成した共有インスタンスを渡す。
        /// 本関数内でVulkanInstance/Adapter/Deviceを新規生成しない
        /// （単一デバイス構成をホスト全体で維持するため）。
        pub fn open(path: &Path, device: &Arc<GpuVideoDevice>) -> Result<Self, String> {
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
                let span = (pts_list[pts_list.len() - 1] - pts_list[0]) as f64
                    * tb.numer.get() as f64
                    / tb.denom.get() as f64;
                (pts_list.len() as f64 - 1.0) / span.max(1e-6)
            } else {
                30.0
            };

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

        /// バックグラウンドスレッド専用。パケット読出しのみ実行しGPU操作を行わない。
        fn prefetch(&mut self, frame_index: i64) -> Result<(), String> {
            if self.cache.map.contains_key(&frame_index) {
                return Ok(());
            }
            loop {
                let already_queued = self.pending.iter().any(|p| p.display_index == frame_index);
                if already_queued {
                    return Ok(());
                }
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
                self.pending.push_back(EncodedPacket {
                    display_index: idx,
                    pts: packet.pts.get(),
                    data: packet.data.to_vec(),
                });
                if idx >= frame_index {
                    return Ok(());
                }
            }
        }

        /// UIスレッド専用。蓄積済みパケットをデコード・変換し確定テクスチャを返す。
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
                let frames = self.decoder.decode(chunk).map_err(|e| e.to_string())?;
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
                    let bind_group = self
                        .converter
                        .create_input_bind_group(&frame)
                        .map_err(|e| e.to_string())?;
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
            self.cache
                .get(frame_index)
                .ok_or("対象フレーム未生成（prefetch未完了）".to_owned())
        }
    }

    // --- 共有GPUデバイス注入 ---
    // ホスト(main.rs)がSlint起動前にgpu_video::Deviceを一度だけ生成し、ここへ設定する。
    // gpuvideo-decoder::open_videoは常にこの共有インスタンスを参照し、内部で
    // VulkanInstance/Adapter/Deviceを新規生成しない（単一デバイス構成の維持）。
    /// create_wgpu_textures_decoder_h264がself: &Arc<Self>を要求するため、
    /// 保持形態はArc<GpuVideoDevice>で固定する（生のVulkanDeviceでは呼出不可）。
    static SHARED_DEVICE: std::sync::OnceLock<Arc<GpuVideoDevice>> = std::sync::OnceLock::new();

    fn set_shared_device(device: Arc<GpuVideoDevice>) {
        let _ = SHARED_DEVICE.set(device);
    }

    /// main.rsがMediaVTable経由でなく直接libloadingでこのシンボルを引く帯域外経路。
    /// MediaVTable(neoutl-media-api)自体はgpu_video型へ依存させないため、Phase0契約
    /// （MediaVTableに変更なし）を保ったまま単一デバイス注入を成立させる。
    /// GpuVideoDeviceはCloneを実装しないため、所有権をポインタ経由で移転しArc化する
    /// （呼出側はptr::read後、当該メモリのDropを実行しないこと。Box::into_rawで
    /// 生成したポインタを渡す運用とする）。
    /// # Safety
    /// deviceは有効なGpuVideoDeviceを指す非nullポインタであり、本関数呼出後は
    /// 呼出側で当該メモリのdropまたは再利用を行わないこと。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn neoutl_gpuvideo_inject_device(device: *mut GpuVideoDevice) {
        if !device.is_null() {
            let owned = unsafe { std::ptr::read(device) };
            set_shared_device(Arc::new(owned));
        }
    }

    pub const INJECT_DEVICE_SYMBOL: &[u8] = b"neoutl_gpuvideo_inject_device\0";

    fn shared_device() -> Result<&'static Arc<GpuVideoDevice>, String> {
        SHARED_DEVICE.get().ok_or_else(|| {
            "gpu_video::Device未初期化（main.rs::set_shared_device未実行）".to_owned()
        })
    }

    // --- プラグインエントリ ---
    // objects/effects/gstreamer-decoderと同一規約: entry関数のみがdylib境界（extern "C"）を越える。
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
        GpuVideoDecoder::open(path, device).map(|d| Box::new(d) as Box<dyn VideoSource>)
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
