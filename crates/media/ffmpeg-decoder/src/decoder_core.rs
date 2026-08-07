use ffmpeg_next as ffmpeg;
use ffmpeg_next::util::frame::Video as VideoFrame;
use ffmpeg_sys_next as sys;
use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
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

#[cfg(target_os = "linux")]
struct HwScaleFilter {
    graph: *mut sys::AVFilterGraph,
    src_ctx: *mut sys::AVFilterContext,
    sink_ctx: *mut sys::AVFilterContext,
}
#[cfg(target_os = "linux")]
unsafe impl Send for HwScaleFilter {}
#[cfg(target_os = "linux")]
impl Drop for HwScaleFilter {
    fn drop(&mut self) {
        unsafe { sys::avfilter_graph_free(&mut self.graph) };
    }
}

#[cfg(target_os = "linux")]
unsafe fn build_vaapi_scale_filter(
    hw_frames_ctx: *mut sys::AVBufferRef,
    in_width: i32,
    in_height: i32,
    hw_pix_fmt: sys::AVPixelFormat,
    out_width: i32,
    out_height: i32,
) -> Result<HwScaleFilter, String> {
    unsafe {
        let graph = sys::avfilter_graph_alloc();
        if graph.is_null() {
            return Err("avfilter_graph_alloc failed".into());
        }

        let buffer_name = CString::new("buffer").unwrap();
        let buffersink_name = CString::new("buffersink").unwrap();
        let in_name = CString::new("in").unwrap();
        let out_name = CString::new("out").unwrap();
        let args = CString::new(format!(
            "video_size={in_width}x{in_height}:pix_fmt={}:time_base=1/1000000",
            hw_pix_fmt as i32,
        ))
        .unwrap();

        let buffersrc = sys::avfilter_get_by_name(buffer_name.as_ptr());
        let mut src_ctx: *mut sys::AVFilterContext = ptr::null_mut();
        let ret = sys::avfilter_graph_create_filter(
            &mut src_ctx,
            buffersrc,
            in_name.as_ptr(),
            args.as_ptr(),
            ptr::null_mut(),
            graph,
        );
        if ret < 0 || src_ctx.is_null() {
            let mut g = graph;
            sys::avfilter_graph_free(&mut g);
            return Err(format!("buffer filter作成失敗 ret={ret}"));
        }

        let par = sys::av_buffersrc_parameters_alloc();
        if par.is_null() {
            let mut g = graph;
            sys::avfilter_graph_free(&mut g);
            return Err("av_buffersrc_parameters_alloc failed".into());
        }
        (*par).hw_frames_ctx = hw_frames_ctx;
        let ret = sys::av_buffersrc_parameters_set(src_ctx, par);
        sys::av_free(par as *mut std::ffi::c_void);
        if ret < 0 {
            let mut g = graph;
            sys::avfilter_graph_free(&mut g);
            return Err(format!("av_buffersrc_parameters_set failed ret={ret}"));
        }

        let buffersink = sys::avfilter_get_by_name(buffersink_name.as_ptr());
        let mut sink_ctx: *mut sys::AVFilterContext = ptr::null_mut();
        let ret = sys::avfilter_graph_create_filter(
            &mut sink_ctx,
            buffersink,
            out_name.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            graph,
        );
        if ret < 0 || sink_ctx.is_null() {
            let mut g = graph;
            sys::avfilter_graph_free(&mut g);
            return Err(format!("buffersink filter作成失敗 ret={ret}"));
        }

        let mut outputs = sys::avfilter_inout_alloc();
        let mut inputs = sys::avfilter_inout_alloc();
        if outputs.is_null() || inputs.is_null() {
            sys::avfilter_inout_free(&mut outputs);
            sys::avfilter_inout_free(&mut inputs);
            let mut g = graph;
            sys::avfilter_graph_free(&mut g);
            return Err("avfilter_inout_alloc failed".into());
        }
        (*outputs).name = sys::av_strdup(in_name.as_ptr());
        (*outputs).filter_ctx = src_ctx;
        (*outputs).pad_idx = 0;
        (*outputs).next = ptr::null_mut();

        (*inputs).name = sys::av_strdup(out_name.as_ptr());
        (*inputs).filter_ctx = sink_ctx;
        (*inputs).pad_idx = 0;
        (*inputs).next = ptr::null_mut();

        let filter_spec =
            CString::new(format!("scale_vaapi=w={out_width}:h={out_height}")).unwrap();
        let ret = sys::avfilter_graph_parse_ptr(
            graph,
            filter_spec.as_ptr(),
            &mut inputs,
            &mut outputs,
            ptr::null_mut(),
        );
        sys::avfilter_inout_free(&mut inputs);
        sys::avfilter_inout_free(&mut outputs);
        if ret < 0 {
            let mut g = graph;
            sys::avfilter_graph_free(&mut g);
            return Err(format!("avfilter_graph_parse_ptr failed ret={ret}"));
        }

        let ret = sys::avfilter_graph_config(graph, ptr::null_mut());
        if ret < 0 {
            let mut g = graph;
            sys::avfilter_graph_free(&mut g);
            return Err(format!("avfilter_graph_config failed ret={ret}"));
        }

        Ok(HwScaleFilter {
            graph,
            src_ctx,
            sink_ctx,
        })
    }
}

