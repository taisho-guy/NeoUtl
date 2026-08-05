use gst::prelude::*;
use gst_app::AppSink;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use neoutl_media_api::VideoSource;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

mod discoverer;

/// 固定枚数のNV12テクスチャを解像度確定時に一括生成し、以後はwrite_textureのみで
/// 内容を上書き（ローテーション）する。毎フレームのcreate_texture呼び出し
/// （GPUアロケーションスパイクの発生源）を排除するための固定リソースプール。
/// 容量はneoutl_media_api::VIDEO_TEXTURE_POOL_CAPACITYに一致させ、host側
/// media/cache.rs::TextureLruの容量を超えないようにする（超えるとLRUが
/// 保持するテクスチャハンドルの実体がローテーションにより上書きされ、
/// 古いフレーム番号で新しい映像が表示されるstale handle aliasingを招く）。
struct TexturePool {
    textures: Vec<wgpu::Texture>,
    next_write_index: usize,
}

fn create_nv12_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    slot: usize,
) -> wgpu::Texture {
    let label = format!("video-nv12-pool-slot-{slot}");
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(&label),
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
    })
}

impl TexturePool {
    fn new(device: &wgpu::Device, width: u32, height: u32, capacity: usize) -> Self {
        let textures = (0..capacity)
            .map(|slot| create_nv12_texture(device, width, height, slot))
            .collect();
        Self {
            textures,
            next_write_index: 0,
        }
    }

    /// ローテーション先のスロットを1つ進めてテクスチャ参照を返す。
    fn next_write_target(&mut self) -> &wgpu::Texture {
        let idx = self.next_write_index;
        self.next_write_index = (self.next_write_index + 1) % self.textures.len();
        &self.textures[idx]
    }
}

/// 既存テクスチャへNV12バイト列を上書きする（create_texture不要）。
fn update_nv12_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    data: &[u8],
    width: u32,
    height: u32,
) {
    let y_plane_size = (width * height) as usize;
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
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
            texture,
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
}

static GST_INIT: Once = Once::new();

