use gst::prelude::*;
use gst_app::AppSink;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use neoutl_media_api::VideoSource;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Once;
use std::sync::mpsc;
use std::thread::JoinHandle;

/// NV12バイト列をNV12テクスチャへアップロードする。cache.rs::materialize撤去に伴い
/// このCPU系decoderクレート内へ複製移動。
fn upload_nv12(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &[u8],
    width: u32,
    height: u32,
) -> wgpu::Texture {
    let y_plane_size = (width * height) as usize;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("video-nv12-frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::NV12,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::Plane0,
        },
        &data[0..y_plane_size],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::Plane1,
        },
        &data[y_plane_size..],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height / 2),
        },
        wgpu::Extent3d {
            width: width / 2,
            height: height / 2,
            depth_or_array_layers: 1,
        },
    );
    texture
}

static GST_INIT: Once = Once::new();

fn ensure_gst_init() {
    GST_INIT.call_once(|| {
        gst::init().expect("gstreamer初期化失敗");
        register_bundled_plugin_dir();
        log_hardware_decoder_availability();
    });
}

fn log_hardware_decoder_availability() {
    let registry = gst::Registry::get();
    let candidates = [
        "vah264dec",
        "vah265dec",
        "vaapih264dec",
        "vaapih265dec",
        "v4l2h264dec",
        "v4l2h265dec",
        "nvh264dec",
        "nvh265dec",
        "d3d11h264dec",
        "d3d11h265dec",
    ];
    let found: Vec<&str> = candidates
        .iter()
        .copied()
        .filter(|name| {
            registry
                .find_feature(name, gst::ElementFactory::static_type())
                .is_some()
        })
        .collect();
    if found.is_empty() {
        eprintln!(
            "[gstreamer-decoder] ハードウェアH.264/HEVCデコーダ要素が未登録です。\
             VAAPI/V4L2/NVCODECいずれかのGStreamerプラグインパッケージを導入して下さい。"
        );
    } else {
        eprintln!("[gstreamer-decoder] ハードウェアデコーダ検出: {found:?}");
    }
}

#[cfg(target_os = "linux")]
fn register_bundled_plugin_dir() {}

#[cfg(not(target_os = "linux"))]
fn register_bundled_plugin_dir() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(exe_dir) = exe.parent() else {
        return;
    };

    #[cfg(target_os = "macos")]
    let plugin_dir = exe_dir.join("../Resources/gstreamer-1.0");
    #[cfg(target_os = "windows")]
    let plugin_dir = exe_dir.join("lib/gstreamer-1.0");

    if !plugin_dir.is_dir() {
        return;
    }
    gst::Registry::get().scan_path(&plugin_dir);
}

#[cfg(target_os = "linux")]
const DOWNLOAD_CHAIN: &str = "vapostproc ! ";
#[cfg(target_os = "windows")]
const DOWNLOAD_CHAIN: &str = "d3d11download ! ";
#[cfg(target_os = "macos")]
const DOWNLOAD_CHAIN: &str = "";

const SYSMEM_CAPS: &str = "video/x-raw,format=NV12";

fn duration_to_frames(duration_ns: u64, frame_duration_ns: u64) -> i64 {
    (duration_ns / frame_duration_ns.max(1)) as i64
}

fn wait_state(pipeline: &gst::Pipeline, timeout: gst::ClockTime) -> Result<(), String> {
    let (result, _, _) = pipeline.state(timeout);
    if result.is_ok() {
        return Ok(());
    }
    let Some(bus) = pipeline.bus() else {
        return Err("状態遷移失敗（バス未取得のため詳細不明）".to_owned());
    };
    if let Some(msg) = bus.timed_pop_filtered(
        gst::ClockTime::from_mseconds(500),
        &[gst::MessageType::Error],
    ) {
        if let gst::MessageView::Error(err) = msg.view() {
            let src = err
                .src()
                .map(|s| s.path_string().to_string())
                .unwrap_or_else(|| "不明".to_owned());
            return Err(format!(
                "状態遷移失敗: 要素={src} 理由={} 詳細={:?}",
                err.error(),
                err.debug()
            ));
        }
    }
    Err("状態遷移失敗（バスにERRORメッセージなし）".to_owned())
}

