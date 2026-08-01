use gstreamer as gst;
use gstreamer_pbutils as gst_pbutils;
use std::path::Path;

/// GstDiscovererによるメディアファイルのストリーム情報取得結果。
/// GstDecoderInner::open()内でパイプライン構築・PAUSED遷移・preroll待機を行う前に
/// この関数を呼び、width/height/fps/duration/seekableを確定させる。
/// videoconvert通過後のpreroll capsから間接的に幅・高さを推定していた旧実装と異なり、
/// コンテナのストリーム記述を直接読む値であり、パイプライン構築失敗時にも
/// 原因（映像ストリーム欠如等）を早期に特定できる。
pub struct DiscoveredVideo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_ns: u64,
    pub seekable: bool,
    pub has_audio: bool,
}

/// discoverer本体の解析タイムアウト。ネットワークURI等、解析に時間を要する
/// ソースを想定し、appsink prerollのPULL_TIMEOUT(10秒)より長めに設定する。
const DISCOVER_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(15);

pub fn discover(path: &Path) -> Result<DiscoveredVideo, String> {
    let uri = gst::glib::filename_to_uri(path, None).map_err(|e| e.to_string())?;

    let discoverer = gst_pbutils::Discoverer::new(DISCOVER_TIMEOUT).map_err(|e| e.to_string())?;
    let info = discoverer
        .discover_uri(&uri)
        .map_err(|e| format!("Discoverer解析失敗: {e}"))?;

    let video_streams = info.video_streams();
    let video_stream = video_streams.first().ok_or_else(|| {
        format!(
            "Discoverer: 映像ストリームが検出されませんでした: {}",
            path.display()
        )
    })?;

    let width = video_stream.width();
    let height = video_stream.height();
    if width == 0 || height == 0 {
        return Err(format!(
            "Discoverer: 映像寸法が不正 (width={width} height={height}): {}",
            path.display()
        ));
    }

    let framerate = video_stream.framerate();
    let fps_num = framerate.numer();
    let fps_denom = framerate.denom();
    let fps = if fps_denom != 0 {
        fps_num as f64 / fps_denom as f64
    } else {
        eprintln!(
            "[gstreamer-decoder] Discoverer: フレームレート未申告のためフォールバック値30.0を使用（VFR疑い）: {}",
            path.display()
        );
        30.0
    };

    let duration_ns = info.duration().map(|d| d.nseconds()).unwrap_or(0);
    let seekable = info.is_seekable();
    let has_audio = !info.audio_streams().is_empty();

    eprintln!(
        "[gstreamer-decoder] Discoverer解析完了: width={width} height={height} fps={fps} \
         duration_ns={duration_ns} seekable={seekable} has_audio={has_audio}: {}",
        path.display()
    );

    Ok(DiscoveredVideo {
        width,
        height,
        fps,
        duration_ns,
        seekable,
        has_audio,
    })
}
