//! Phase 2: ハードウェアデコード + ゼロコピー統一パス（AVVkFrame経由）。
//! swscale/CPU RGBAアップロード経路を全廃し、hwmap=derive_device=vulkanで
//! 全OSのネイティブハードウェアフレームをVulkan統一フレームへ変換したのち
//! wgpu-hal経由でVkImageをwgpu::Textureへゼロコピー import する。
//!
//! Phase 3でOS別直接抽出パス（Windows: ID3D11Texture2D直接 / Linux: DRM-fdインポート /
//! macOS: CVPixelBuffer直接）を本パスの前段に追加する。本ファイル単体では
//! hwmap=vulkan統一パスのみが有効。
//!
//! 未検証境界: AVVkFrame構造体（libavutil/hwcontext_vulkan.h由来、
//! ffmpeg-sys-next "vulkan" feature経由）のフィールドレイアウトは、
//! リンクするFFmpegビルドのバージョンに一致する ffmpeg-sys-next のバインディング
//! 生成結果に依存する。本実装は公開ヘッダのフィールド名（img/tiling/mem/size/layout/
//! sem/sem_value/queue_family_index）をそのまま参照し、配列長は
//! AV_NUM_DATA_POINTERS（8）を仮定する。CIビルド時に `bindgen`出力との不一致が
//! あればコンパイルエラーとして即時検出される（フィールド名・型が変われば
//! ビルド不能になるため、サイレントな不整合は生じない）。

use ash::vk::Handle;
use ffmpeg_next as ffmpeg;
use ffmpeg_next::util::frame::Video as VideoFrame;
use ffmpeg_sys_next as sys;
use neoutl_media_api::{DEFAULT_DECODE_CACHE_BYTES, VideoSource};
use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
use std::path::Path;
use std::ptr;

struct IndexEntry {
    pts: i64,
    key: bool,
}

