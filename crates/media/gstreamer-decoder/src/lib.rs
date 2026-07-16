use gst::prelude::*;
use gst_app::AppSink;
use gstreamer as gst;
use gstreamer_app as gst_app;
use neoutl_media_api::VideoSource;
use std::path::Path;
use std::sync::Once;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux::import_frame;
#[cfg(target_os = "macos")]
use macos::import_frame;
#[cfg(target_os = "windows")]
use windows::import_frame;

static GST_INIT: Once = Once::new();

fn ensure_gst_init() {
    GST_INIT.call_once(|| {
        gst::init().expect("gstreamer初期化失敗");
        register_bundled_plugin_dir();
        log_hardware_decoder_availability();
    });
}

/// H.264/HEVC等をDMABuf/D3D11出力できるハードウェアデコーダ要素の有無を起動時に一度だけ検査する。
/// ゼロコピーパイプラインは該当要素が存在しない環境では原理的に成立しない
/// （ソフトウェアデコーダはシステムメモリしか出力できないため）。
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
             DMABuf/D3D11ゼロコピー出力は不可能なため動画プレビューは失敗します。\
             VAAPI/V4L2/NVCODECいずれかのGStreamerプラグインパッケージを導入し、\
             `gst-inspect-1.0 --gst-disable-registry-fork` 等でレジストリ再構築を確認して下さい。"
        );
    } else {
        eprintln!("[gstreamer-decoder] ハードウェアデコーダ検出: {found:?}");
    }
}

/// state()の失敗時、バスからERRORメッセージを取り出し具体的な失敗要素・理由を返す。
/// map_err(|e| e.to_string())単体では"Element failed to change its state"としか出ず、
/// どの要素が何故失敗したか特定できないため、ここで詳細化する。
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
    Err(
        "状態遷移失敗（バスにERRORメッセージなし。DMABuf/D3D11出力に対応するデコーダ要素が\
         選択されていない可能性が高い）"
            .to_owned(),
    )
}

/// アプリに同梱されたGStreamerプラグインの実行ファイル相対パスを、
/// システムのプラグインパスに加えてRegistryへ追加登録する。
/// Linuxは配布物にGStreamerプラグインを同梱していない（システムのapt版に依存）ため何もしない。
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
const ZEROCOPY_CAPS: &str = "video/x-raw(memory:DMABufMemory),format=NV12";
#[cfg(target_os = "windows")]
const ZEROCOPY_CAPS: &str = "video/x-raw(memory:D3D11Memory),format=NV12";
#[cfg(target_os = "macos")]
const ZEROCOPY_CAPS: &str = "video/x-raw(memory:GLMemory),format=NV12";

/// デコーダ要素は種別固有のメモリ（VAMemory/D3D11Memory等）で出力するため、
/// appsinkが要求する最終メモリ種別へ明示変換する要素が必要。
/// decodebinの自動プラグイン機構はメモリ種別変換要素までは自動挿入しないため、
/// パイプライン記述に固定で挟み込む（末尾に" ! "を含む／不要環境では空文字）。
/// vapostproc: gst-plugin-va（VAMemory → DMABufMemory）
#[cfg(target_os = "linux")]
const POSTPROC_CHAIN: &str = "vapostproc ! ";
#[cfg(not(target_os = "linux"))]
const POSTPROC_CHAIN: &str = "";

pub struct GstZeroCopyDecoder {
    pipeline: gst::Pipeline,
    appsink: AppSink,
    width: u32,
    height: u32,
    fps: f64,
    frame_duration_ns: u64,
    total_frames: i64,
}

fn duration_to_frames(duration_ns: u64, frame_duration_ns: u64) -> i64 {
    (duration_ns / frame_duration_ns.max(1)) as i64
}

impl GstZeroCopyDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        ensure_gst_init();

        let uri = gst::glib::filename_to_uri(path, None).map_err(|e| e.to_string())?;
        // uridecodebinは音声・動画等ソース内の全ストリームを動的ペインとしてexposeする。
        // 動画ペインのみを"src. ! ..."へ静的リンクすると、音声等の残りペインが未接続のまま
        // 残り、qtdemux内部ループがNOT_LINKEDでストリームエラーを起こし全体が失敗する。
        // "src."を複数回参照し、残りペインはfakesinkで消費して未リンク状態を解消する。
        let pipeline_desc = format!(
            "uridecodebin uri={uri} name=src \
             src. ! {POSTPROC_CHAIN}appsink name=sink sync=false max-buffers=1 drop=true \
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
            &gst::Caps::from_str(ZEROCOPY_CAPS).map_err(|e| e.to_string())?,
        ));

        // 以降の早期returnはすべてこのマクロ経由とし、Pipelineを必ずNULLへ戻してから
        // Errを返す。素のpipeline変数がNULLに戻らず関数を抜けるとGStreamerが
        // "disposing element in READY state" CRITICALを出し、内部スレッド/fdがリークする。
        macro_rules! fail {
            ($err:expr) => {{
                let _ = pipeline.set_state(gst::State::Null);
                return Err($err);
            }};
        }

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

        Ok(Self {
            pipeline,
            appsink,
            width,
            height,
            fps,
            frame_duration_ns,
            total_frames,
        })
    }

    fn sample_at(&mut self, frame_index: i64) -> Result<gst::Sample, String> {
        let target_ns = frame_index.max(0) as u64 * self.frame_duration_ns;
        self.pipeline
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::ClockTime::from_nseconds(target_ns),
            )
            .map_err(|e| e.to_string())?;
        wait_state(&self.pipeline, gst::ClockTime::from_seconds(10))?;
        self.appsink
            .pull_preroll()
            .or_else(|_| self.appsink.pull_sample())
            .map_err(|e| e.to_string())
    }
}

impl VideoSource for GstZeroCopyDecoder {
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

    fn frame_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame_index: i64,
    ) -> Result<wgpu::Texture, String> {
        let target = frame_index.clamp(0, self.total_frames - 1);
        let sample = self.sample_at(target)?;
        let buffer = sample.buffer().ok_or("buffer未取得")?;
        unsafe { import_frame(device, queue, buffer, self.width, self.height) }
    }
}

impl Drop for GstZeroCopyDecoder {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

use gstreamer_video as gst_video;
use std::str::FromStr;