fn ensure_gst_init() {
    GST_INIT.call_once(|| {
        gst::init().expect(&t!("gstreamer初期化失敗"));
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
            "{}",
            t!(
                "[gstreamer-decoder] ハードウェアH.264/HEVCデコーダ要素が未登録です。VAAPI/V4L2/NVCODECいずれかのGStreamerプラグインパッケージを導入して下さい。"
            )
        );
    } else {
        eprintln!(
            "{}",
            t!(
                "[gstreamer-decoder] ハードウェアデコーダ検出: %{arg0}",
                arg0 = format!("{found:?}")
            )
        );
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
/// pull_preroll/pull_sample系の無期限ブロック回避用タイムアウト。
/// オートプラグ先デコーダがハードウェア制約等でサンプルを一切生成できない場合に
/// この時間で打ち切りErrへ変換する。
const PULL_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(10);
/// pending（prefetch済みNV12バイト列）の保持上限件数。超過時、target近傍以外を破棄する。
const PENDING_PURGE_THRESHOLD: usize = 16;
/// pending破棄時にtargetから残す半径。
const PENDING_KEEP_RADIUS: i64 = 8;
/// GOP保護区間[gop_start, frame_index]として無条件保持してよい最大フレーム数。
/// 超過する場合（長大GOPを持つ配信系コンテンツ等）はPENDING_KEEP_RADIUSのみへ縮退し、
/// pending肥大化の再発を防ぐ。
const MAX_GOP_PROTECT_SPAN: i64 = 256;
/// sample_at_sequentialの`target == last_frame + 1`厳密一致から外れた際、
/// ACCURATE seekへ即フォールバックせず追いつき用に許容する最大ギャップ。
/// DECODE_PREFETCH_RADIUS（呼び出し元 media/worker.rs）と揃える。
/// これを超えるギャップは実質シーク（逆再生・スクラブ）とみなしseek経路へ委ねる。
const SEQUENTIAL_CATCHUP_MAX: i64 = 8;
/// prefetch()がcommand_threadからの応答を待つ上限。sample_at内部の各種待機
/// （PULL_TIMEOUT×2 + wait_state 10秒等）の合算最悪値に対し十分な余裕を持たせる。
/// 超過時はcommand_threadを待たずErrへ変換する（バグ7: 無限ブロック防止。
/// gpuvideo-decoder::DECODE_WATCHDOG_TIMEOUTと同種の防御をGStreamer経路にも導入）。
const COMMAND_REPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// バス排出スレッドのポーリング間隔。gst::Bus::timed_popの単発ブロック時間として使う。
const BUS_DRAIN_POLL: gst::ClockTime = gst::ClockTime::from_mseconds(200);
/// Drop時、スタックしたスレッドの終了をポーリングで待つ上限（超過時は明示的にリークする）。
const THREAD_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

/// decodebin3(uridecodebin3)が公開したパッド(音声・字幕・重複映像ストリーム等、
/// このプラグインの責務外のもの)をfakesinkへ接続して消費する。sync/asyncを
/// falseとし、パイプラインの状態遷移・クロック同期に一切関与させない。
/// 未リンクのまま放置すると、decodebin3が「公開した全パッドが消費される」ことを
/// 前提に内部キューを進行させるため、音声トラックを持つファイルで
/// バックプレッシャが発生し映像側も巻き込んでブロックしうる。
fn drain_to_fakesink(pipeline: &gst::Pipeline, pad: &gst::Pad) {
    let fakesink = match gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("async", false)
        .build()
    {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "{}",
                t!(
                    "[gstreamer-decoder] fakesink生成失敗（未消費パッドが残留します）: %{arg0}",
                    arg0 = format!("{e}")
                )
            );
            return;
        }
    };
    if let Err(e) = pipeline.add(&fakesink) {
        eprintln!(
            "{}",
            t!(
                "[gstreamer-decoder] fakesinkのパイプライン追加失敗: %{arg0}",
                arg0 = format!("{e}")
            )
        );
        return;
    }
    if let Err(e) = fakesink.sync_state_with_parent() {
        eprintln!(
            "{}",
            t!(
                "[gstreamer-decoder] fakesinkの状態同期失敗: %{arg0}",
                arg0 = format!("{e}")
            )
        );
    }
    let Some(sinkpad) = fakesink.static_pad("sink") else {
        eprintln!("{}", t!("[gstreamer-decoder] fakesink sinkパッド未取得"));
        return;
    };
    if let Err(e) = pad.link(&sinkpad) {
        eprintln!(
            "{}",
            t!(
                "[gstreamer-decoder] fakesinkへのリンク失敗: %{arg0}",
                arg0 = format!("{e:?}")
            )
        );
    }
}

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
    ) && let gst::MessageView::Error(err) = msg.view()
    {
        let src = err
            .src()
            .map(|s| s.path_string().to_string())
            .unwrap_or_else(|| t!("不明").to_string());
        return Err(t!(
            "状態遷移失敗: 要素=%{arg0} 理由=%{arg1} 詳細=%{arg2}",
            arg0 = format!("{src}"),
            arg1 = format!("{}", err.error()),
            arg2 = format!("{:?}", err.debug())
        )
        .to_string());
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
        "{}",
        t!(
            "[gstreamer-decoder] extract_nv12_bytes: width=%{arg0} height=%{arg1} data_len=%{arg2} 必要バイト数=%{arg3}",
            arg0 = format!("{width}"),
            arg1 = format!("{height}"),
            arg2 = format!("{}", data.len()),
            arg3 = format!("{}", y_plane_size + uv_plane_size)
        )
    );
    if data.len() < y_plane_size + uv_plane_size {
        let msg = t!(
            "NV12バッファサイズ不足: data_len=%{arg0} 必要=%{arg1}",
            arg0 = format!("{}", data.len()),
            arg1 = format!("{}", y_plane_size + uv_plane_size)
        )
        .to_string();
        eprintln!("[gstreamer-decoder] {msg}");
        return Err(msg);
    }
    Ok(data[..y_plane_size + uv_plane_size].to_vec())
}

/// GStreamer実体。command_thread専有。busdrain_thread（バスメッセージの
/// 継続排出専用スレッド）とは別スレッドでsample_at（ブロッキング呼び出し）
/// を実行するため、両者は並行して動作する。
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
    /// 直近の「決定論的デコード起点」のフレーム番号。同一GOP内のスクラブで
    /// pending破棄を回避する判定に用いる（GstDecoder::prefetch参照）。
    ///
    /// バグ2の修正: 旧実装は`buffer.flags().contains(DELTA_UNIT)`でキーフレーム判定を
    /// 行っていたが、videoconvert通過後の全バッファでこのフラグが観測されず
    /// （実機ログ上、全フレームでgop_start==frame_indexとなり判定が機能していなかった）、
    /// GOP保護は事実上常に無効化されていた。DELTA_UNITフラグの消失原因（videoconvertの
    /// バッファ複製実装、または特定パイプライン構成での不伝播）を特定・修正する代わりに、
    /// フラグに一切依存しない決定論的な代替指標へ置き換える：
    /// ACCURATE seekが成功した時点のtargetを「新しい安全な再デコード起点」として記録し
    /// （sample_at_seek内で更新）、連番再生・追いつき経路では起点を変更しない
    /// （sample_at_sequential/catchup内では更新しない）。
    /// これにより「前回のシーク以降、連続デコードが途切れていない区間」を正確に表現でき、
    /// GStreamer側のバッファフラグ実装に依存しない。
    last_gop_start: i64,
}