#[cfg(target_os = "linux")]
unsafe fn run_hw_scale_filter(
    filter: &HwScaleFilter,
    hw_frame: *mut sys::AVFrame,
) -> Result<*mut sys::AVFrame, String> {
    unsafe {
        let ret = sys::av_buffersrc_add_frame_flags(
            filter.src_ctx,
            hw_frame,
            sys::AV_BUFFERSRC_FLAG_KEEP_REF as i32,
        );
        if ret < 0 {
            return Err(format!("av_buffersrc_add_frame_flags failed ret={ret}"));
        }
        let filtered = sys::av_frame_alloc();
        if filtered.is_null() {
            return Err("av_frame_alloc failed".into());
        }
        let ret = sys::av_buffersink_get_frame(filter.sink_ctx, filtered);
        if ret < 0 {
            let mut f = filtered;
            sys::av_frame_free(&mut f);
            return Err(format!("av_buffersink_get_frame failed ret={ret}"));
        }
        Ok(filtered)
    }
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
    let ctx = unsafe { input.as_mut_ptr() };
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
) -> Result<(Vec<IndexEntry>, Vec<i64>, Vec<i64>), ffmpeg::Error> {
    let mut index = Vec::new();
    for (stream, packet) in input.packets() {
        if stream.index() != stream_index {
            continue;
        }
        let pts = packet.pts().or_else(|| packet.dts()).unwrap_or(0);
        index.push(IndexEntry {
            pts,
            key: packet.is_key(),
        });
    }
    index.sort_by_key(|e| e.pts);

    let n = index.len();
    let mut prev_keyframe = vec![0i64; n];
    let mut last_key = 0i64;
    for i in 0..n {
        if index[i].key {
            last_key = i as i64;
        }
        prev_keyframe[i] = last_key;
    }

    let mut gop_end = vec![0i64; n.max(1)];
    let mut end = n as i64 - 1;
    for i in (0..n).rev() {
        gop_end[i] = end;
        if i > 0 && index[i].key {
            end = i as i64 - 1;
        }
    }

    Ok((index, prev_keyframe, gop_end))
}

const TEXTURE_CACHE_CAPACITY: usize =
    (neoutl_media_api::DEFAULT_DECODE_CACHE_BYTES / (1920 * 1080 * 4)) as usize;
struct SendPacket(ffmpeg::codec::packet::Packet);
unsafe impl Send for SendPacket {}

struct PacketQueueState {
    packets: VecDeque<SendPacket>,
    eof: bool,
    generation: u64,
}

enum ReaderCmd {
    Seek(i64),
    Stop,
}

