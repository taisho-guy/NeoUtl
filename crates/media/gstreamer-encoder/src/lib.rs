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
        gst::init().expect(&t!("gstreamer初期化失敗"));
        log_hardware_encoder_availability();
    });
}

/// アプリ起動時にバックグラウンドスレッドから呼び、gst::init()（プラグインレジストリ
/// 全走査。初回のみ数秒〜十数秒規模）を書き出し操作より前に完了させる。
/// export()/mux_encoded()呼び出し時点で未完了ならそちらのensure_gst_init()が
/// 通常どおりブロックして待つため、呼び忘れても機能的には安全（体感速度のみの差）。
pub fn warm_up() {
    ensure_gst_init();
}

fn log_hardware_encoder_availability() {
    let registry = gst::Registry::get();
    let candidates = [
        "vah264enc",
        "vah265enc",
        "vaapih264enc",
        "vaapih265enc",
        "v4l2h264enc",
        "v4l2h265enc",
        "nvh264enc",
        "nvh265enc",
        "d3d11h264enc",
        "d3d11h265enc",
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
            "{}",
            t!(
                "[gstreamer-encoder] ハードウェアH.264/HEVCエンコーダ要素が未登録です。ソフトウェアエンコーダ(x264enc/x265enc)へ縮退します。"
            )
        );
    } else {
        eprintln!(
            "{}",
            t!(
                "[gstreamer-encoder] ハードウェアエンコーダ検出: %{arg0}",
                arg0 = format!("{found:?}")
            )
        );
    }
}

/// エクスポート出力プリセット。コンテナ/映像コーデック/音声コーデックの組み合わせを
/// GstEncodingProfileへ変換する。encodebin2はプロファイルから内部のエンコーダ・
/// マルチプレクサ要素選定、pad-template解決、mux前段のqueue挿入を自動で行うため、
/// 呼び出し側は「何を作りたいか」の宣言（プリセット）のみを持てばよい。
#[derive(Clone, Copy, Debug)]
pub enum ExportPreset {
    /// MP4コンテナ + H.264映像 + AAC音声。汎用配布・共有向け。
    Mp4H264Aac,
    /// MP4コンテナ + H.265映像 + AAC音声。高圧縮・高解像度向け。
    Mp4H265Aac,
    /// MKVコンテナ + H.264映像 + Opus音声。
    MkvH264Opus,
    /// MKVコンテナ + H.265映像 + Opus音声。
    MkvH265Opus,
    /// WebMコンテナ + VP9映像 + Vorbis音声。Web埋め込み向け。
    WebmVp9Vorbis,
    /// QuickTimeコンテナ + ProRes映像（音声なし）。中間コーデック・再編集向け。
    MovProResNoAudio,
}

impl ExportPreset {
    fn container_caps(self) -> &'static str {
        match self {
            ExportPreset::Mp4H264Aac | ExportPreset::Mp4H265Aac => "video/quicktime,variant=iso",
            ExportPreset::MkvH264Opus | ExportPreset::MkvH265Opus => "video/x-matroska",
            ExportPreset::WebmVp9Vorbis => "video/webm",
            ExportPreset::MovProResNoAudio => "video/quicktime",
        }
    }
    fn video_caps(self) -> &'static str {
        match self {
            ExportPreset::Mp4H264Aac | ExportPreset::MkvH264Opus => "video/x-h264,profile=high",
            ExportPreset::Mp4H265Aac | ExportPreset::MkvH265Opus => "video/x-h265,profile=main",
            ExportPreset::WebmVp9Vorbis => "video/x-vp9",
            ExportPreset::MovProResNoAudio => "video/x-prores,variant=standard",
        }
    }
    /// Noneの場合、音声トラックを持たないプロファイルを構築する。
    fn audio_caps(self) -> Option<&'static str> {
        match self {
            ExportPreset::Mp4H264Aac | ExportPreset::Mp4H265Aac => Some("audio/mpeg,mpegversion=4"),
            ExportPreset::MkvH264Opus | ExportPreset::MkvH265Opus => Some("audio/x-opus"),
            ExportPreset::WebmVp9Vorbis => Some("audio/x-vorbis"),
            ExportPreset::MovProResNoAudio => None,
        }
    }

    /// video_caps()の要素がHW/SWいずれの実装を持つかをレジストリから確認する。
    /// encodebin2自体はランク上位（通常HW優先登録時はHWエレメント）を自動選択するため、
    /// 本関数は選択結果のログ出力・診断用途に限る。
    pub fn preferred_encoder_element(self, video_caps: &gst::Caps) -> Option<String> {
        gst::ElementFactory::factories_with_type(
            gst::ElementFactoryType::ENCODER,
            gst::Rank::MARGINAL,
        )
        .into_iter()
        .filter(|f| {
            f.static_pad_templates().iter().any(|t| {
                t.direction() == gst::PadDirection::Src && t.caps().can_intersect(video_caps)
            })
        })
        .max_by_key(|f| f.rank())
        .map(|f| f.name().to_string())
    }
}