/// NV12バッファからCPU側バイト列を取り出す。
/// GPUへのアップロード(create_texture + write_texture)は行わず、
/// 呼び出し元(UIスレッド)が行う前提。デコードスレッドからwgpu::Queueを
/// 操作するとSurface::present()との競合でデッドロックするため分離している。
fn extract_nv12_bytes(buffer: &gst::BufferRef, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let map = buffer.map_readable().map_err(|e| e.to_string())?;
    let data = map.as_slice();

    let y_plane_size = (width * height) as usize;
    let uv_plane_size = (width * height / 2) as usize;
    eprintln!(
        "[gstreamer-decoder] extract_nv12_bytes: width={width} height={height} \
         data_len={} 必要バイト数={}",
        data.len(),
        y_plane_size + uv_plane_size
    );
    if data.len() < y_plane_size + uv_plane_size {
        let msg = format!(
            "NV12バッファサイズ不足: data_len={} 必要={}",
            data.len(),
            y_plane_size + uv_plane_size
        );
        eprintln!("[gstreamer-decoder] {msg}");
        return Err(msg);
    }
    Ok(data[..y_plane_size + uv_plane_size].to_vec())
}

/// GStreamer実体。command_thread専有。MainLoop駆動スレッドとは別スレッドで
/// sample_at（ブロッキング呼び出し）を実行するため、バスメッセージの
/// ディスパッチはmainloop_threadが並行して継続する。
struct GstDecoderInner {
    pipeline: gst::Pipeline,
    appsink: AppSink,
    width: u32,
    height: u32,
    fps: f64,
    frame_duration_ns: u64,
    total_frames: i64,
    /// 直近に配信したフレーム番号。次要求がlast_frame+1（連番再生・先読み）の場合、
    /// ACCURATEシークを省略しPLAYING状態での継続デコードへ切替える。
    /// 非連番（スクラブ・逆再生・シーク）検出時は-1へ戻さず、単に不一致として扱う。
    last_frame: i64,
}

impl GstDecoderInner {
    /// main_context: バスウォッチ登録先。登録自体はmain_context.invoke経由で
    /// mainloop_thread上で実行されるため、この関数はどのスレッドから呼んでもよい。
    fn open(path: &Path, main_context: &gst::glib::MainContext) -> Result<Self, String> {
        let uri = gst::glib::filename_to_uri(path, None).map_err(|e| e.to_string())?;
        let pipeline_desc = format!(
            "uridecodebin uri={uri} name=src \
             src. ! {DOWNLOAD_CHAIN}videoconvert ! appsink name=sink sync=false max-buffers=1 drop=true \
             src. ! fakesink name=drain sync=false async=false"
        );
        let pipeline = gst::parse::launch(&pipeline_desc)
            .map_err(|e| e.to_string())?
            .downcast::<gst::Pipeline>()
            .map_err(|_| "パイプライン構築失敗".to_owned())?;

        let appsink = pipeline
            .by_name("sink")
            .ok_or("appsink未検出")?
            .downcast::<AppSink>()
            .map_err(|_| "appsinkキャスト失敗".to_owned())?;
        appsink.set_caps(Some(
            &gst::Caps::from_str(SYSMEM_CAPS).map_err(|e| e.to_string())?,
        ));

        macro_rules! fail {
            ($err:expr) => {{
                let _ = pipeline.set_state(gst::State::Null);
                return Err($err);
            }};
        }

        // バスウォッチはpipeline状態遷移開始前にmainloop_thread上へ登録する。
        // add_watchはSendクロージャを要求する代わりに呼び出し元スレッドを問わないため、
        // main_context.invokeでmainloop_thread側へ登録処理自体を委譲できる。
        let bus = match pipeline.bus() {
            Some(b) => b,
            None => fail!("バス未取得".to_owned()),
        };
        main_context.invoke(move || {
            let _ = bus.add_watch(|_, msg| {
                let _ = msg;
                gst::glib::ControlFlow::Continue
            });
        });

        if let Err(e) = pipeline.set_state(gst::State::Paused) {
            fail!(e.to_string());
        }
        if let Err(e) = wait_state(&pipeline, gst::ClockTime::from_seconds(10)) {
            fail!(e);
        }

        let preroll = match appsink.pull_preroll() {
            Ok(p) => p,
            Err(e) => fail!(e.to_string()),
        };
        let caps = match preroll.caps() {
            Some(c) => c,
            None => fail!("caps未取得".to_owned()),
        };
        let video_info = match gst_video::VideoInfo::from_caps(caps) {
            Ok(v) => v,
            Err(e) => fail!(e.to_string()),
        };
        let width = video_info.width();
        let height = video_info.height();
        let fps_frac = video_info.fps();
        let fps = if fps_frac.denom() != 0 {
            fps_frac.numer() as f64 / fps_frac.denom() as f64
        } else {
            30.0
        };
        let frame_duration_ns = (1_000_000_000.0 / fps.max(1e-6)) as u64;

        let duration_ns = pipeline
            .query_duration::<gst::ClockTime>()
            .map(|d| d.nseconds())
            .unwrap_or(0);
        let total_frames = duration_to_frames(duration_ns, frame_duration_ns).max(1);

        eprintln!(
            "[gstreamer-decoder] open完了: caps={caps} width={width} height={height} \
             fps={fps} total_frames={total_frames} duration_ns={duration_ns}"
        );

        Ok(Self {
            pipeline,
            appsink,
            width,
            height,
            fps,
            frame_duration_ns,
            total_frames,
            last_frame: -1,
        })
    }