const READER_QUEUE_CAPACITY: usize = 64;

fn spawn_packet_reader(
    mut input: ffmpeg::format::context::Input,
    stream_index: usize,
) -> (
    std::sync::mpsc::Sender<ReaderCmd>,
    std::sync::Arc<(std::sync::Mutex<PacketQueueState>, std::sync::Condvar)>,
    std::thread::JoinHandle<()>,
) {
    let shared = std::sync::Arc::new((
        std::sync::Mutex::new(PacketQueueState {
            packets: VecDeque::new(),
            eof: false,
            generation: 0,
        }),
        std::sync::Condvar::new(),
    ));
    let shared_thread = std::sync::Arc::clone(&shared);
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<ReaderCmd>();

    let join = std::thread::Builder::new()
        .name("neoutl-ffmpeg-packet-reader".into())
        .spawn(move || {
            let (lock, cvar) = &*shared_thread;
            let do_seek = |input: &mut ffmpeg::format::context::Input, pts: i64| {
                let _ = unsafe { seek_stream_backward(input, stream_index, pts) };
                let mut q = lock.lock().expect("packet queue mutex poisoned");
                q.packets.clear();
                q.eof = false;
                q.generation += 1;
                cvar.notify_all();
            };
            'outer: loop {
                match cmd_rx.try_recv() {
                    Ok(ReaderCmd::Seek(pts)) => {
                        do_seek(&mut input, pts);
                        continue 'outer;
                    }
                    Ok(ReaderCmd::Stop) => break 'outer,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break 'outer,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
                {
                    let q = lock.lock().expect("packet queue mutex poisoned");
                    if q.packets.len() >= READER_QUEUE_CAPACITY {
                        drop(q);
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        continue 'outer;
                    }
                }
                match input.packets().next() {
                    Some((stream, packet)) => {
                        if stream.index() == stream_index {
                            let mut q = lock.lock().expect("packet queue mutex poisoned");
                            q.packets.push_back(SendPacket(packet));
                            cvar.notify_all();
                        }
                    }
                    None => {
                        {
                            let mut q = lock.lock().expect("packet queue mutex poisoned");
                            q.eof = true;
                            cvar.notify_all();
                        }
                        match cmd_rx.recv() {
                            Ok(ReaderCmd::Seek(pts)) => do_seek(&mut input, pts),
                            Ok(ReaderCmd::Stop) => break 'outer,
                            Err(_) => break 'outer,
                        }
                    }
                }
            }
        })
        .expect("packet reader thread spawn failed");

    (cmd_tx, shared, join)
}

const SEEK_THRESHOLD: i64 = 64;

pub struct DecoderCore {
    reader_cmd_tx: std::sync::mpsc::Sender<ReaderCmd>,
    reader_shared: std::sync::Arc<(std::sync::Mutex<PacketQueueState>, std::sync::Condvar)>,
    reader_join: Option<std::thread::JoinHandle<()>>,
    decoder: ffmpeg::decoder::Video,
    _hw_device_ctx: Option<HwDeviceCtx>,
    hw_pix_fmt: Option<sys::AVPixelFormat>,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    index: Vec<IndexEntry>,
    prev_keyframe: Vec<i64>,
    gop_end: Vec<i64>,
    cache: TextureCache,
    last_display_index: i64,
    exhausted: bool,
    hw_pix_fmt_box: Option<*mut sys::AVPixelFormat>,
    hw_scale_target: Option<(u32, u32)>,
    #[cfg(target_os = "linux")]
    hw_scale_filter: Option<HwScaleFilter>,
}

unsafe impl Send for DecoderCore {}

