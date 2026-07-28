use gst::prelude::*;
use gst_app::AppSrc;
use gst_pbutils::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_pbutils as gst_pbutils;
use std::path::Path;
use std::str::FromStr;
use std::sync::Once;

static GST_INIT: Once = Once::new();

fn ensure_gst_init() {
    GST_INIT.call_once(|| {
        gst::init().expect("gstreamer初期化失敗");
    });
}

/// エクスポート出力プリセット。コンテナ/映像コーデック/音声コーデックの組み合わせを
/// GstEncodingProfileへ変換する。encodebin2はプロファイルから内部のエンコーダ・
/// マルチプレクサ要素選定、pad-template解決、mux前段のqueue挿入を自動で行うため、
/// 呼び出し側は「何を作りたいか」の宣言（プリセット）のみを持てばよい。
#[derive(Clone, Copy, Debug)]
pub enum ExportPreset {
    /// MP4コンテナ + H.264映像 + AAC音声。汎用配布・共有向け。
    Mp4H264Aac,
    /// WebMコンテナ + VP9映像 + Vorbis音声。Web埋め込み向け。
    WebmVp9Vorbis,
    /// QuickTimeコンテナ + ProRes映像（音声なし）。中間コーデック・再編集向け。
    MovProResNoAudio,
}

impl ExportPreset {
    fn container_caps(self) -> &'static str {
        match self {
            ExportPreset::Mp4H264Aac => "video/quicktime,variant=iso",
            ExportPreset::WebmVp9Vorbis => "video/webm",
            ExportPreset::MovProResNoAudio => "video/quicktime",
        }
    }
    fn video_caps(self) -> &'static str {
        match self {
            ExportPreset::Mp4H264Aac => "video/x-h264,profile=high",
            ExportPreset::WebmVp9Vorbis => "video/x-vp9",
            ExportPreset::MovProResNoAudio => "video/x-prores,variant=standard",
        }
    }
    /// Noneの場合、音声トラックを持たないプロファイルを構築する。
    fn audio_caps(self) -> Option<&'static str> {
        match self {
            ExportPreset::Mp4H264Aac => Some("audio/mpeg,mpegversion=4"),
            ExportPreset::WebmVp9Vorbis => Some("audio/x-vorbis"),
            ExportPreset::MovProResNoAudio => None,
        }
    }
}

/// GstEncodingContainerProfileを構築する。手動でのエンコーダ/マルチプレクサ要素選定、
/// x264enc/vp9enc/proresenc等の個別プロパティ設定、pad-template解決、
/// エンコーダ→マルチプレクサ間のqueue挿入は一切行わず、コンテナ/コーデックcapsの
/// 宣言のみをencodebin2へ渡す。
fn build_profile(preset: ExportPreset) -> Result<gst_pbutils::EncodingContainerProfile, String> {
    let container_caps =
        gst::Caps::from_str(preset.container_caps()).map_err(|e| e.to_string())?;
    let video_caps = gst::Caps::from_str(preset.video_caps()).map_err(|e| e.to_string())?;

    let video_profile = gst_pbutils::EncodingVideoProfile::builder(&video_caps)
        .presence(1)
        .build();

    let mut builder = gst_pbutils::EncodingContainerProfile::builder(&container_caps)
        .name("neoutl-export")
        .add_profile(video_profile);

    if let Some(audio_caps_str) = preset.audio_caps() {
        let audio_caps = gst::Caps::from_str(audio_caps_str).map_err(|e| e.to_string())?;
        let audio_profile = gst_pbutils::EncodingAudioProfile::builder(&audio_caps)
            .presence(1)
            .build();
        builder = builder.add_profile(audio_profile);
    }

    Ok(builder.build())
}

/// エクスポート対象映像の基本情報。
pub struct VideoFrameSource {
    pub width: u32,
    pub height: u32,
    pub fps_num: i32,
    pub fps_denom: i32,
    pub total_frames: i64,
}

/// フレームプロデューサ。frame_index順にRGBA8バイト列（width*height*4バイト）を
/// 返すコールバック。exportからの呼び出しスレッド上で同期的に実行される
/// （レンダラー側の合成・エフェクト適用結果を1フレームずつ取り出す想定）。
pub type FrameProducer<'a> = dyn FnMut(i64) -> Result<Vec<u8>, String> + 'a;