/// テクスチャキャッシュ。CPU側はゼロコピー化によりバイト列を保持せず、
/// wgpu::Texture自体をLRU保持する（VRAM上限は呼び出し側のcapacity_countで制御）。
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
    fn invalidate_all(&mut self) {
        self.order.clear();
        self.map.clear();
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

/// hwmap=derive_device=vulkan単一フィルタのみを持つ最小グラフ。
/// buffer(hwソース) -> hwmap -> buffersink。
struct HwmapGraph {
    graph: *mut sys::AVFilterGraph,
    src_ctx: *mut sys::AVFilterContext,
    sink_ctx: *mut sys::AVFilterContext,
}
unsafe impl Send for HwmapGraph {}
impl Drop for HwmapGraph {
    fn drop(&mut self) {
        unsafe { sys::avfilter_graph_free(&mut self.graph) };
    }
}

/// OS別のネイティブハードウェアデコード種別。Vulkan統一パスへの入力側デバイス種別。
#[cfg(target_os = "windows")]
const NATIVE_HW_TYPE: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA;
#[cfg(target_os = "linux")]
const NATIVE_HW_TYPE: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI;
#[cfg(target_os = "macos")]
const NATIVE_HW_TYPE: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX;

fn cstr(s: &str) -> CString {
    CString::new(s).expect("filter descriptor contains NUL")
}

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

/// hwmap=derive_device=vulkan の最小フィルタグラフを構築する。
/// 入力AVFrameのhw_frames_ctxをbufferソースのpar経由で共有し、
/// 出力側はAV_PIX_FMT_VULKANフレーム（AVVkFrame）を1枚返す。
unsafe fn build_hwmap_graph(
    time_base: sys::AVRational,
    hw_frames_ctx: *mut sys::AVBufferRef,
) -> Result<HwmapGraph, String> {
    let graph = unsafe { sys::avfilter_graph_alloc() };
    if graph.is_null() {
        return Err("avfilter_graph_alloc failed".into());
    }

    let buffer_filter = unsafe { sys::avfilter_get_by_name(cstr("buffer").as_ptr()) };
    let hwmap_filter = unsafe { sys::avfilter_get_by_name(cstr("hwmap").as_ptr()) };
    let sink_filter = unsafe { sys::avfilter_get_by_name(cstr("buffersink").as_ptr()) };
    if buffer_filter.is_null() || hwmap_filter.is_null() || sink_filter.is_null() {
        unsafe { sys::avfilter_graph_free(&mut { graph }) };
        return Err("required avfilter not found (buffer/hwmap/buffersink)".into());
    }

    let par = unsafe { sys::av_buffersrc_parameters_alloc() };
    if par.is_null() {
        unsafe { sys::avfilter_graph_free(&mut { graph }) };
        return Err("av_buffersrc_parameters_alloc failed".into());
    }
    unsafe {
        (*par).hw_frames_ctx = hw_frames_ctx;
    }

    let mut src_ctx: *mut sys::AVFilterContext = ptr::null_mut();
    let ret = unsafe {
        sys::avfilter_graph_create_filter(
            &mut src_ctx,
            buffer_filter,
            cstr("in").as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            graph,
        )
    };
    if ret < 0 || src_ctx.is_null() {
        unsafe {
            sys::av_free(par as *mut _);
            sys::avfilter_graph_free(&mut { graph });
        }
        return Err(format!("buffer filter create failed ret={ret}"));
    }
    unsafe {
        (*par).time_base = time_base;
        let set_ret = sys::av_buffersrc_parameters_set(src_ctx, par);
        sys::av_free(par as *mut _);
        if set_ret < 0 {
            sys::avfilter_graph_free(&mut { graph });
            return Err(format!("av_buffersrc_parameters_set failed ret={set_ret}"));
        }
    }

    let mut hwmap_ctx: *mut sys::AVFilterContext = ptr::null_mut();
    let ret = unsafe {
        sys::avfilter_graph_create_filter(
            &mut hwmap_ctx,
            hwmap_filter,
            cstr("hwmap0").as_ptr(),
            cstr("derive_device=vulkan").as_ptr(),
            ptr::null_mut(),
            graph,
        )
    };
    if ret < 0 || hwmap_ctx.is_null() {
        unsafe { sys::avfilter_graph_free(&mut { graph }) };
        return Err(format!("hwmap filter create failed ret={ret}"));
    }

    let mut sink_ctx: *mut sys::AVFilterContext = ptr::null_mut();
    let ret = unsafe {
        sys::avfilter_graph_create_filter(
            &mut sink_ctx,
            sink_filter,
            cstr("out").as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            graph,
        )
    };
    if ret < 0 || sink_ctx.is_null() {
        unsafe { sys::avfilter_graph_free(&mut { graph }) };
        return Err(format!("buffersink filter create failed ret={ret}"));
    }

    let ret = unsafe { sys::avfilter_link(src_ctx, 0, hwmap_ctx, 0) };
    if ret < 0 {
        unsafe { sys::avfilter_graph_free(&mut { graph }) };
        return Err(format!("avfilter_link(src,hwmap) failed ret={ret}"));
    }
    let ret = unsafe { sys::avfilter_link(hwmap_ctx, 0, sink_ctx, 0) };
    if ret < 0 {
        unsafe { sys::avfilter_graph_free(&mut { graph }) };
        return Err(format!("avfilter_link(hwmap,sink) failed ret={ret}"));
    }

    let ret = unsafe { sys::avfilter_graph_config(graph, ptr::null_mut()) };
    if ret < 0 {
        unsafe { sys::avfilter_graph_free(&mut { graph }) };
        return Err(format!("avfilter_graph_config failed ret={ret}"));
    }

    Ok(HwmapGraph {
        graph,
        src_ctx,
        sink_ctx,
    })
}

/// hwネイティブフレーム(*mut AVFrame)をグラフへ投入しVulkanフレームを1枚取得する。
/// 戻り値のAVFrameはformat==AV_PIX_FMT_VULKAN、data[0]が`*mut AVVkFrame`を指す。
unsafe fn map_to_vulkan_frame(
    graph: &HwmapGraph,
    native_frame: *mut sys::AVFrame,
) -> Result<*mut sys::AVFrame, String> {
    let ret = unsafe {
        sys::av_buffersrc_add_frame_flags(
            graph.src_ctx,
            native_frame,
            sys::AV_BUFFERSRC_FLAG_KEEP_REF as i32,
        )
    };
    if ret < 0 {
        return Err(format!("av_buffersrc_add_frame_flags failed ret={ret}"));
    }
    let vk_frame = unsafe { sys::av_frame_alloc() };
    if vk_frame.is_null() {
        return Err("av_frame_alloc failed".into());
    }
    let ret = unsafe { sys::av_buffersink_get_frame(graph.sink_ctx, vk_frame) };
    if ret < 0 {
        unsafe { sys::av_frame_free(&mut { vk_frame }) };
        return Err(format!("av_buffersink_get_frame failed ret={ret}"));
    }
    Ok(vk_frame)
}

/// AVVkFrame（libavutil/hwcontext_vulkan.h）からVkImageハンドルを抽出し、
/// wgpu-hal Vulkan backend経由でwgpu::Textureへゼロコピーimportする。
/// plane 0（NV12 Y/UV結合はAVVkFrame側で単一VkImage・複数planeのため、
/// ここではimg[0]をカラーアタッチメント全体のベースイメージとして扱う）。
///
/// 前提: wgpu::Deviceがinstance生成時にVulkan backendで初期化され、
/// gpu_shared.rs（Phase5）がset_shared_deviceでこのVkImageと同一
/// VkPhysicalDevice/VkDeviceを共有していること。異なるVkDeviceが指す
/// VkImageをimportした場合、wgpu-hal内部でVK_ERROR_DEVICE_LOSTとなる。
unsafe fn import_vkimage_as_texture(
    device: &wgpu::Device,
    vk_frame: *mut sys::AVFrame,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, String> {
    let frame_ref = unsafe { &*vk_frame };
    if frame_ref.format != sys::AVPixelFormat::AV_PIX_FMT_VULKAN as i32 {
        return Err(format!(
            "unexpected mapped frame format={}（AV_PIX_FMT_VULKAN以外）",
            frame_ref.format
        ));
    }
    let vk = unsafe { &*(frame_ref.data[0] as *const sys::AVVkFrame) };
    let image = vk.img[0];
    let memory = vk.mem[0];
    if image == 0 {
        return Err("AVVkFrame.img[0] is VK_NULL_HANDLE".into());
    }
    if memory == 0 {
        return Err("AVVkFrame.mem[0] is VK_NULL_HANDLE".into());
    }

    let hal_desc = wgpu_hal::TextureDescriptor {
        label: Some("ffmpeg-decoder vulkan zero-copy frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUses::RESOURCE,
        memory_flags: wgpu_hal::MemoryFlags::empty(),
        view_formats: vec![],
    };

    let texture = unsafe {
        let hal_device_guard = device.as_hal::<wgpu_hal::api::Vulkan>().expect(
            "wgpu::Device::as_hal::<Vulkan>: backend不一致（Vulkan以外で初期化されたDevice）",
        );
        let hal_texture = hal_device_guard.texture_from_raw(
            ash::vk::Image::from_raw(image),
            &hal_desc,
            None,
            wgpu_hal::vulkan::TextureMemory::Dedicated(ash::vk::DeviceMemory::from_raw(memory)),
        );
        device.create_texture_from_hal::<wgpu_hal::api::Vulkan>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: hal_desc.label,
                size: hal_desc.size,
                mip_level_count: hal_desc.mip_level_count,
                sample_count: hal_desc.sample_count,
                dimension: hal_desc.dimension,
                format: hal_desc.format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        )
    };

    Ok(texture)
}

pub struct FfmpegVideoDecoder {
    input: ffmpeg::format::context::Input,
    stream_index: usize,
    decoder: ffmpeg::decoder::Video,
    _hw_device_ctx: Option<HwDeviceCtx>,
    hw_pix_fmt: Option<sys::AVPixelFormat>,
    hwmap_graph: Option<HwmapGraph>,
    fps: f64,
    width: u32,
    height: u32,
    index: Vec<IndexEntry>,
    cache: TextureCache,
    last_display_index: i64,
}

unsafe impl Send for FfmpegVideoDecoder {}

const TEXTURE_CACHE_CAPACITY: usize = (DEFAULT_DECODE_CACHE_BYTES / (1920 * 1080 * 4)) as usize;

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
        let time_base = stream.time_base();

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

        if let Some(ctx) = &hw_device_ctx {
            unsafe {
                let raw = decoder.as_mut_ptr();
                (*raw).hw_device_ctx = sys::av_buffer_ref(ctx.0);
            }
        }

        let hwmap_graph = match &hw_device_ctx {
            Some(ctx) => {
                let av_time_base = sys::AVRational {
                    num: time_base.numerator(),
                    den: time_base.denominator(),
                };
                unsafe { build_hwmap_graph(av_time_base, ctx.0) }.ok()
            }
            None => None,
        };

        let index = build_index(&mut input, stream_index, &mut decoder)?;
        input.seek(i64::MIN, ..)?;
        decoder.flush();

        Ok(Self {
            input,
            stream_index,
            decoder,
            _hw_device_ctx: hw_device_ctx,
            hw_pix_fmt,
            hwmap_graph,
            fps: if fps > 0.0 { fps } else { 30.0 },
            width,
            height,
            index,
            cache: TextureCache::new(TEXTURE_CACHE_CAPACITY.max(4)),
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

    /// バックグラウンドスレッド専用。デコードのみ実行し内部indexへ位置を反映する。
    /// GPU操作（ゼロコピーimport含む）はframe_gpu側（UIスレッド）でのみ行う。
    fn decode_until_index(&mut self, target_index: i64) -> Result<(), String> {
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
                if display_index >= target_index {
                    return Ok(());
                }
            }
        }
        Err("EOF".to_owned())
    }

    /// UIスレッド専用。target_indexのネイティブハードウェアフレームを再デコードし、
    /// hwmapグラフでVulkanフレーム化のうえVkImageをゼロコピーimportする。
    /// prefetchはpts→display_indexの位置解決のみを担い、実フレームデータは
    /// GPU側で毎回引き直す（AVFrameのhw参照をスレッド間で共有しないため）。
    fn texture_at(
        &mut self,
        frame_index: i64,
        device: &wgpu::Device,
    ) -> Result<wgpu::Texture, String> {
        let target = frame_index.clamp(0, self.total_frames() - 1);
        if let Some(tex) = self.cache.get(target) {
            return Ok(tex);
        }

        let need_seek = self.last_display_index < 0 || target < self.last_display_index;
        if need_seek {
            let key = self.preceding_keyframe(target);
            let seek_pts = self.index[key as usize].pts;
            self.input
                .seek(seek_pts, ..seek_pts)
                .map_err(|e| e.to_string())?;
            self.decoder.flush();
            self.last_display_index = -1;
        }

        let Some(graph) = &self.hwmap_graph else {
            return Err(
                "hwmapグラフ未初期化（ハードウェアデコード非対応環境。ソフトウェア経路は本ファイル対象外）"
                    .into(),
            );
        };

        let mut decoded = VideoFrame::empty();
        let stream_index = self.stream_index;
        loop {
            let Some((stream, packet)) = self.input.packets().next() else {
                return Err("EOF".into());
            };
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

                if decoded.format() as i32 != self.hw_pix_fmt.map(|f| f as i32).unwrap_or(-1) {
                    continue;
                }

                let vk_frame = unsafe { map_to_vulkan_frame(graph, decoded.as_mut_ptr()) }
                    .map_err(|e| format!("hwmap failed at index={display_index}: {e}"))?;
                let texture_result =
                    unsafe { import_vkimage_as_texture(device, vk_frame, self.width, self.height) };
                unsafe { sys::av_frame_free(&mut { vk_frame }) };
                let texture = texture_result?;

                self.cache.put(display_index, texture.clone());
                if display_index >= target {
                    return Ok(texture);
                }
            }
        }
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

    /// バックグラウンドスレッド専用。GPU操作禁止契約のため、位置解決（pts探索・
    /// キーフレーム基準のシーク要否判定）のみ先行させる。実フレーム取得は
    /// frame_gpu側でのみ行う（hwフレームはOSネイティブハンドルでありスレッド間
    /// 共有がFFmpeg/ドライバ側で保証されないため）。
    fn prefetch(&mut self, frame_index: i64) -> Result<(), String> {
        let target = frame_index.clamp(0, self.total_frames() - 1);
        if self.cache.contains(target) {
            return Ok(());
        }
        if self.last_display_index >= target && self.last_display_index >= 0 {
            return Ok(());
        }
        self.decode_until_index(target)
    }

    fn frame_gpu(
        &mut self,
        frame_index: i64,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        self.texture_at(frame_index, device)
    }
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

/// 音声デコード経路。Phase 2のゼロコピー対象外（音声はPCM展開が必須のためCPU経路のまま）。
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