impl Drop for DecoderCore {
    fn drop(&mut self) {
        let _ = self.reader_cmd_tx.send(ReaderCmd::Stop);
        if let Some(join) = self.reader_join.take() {
            let _ = join.join();
        }
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

        if let Ok(v) = std::env::var("NEOUTL_AVFORMAT_THREADS") {
            if let Ok(n) = v.parse::<i32>() {
                unsafe {
                    (*decoder.as_mut_ptr()).thread_count = n;
                }
            }
        }

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
        let (index, prev_keyframe, gop_end) = build_index(&mut input, stream_index)?;
        unsafe { seek_stream_backward(&mut input, stream_index, 0) }
            .map_err(|_| ffmpeg::Error::Bug)?;
        decoder.flush();

        let (reader_cmd_tx, reader_shared, reader_join) = spawn_packet_reader(input, stream_index);

        Ok(Self {
            reader_cmd_tx,
            reader_shared,
            reader_join: Some(reader_join),
            decoder,
            _hw_device_ctx: hw_device_ctx,
            hw_pix_fmt,
            fps: if fps > 0.0 { fps } else { 30.0 },
            width,
            height,
            index,
            prev_keyframe,
            gop_end,
            cache: TextureCache::new(TEXTURE_CACHE_CAPACITY.max(4)),
            last_display_index: -1,
            exhausted: false,
            hw_pix_fmt_box,
            hw_scale_target: None,
            #[cfg(target_os = "linux")]
            hw_scale_filter: None,
        })
    }

    pub fn total_frames(&self) -> i64 {
        self.index.len() as i64
    }

    pub fn set_output_size(&mut self, width: u32, height: u32) {
        let target = if width == 0 || height == 0 || (width, height) == (self.width, self.height) {
            None
        } else {
            Some((width, height))
        };
        if target != self.hw_scale_target {
            self.hw_scale_target = target;
            #[cfg(target_os = "linux")]
            {
                self.hw_scale_filter = None;
            }
        }
    }

    fn preceding_keyframe(&self, target_index: i64) -> i64 {
        self.prev_keyframe[target_index as usize]
    }

    fn gop_end_index(&self, target_index: i64) -> i64 {
        self.gop_end[self.preceding_keyframe(target_index) as usize]
    }

    fn decode_budget(&self, target: i64) -> i64 {
        let key = self.preceding_keyframe(target);
        let gop_end = self.gop_end_index(target);
        (gop_end - key + 10).max(500)
    }

    fn ensure_seek(&mut self, target: i64) -> Result<(), String> {
        let need_seek = self.exhausted
            || self.last_display_index < 0
            || target < self.last_display_index
            || target - self.last_display_index >= SEEK_THRESHOLD;
        if !need_seek {
            return Ok(());
        }
        let key = self.preceding_keyframe(target);
        let seek_pts = self.index[key as usize].pts;

        let (lock, cvar) = &*self.reader_shared;
        let generation_before = lock.lock().expect("packet queue mutex poisoned").generation;
        self.reader_cmd_tx
            .send(ReaderCmd::Seek(seek_pts))
            .map_err(|_| "packet reader threadが消失".to_string())?;
        {
            let mut q = lock.lock().expect("packet queue mutex poisoned");
            while q.generation == generation_before {
                q = cvar.wait(q).expect("packet queue condvar poisoned");
            }
        }

        self.decoder.flush();
        self.last_display_index = -1;
        self.exhausted = false;
        unsafe {
            (*self.decoder.as_mut_ptr()).skip_loop_filter = sys::AVDiscard::AVDISCARD_NONREF;
        }
        Ok(())
    }

    fn next_packet(&mut self) -> Option<ffmpeg::codec::packet::Packet> {
        let (lock, cvar) = &*self.reader_shared;
        let mut q = lock.lock().expect("packet queue mutex poisoned");
        loop {
            if let Some(p) = q.packets.pop_front() {
                return Some(p.0);
            }
            if q.eof {
                return None;
            }
            q = cvar.wait(q).expect("packet queue condvar poisoned");
        }
    }

