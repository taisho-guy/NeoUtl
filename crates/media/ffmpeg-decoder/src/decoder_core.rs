//! `DecoderCore`は単一のワーカースレッドからのみ所有・呼び出しされる前提のデコード実体。
//! `prefetch_at`と`frame_gpu_at`は同一スレッド上で逐次実行されるため、
//! `input`/`decoder`/`last_display_index`/`exhausted`に対する競合するシークは発生しない。
//! GPU→CPU 1回転送 + CPU側RGBA8合成方式は変更しない
//! （Vulkan統一ゼロコピー撤回の経緯は本ファイル導入前と同一。詳細はworker.rs冒頭コメント参照）。

use ffmpeg_next as ffmpeg;
use ffmpeg_next::util::frame::Video as VideoFrame;
use ffmpeg_sys_next as sys;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::ptr;

pub struct IndexEntry {
    pub pts: i64,
    pub key: bool,
}

/// テクスチャキャッシュ。GPU→CPU転送+RGBA8アップロード済みのwgpu::Textureを
/// LRU保持する（VRAM上限は呼び出し側のcapacity_countで制御）。
struct TextureCache {
    capacity_count: usize,
    order: VecDeque<i64>,
    map: HashMap<i64, wgpu::Texture>,
}

impl TextureCache {
    fn new(capacity_count: usize) -> Self {
        Self {
            capacity_count,
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }
    fn get(&mut self, index: i64) -> Option<wgpu::Texture> {
        let tex = self.map.get(&index)?.clone();
        self.order.retain(|&i| i != index);
        self.order.push_back(index);
        Some(tex)
    }
    fn put(&mut self, index: i64, tex: wgpu::Texture) {
        if self.map.contains_key(&index) {
            return;
        }
        self.map.insert(index, tex);
        self.order.push_back(index);
        while self.map.len() > self.capacity_count {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.map.remove(&oldest);
        }
    }
    fn contains(&self, index: i64) -> bool {
        self.map.contains_key(&index)
    }
}

/// hw_device_ctx確保後のAVBufferRef所有権ラッパ。Drop時にav_buffer_unref。
struct HwDeviceCtx(*mut sys::AVBufferRef);
unsafe impl Send for HwDeviceCtx {}
impl Drop for HwDeviceCtx {
    fn drop(&mut self) {
        unsafe { sys::av_buffer_unref(&mut self.0) };
    }
}

/// OS別のネイティブハードウェアデコード種別。Vulkan統一パスへの入力側デバイス種別。
#[cfg(target_os = "windows")]
const NATIVE_HW_TYPE: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA;
#[cfg(target_os = "linux")]
const NATIVE_HW_TYPE: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI;
#[cfg(target_os = "macos")]
const NATIVE_HW_TYPE: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX;

/// 対象デコーダが NATIVE_HW_TYPE + デコーダ側 pix_fmt の組み合わせをサポートする場合のみ
/// hw_device_ctxを確保する。非対応時は None を返しソフトウェアデコードへフォールバックする。
unsafe fn try_create_hw_device_ctx(
    codec: *const sys::AVCodec,
) -> Option<(HwDeviceCtx, sys::AVPixelFormat)> {
    let mut hw_pix_fmt: Option<sys::AVPixelFormat> = None;
    let mut i = 0;
    loop {
        let cfg = unsafe { sys::avcodec_get_hw_config(codec, i) };
        if cfg.is_null() {
            break;
        }
        let cfg_ref = unsafe { &*cfg };
        if cfg_ref.device_type == NATIVE_HW_TYPE
            && (cfg_ref.methods & sys::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32) != 0
        {
            hw_pix_fmt = Some(cfg_ref.pix_fmt);
            break;
        }
        i += 1;
    }
    let pix_fmt = hw_pix_fmt?;

    let mut device_ctx: *mut sys::AVBufferRef = ptr::null_mut();
    let ret = unsafe {
        sys::av_hwdevice_ctx_create(
            &mut device_ctx,
            NATIVE_HW_TYPE,
            ptr::null(),
            ptr::null_mut(),
            0,
        )
    };
    if ret < 0 || device_ctx.is_null() {
        return None;
    }
    Some((HwDeviceCtx(device_ctx), pix_fmt))
}

/// ハードウェアフレームをシステムメモリ(NV12想定)へ転送する。
/// GPU→CPU 1回のコピーのみ。色空間変換(swscale)は行わない。
unsafe fn transfer_hw_frame_to_sw(
    hw_frame: *mut sys::AVFrame,
) -> Result<*mut sys::AVFrame, String> {
    let sw_frame = unsafe { sys::av_frame_alloc() };
    if sw_frame.is_null() {
        return Err("av_frame_alloc failed".into());
    }
    let ret = unsafe { sys::av_hwframe_transfer_data(sw_frame, hw_frame, 0) };
    if ret < 0 {
        unsafe { sys::av_frame_free(&mut { sw_frame }) };
        return Err(format!("av_hwframe_transfer_data failed ret={ret}"));
    }
    Ok(sw_frame)
}

/// NV12(Y平面 + インターリーブUV平面)をRGBA8へCPU変換する(BT.709 limited range想定)。
fn nv12_to_rgba8(sw_frame: *const sys::AVFrame, width: u32, height: u32) -> Vec<u8> {
    let frame = unsafe { &*sw_frame };
    let y_stride = frame.linesize[0] as usize;
    let uv_stride = frame.linesize[1] as usize;
    let y_plane = unsafe { std::slice::from_raw_parts(frame.data[0], y_stride * height as usize) };
    let uv_plane =
        unsafe { std::slice::from_raw_parts(frame.data[1], uv_stride * (height as usize / 2 + 1)) };

    let mut out = vec![0u8; width as usize * height as usize * 4];
    for row in 0..height as usize {
        let y_row = &y_plane[row * y_stride..row * y_stride + width as usize];
        let uv_row = &uv_plane[(row / 2) * uv_stride..(row / 2) * uv_stride + width as usize];
        for col in 0..width as usize {
            let y = y_row[col] as f32;
            let u = uv_row[(col / 2) * 2] as f32 - 128.0;
            let v = uv_row[(col / 2) * 2 + 1] as f32 - 128.0;

            let y_n = (y - 16.0).max(0.0) * 1.164_38;
            let r = (y_n + 1.792_74 * v).clamp(0.0, 255.0) as u8;
            let g = (y_n - 0.213_25 * u - 0.532_91 * v).clamp(0.0, 255.0) as u8;
            let b = (y_n + 2.112_40 * u).clamp(0.0, 255.0) as u8;

            let idx = (row * width as usize + col) * 4;
            out[idx] = r;
            out[idx + 1] = g;
            out[idx + 2] = b;
            out[idx + 3] = 255;
        }
    }
    out
}

/// P010LE(Y平面16bit + インターリーブUV平面16bit、有効値は各ワード上位10bit)を
/// RGBA8へCPU変換する(BT.709 limited range想定)。HEVC Main10のVAAPI転送先フォーマット。
fn p010le_to_rgba8(sw_frame: *const sys::AVFrame, width: u32, height: u32) -> Vec<u8> {
    let frame = unsafe { &*sw_frame };
    let y_stride_words = frame.linesize[0] as usize / 2;
    let uv_stride_words = frame.linesize[1] as usize / 2;
    let y_plane = unsafe {
        std::slice::from_raw_parts(
            frame.data[0] as *const u16,
            y_stride_words * height as usize,
        )
    };
    let uv_plane = unsafe {
        std::slice::from_raw_parts(
            frame.data[1] as *const u16,
            uv_stride_words * (height as usize / 2 + 1),
        )
    };

    let mut out = vec![0u8; width as usize * height as usize * 4];
    for row in 0..height as usize {
        let y_row = &y_plane[row * y_stride_words..row * y_stride_words + width as usize];
        let uv_row =
            &uv_plane[(row / 2) * uv_stride_words..(row / 2) * uv_stride_words + width as usize];
        for col in 0..width as usize {
            let y = (y_row[col] >> 8) as f32;
            let u = (uv_row[(col / 2) * 2] >> 8) as f32 - 128.0;
            let v = (uv_row[(col / 2) * 2 + 1] >> 8) as f32 - 128.0;

            let y_n = (y - 16.0).max(0.0) * 1.164_38;
            let r = (y_n + 1.792_74 * v).clamp(0.0, 255.0) as u8;
            let g = (y_n - 0.213_25 * u - 0.532_91 * v).clamp(0.0, 255.0) as u8;
            let b = (y_n + 2.112_40 * u).clamp(0.0, 255.0) as u8;

            let idx = (row * width as usize + col) * 4;
            out[idx] = r;
            out[idx + 1] = g;
            out[idx + 2] = b;
            out[idx + 3] = 255;
        }
    }
    out
}

/// hw_pix_fmtデコード済みフレームをRGBA8のwgpu::Textureへアップロードする。
unsafe fn upload_hw_frame_as_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    hw_frame: *mut sys::AVFrame,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, String> {
    let sw_frame = unsafe { transfer_hw_frame_to_sw(hw_frame) }?;
    let sw_format = unsafe { (*sw_frame).format };
    let rgba = if sw_format == sys::AVPixelFormat::AV_PIX_FMT_P010LE as i32 {
        p010le_to_rgba8(sw_frame, width, height)
    } else {
        nv12_to_rgba8(sw_frame, width, height)
    };
    unsafe { sys::av_frame_free(&mut { sw_frame }) };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ffmpeg-decoder frame"),
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
        &rgba,
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

    Ok(texture)
}

/// MLT `producer_avformat.c` の手法を移植: `av_seek_frame`をストリーム番号を明示して
/// 直接呼び出す。ffmpeg-nextの安全ラッパー`Input::seek`はstream=-1固定でAV_TIME_BASE
/// (マイクロ秒)単位を要求するが、本デコーダの`IndexEntry::pts`は`decoded.pts()`由来の
/// 「ストリーム固有タイムベース」値であり、そのまま渡すと単位が一致せず常にファイル
/// 先頭付近への誤ったシークになる。`AVSEEK_FLAG_BACKWARD`も必須。
unsafe fn seek_stream_backward(
    input: &mut ffmpeg::format::context::Input,
    stream_index: usize,
    pts: i64,
) -> Result<(), String> {
    let ctx = input.as_mut_ptr();
    let ret = unsafe {
        sys::av_seek_frame(
            ctx,
            stream_index as i32,
            pts,
            sys::AVSEEK_FLAG_BACKWARD as i32,
        )
    };
    if ret < 0 {
        return Err(format!("av_seek_frame failed ret={ret} pts={pts}"));
    }
    Ok(())
}

/// FFmpeg公式サンプル`hw_decode.c`が必須とするコールバック。`AVCodecContext.opaque`に
/// 事前に格納したhw_pix_fmtと照合して返す。
unsafe extern "C" fn get_hw_format(
    ctx: *mut sys::AVCodecContext,
    pix_fmts: *const sys::AVPixelFormat,
) -> sys::AVPixelFormat {
    let target = unsafe { *((*ctx).opaque as *const sys::AVPixelFormat) };
    let mut p = pix_fmts;
    unsafe {
        while *p != sys::AVPixelFormat::AV_PIX_FMT_NONE {
            if *p == target {
                return *p;
            }
            p = p.add(1);
        }
    }
    sys::AVPixelFormat::AV_PIX_FMT_NONE
}

pub fn build_index(
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

const TEXTURE_CACHE_CAPACITY: usize =
    (neoutl_media_api::DEFAULT_DECODE_CACHE_BYTES / (1920 * 1080 * 4)) as usize;
/// MLT `producer_avformat.c` L1857-1859 相当。前方への要求位置が現在位置から
/// この閾値以上離れている場合、逐次デコードでなくシークで追いつく。
const SEEK_THRESHOLD: i64 = 64;

/// 単一ワーカースレッドが専有するデコード状態。`input`/`decoder`への全アクセスは
/// `prefetch_at`/`frame_gpu_at`経由のみとし、両者は`worker.rs`の直列ループからしか
/// 呼び出されない（＝呼び出し元が単一スレッドである不変条件をモジュール外へ公開しない
/// ことで保証する）。
pub struct DecoderCore {
    input: ffmpeg::format::context::Input,
    stream_index: usize,
    decoder: ffmpeg::decoder::Video,
    _hw_device_ctx: Option<HwDeviceCtx>,
    hw_pix_fmt: Option<sys::AVPixelFormat>,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    index: Vec<IndexEntry>,
    cache: TextureCache,
    last_display_index: i64,
    /// packets()が尽きてtarget未到達のまま関数を抜けた直後にtrueとなる。次回呼び出しで
    /// 強制的にシークさせるためのフラグ。
    exhausted: bool,
    /// get_hw_formatコールバックがAVCodecContext.opaque経由で参照するhw_pix_fmt。
    /// デコーダより長生きさせる必要があるためヒープ確保しDropで解放する。
    hw_pix_fmt_box: Option<*mut sys::AVPixelFormat>,
}

unsafe impl Send for DecoderCore {}

impl Drop for DecoderCore {
    fn drop(&mut self) {
        if let Some(p) = self.hw_pix_fmt_box.take() {
            unsafe {
                drop(Box::from_raw(p));
            }
        }
    }
}

/// FFmpeg内部ログ(hwaccel初期化失敗時の`vaInitialize failed`等)を可視化する。
/// 現状の観測(`hw_pix_fmt`はSome=hw_device_ctx確保に成功=VAAPI経由の
/// av_hwdevice_ctx_createまでは成功しているにもかかわらず、実デコード時に
/// hw_pix_fmtフレームが1枚も出力されない)は、hw_device_ctx確保後の
/// 「実プロファイル/解像度に対するhwaccelコンテキスト初期化」段階での失敗を
/// 強く示唆する。この段階のエラーはffmpeg-next側のResult型に現れず、
/// libavcodec内部でav_log経由にのみ出力される。
/// Cコールバック(`av_log_set_callback`)によるRust側捕捉はva_list引数の受け渡しに
/// `#![feature(c_variadic)]`(nightly限定)を要するため実装不能。
/// 代わりにログレベルのみ引き上げ、デフォルトコールバック(stderr出力)経由で
/// 診断メッセージをそのまま可視化する。
static AV_LOG_LEVEL_INIT: std::sync::Once = std::sync::Once::new();
fn install_av_log_level() {
    AV_LOG_LEVEL_INIT.call_once(|| unsafe {
        sys::av_log_set_level(sys::AV_LOG_DEBUG);
    });
}

impl DecoderCore {
    pub fn open(path: &Path) -> Result<Self, ffmpeg::Error> {
        install_av_log_level();
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

        let codec_ptr = decoder
            .codec()
            .map(|c| unsafe { c.as_ptr() })
            .unwrap_or(ptr::null());
        let (hw_device_ctx, hw_pix_fmt) = if codec_ptr.is_null() {
            (None, None)
        } else {
            match unsafe { try_create_hw_device_ctx(codec_ptr) } {
                Some((ctx, pix_fmt)) => (Some(ctx), Some(pix_fmt)),
                None => (None, None),
            }
        };

        let mut hw_pix_fmt_box: Option<*mut sys::AVPixelFormat> = None;
        if let Some(ctx) = &hw_device_ctx {
            unsafe {
                let raw = decoder.as_mut_ptr();
                (*raw).hw_device_ctx = sys::av_buffer_ref(ctx.0);
                let boxed = Box::into_raw(Box::new(hw_pix_fmt.expect(
                    "hw_device_ctxが存在する場合hw_pix_fmtも必ず存在する（try_create_hw_device_ctxの契約）",
                )));
                (*raw).opaque = boxed as *mut std::ffi::c_void;
                (*raw).get_format = Some(get_hw_format);
                hw_pix_fmt_box = Some(boxed);
            }
        }

        unsafe {
            let ctx = &mut *input.as_mut_ptr();
            for i in 0..ctx.nb_streams as usize {
                if i != stream_index {
                    (*(*ctx.streams.add(i))).discard = sys::AVDiscard::AVDISCARD_ALL;
                }
            }
        }
        let index = build_index(&mut input, stream_index, &mut decoder)?;
        unsafe { seek_stream_backward(&mut input, stream_index, 0) }
            .map_err(|_| ffmpeg::Error::Bug)?;
        decoder.flush();

        Ok(Self {
            input,
            stream_index,
            decoder,
            _hw_device_ctx: hw_device_ctx,
            hw_pix_fmt,
            fps: if fps > 0.0 { fps } else { 30.0 },
            width,
            height,
            index,
            cache: TextureCache::new(TEXTURE_CACHE_CAPACITY.max(4)),
            last_display_index: -1,
            exhausted: false,
            hw_pix_fmt_box,
        })
    }

    pub fn total_frames(&self) -> i64 {
        self.index.len() as i64
    }

    fn preceding_keyframe(&self, target_index: i64) -> i64 {
        for i in (0..=target_index).rev() {
            if self.index[i as usize].key {
                return i;
            }
        }
        0
    }

    /// ワーカースレッド専用。デコードのみ実行し内部indexへ位置を反映する。
    /// GPU操作（テクスチャ生成）は行わない。単一スレッドから逐次呼ばれる前提のため
    /// `frame_gpu_at`とのシーク競合は構造的に発生しない。
    pub fn prefetch_at(&mut self, target_index: i64) -> Result<(), String> {
        let target = target_index.clamp(0, self.total_frames() - 1);
        if self.cache.contains(target) {
            return Ok(());
        }
        if self.last_display_index >= target && self.last_display_index >= 0 {
            return Ok(());
        }

        let need_seek = self.exhausted
            || self.last_display_index < 0
            || target < self.last_display_index
            || target - self.last_display_index >= SEEK_THRESHOLD;
        if need_seek {
            let key = self.preceding_keyframe(target);
            let seek_pts = self.index[key as usize].pts;
            unsafe { seek_stream_backward(&mut self.input, self.stream_index, seek_pts) }?;
            self.decoder.flush();
            self.last_display_index = -1;
            self.exhausted = false;
            unsafe {
                (*self.decoder.as_mut_ptr()).skip_loop_filter = sys::AVDiscard::AVDISCARD_NONREF;
            }
        }

        let mut decoded = VideoFrame::empty();
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
                if display_index >= target {
                    unsafe {
                        (*self.decoder.as_mut_ptr()).skip_loop_filter =
                            sys::AVDiscard::AVDISCARD_NONE;
                    }
                    return Ok(());
                }
            }
        }
        self.decoder.send_eof().map_err(|e| e.to_string())?;
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
            if display_index >= target {
                unsafe {
                    (*self.decoder.as_mut_ptr()).skip_loop_filter = sys::AVDiscard::AVDISCARD_NONE;
                }
                return Ok(());
            }
        }
        self.exhausted = true;
        Err("EOF".to_owned())
    }

    /// ワーカースレッド専用。target_indexのネイティブハードウェアフレームを再デコードし、
    /// av_hwframe_transfer_dataでシステムメモリへ転送のうえRGBA8変換・アップロードする。
    /// 単一スレッドから逐次呼ばれる前提のため`prefetch_at`とのシーク競合は構造的に
    /// 発生しない。本関数はハードウェアデコード成立時のみ動作する。
    pub fn frame_gpu_at(
        &mut self,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        let target = frame_index.clamp(0, self.total_frames() - 1);
        if let Some(tex) = self.cache.get(target) {
            return Ok(tex);
        }

        let need_seek = self.exhausted
            || self.last_display_index < 0
            || target < self.last_display_index
            || target - self.last_display_index >= SEEK_THRESHOLD;
        if need_seek {
            let key = self.preceding_keyframe(target);
            let seek_pts = self.index[key as usize].pts;
            unsafe { seek_stream_backward(&mut self.input, self.stream_index, seek_pts) }?;
            self.decoder.flush();
            self.last_display_index = -1;
            self.exhausted = false;
            unsafe {
                (*self.decoder.as_mut_ptr()).skip_loop_filter = sys::AVDiscard::AVDISCARD_NONREF;
            }
        }

        let Some(hw_pix_fmt) = self.hw_pix_fmt else {
            return Err(
                "ハードウェアデコード非対応環境（ソフトウェア経路は本ファイル対象外）".into(),
            );
        };

        let mut decoded = VideoFrame::empty();
        let stream_index = self.stream_index;
        let mut hw_frames_seen: u32 = 0;
        loop {
            let Some((stream, packet)) = self.input.packets().next() else {
                break;
            };
            if stream.index() != stream_index {
                continue;
            }
            self.decoder
                .send_packet(&packet)
                .map_err(|e| e.to_string())?;
            if let Some(texture) = self.drain_hw_frames(
                &mut decoded,
                hw_pix_fmt,
                target,
                device,
                queue,
                &mut hw_frames_seen,
            )? {
                return Ok(texture);
            }
        }
        self.decoder.send_eof().map_err(|e| e.to_string())?;
        if let Some(texture) = self.drain_hw_frames(
            &mut decoded,
            hw_pix_fmt,
            target,
            device,
            queue,
            &mut hw_frames_seen,
        )? {
            return Ok(texture);
        }
        self.exhausted = true;
        if hw_frames_seen == 0 {
            Err("EOF (ハードウェアフレーム0枚: get_hw_format交渉が不成立の可能性)".into())
        } else {
            Err("EOF".into())
        }
    }

    /// receive_frameをEAGAIN相当まで汲み尽くし、hw_pix_fmt一致フレームをアップロード・
    /// キャッシュする。target到達時のみSome(texture)を返す。prefetch_at/frame_gpu_atの
    /// 呼び出し順序(packets枯渇前→send_eof後)で2回呼ばれる想定の共通処理。
    fn drain_hw_frames(
        &mut self,
        decoded: &mut VideoFrame,
        hw_pix_fmt: sys::AVPixelFormat,
        target: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        hw_frames_seen: &mut u32,
    ) -> Result<Option<wgpu::Texture>, String> {
        let (width, height) = (self.width, self.height);
        while self.decoder.receive_frame(decoded).is_ok() {
            let pts = decoded.pts().unwrap_or(0);
            let raw_format = unsafe { (*decoded.as_ptr()).format };
            let found = self.index.binary_search_by_key(&pts, |e| e.pts).ok();
            if let Some(idx) = found {
                if (idx as i64 - target).abs() <= 3 {
                    eprintln!(
                        "[decoder_core][diag] pts={pts} display_index={idx} target={target} \
                         raw_format={raw_format} hw_pix_fmt={:?} format_match={}",
                        hw_pix_fmt as i32,
                        raw_format == hw_pix_fmt as i32,
                    );
                }
            } else {
                eprintln!("[decoder_core][diag] pts={pts} はindexに一致なし(binary_search失敗)");
            }
            let Some(display_index) = found.map(|i| i as i64) else {
                continue;
            };
            self.last_display_index = display_index;

            if raw_format != hw_pix_fmt as i32 {
                continue;
            }
            *hw_frames_seen += 1;

            let texture = unsafe {
                upload_hw_frame_as_texture(device, queue, decoded.as_mut_ptr(), width, height)
            }
            .map_err(|e| format!("frame upload failed at index={display_index}: {e}"))?;

            self.cache.put(display_index, texture.clone());
            if display_index >= target {
                unsafe {
                    (*self.decoder.as_mut_ptr()).skip_loop_filter = sys::AVDiscard::AVDISCARD_NONE;
                }
                return Ok(Some(texture));
            }
        }
        Ok(None)
    }
}