/// encodebin2 + GstEncodingProfileによる映像エクスポート。
/// appsrcへRGBA8フレームを順次push_bufferし、videoconvert経由でencodebin2へ渡す。
/// エンコーダ・マルチプレクサの選定、コーデックパラメータの既定値決定、
/// pad-template解決はencodebin2がprofileから解決するため、本関数は
/// 「フレーム供給」と「エラー・EOS監視」のみに責務を絞る。
///
/// 注記: encodebin2のリクエストパッド名("video_%u"/"audio_%u")はgst-plugins-baseの
/// バージョンにより変わりうる。導入先の`gst-inspect-1.0 encodebin2`出力と
/// 突き合わせて確認すること。
pub fn export(
    output_path: &Path,
    preset: ExportPreset,
    source: VideoFrameSource,
    mut produce_frame: Box<FrameProducer<'_>>,
) -> Result<(), String> {
    ensure_gst_init();

    if source.fps_num <= 0 || source.fps_denom <= 0 {
        return Err(format!(
            "不正なフレームレート: {}/{}",
            source.fps_num, source.fps_denom
        ));
    }

    let profile = build_profile(preset)?;

    let pipeline = gst::Pipeline::new();

    let src_caps = gst::Caps::builder("video/x-raw")
        .field("format", "RGBA")
        .field("width", source.width as i32)
        .field("height", source.height as i32)
        .field(
            "framerate",
            gst::Fraction::new(source.fps_num, source.fps_denom),
        )
        .build();

    let appsrc = AppSrc::builder()
        .caps(&src_caps)
        .format(gst::Format::Time)
        .is_live(false)
        .build();

    let videoconvert = gst::ElementFactory::make("videoconvert")
        .build()
        .map_err(|e| format!("videoconvert生成失敗: {e}"))?;
    let encodebin = gst::ElementFactory::make("encodebin2")
        .property("profile", &profile)
        .build()
        .map_err(|e| format!("encodebin2生成失敗: {e}"))?;
    let filesink = gst::ElementFactory::make("filesink")
        .property("location", output_path.to_string_lossy().as_ref())
        .build()
        .map_err(|e| format!("filesink生成失敗: {e}"))?;

    macro_rules! fail {
        ($err:expr) => {{
            let _ = pipeline.set_state(gst::State::Null);
            return Err($err);
        }};
    }

    pipeline
        .add_many([
            appsrc.upcast_ref::<gst::Element>(),
            &videoconvert,
            &encodebin,
            &filesink,
        ])
        .map_err(|e| e.to_string())?;

    if let Err(e) = gst::Element::link(appsrc.upcast_ref::<gst::Element>(), &videoconvert) {
        fail!(format!("appsrc -> videoconvertリンク失敗: {e}"));
    }
    if let Err(e) = gst::Element::link(&encodebin, &filesink) {
        fail!(format!("encodebin2 -> filesinkリンク失敗: {e}"));
    }

    let Some(video_sink_pad) = encodebin.request_pad_simple("video_%u") else {
        fail!("encodebin2: 映像シンクパッド要求失敗（profile不整合の可能性）".to_owned());
    };
    let Some(convert_src_pad) = videoconvert.static_pad("src") else {
        fail!("videoconvert srcパッド未取得".to_owned());
    };
    if let Err(e) = convert_src_pad.link(&video_sink_pad) {
        fail!(format!("videoconvert -> encodebin2リンク失敗: {e:?}"));
    }

    if let Err(e) = pipeline.set_state(gst::State::Playing) {
        fail!(format!("PLAYING遷移失敗: {e}"));
    }

    let frame_duration_ns =
        1_000_000_000u64 * source.fps_denom as u64 / source.fps_num as u64;

    for frame_index in 0..source.total_frames {
        let bytes = match produce_frame(frame_index) {
            Ok(b) => b,
            Err(e) => fail!(format!("フレーム{frame_index}生成失敗: {e}")),
        };
        let expected_len = (source.width * source.height * 4) as usize;
        if bytes.len() != expected_len {
            fail!(format!(
                "フレーム{frame_index}のバイト長不一致: 期待={expected_len} 実際={}",
                bytes.len()
            ));
        }

        let mut buffer = gst::Buffer::from_slice(bytes);
        {
            let Some(buffer_mut) = buffer.get_mut() else {
                fail!("バッファ可変参照取得失敗".to_owned());
            };
            buffer_mut.set_pts(gst::ClockTime::from_nseconds(
                frame_index as u64 * frame_duration_ns,
            ));
            buffer_mut.set_duration(gst::ClockTime::from_nseconds(frame_duration_ns));
        }
        if let Err(e) = appsrc.push_buffer(buffer) {
            fail!(format!("appsrc push失敗(frame={frame_index}): {e:?}"));
        }
    }

    if let Err(e) = appsrc.end_of_stream() {
        fail!(format!("EOS送出失敗: {e:?}"));
    }

    let Some(bus) = pipeline.bus() else {
        fail!("バス未取得".to_owned());
    };
    let mut encode_error: Option<String> = None;
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            gst::MessageView::Eos(_) => break,
            gst::MessageView::Error(err) => {
                let src = err
                    .src()
                    .map(|s| s.path_string().to_string())
                    .unwrap_or_else(|| "不明".to_owned());
                encode_error = Some(format!(
                    "エンコード中にエラー: 要素={src} 理由={} 詳細={:?}",
                    err.error(),
                    err.debug()
                ));
                break;
            }
            _ => {}
        }
    }

    let _ = pipeline.set_state(gst::State::Null);

    if let Some(e) = encode_error {
        return Err(e);
    }
    Ok(())
}
