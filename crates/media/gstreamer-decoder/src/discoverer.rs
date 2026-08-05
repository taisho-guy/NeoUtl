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
        .map_err(|e| t!("Discoverer解析失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;

    let video_streams = info.video_streams();
    let video_stream = video_streams.first().ok_or_else(|| {
        t!(
            "Discoverer: 映像ストリームが検出されませんでした: %{arg0}",
            arg0 = format!("{}", path.display())
        )
        .to_string()
    })?;

    let width = video_stream.width();
    let height = video_stream.height();
    if width == 0 || height == 0 {
        return Err(t!(
            "Discoverer: 映像寸法が不正 (width=%{arg0} height=%{arg1}): %{arg2}",
            arg0 = format!("{width}"),
            arg1 = format!("{height}"),
            arg2 = format!("{}", path.display())
        )
        .to_string());
    }

    let framerate = video_stream.framerate();
    let fps_num = framerate.numer();
    let fps_denom = framerate.denom();
    let fps = if fps_denom != 0 {
        fps_num as f64 / fps_denom as f64
    } else {
        eprintln!(
            "{}",
            t!(
                "[gstreamer-decoder] Discoverer: フレームレート未申告のためフォールバック値30.0を使用（VFR疑い）: %{arg0}",
                arg0 = format!("{}", path.display())
            )
        );
        30.0
    };

    let duration_ns = info.duration().map(|d| d.nseconds()).unwrap_or(0);
    let seekable = info.is_seekable();
    let has_audio = !info.audio_streams().is_empty();

    eprintln!(
        "{}",
        t!(
            "[gstreamer-decoder] Discoverer解析完了: width=%{arg0} height=%{arg1} fps=%{arg2} duration_ns=%{arg3} seekable=%{arg4} has_audio=%{arg5}: %{arg6}",
            arg0 = format!("{width}"),
            arg1 = format!("{height}"),
            arg2 = format!("{fps}"),
            arg3 = format!("{duration_ns}"),
            arg4 = format!("{seekable}"),
            arg5 = format!("{has_audio}"),
            arg6 = format!("{}", path.display())
        )
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
