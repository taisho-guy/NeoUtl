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
    });
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
        let pipeline_desc = format!(
            "uridecodebin uri={uri} name=src ! appsink name=sink sync=false max-buffers=1 drop=true"
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

        pipeline
            .set_state(gst::State::Paused)
            .map_err(|e| e.to_string())?;
        pipeline
            .state(gst::ClockTime::from_seconds(10))
            .0
            .map_err(|e| e.to_string())?;

        let preroll = appsink.pull_preroll().map_err(|e| e.to_string())?;
        let caps = preroll.caps().ok_or("caps未取得")?;
        let video_info = gst_video::VideoInfo::from_caps(caps).map_err(|e| e.to_string())?;
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
        self.pipeline
            .state(gst::ClockTime::from_seconds(10))
            .0
            .map_err(|e| e.to_string())?;
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