/// GstEncodingContainerProfileを構築する。手動でのエンコーダ/マルチプレクサ要素選定、
/// x264enc/vp9enc/proresenc等の個別プロパティ設定、pad-template解決、
/// エンコーダ→マルチプレクサ間のqueue挿入は一切行わず、コンテナ/コーデックcapsの
/// 宣言のみをencodebin2へ渡す。
fn build_profile(preset: ExportPreset) -> Result<gst_pbutils::EncodingContainerProfile, String> {
    let container_caps = gst::Caps::from_str(preset.container_caps()).map_err(|e| e.to_string())?;
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

/// エクスポート対象音声の基本情報。Noneの場合、音声トラックなしのプロファイルを要求する
/// （ExportPreset::MovProResNoAudio等）前提でexport()呼び出し側がpresetと整合させること。
pub struct AudioFrameSource {
    pub sample_rate: u32,
    pub channels: u16,
}

/// フレームプロデューサ。frame_index順にRGBA8バイト列（width*height*4バイト）を
/// 返すコールバック。exportからの呼び出しスレッド上で同期的に実行される
/// （レンダラー側の合成・エフェクト適用結果を1フレームずつ取り出す想定）。
pub type FrameProducer<'a> = dyn FnMut(i64) -> Result<Vec<u8>, String> + 'a;

/// 音声プロデューサ。frame_indexに対応する1ビデオフレーム分のインターリーブf32
/// サンプル列（sample_count*channels要素）を返すコールバック。
pub type AudioProducer<'a> = dyn FnMut(i64, usize) -> Vec<f32> + 'a;

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
    audio: Option<AudioFrameSource>,
    mut produce_frame: Box<FrameProducer<'_>>,
    mut produce_audio: Option<Box<AudioProducer<'_>>>,
) -> Result<(), String> {
    ensure_gst_init();

    if source.fps_num <= 0 || source.fps_denom <= 0 {
        return Err(t!(
            "不正なフレームレート: %{arg0}/%{arg1}",
            arg0 = format!("{}", source.fps_num),
            arg1 = format!("{}", source.fps_denom)
        )
        .to_string());
    }
    if audio.is_some() != preset.audio_caps().is_some() {
        return Err("presetの音声有無とaudio引数の有無が不一致".to_owned());
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
        .map_err(|e| t!("videoconvert生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
    let encodebin = gst::ElementFactory::make("encodebin2")
        .property("profile", &profile)
        .build()
        .map_err(|e| t!("encodebin2生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
    let filesink = gst::ElementFactory::make("filesink")
        .property("location", output_path.to_string_lossy().as_ref())
        .build()
        .map_err(|e| t!("filesink生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;

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
        fail!(
            t!(
                "appsrc -> videoconvertリンク失敗: %{arg0}",
                arg0 = format!("{e}")
            )
            .to_string()
        );
    }
    if let Err(e) = gst::Element::link(&encodebin, &filesink) {
        fail!(
            t!(
                "encodebin2 -> filesinkリンク失敗: %{arg0}",
                arg0 = format!("{e}")
            )
            .to_string()
        );
    }

    let Some(video_sink_pad) = encodebin.request_pad_simple("video_%u") else {
        fail!("encodebin2: 映像シンクパッド要求失敗（profile不整合の可能性）".to_owned());
    };
    let Some(convert_src_pad) = videoconvert.static_pad("src") else {
        fail!("videoconvert srcパッド未取得".to_owned());
    };
    if let Err(e) = convert_src_pad.link(&video_sink_pad) {
        fail!(
            t!(
                "videoconvert -> encodebin2リンク失敗: %{arg0}",
                arg0 = format!("{e:?}")
            )
            .to_string()
        );
    }

    let audio_appsrc = if let Some(audio) = &audio {
        let audio_caps = gst::Caps::builder("audio/x-raw")
            .field("format", "F32LE")
            .field("layout", "interleaved")
            .field("rate", audio.sample_rate as i32)
            .field("channels", audio.channels as i32)
            .build();
        let audio_appsrc = AppSrc::builder()
            .caps(&audio_caps)
            .format(gst::Format::Time)
            .is_live(false)
            .build();
        let audioconvert = gst::ElementFactory::make("audioconvert")
            .build()
            .map_err(|e| t!("audioconvert生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
        let audioresample = gst::ElementFactory::make("audioresample")
            .build()
            .map_err(|e| t!("audioresample生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
        pipeline
            .add_many([
                audio_appsrc.upcast_ref::<gst::Element>(),
                &audioconvert,
                &audioresample,
            ])
            .map_err(|e| e.to_string())?;
        if let Err(e) = gst::Element::link_many([
            audio_appsrc.upcast_ref::<gst::Element>(),
            &audioconvert,
            &audioresample,
        ]) {
            fail!(t!("音声チェーンリンク失敗: %{arg0}", arg0 = format!("{e}")).to_string());
        }
        let Some(audio_sink_pad) = encodebin.request_pad_simple("audio_%u") else {
            fail!("encodebin2: 音声シンクパッド要求失敗".to_owned());
        };
        let Some(resample_src_pad) = audioresample.static_pad("src") else {
            fail!("audioresample srcパッド未取得".to_owned());
        };
        if let Err(e) = resample_src_pad.link(&audio_sink_pad) {
            fail!(
                t!(
                    "audioresample -> encodebin2リンク失敗: %{arg0}",
                    arg0 = format!("{e:?}")
                )
                .to_string()
            );
        }
        Some(audio_appsrc)
    } else {
        None
    };

    if let Err(e) = pipeline.set_state(gst::State::Playing) {
        fail!(t!("PLAYING遷移失敗: %{arg0}", arg0 = format!("{e}")).to_string());
    }

    let frame_duration_ns = 1_000_000_000u64 * source.fps_denom as u64 / source.fps_num as u64;
    let samples_per_frame = audio
        .as_ref()
        .map(|a| (a.sample_rate as u64 * source.fps_denom as u64 / source.fps_num as u64) as usize)
        .unwrap_or(0);

    for frame_index in 0..source.total_frames {
        let bytes = match produce_frame(frame_index) {
            Ok(b) => b,
            Err(e) => fail!(
                t!(
                    "フレーム%{arg0}生成失敗: %{arg1}",
                    arg0 = format!("{frame_index}"),
                    arg1 = format!("{e}")
                )
                .to_string()
            ),
        };
        let expected_len = (source.width * source.height * 4) as usize;
        if bytes.len() != expected_len {
            fail!(
                t!(
                    "フレーム%{arg0}のバイト長不一致: 期待=%{arg1} 実際=%{arg2}",
                    arg0 = format!("{frame_index}"),
                    arg1 = format!("{expected_len}"),
                    arg2 = format!("{}", bytes.len())
                )
                .to_string()
            );
        }

        let pts = gst::ClockTime::from_nseconds(frame_index as u64 * frame_duration_ns);
        let mut buffer = gst::Buffer::from_slice(bytes);
        {
            let Some(buffer_mut) = buffer.get_mut() else {
                fail!("バッファ可変参照取得失敗".to_owned());
            };
            buffer_mut.set_pts(pts);
            buffer_mut.set_duration(gst::ClockTime::from_nseconds(frame_duration_ns));
        }
        if let Err(e) = appsrc.push_buffer(buffer) {
            fail!(
                t!(
                    "appsrc push失敗(frame=%{arg0}): %{arg1}",
                    arg0 = format!("{frame_index}"),
                    arg1 = format!("{e:?}")
                )
                .to_string()
            );
        }

        if let (Some(audio_appsrc), Some(produce_audio)) =
            (audio_appsrc.as_ref(), produce_audio.as_mut())
        {
            let samples = produce_audio(frame_index, samples_per_frame);
            let mut audio_buffer = gst::Buffer::from_slice(bytemuck::cast_slice(&samples).to_vec());
            {
                let Some(buffer_mut) = audio_buffer.get_mut() else {
                    fail!("音声バッファ可変参照取得失敗".to_owned());
                };
                buffer_mut.set_pts(pts);
                buffer_mut.set_duration(gst::ClockTime::from_nseconds(frame_duration_ns));
            }
            if let Err(e) = audio_appsrc.push_buffer(audio_buffer) {
                fail!(
                    t!(
                        "音声appsrc push失敗(frame=%{arg0}): %{arg1}",
                        arg0 = format!("{frame_index}"),
                        arg1 = format!("{e:?}")
                    )
                    .to_string()
                );
            }
        }
    }

    if let Err(e) = appsrc.end_of_stream() {
        fail!(t!("EOS送出失敗: %{arg0}", arg0 = format!("{e:?}")).to_string());
    }
    if let Some(audio_appsrc) = &audio_appsrc {
        if let Err(e) = audio_appsrc.end_of_stream() {
            fail!(t!("音声EOS送出失敗: %{arg0}", arg0 = format!("{e:?}")).to_string());
        }
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
                    .unwrap_or_else(|| t!("不明").to_string());
                encode_error = Some(
                    t!(
                        "エンコード中にエラー: 要素=%{arg0} 理由=%{arg1} 詳細=%{arg2}",
                        arg0 = format!("{src}"),
                        arg1 = format!("{}", err.error()),
                        arg2 = format!("{:?}", err.debug())
                    )
                    .to_string(),
                );
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

/// mux先コンテナ。gpuvideo-encoder(Vulkan HW)が生成済みのAnnexBビットストリームを
/// 再エンコード無しで格納する。
#[derive(Clone, Copy, Debug)]
pub enum MuxContainer {
    Mp4,
    Mkv,
}

impl MuxContainer {
    fn muxer_element(self) -> &'static str {
        match self {
            MuxContainer::Mp4 => "mp4mux",
            MuxContainer::Mkv => "matroskamux",
        }
    }
}

/// neoutl_media_api::VideoCodecの1エンコード済みチャンクをmuxへ供給するプロデューサ。
/// 呼び出し側(export.rs)がgpuvideo-encoder::VideoEncoder::encode_rgbaの戻り値を
/// そのままこの型へ渡す。
pub type EncodedChunkProducer<'a> =
    dyn FnMut() -> Result<Option<(Vec<u8>, i64, bool)>, String> + 'a;

/// 事前エンコード済みH.264/H.265ビットストリーム(AnnexB, stream-format=byte-stream)を
/// 再エンコード無しでmuxし、任意でPCM音声(F32LE interleaved)を合流させる。
/// gpuvideo-encoder(Vulkan HW)経路の出力先として使う。
pub fn mux_encoded(
    output_path: &Path,
    container: MuxContainer,
    codec: neoutl_media_api::VideoCodec,
    mut produce_video: Box<EncodedChunkProducer<'_>>,
    audio: Option<AudioFrameSource>,
    total_frames: i64,
    fps_num: i32,
    fps_denom: i32,
    mut produce_audio: Option<Box<AudioProducer<'_>>>,
) -> Result<(), String> {
    ensure_gst_init();

    let video_caps_str = match codec {
        neoutl_media_api::VideoCodec::H264 => "video/x-h264,stream-format=byte-stream,alignment=au",
        neoutl_media_api::VideoCodec::H265 => "video/x-h265,stream-format=byte-stream,alignment=au",
    };
    let video_caps = gst::Caps::from_str(video_caps_str).map_err(|e| e.to_string())?;

    let pipeline = gst::Pipeline::new();
    let appsrc = AppSrc::builder()
        .caps(&video_caps)
        .format(gst::Format::Time)
        .is_live(false)
        .build();
    let parser = gst::ElementFactory::make(match codec {
        neoutl_media_api::VideoCodec::H264 => "h264parse",
        neoutl_media_api::VideoCodec::H265 => "h265parse",
    })
    .build()
    .map_err(|e| t!("パーサ生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
    let muxer = gst::ElementFactory::make(container.muxer_element())
        .build()
        .map_err(|e| t!("mux要素生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
    let filesink = gst::ElementFactory::make("filesink")
        .property("location", output_path.to_string_lossy().as_ref())
        .build()
        .map_err(|e| t!("filesink生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;

    macro_rules! fail {
        ($err:expr) => {{
            let _ = pipeline.set_state(gst::State::Null);
            return Err($err);
        }};
    }

    pipeline
        .add_many([
            appsrc.upcast_ref::<gst::Element>(),
            &parser,
            &muxer,
            &filesink,
        ])
        .map_err(|e| e.to_string())?;
    if let Err(e) = gst::Element::link_many([appsrc.upcast_ref::<gst::Element>(), &parser, &muxer])
    {
        fail!(
            t!(
                "appsrc -> parser -> muxリンク失敗: %{arg0}",
                arg0 = format!("{e}")
            )
            .to_string()
        );
    }
    if let Err(e) = gst::Element::link(&muxer, &filesink) {
        fail!(t!("mux -> filesinkリンク失敗: %{arg0}", arg0 = format!("{e}")).to_string());
    }

    let audio_appsrc = if let Some(audio) = &audio {
        let audio_caps = gst::Caps::builder("audio/x-raw")
            .field("format", "F32LE")
            .field("layout", "interleaved")
            .field("rate", audio.sample_rate as i32)
            .field("channels", audio.channels as i32)
            .build();
        let audio_appsrc = AppSrc::builder()
            .caps(&audio_caps)
            .format(gst::Format::Time)
            .is_live(false)
            .build();
        let audioconvert = gst::ElementFactory::make("audioconvert")
            .build()
            .map_err(|e| t!("audioconvert生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
        let audioresample = gst::ElementFactory::make("audioresample")
            .build()
            .map_err(|e| t!("audioresample生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
        let opusenc = gst::ElementFactory::make(match container {
            MuxContainer::Mp4 => "avenc_aac",
            MuxContainer::Mkv => "opusenc",
        })
        .build()
        .map_err(|e| t!("音声エンコーダ生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
        pipeline
            .add_many([
                audio_appsrc.upcast_ref::<gst::Element>(),
                &audioconvert,
                &audioresample,
                &opusenc,
            ])
            .map_err(|e| e.to_string())?;
        if let Err(e) = gst::Element::link_many([
            audio_appsrc.upcast_ref::<gst::Element>(),
            &audioconvert,
            &audioresample,
            &opusenc,
        ]) {
            fail!(t!("音声チェーンリンク失敗: %{arg0}", arg0 = format!("{e}")).to_string());
        }
        if let Err(e) = gst::Element::link(&opusenc, &muxer) {
            fail!(
                t!(
                    "音声エンコーダ -> muxリンク失敗: %{arg0}",
                    arg0 = format!("{e}")
                )
                .to_string()
            );
        }
        Some(audio_appsrc)
    } else {
        None
    };

    if let Err(e) = pipeline.set_state(gst::State::Playing) {
        fail!(t!("PLAYING遷移失敗: %{arg0}", arg0 = format!("{e}")).to_string());
    }

    let frame_duration_ns = 1_000_000_000u64 * fps_denom as u64 / fps_num.max(1) as u64;
    let samples_per_frame = audio
        .as_ref()
        .map(|a| (a.sample_rate as u64 * fps_denom as u64 / fps_num.max(1) as u64) as usize)
        .unwrap_or(0);

    let mut frame_index: i64 = 0;
    loop {
        let chunk = match produce_video() {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(e) => fail!(
                t!(
                    "エンコード済みチャンク取得失敗: %{arg0}",
                    arg0 = format!("{e}")
                )
                .to_string()
            ),
        };
        let (data, pts_i64, _keyframe) = chunk;
        let pts = gst::ClockTime::from_nseconds(pts_i64.max(0) as u64);
        let mut buffer = gst::Buffer::from_slice(data);
        {
            let Some(buffer_mut) = buffer.get_mut() else {
                fail!("バッファ可変参照取得失敗".to_owned());
            };
            buffer_mut.set_pts(pts);
            buffer_mut.set_duration(gst::ClockTime::from_nseconds(frame_duration_ns));
        }
        if let Err(e) = appsrc.push_buffer(buffer) {
            fail!(
                t!(
                    "appsrc push失敗(frame=%{arg0}): %{arg1}",
                    arg0 = format!("{frame_index}"),
                    arg1 = format!("{e:?}")
                )
                .to_string()
            );
        }
        if let (Some(audio_appsrc), Some(produce_audio)) =
            (audio_appsrc.as_ref(), produce_audio.as_mut())
        {
            let samples = produce_audio(frame_index, samples_per_frame);
            let mut audio_buffer = gst::Buffer::from_slice(bytemuck::cast_slice(&samples).to_vec());
            {
                let Some(buffer_mut) = audio_buffer.get_mut() else {
                    fail!("音声バッファ可変参照取得失敗".to_owned());
                };
                buffer_mut.set_pts(pts);
                buffer_mut.set_duration(gst::ClockTime::from_nseconds(frame_duration_ns));
            }
            if let Err(e) = audio_appsrc.push_buffer(audio_buffer) {
                fail!(
                    t!(
                        "音声appsrc push失敗(frame=%{arg0}): %{arg1}",
                        arg0 = format!("{frame_index}"),
                        arg1 = format!("{e:?}")
                    )
                    .to_string()
                );
            }
        }
        frame_index += 1;
        let _ = total_frames;
    }

    if let Err(e) = appsrc.end_of_stream() {
        fail!(t!("EOS送出失敗: %{arg0}", arg0 = format!("{e:?}")).to_string());
    }
    if let Some(audio_appsrc) = &audio_appsrc {
        if let Err(e) = audio_appsrc.end_of_stream() {
            fail!(t!("音声EOS送出失敗: %{arg0}", arg0 = format!("{e:?}")).to_string());
        }
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
                    .unwrap_or_else(|| t!("不明").to_string());
                encode_error = Some(
                    t!(
                        "mux中にエラー: 要素=%{arg0} 理由=%{arg1} 詳細=%{arg2}",
                        arg0 = format!("{src}"),
                        arg1 = format!("{}", err.error()),
                        arg2 = format!("{:?}", err.debug())
                    )
                    .to_string(),
                );
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