    /// 連番再生専用の高速経路。PLAYING状態のまま継続デコードさせ、
    /// bounded timeoutでappsinkから直接次フレームを取得する。
    /// シークを一切発行しないため、キーフレームからの再デコードが発生しない。
    fn sample_at_sequential(&mut self, target: i64) -> Option<gst::Sample> {
        if self.pipeline.current_state() != gst::State::Playing
            && let Err(e) = self.pipeline.set_state(gst::State::Playing)
        {
            eprintln!("[gstreamer-decoder] 連番再生パス: PLAYING遷移失敗 {e}");
            return None;
        }
        if wait_state(&self.pipeline, gst::ClockTime::from_seconds(2)).is_err() {
            eprintln!("[gstreamer-decoder] 連番再生パス: 状態遷移待機失敗");
            return None;
        }
        match self
            .appsink
            .try_pull_sample(gst::ClockTime::from_seconds(2))
        {
            Some(sample) => {
                eprintln!("[gstreamer-decoder] 連番再生パス成功: target={target}");
                self.last_frame = target;
                Some(sample)
            }
            None => {
                eprintln!("[gstreamer-decoder] 連番再生パス: サンプル取得タイムアウト");
                None
            }
        }
    }

    /// 非連番アクセス（スクラブ・逆再生・初回シーク）専用の正確シーク経路。
    /// PAUSED状態へ戻した上でACCURATEシークを行い、対象フレームを一意に確定する。
    fn sample_at_seek(&mut self, frame_index: i64, target: i64) -> Result<gst::Sample, String> {
        let target_ns = target as u64 * self.frame_duration_ns;
        eprintln!(
            "[gstreamer-decoder] sample_at正確シーク: frame_index={frame_index} target={target} target_ns={target_ns}"
        );
        if self.pipeline.current_state() != gst::State::Paused {
            let _ = self.pipeline.set_state(gst::State::Paused);
        }
        self.pipeline
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::ClockTime::from_nseconds(target_ns),
            )
            .map_err(|e| {
                let msg = e.to_string();
                eprintln!("[gstreamer-decoder] seek失敗: {msg}");
                msg
            })?;
        wait_state(&self.pipeline, gst::ClockTime::from_seconds(10)).map_err(|e| {
            eprintln!("[gstreamer-decoder] seek後の状態遷移失敗: {e}");
            e
        })?;
        let result = self
            .appsink
            .pull_preroll()
            .or_else(|_| self.appsink.pull_sample())
            .map_err(|e| e.to_string());
        match &result {
            Ok(sample) => {
                let buffer_size = sample.buffer().map(|b| b.size()).unwrap_or(0);
                let caps_str = sample
                    .caps()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "なし".to_owned());
                eprintln!(
                    "[gstreamer-decoder] sample取得成功: frame_index={frame_index} \
                     buffer_size={buffer_size} caps={caps_str}"
                );
                self.last_frame = target;
            }
            Err(e) => {
                eprintln!("[gstreamer-decoder] sample取得失敗: frame_index={frame_index} 理由={e}");
            }
        }
        result
    }

    fn sample_at(&mut self, frame_index: i64) -> Result<gst::Sample, String> {
        let target = frame_index.clamp(0, self.total_frames - 1);
        if target == self.last_frame + 1
            && let Some(sample) = self.sample_at_sequential(target)
        {
            return Ok(sample);
        }
        self.sample_at_seek(frame_index, target)
    }
}

