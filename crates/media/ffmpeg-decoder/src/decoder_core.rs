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

struct HwDeviceCtx(*mut sys::AVBufferRef);
unsafe impl Send for HwDeviceCtx {}
impl Drop for HwDeviceCtx {
    fn drop(&mut self) {
        unsafe { sys::av_buffer_unref(&mut self.0) };
    }
}

#[cfg(target_os = "windows")]
const NATIVE_HW_TYPE: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA;
#[cfg(target_os = "linux")]
const NATIVE_HW_TYPE: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI;
#[cfg(target_os = "macos")]
const NATIVE_HW_TYPE: sys::AVHWDeviceType = sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX;

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
const SEEK_THRESHOLD: i64 = 64;

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
    exhausted: bool,
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