    pub fn prefetch_at(&mut self, target_index: i64) -> Result<(), String> {
        let target = target_index.clamp(0, self.total_frames() - 1);
        if self.cache.contains(target) {
            return Ok(());
        }
        if self.last_display_index >= target && self.last_display_index >= 0 {
            return Ok(());
        }
        self.ensure_seek(target)?;

        let mut budget = self.decode_budget(target);
        let mut decoded = VideoFrame::empty();
        while budget > 0 {
            let Some(packet) = self.next_packet() else {
                break;
            };
            self.decoder
                .send_packet(&packet)
                .map_err(|e| e.to_string())?;
            while budget > 0 && self.decoder.receive_frame(&mut decoded).is_ok() {
                budget -= 1;
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
        while budget > 0 && self.decoder.receive_frame(&mut decoded).is_ok() {
            budget -= 1;
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
        self.ensure_seek(target)?;

        let Some(hw_pix_fmt) = self.hw_pix_fmt else {
            return Err(
                "ハードウェアデコード非対応環境（ソフトウェア経路は本ファイル対象外）".into(),
            );
        };

        let mut decoded = VideoFrame::empty();
        let mut hw_frames_seen: u32 = 0;
        let mut budget = self.decode_budget(target);
        while budget > 0 {
            let Some(packet) = self.next_packet() else {
                break;
            };
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
                &mut budget,
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
            &mut budget,
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
        budget: &mut i64,
    ) -> Result<Option<wgpu::Texture>, String> {
        let (width, height) = (self.width, self.height);
        while *budget > 0 && self.decoder.receive_frame(decoded).is_ok() {
            *budget -= 1;
            let pts = decoded.pts().unwrap_or(0);
            let raw_format = unsafe { (*decoded.as_ptr()).format };
            let found = self.index.binary_search_by_key(&pts, |e| e.pts).ok();
            let Some(display_index) = found.map(|i| i as i64) else {
                continue;
            };
            self.last_display_index = display_index;

            if raw_format != hw_pix_fmt as i32 {
                continue;
            }
            *hw_frames_seen += 1;

            #[cfg(target_os = "linux")]
            let (upload_ptr, out_width, out_height, scaled_owned) = if let Some((ow, oh)) =
                self.hw_scale_target
            {
                if self.hw_scale_filter.is_none() {
                    let hw_frames_ctx = unsafe { (*decoded.as_ptr()).hw_frames_ctx };
                    if !hw_frames_ctx.is_null() {
                        match unsafe {
                            build_vaapi_scale_filter(
                                hw_frames_ctx,
                                width as i32,
                                height as i32,
                                hw_pix_fmt,
                                ow as i32,
                                oh as i32,
                            )
                        } {
                            Ok(f) => self.hw_scale_filter = Some(f),
                            Err(e) => eprintln!(
                                "[decoder_core] hwaccelスケールフィルタ構築失敗、原寸のままアップロード: {e}"
                            ),
                        }
                    }
                }
                match &self.hw_scale_filter {
                    Some(filter) => {
                        match unsafe { run_hw_scale_filter(filter, decoded.as_mut_ptr()) } {
                            Ok(scaled) => (scaled, ow, oh, Some(scaled)),
                            Err(e) => {
                                eprintln!(
                                    "[decoder_core] hwaccelスケール適用失敗、原寸のままアップロード: {e}"
                                );
                                (unsafe { decoded.as_mut_ptr() }, width, height, None)
                            }
                        }
                    }
                    None => (unsafe { decoded.as_mut_ptr() }, width, height, None),
                }
            } else {
                (unsafe { decoded.as_mut_ptr() }, width, height, None)
            };
            #[cfg(not(target_os = "linux"))]
            let (upload_ptr, out_width, out_height) =
                (unsafe { decoded.as_mut_ptr() }, width, height);

            let texture = unsafe {
                upload_hw_frame_as_texture(device, queue, upload_ptr, out_width, out_height)
            }
            .map_err(|e| format!("frame upload failed at index={display_index}: {e}"))?;

            #[cfg(target_os = "linux")]
            if let Some(mut scaled) = scaled_owned {
                unsafe { sys::av_frame_free(&mut scaled) };
            }

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