impl GstDecoderInner {
    fn open(path: &Path) -> Result<Self, String> {
        let discovered = discoverer::discover(path)?;
        if !discovered.seekable {
            eprintln!(
                "{}",
                t!(
                    "[gstreamer-decoder] 警告: このソースはシーク不可と報告されました。ACCURATE seekが失敗する可能性があります: %{arg0}",
                    arg0 = format!("{}", path.display())
                )
            );
        }

        let uri = gst::glib::filename_to_uri(path, None).map_err(|e| e.to_string())?;

        let pipeline = gst::Pipeline::new();
        let uridecodebin3 = gst::ElementFactory::make("uridecodebin3")
            .property("uri", uri.as_str())
            .build()
            .map_err(|e| t!("uridecodebin3生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;

        let download_elems: Vec<gst::Element> = DOWNLOAD_CHAIN
            .split('!')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|desc| {
                let name = desc.split_whitespace().next().unwrap_or(desc);
                gst::ElementFactory::make(name).build().map_err(|e| {
                    t!(
                        "%{arg0}生成失敗: %{arg1}",
                        arg0 = format!("{name}"),
                        arg1 = format!("{e}")
                    )
                    .to_string()
                })
            })
            .collect::<Result<_, _>>()?;
        let videoconvert = gst::ElementFactory::make("videoconvert")
            .build()
            .map_err(|e| t!("videoconvert生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
        let queue = gst::ElementFactory::make("queue")
            .property_from_str("leaky", "downstream")
            .property("max-size-buffers", 4u32)
            .property("max-size-bytes", 0u32)
            .property("max-size-time", 0u64)
            .build()
            .map_err(|e| t!("queue生成失敗: %{arg0}", arg0 = format!("{e}")).to_string())?;
        let appsink = AppSink::builder()
            .caps(&gst::Caps::from_str(SYSMEM_CAPS).map_err(|e| e.to_string())?)
            .sync(false)
            .name("sink")
            .build();

        pipeline.add(&uridecodebin3).map_err(|e| e.to_string())?;
        for elem in &download_elems {
            pipeline.add(elem).map_err(|e| e.to_string())?;
        }
        pipeline
            .add_many([&videoconvert, &queue, appsink.upcast_ref::<gst::Element>()])
            .map_err(|e| e.to_string())?;

        let mut video_chain: Vec<gst::Element> = download_elems.clone();
        video_chain.push(videoconvert.clone());
        video_chain.push(queue.clone());
        video_chain.push(appsink.clone().upcast::<gst::Element>());
        gst::Element::link_many(video_chain.iter().collect::<Vec<_>>()).map_err(|e| {
            t!("映像チェーンのリンク失敗: %{arg0}", arg0 = format!("{e}")).to_string()
        })?;

        let video_chain_head_sink = video_chain
            .first()
            .expect(&t!("video_chainは常に非空"))
            .static_pad("sink")
            .ok_or("映像チェーン先頭のsinkパッド未取得")?;

        let pipeline_weak = pipeline.downgrade();
        uridecodebin3.connect_pad_added(move |_bin, pad| {
            let Some(pipeline) = pipeline_weak.upgrade() else {
                return;
            };
            let pad_name = pad.name();
            if pad_name.starts_with("video_") {
                if video_chain_head_sink.is_linked() {
                    eprintln!("{}", t!("[gstreamer-decoder] 追加の映像ストリーム%{arg0}を検出しましたが、最初の映像ストリームのみ使用します（fakesinkへ排出）", arg0 = format!("{pad_name}")));
                    drain_to_fakesink(&pipeline, pad);
                    return;
                }
                if let Err(e) = pad.link(&video_chain_head_sink) {
                    eprintln!("{}", t!("[gstreamer-decoder] 映像パッド%{arg0}のリンク失敗: %{arg1}", arg0 = format!("{pad_name}"), arg1 = format!("{e:?}")));
                }
            } else {
                drain_to_fakesink(&pipeline, pad);
            }
        });

        macro_rules! fail {
            ($err:expr) => {{
                let _ = pipeline.set_state(gst::State::Null);
                return Err($err);
            }};
        }

        if pipeline.bus().is_none() {
            fail!("バス未取得".to_owned());
        }

        if let Err(e) = pipeline.set_state(gst::State::Paused) {
            fail!(e.to_string());
        }
        if let Err(e) = wait_state(&pipeline, gst::ClockTime::from_seconds(10)) {
            fail!(e);
        }

        let preroll = match appsink.try_pull_preroll(PULL_TIMEOUT) {
            Some(p) => p,
            None => fail!("preroll取得タイムアウト（デコーダがサンプルを生成しません）".to_owned()),
        };
        let caps = match preroll.caps() {
            Some(c) => c,
            None => fail!("caps未取得".to_owned()),
        };
        if gst_video::VideoInfo::from_caps(caps).is_err() {
            fail!("appsink caps解析失敗（videoconvert出力が不正）".to_owned());
        }

        let width = discovered.width;
        let height = discovered.height;
        let fps = discovered.fps;
        let frame_duration_ns = (1_000_000_000.0 / fps.max(1e-6)) as u64;

        if width % 2 != 0 || height % 2 != 0 {
            eprintln!(
                "{}",
                t!(
                    "[gstreamer-decoder] 警告: 奇数寸法動画（width=%{arg0} height=%{arg1}）はNV12 4:2:0平面計算が破綻する可能性: %{arg2}",
                    arg0 = format!("{width}"),
                    arg1 = format!("{height}"),
                    arg2 = format!("{}", path.display())
                )
            );
        }

        let duration_ns = if discovered.duration_ns > 0 {
            discovered.duration_ns
        } else {
            pipeline
                .query_duration::<gst::ClockTime>()
                .map(|d| d.nseconds())
                .unwrap_or(0)
        };
        let total_frames = duration_to_frames(duration_ns, frame_duration_ns).max(1);

        eprintln!(
            "{}",
            t!(
                "[gstreamer-decoder] open完了: caps=%{arg0} width=%{arg1} height=%{arg2} fps=%{arg3} total_frames=%{arg4} duration_ns=%{arg5} has_audio=%{arg6}",
                arg0 = format!("{caps}"),
                arg1 = format!("{width}"),
                arg2 = format!("{height}"),
                arg3 = format!("{fps}"),
                arg4 = format!("{total_frames}"),
                arg5 = format!("{duration_ns}"),
                arg6 = format!("{}", discovered.has_audio)
            )
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
            last_gop_start: 0,
        })
    }

    /// 連番再生専用の高速経路。PLAYING状態のまま継続デコードさせ、
    /// bounded timeoutでappsinkから直接次フレームを取得する。
    /// シークを一切発行しないため、キーフレームからの再デコードが発生しない。
    fn sample_at_sequential(&mut self, target: i64) -> Option<gst::Sample> {
        if self.pipeline.current_state() != gst::State::Playing
            && let Err(e) = self.pipeline.set_state(gst::State::Playing)
        {
            eprintln!(
                "{}",
                t!(
                    "[gstreamer-decoder] 連番再生パス: PLAYING遷移失敗 %{arg0}",
                    arg0 = format!("{e}")
                )
            );
            return None;
        }
        if wait_state(&self.pipeline, gst::ClockTime::from_seconds(2)).is_err() {
            eprintln!(
                "{}",
                t!("[gstreamer-decoder] 連番再生パス: 状態遷移待機失敗")
            );
            return None;
        }
        match self
            .appsink
            .try_pull_sample(gst::ClockTime::from_seconds(2))
        {
            Some(sample) => {
                eprintln!(
                    "{}",
                    t!(
                        "[gstreamer-decoder] 連番再生パス成功: target=%{arg0}",
                        arg0 = format!("{target}")
                    )
                );
                self.last_frame = target;
                Some(sample)
            }
            None => {
                eprintln!(
                    "{}",
                    t!("[gstreamer-decoder] 連番再生パス: サンプル取得タイムアウト")
                );
                None
            }
        }
    }

    /// 非連番アクセス（スクラブ・逆再生・初回シーク）専用のシーク経路。
    /// PAUSED状態へ戻した上でシークを行い、対象フレームを確定する。
    /// 常にACCURATEを用いる。ACCURATE seekはGStreamer内部で
    /// 直近キーフレームへ着地後、target位置まで自動的に前進デコードするため、
    /// KEY_UNITへ切替える距離最適化は不要であり、かつ危険である：
    /// 旧実装はKEY_UNIT時に着地フレームがtargetと一致する保証がないにも
    /// かかわらず`self.last_frame = target`を代入していた。これにより
    /// 着地フレームの画素内容が`frame_index`という誤ったラベルでpendingへ
    /// 格納され、以後の連番再生パスも誤位置基準のまま進行し続ける
    /// 前後跳躍・速度異常の原因）。
    /// 着地バッファのPTSはログ出力のみに用い、target一致検証は行わない
    /// （コンテナのPTSベースオフセットやB-frame遅延により、正しく着地して
    /// いてもPTS/frame_duration_nsの整数演算はtargetと恒常的にずれうるため、
    /// 誤検知によるprefetch失敗の連鎖・デコーダフォールバックを避ける）。
    fn sample_at_seek(&mut self, frame_index: i64, target: i64) -> Result<gst::Sample, String> {
        let target_ns = target as u64 * self.frame_duration_ns;
        let seek_flags = gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE;
        eprintln!(
            "{}",
            t!(
                "[gstreamer-decoder] sample_atシーク: frame_index=%{arg0} target=%{arg1} target_ns=%{arg2}",
                arg0 = format!("{frame_index}"),
                arg1 = format!("{target}"),
                arg2 = format!("{target_ns}")
            )
        );
        if self.pipeline.current_state() != gst::State::Paused {
            let _ = self.pipeline.set_state(gst::State::Paused);
        }
        self.pipeline
            .seek_simple(seek_flags, gst::ClockTime::from_nseconds(target_ns))
            .map_err(|e| {
                let msg = e.to_string();
                eprintln!(
                    "{}",
                    t!(
                        "[gstreamer-decoder] seek失敗: %{arg0}",
                        arg0 = format!("{msg}")
                    )
                );
                msg
            })?;
        wait_state(&self.pipeline, gst::ClockTime::from_seconds(10)).map_err(|e| {
            eprintln!(
                "{}",
                t!(
                    "[gstreamer-decoder] seek後の状態遷移失敗: %{arg0}",
                    arg0 = format!("{e}")
                )
            );
            e
        })?;
        let result = self
            .appsink
            .try_pull_preroll(PULL_TIMEOUT)
            .or_else(|| self.appsink.try_pull_sample(PULL_TIMEOUT))
            .ok_or_else(|| "sample取得タイムアウト（デコーダがサンプルを生成しません）".to_owned());
        match &result {
            Ok(sample) => {
                let buffer_size = sample.buffer().map(|b| b.size()).unwrap_or(0);
                let caps_str = sample
                    .caps()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| t!("なし").to_string());
                let landed_pts_ns = sample.buffer().and_then(|b| b.pts()).map(|p| p.nseconds());
                eprintln!(
                    "{}",
                    t!(
                        "[gstreamer-decoder] sample取得成功: frame_index=%{arg0} buffer_size=%{arg1} caps=%{arg2} landed_pts_ns=%{arg3}",
                        arg0 = format!("{frame_index}"),
                        arg1 = format!("{buffer_size}"),
                        arg2 = format!("{caps_str}"),
                        arg3 = format!("{landed_pts_ns:?}")
                    )
                );
                self.last_frame = target;
                self.last_gop_start = target;
            }
            Err(e) => {
                eprintln!(
                    "{}",
                    t!(
                        "[gstreamer-decoder] sample取得失敗: frame_index=%{arg0} 理由=%{arg1}",
                        arg0 = format!("{frame_index}"),
                        arg1 = format!("{e}")
                    )
                );
            }
        }
        result
    }

    /// last_frame+1からtargetまでの間に小さな前方ギャップがある場合の追いつき経路。
    /// PLAYING状態を維持したままappsinkから連続してpull_sampleし、target未満のサンプルは
    /// 表示に使わず読み捨てる。ACCURATE seek（キーフレームからの再デコード＋FLUSH）より
    /// 常に安価であり、prefetch側の先読み半径超過（worker.rs::PREFETCH_RADIUS由来の
    /// わずかな前方ずれ）でシークへ誤って落ちることを防ぐ。
    fn sample_at_sequential_catchup(&mut self, target: i64) -> Option<gst::Sample> {
        let gap = target - self.last_frame;
        if gap <= 0 || gap > SEQUENTIAL_CATCHUP_MAX {
            return None;
        }
        if self.pipeline.current_state() != gst::State::Playing
            && let Err(e) = self.pipeline.set_state(gst::State::Playing)
        {
            eprintln!(
                "{}",
                t!(
                    "[gstreamer-decoder] 追いつきパス: PLAYING遷移失敗 %{arg0}",
                    arg0 = format!("{e}")
                )
            );
            return None;
        }
        if wait_state(&self.pipeline, gst::ClockTime::from_seconds(2)).is_err() {
            eprintln!(
                "{}",
                t!("[gstreamer-decoder] 追いつきパス: 状態遷移待機失敗")
            );
            return None;
        }
        for step in 1..=gap {
            match self
                .appsink
                .try_pull_sample(gst::ClockTime::from_seconds(2))
            {
                Some(sample) => {
                    self.last_frame += 1;
                    if step == gap {
                        eprintln!(
                            "{}",
                            t!(
                                "[gstreamer-decoder] 追いつきパス成功: target=%{arg0} gap=%{arg1}",
                                arg0 = format!("{target}"),
                                arg1 = format!("{gap}")
                            )
                        );
                        return Some(sample);
                    }
                }
                None => {
                    eprintln!(
                        "{}",
                        t!(
                            "[gstreamer-decoder] 追いつきパス: サンプル取得タイムアウト target=%{arg0} step=%{arg1}/%{arg2}",
                            arg0 = format!("{target}"),
                            arg1 = format!("{step}"),
                            arg2 = format!("{gap}")
                        )
                    );
                    return None;
                }
            }
        }
        None
    }

    fn sample_at(&mut self, frame_index: i64) -> Result<gst::Sample, String> {
        let target = frame_index.clamp(0, self.total_frames - 1);
        if target == self.last_frame + 1
            && let Some(sample) = self.sample_at_sequential(target)
        {
            return Ok(sample);
        }
        if target > self.last_frame
            && let Some(sample) = self.sample_at_sequential_catchup(target)
        {
            return Ok(sample);
        }
        self.sample_at_seek(frame_index, target)
    }
}

enum Command {
    Frame {
        frame_index: i64,
        reply: mpsc::Sender<Result<(Vec<u8>, i64), String>>,
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
    /// バス排出スレッド停止フラグ。Dropで先に立ててからjoinを試みる。
    bus_stop: Arc<AtomicBool>,
    busdrain_thread: Option<JoinHandle<()>>,
    command_thread: Option<JoinHandle<()>>,
    /// prefetchが取得したNV12バイト列。frame_gpuがここからテクスチャアップロードする。
    /// キーは常にclamp済みフレーム番号（バグ9修正: 範囲外の生frame_indexで
    /// 重複登録されないよう、prefetch()冒頭で一度だけclampする）。
    pending: HashMap<i64, Vec<u8>>,
    /// 固定テクスチャプール。device取得後（初回frame_gpu呼び出し時）に遅延初期化する。
    pool: Option<TexturePool>,
}

/// スタックしたスレッドの終了を一定時間ポーリング待機する。
/// 超過時はJoinHandleを明示的に手放し（mem::forget）、呼び出し元をブロックしない。
/// バグ8修正: 旧Drop実装はcommand_thread/mainloop_threadを無条件joinしており、
/// GStreamer内部の無期限ブロック（例: ドライバハング中のpipeline.state()）に
///巻き込まれるとプロセス終了処理自体が止まりかねなかった。
fn bounded_join(handle: Option<JoinHandle<()>>, timeout: Duration, name: &str) {
    let Some(handle) = handle else {
        return;
    };
    let start = std::time::Instant::now();
    loop {
        if handle.is_finished() {
            let _ = handle.join();
            return;
        }
        if start.elapsed() >= timeout {
            eprintln!(
                "{}",
                t!(
                    "[gstreamer-decoder] %{arg0} 終了待機タイムアウト（%{arg1}）。スレッドを解放せず放棄します（プロセス終了までのリークを許容し、呼び出し元の停止を回避）",
                    arg0 = format!("{name}"),
                    arg1 = format!("{timeout:?}")
                )
            );
            std::mem::forget(handle);
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

impl GstDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        ensure_gst_init();
        let path: PathBuf = path.to_owned();

        let inner = GstDecoderInner::open(&path)?;

        let width = inner.width;
        let height = inner.height;
        let fps = inner.fps;
        let total_frames = inner.total_frames;

        let bus_stop = Arc::new(AtomicBool::new(false));
        let busdrain_thread = {
            let pipeline = inner.pipeline.clone();
            let bus_stop = bus_stop.clone();
            std::thread::Builder::new()
                .name("gst-decoder-busdrain".to_owned())
                .spawn(move || {
                    let Some(bus) = pipeline.bus() else {
                        eprintln!("{}", t!("[gstreamer-decoder] busdrain_thread: バス未取得のため即終了"));
                        return;
                    };
                    eprintln!("{}", t!("[gstreamer-decoder] busdrain_thread起動完了"));
                    while !bus_stop.load(Ordering::Acquire) {
                        let Some(msg) = bus.timed_pop(BUS_DRAIN_POLL) else {
                            continue;
                        };
                        match msg.view() {
                            gst::MessageView::Error(err) => {
                                let src = err
                                    .src()
                                    .map(|s| s.path_string().to_string())
                                    .unwrap_or_else(|| t!("不明").to_string());
                                eprintln!("{}", t!("[gstreamer-decoder] busdrain: ERROR 要素=%{arg0} 理由=%{arg1} 詳細=%{arg2}", arg0 = format!("{src}"), arg1 = format!("{}", err.error()), arg2 = format!("{:?}", err.debug())));
                            }
                            gst::MessageView::Warning(warn) => {
                                eprintln!("{}", t!("[gstreamer-decoder] busdrain: WARNING 理由=%{arg0}", arg0 = format!("{}", warn.error())));
                            }
                            gst::MessageView::Eos(_) => {
                                eprintln!("{}", t!("[gstreamer-decoder] busdrain: EOS受信"));
                            }
                            _ => {}
                        }
                    }
                    eprintln!("{}", t!("[gstreamer-decoder] busdrain_thread終了"));
                })
                .map_err(|e| e.to_string())?
        };

        let (tx, rx) = mpsc::channel::<Command>();

        let command_thread = {
            std::thread::Builder::new()
                .name("gst-decoder-command".to_owned())
                .spawn(move || {
                    let mut inner = inner;
                    eprintln!("{}", t!("[gstreamer-decoder] command_thread起動完了"));
                    while let Ok(command) = rx.recv() {
                        match command {
                            Command::Frame {
                                frame_index,
                                reply,
                            } => {
                                eprintln!("{}", t!("[gstreamer-decoder] command_thread: Frame受信 frame_index=%{arg0}", arg0 = format!("{frame_index}")));
                                let result = inner.sample_at(frame_index).and_then(|sample| {
                                    let buffer = sample.buffer().ok_or("buffer未取得".to_owned())?;
                                    extract_nv12_bytes(buffer, inner.width, inner.height)
                                        .map(|bytes| (bytes, inner.last_gop_start))
                                });
                                if let Err(e) = &result {
                                    eprintln!("{}", t!("[gstreamer-decoder] command_thread: フレーム処理失敗 frame_index=%{arg0} 理由=%{arg1}", arg0 = format!("{frame_index}"), arg1 = format!("{e}")));
                                }
                                let _ = reply.send(result);
                            }
                            Command::Shutdown => {
                                eprintln!("{}", t!("[gstreamer-decoder] command_thread: Shutdown受信"));
                                break;
                            }
                        }
                    }
                    let _ = inner.pipeline.set_state(gst::State::Null);
                    eprintln!("{}", t!("[gstreamer-decoder] command_thread終了"));
                })
                .map_err(|e| e.to_string())?
        };

        Ok(Self {
            width,
            height,
            fps,
            total_frames,
            tx,
            bus_stop,
            busdrain_thread: Some(busdrain_thread),
            command_thread: Some(command_thread),
            pending: HashMap::new(),
            pool: None,
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
        let clamped = frame_index.clamp(0, (self.total_frames - 1).max(0));
        if self.pending.contains_key(&clamped) {
            return Ok(());
        }
        eprintln!(
            "{}",
            t!(
                "[gstreamer-decoder] prefetch呼び出し: frame_index=%{arg0}",
                arg0 = format!("{clamped}")
            )
        );
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::Frame {
                frame_index: clamped,
                reply: reply_tx,
            })
            .map_err(|e| {
                let msg = "command_thread終了済み".to_owned();
                eprintln!(
                    "{}",
                    t!(
                        "[gstreamer-decoder] コマンド送信失敗: %{arg0} (%{arg1})",
                        arg0 = format!("{e}"),
                        arg1 = format!("{msg}")
                    )
                );
                msg
            })?;
        let (bytes, gop_start) = reply_rx
            .recv_timeout(COMMAND_REPLY_TIMEOUT)
            .map_err(|e| {
                let msg = t!(
                    "command_thread応答タイムアウト（%{arg0}経過、詳細=%{arg1}）",
                    arg0 = format!("{COMMAND_REPLY_TIMEOUT:?}"),
                    arg1 = format!("{e}")
                )
                .to_string();
                eprintln!("[gstreamer-decoder] {msg}");
                msg
            })
            .and_then(|inner| inner)?;
        eprintln!(
            "{}",
            t!(
                "[gstreamer-decoder] prefetch完了: frame_index=%{arg0} bytes=%{arg1} gop_start=%{arg2}",
                arg0 = format!("{clamped}"),
                arg1 = format!("{}", bytes.len()),
                arg2 = format!("{gop_start}")
            )
        );
        if self.pending.len() >= PENDING_PURGE_THRESHOLD {
            let gop_span = clamped - gop_start;
            let protect_gop = (0..=MAX_GOP_PROTECT_SPAN).contains(&gop_span);
            self.pending.retain(|k, _| {
                (protect_gop && *k >= gop_start && *k <= clamped)
                    || (k - clamped).abs() <= PENDING_KEEP_RADIUS
            });
        }
        self.pending.insert(clamped, bytes);
        Ok(())
    }

    /// UIスレッド専用。prefetch済みNV12バイト列をテクスチャへアップロードする。
    /// pending未生成時にself.prefetch()を呼ぶ同期フォールバックは行わない。
    /// prefetch()はコマンドチャネル経由でcommand_threadへの往復を伴うブロッキング
    /// 呼び出しであり、UIスレッド上のこの関数から呼ぶと、呼び出し元
    /// （media/cache.rs::frame_at）が保持するentryロックを長時間（最悪
    /// PULL_TIMEOUT×複数回分）占有し続け、他の全アクセスを道連れに停止させる。
    /// 未生成時は即座にErrを返し、非同期のDecodeWorkerによる生成を待つ。
    fn frame_gpu(
        &mut self,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        let clamped = frame_index.clamp(0, (self.total_frames - 1).max(0));
        let bytes = self
            .pending
            .remove(&clamped)
            .ok_or("対象フレーム未生成（prefetch未完了）".to_owned())?;
        let pool = self.pool.get_or_insert_with(|| {
            TexturePool::new(
                device,
                self.width,
                self.height,
                neoutl_media_api::VIDEO_TEXTURE_POOL_CAPACITY,
            )
        });
        let texture = pool.next_write_target();
        update_nv12_texture(queue, texture, &bytes, self.width, self.height);
        Ok(texture.clone())
    }
}

impl Drop for GstDecoder {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        self.bus_stop.store(true, Ordering::Release);
        bounded_join(
            self.command_thread.take(),
            THREAD_JOIN_TIMEOUT,
            "command_thread",
        );
        bounded_join(
            self.busdrain_thread.take(),
            THREAD_JOIN_TIMEOUT,
            "busdrain_thread",
        );
    }
}

use neoutl_media_api::{MediaKind, MediaMeta, MediaVTable};

static EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "webm", "avi"];

static META: MediaMeta = MediaMeta {
    id: "neoutl.media.gstreamer",
    name: "GStreamer Video Decoder",
    kind: MediaKind::Video,
    extensions_ptr: EXTENSIONS.as_ptr(),
    extensions_len: EXTENSIONS.len(),
};

pub fn meta() -> &'static MediaMeta {
    &META
}

fn open_video(path: &std::path::Path) -> Result<Box<dyn neoutl_media_api::VideoSource>, String> {
    GstDecoder::open(path).map(|d| Box::new(d) as Box<dyn neoutl_media_api::VideoSource>)
}

/// src/media/loader.rsのネイティブプラグインレジストリへ直接登録するためのVTable生成。
/// gpuvideo-decoder::native_vtable()と同様、dylib境界を経由しない。
pub fn native_vtable() -> MediaVTable {
    MediaVTable {
        meta,
        open_video: Some(open_video),
        open_image: None,
        decode_audio: None,
    }
}
rust_i18n::i18n!("../../i18n");
#[macro_use]
extern crate rust_i18n;