enum Command {
    Frame {
        frame_index: i64,
        reply: mpsc::Sender<Result<Vec<u8>, String>>,
    },
    Shutdown,
}

/// UIスレッドが保持するハンドル。GStreamer実体は一切保持せず、
/// 全操作をコマンドチャネル経由でcommand_threadへ委譲する。
pub struct GstDecoder {
    width: u32,
    height: u32,
    fps: f64,
    total_frames: i64,
    tx: mpsc::Sender<Command>,
    mainloop_thread: Option<JoinHandle<()>>,
    command_thread: Option<JoinHandle<()>>,
    /// prefetchが取得したNV12バイト列。frame_gpuがここからテクスチャアップロードする。
    pending: HashMap<i64, Vec<u8>>,
}

impl GstDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        ensure_gst_init();
        let path: PathBuf = path.to_owned();

        let main_context = gst::glib::MainContext::new();
        let main_loop = gst::glib::MainLoop::new(Some(&main_context), false);

        // mainloop_thread: MainContextの所有スレッド。ここでのみ
        // with_thread_defaultを呼び、run()を占有的に駆動し続ける。
        // このスレッドはGstDecoderInnerを一切保持しない（完全分離）。
        let mainloop_thread = {
            let main_context = main_context.clone();
            let main_loop = main_loop.clone();
            std::thread::Builder::new()
                .name("gst-decoder-mainloop".to_owned())
                .spawn(move || {
                    main_context
                        .with_thread_default(|| {
                            main_loop.run();
                        })
                        .expect("MainContext設定失敗");
                })
                .map_err(|e| e.to_string())?
        };

        // open()自体はこの呼び出しスレッド（GstDecoder::open呼び出し元、
        // 通常はcommand_thread生成前の一時スレッド文脈）で実行する。
        // バスウォッチ登録はmain_context.invoke経由でmainloop_threadへ委譲済みのため、
        // wait_state内のブロッキング待機中もmainloop_threadがメッセージを処理できる。
        let inner = match GstDecoderInner::open(&path, &main_context) {
            Ok(inner) => inner,
            Err(e) => {
                main_loop.quit();
                let _ = mainloop_thread.join();
                return Err(e);
            }
        };

        let width = inner.width;
        let height = inner.height;
        let fps = inner.fps;
        let total_frames = inner.total_frames;

        let (tx, rx) = mpsc::channel::<Command>();

        // command_thread: GstDecoderInnerの所有スレッド。sample_at等の
        // ブロッキング呼び出しはここでのみ発生し、mainloop_threadには一切影響しない。
        let command_thread = {
            let main_loop = main_loop.clone();
            std::thread::Builder::new()
                .name("gst-decoder-command".to_owned())
                .spawn(move || {
                    let mut inner = inner;
                    eprintln!("[gstreamer-decoder] command_thread起動完了");
                    while let Ok(command) = rx.recv() {
                        match command {
                            Command::Frame {
                                frame_index,
                                reply,
                            } => {
                                eprintln!(
                                    "[gstreamer-decoder] command_thread: Frame受信 frame_index={frame_index}"
                                );
                                let result = inner.sample_at(frame_index).and_then(|sample| {
                                    let buffer = sample.buffer().ok_or("buffer未取得".to_owned())?;
                                    extract_nv12_bytes(buffer, inner.width, inner.height)
                                });
                                if let Err(e) = &result {
                                    eprintln!(
                                        "[gstreamer-decoder] command_thread: フレーム処理失敗 frame_index={frame_index} 理由={e}"
                                    );
                                }
                                let _ = reply.send(result);
                            }
                            Command::Shutdown => {
                                eprintln!("[gstreamer-decoder] command_thread: Shutdown受信");
                                break;
                            }
                        }
                    }
                    let _ = inner.pipeline.set_state(gst::State::Null);
                    main_loop.quit();
                    eprintln!("[gstreamer-decoder] command_thread終了");
                })
                .map_err(|e| e.to_string())?
        };

        Ok(Self {
            width,
            height,
            fps,
            total_frames,
            tx,
            mainloop_thread: Some(mainloop_thread),
            command_thread: Some(command_thread),
            pending: HashMap::new(),
        })
    }
}

impl VideoSource for GstDecoder {
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

    /// バックグラウンドスレッド専用。command_threadへNV12バイト列を要求し内部キューへ蓄積する。
    /// GPU操作なし。
    fn prefetch(&mut self, frame_index: i64) -> Result<(), String> {
        if self.pending.contains_key(&frame_index) {
            return Ok(());
        }
        eprintln!("[gstreamer-decoder] prefetch呼び出し: frame_index={frame_index}");
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::Frame {
                frame_index,
                reply: reply_tx,
            })
            .map_err(|e| {
                let msg = "command_thread終了済み".to_owned();
                eprintln!("[gstreamer-decoder] コマンド送信失敗: {e} ({msg})");
                msg
            })?;
        let bytes = reply_rx
            .recv()
            .map_err(|e| e.to_string())
            .and_then(|inner| inner)?;
        eprintln!(
            "[gstreamer-decoder] prefetch完了: frame_index={frame_index} bytes={}",
            bytes.len()
        );
        self.pending.insert(frame_index, bytes);
        Ok(())
    }

    /// UIスレッド専用。prefetch済みNV12バイト列をテクスチャへアップロードする。
    fn frame_gpu(
        &mut self,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        if !self.pending.contains_key(&frame_index) {
            self.prefetch(frame_index)?;
        }
        let bytes = self
            .pending
            .remove(&frame_index)
            .ok_or("対象フレーム未生成（prefetch未完了）".to_owned())?;
        Ok(upload_nv12(device, queue, &bytes, self.width, self.height))
    }
}

impl Drop for GstDecoder {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(command_thread) = self.command_thread.take() {
            let _ = command_thread.join();
        }
        if let Some(mainloop_thread) = self.mainloop_thread.take() {
            let _ = mainloop_thread.join();
        }
    }
}

// --- プラグインエントリ ---
// objects/effectsと同一規約: entry関数のみがdylib境界（extern "C"）を越え、
// VTable本体はホストと同一Cargo.lock・同一rustcで一括ビルドされる前提の素のRust型を保持する。
use neoutl_media_api::{EntryFn, MediaKind, MediaMeta, MediaVTable};

static EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "webm", "avi"];

static META: MediaMeta = MediaMeta {
    id: "neoutl.media.gstreamer",
    name: "GStreamer Video Decoder",
    kind: MediaKind::Video,
    extensions_ptr: EXTENSIONS.as_ptr(),
    extensions_len: EXTENSIONS.len(),
};
static VTABLE: std::sync::OnceLock<MediaVTable> = std::sync::OnceLock::new();

fn meta() -> &'static MediaMeta {
    &META
}

fn open_video(path: &std::path::Path) -> Result<Box<dyn neoutl_media_api::VideoSource>, String> {
    GstDecoder::open(path).map(|d| Box::new(d) as Box<dyn neoutl_media_api::VideoSource>)
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
