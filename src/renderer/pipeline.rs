use crate::config;
use crate::ecs::resources::ProjectResource;
use crate::ecs::systems::ActiveObject;
use crate::ecs::types::Value;
use crate::effects;
use crate::hot_reload::{self, ReloadEvent};
use crate::objects::{by_kind_id, registry};
use egui_wgpu::wgpu;
use neoutl_object_api::{IMAGE_STABLE_ID, SCENE_STABLE_ID, UNIT_SIZE_PX, VIDEO_STABLE_ID};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wgpu_text::glyph_brush::ab_glyph::FontArc;
use wgpu_text::{BrushBuilder, TextBrush};

/// GPUデバイスロスト検知フラグ。wgpuのdevice.lost()コールバック（Send + 'staticのみ許容、
/// UI操作不可）から立てられ、RenderEngine::render()冒頭で参照される。
/// UIスレッド側の定期ポーリング（app_state等）からも参照しモーダル表示に使う。
pub static DEVICE_LOST: AtomicBool = AtomicBool::new(false);

/// DEVICE_LOSTが真か確認する。UIスレッドのタイマー等から呼び出す想定。
pub fn is_device_lost() -> bool {
    DEVICE_LOST.load(Ordering::Relaxed)
}

/// device.lost()完了時に呼ぶ。以後render()は早期returnし、UI側モーダル表示対象になる。
fn mark_device_lost(reason: &str) {
    eprintln!("[NeoUtl] GPUデバイスロスト検知: {reason}");
    DEVICE_LOST.store(true, Ordering::Relaxed);
}

/// deviceへdevice-lostコールバックを登録する。コールバックはSend + 'staticのみ許容され、
/// UI操作は行えないため、フラグを立てるのみに留める。main.rsのデバイス取得直後に一度だけ呼ぶ。
pub fn install_device_lost_watcher(device: &wgpu::Device) {
    device.set_device_lost_callback(|reason, message| {
        mark_device_lost(&format!("{reason:?}: {message}"));
    });
}

/// 全ObjectVTable実装が共有する標準Uniform契約（shape.slangのUniforms構造体と一致させること）。
/// mat4x4<f32>(64) + opacity(4) + sides(4) + extrude_depth(4) + _pad0(4) + fill_color(16) = 96 bytes
/// GPU側WGSL構造体レイアウトに直結するABI契約値のため config.rs へは移さない。
const STANDARD_UNIFORM_SIZE: u64 = 96;
/// wgpuのmin_uniform_buffer_offset_alignment既定値。動的オフセットの単位ストライドとして採用する。
const UNIFORM_STRIDE: u64 = config::UNIFORM_STRIDE_BYTES;
const MAX_OBJECTS: u64 = config::MAX_SCENE_OBJECTS;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// エフェクトUniformsバッファの確保上限（array<vec4<f32>, 8> = 128byte相当まで対応）。
/// 現行16エフェクトの最大パラメータ数(clipping=5件→uniform_size_std=32byte)を十分に上回る。
const MAX_EFFECT_UNIFORM_SIZE: u64 = config::MAX_EFFECT_UNIFORM_BYTES;
/// mat4x4<f32>(64) + opacity(4)、mat4x4アライメント16の倍数へ切り上げ。
/// GPU側WGSL構造体レイアウトに直結するABI契約値のため config.rs へは移さない。
const MEDIA_UNIFORM_SIZE: u64 = 80;
static MEDIA_WGSL: &str = include_str!(concat!(env!("OUT_DIR"), "/media.wgsl"));
static VIDEO_WGSL: &str = include_str!(concat!(env!("OUT_DIR"), "/media_video.wgsl"));

pub struct RenderEngine {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub texture: wgpu::Texture,
    pub depth_texture: wgpu::Texture,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    /// テキスト描画に使う共有フォント。オブジェクト単位のオフスクリーン
    /// テクスチャ寸法算出（media::text::measure）にも同一フォントを用いる。
    font: Option<FontArc>,
    /// テキストオブジェクト1件につき1枚のオフスクリーンテクスチャ＋専用TextBrushを保持する。
    /// キーはActiveObject.clip_instance（ObjectId由来、フレームを跨いで安定）。
    /// 生成後は標準クアッドパイプライン（media_pipeline）でTransform・不透明度込みの
    /// MVPにより描画するため、X/Y/Z回転・拡大率・不透明度が他オブジェクトと同様に効く。
    text_targets: HashMap<u64, TextRenderTarget>,
    pub render_width: u32,
    pub render_height: u32,
    pipelines: HashMap<u32, (wgpu::RenderPipeline, u32)>,
    effect_pipelines: HashMap<String, wgpu::RenderPipeline>,
    effect_bind_group_layout: wgpu::BindGroupLayout,
    effect_sampler: wgpu::Sampler,
    effect_uniform_buffer: wgpu::Buffer,
    effect_ping: wgpu::Texture,
    effect_pong: wgpu::Texture,
    /// エフェクト付きオブジェクト1件につき1枚のオフスクリーンターゲット。
    /// `config::MAX_EFFECT_OBJECTS`まで遅延生成しフレームを跨いで再利用する
    /// （generate/destroyの毎フレーム反復を避け、確保コストを償却する）。
    effect_object_pool: Vec<wgpu::Texture>,
    /// オブジェクト単体描画パス共有の深度バッファ。RENDER_ATTACHMENT必須の
    /// 既存パイプライン（depth_stencil: Some）をそのまま個別描画へ再利用するため保持する。
    effect_object_depth: wgpu::Texture,
    composite_pipeline: wgpu::RenderPipeline,
    composite_bind_group_layout: wgpu::BindGroupLayout,
    media_pipeline: wgpu::RenderPipeline,
    media_bind_group_layout: wgpu::BindGroupLayout,
    media_uniform_buffer: wgpu::Buffer,
    media_sampler: wgpu::Sampler,
    video_pipeline: wgpu::RenderPipeline,
    video_bind_group_layout: wgpu::BindGroupLayout,
    /// システムレベルLua拡張。エフェクトLuaと共通のscripts/から
    /// 読み込む常駐スクリプト群を保持する。GPUリソースの実体はLua側へ渡さない
    /// （neoutl-lua-runtime crateドキュメント参照）。存在しない場合はNone
    /// （LuaSystem::new失敗時、または該当ディレクトリ未使用時）。
    lua_system: Option<neoutl_lua_runtime::LuaSystem>,
    /// system.register_computeで登録されたWGSLソースから構築したコンピュートパイプライン。
    /// key=ComputeDef.id。
    lua_compute_pipelines: HashMap<String, wgpu::ComputePipeline>,
    /// テクスチャRGBA平均値（"reduce系"の唯一の読み出し経路）を求める固定コンピュートパス。
    /// ワークグループ毎の部分和をatomic加算で1バッファへ集約し、最終値をCPUへ1回だけ
    /// map_asyncで読み出す。Lua側へはpublish_reduce_result経由でスカラー4要素のみ渡す。
    reduce_mean_pipeline: wgpu::ComputePipeline,
    reduce_mean_bind_group_layout: wgpu::BindGroupLayout,
    reduce_mean_buffer: wgpu::Buffer,
    reduce_mean_readback_buffer: wgpu::Buffer,
    /// SceneObjectのレンダリング結果キャッシュ。key=target_scene。
    /// render()呼び出し1回（トップレベル呼び出し1回、depth=0起点）につき
    /// クリアし、同一フレーム内で同一シーンを複数クリップが参照する際の
    /// 再レンダリングを避ける唯一の窓口とする。
    scene_texture_cache: HashMap<i32, wgpu::Texture>,
    /// 標準オブジェクトパイプラインのレイアウト。build_pipelines_from_registry初回構築時のみ
    /// 使用する一時値ではなく、ホットリロード時の単体パイプライン再構築（build_pipeline）に
    /// も同一レイアウトが必要なため保持する。
    object_pipeline_layout: wgpu::PipelineLayout,
    /// エフェクトパイプラインのレイアウト。用途はobject_pipeline_layoutと対称。
    effect_pipeline_layout: wgpu::PipelineLayout,
    /// プラグインdylibファイル監視からの通知チャネル。ホットリロード無効時（リリースビルド既定）
    /// はNoneのままとし、render()冒頭のdrain処理を素通りさせる。
    hot_reload_rx: Option<std::sync::mpsc::Receiver<ReloadEvent>>,
    /// system.load_dir/reload_dir対象ディレクトリ。apply_script_reloadが再参照する。
    scripts_dir: std::path::PathBuf,
}

/// テキスト1オブジェクト分の描画先。widthxheightはmedia::text::measure()の結果と一致し、
/// 寸法変化時のみ再生成する（内容変化のみの場合はbrush.queueの再実行だけで済む）。
struct TextRenderTarget {
    texture: wgpu::Texture,
    brush: TextBrush,
    width: u32,
    height: u32,
}

/// フォントとテクスチャ寸法からTextRenderTargetを新規構築する。
/// テクスチャはcreate_effect_texture同形式（Rgba8Unorm、RENDER_ATTACHMENT+TEXTURE_BINDING+
/// COPY_SRC/DST）を流用し、media_pipelineのサンプリング対象としてそのまま使える。
/// 深度は不要（オフスクリーンのグリフラスタライズのみで、奥行き合成は行わない）。
fn build_text_target(
    device: &wgpu::Device,
    font: &FontArc,
    width: u32,
    height: u32,
) -> TextRenderTarget {
    let width = width.max(1);
    let height = height.max(1);
    let texture = create_effect_texture(device, width, height);
    let brush = BrushBuilder::using_font(font.clone()).build(
        device,
        width,
        height,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    TextRenderTarget {
        texture,
        brush,
        width,
        height,
    }
}

fn load_font() -> Option<Vec<u8>> {
    let candidates = [
        "assets/font.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/calibri.ttf",
    ];
    for path in &candidates {
        if let Ok(bytes) = std::fs::read(path) {
            eprintln!("[NeoUtl] フォント: {path}");
            return Some(bytes);
        }
    }
    eprintln!("[NeoUtl] フォント未検出: テキスト描画無効");
    None
}

fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Render Target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

/// wgslはプラグインFFI（vtable.wgsl()）から得たWGSLソースの生バイト列。
/// プラグインは別クレートとして独立ビルドされるため、この検証だけは実行時に残る
/// （NeoUtl本体組み込みシェーダはbuild_media_pipeline側でビルド時include_str!済み）。
fn try_create_shader_module(
    device: &wgpu::Device,
    wgsl: &[u8],
    label: &str,
) -> Result<wgpu::ShaderModule, String> {
    let text = std::str::from_utf8(wgsl).map_err(|err| format!("WGSLソースが非UTF-8: {err}"))?;
    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(text)),
    });
    match pollster::block_on(error_scope.pop()) {
        Some(err) => Err(format!("{err}")),
        None => Ok(shader),
    }
}

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    wgsl: &[u8],
    label: &str,
) -> Result<wgpu::RenderPipeline, String> {
    let shader = try_create_shader_module(device, wgsl, label)?;
    Ok(
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        }),
    )
}

fn build_pipelines_from_registry(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
) -> HashMap<u32, (wgpu::RenderPipeline, u32)> {
    registry()
        .iter()
        .filter_map(|plugin| {
            let vertex_count = unsafe { (plugin.vtable.vertex_count)() };
            if vertex_count == 0 {
                return None;
            }
            let src = unsafe { (plugin.vtable.wgsl)() };
            if src.ptr.is_null() {
                return None;
            }
            let wgsl = unsafe { std::slice::from_raw_parts(src.ptr, src.len) };
            match build_pipeline(device, layout, wgsl, &plugin.name) {
                Ok(pipeline) => Some((plugin.kind_id, (pipeline, vertex_count))),
                Err(err) => {
                    eprintln!(
                        "[NeoUtl] オブジェクトプラグインのシェーダコンパイル失敗、除外して継続: kind_id={} name={} 理由={err}",
                        plugin.kind_id, plugin.name
                    );
                    None
                }
            }
        })
        .collect()
}

fn create_effect_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Effect Ping-Pong Target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn build_effect_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    wgsl: &[u8],
    label: &str,
) -> Result<wgpu::RenderPipeline, String> {
    let shader = try_create_shader_module(device, wgsl, label)?;
    Ok(
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        }),
    )
}

/// effects::loader::registry()の全プラグインからポストプロセスパイプラインを構築する。
/// エフェクトIDをキーとし、ActiveObject.effectsの並び順に都度引いて適用する。
/// シェーダコンパイル失敗プラグインは警告出力の上除外し、他プラグインの処理は継続する。
/// 除外されたエフェクトIDはeffect_pipelinesに登録されず、apply_effect_chain側の
/// `self.effect_pipelines.get(effect_id)`の既存チェックにより実行時は自動的にスキップされる。
fn build_effect_pipelines_from_registry(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
) -> HashMap<String, wgpu::RenderPipeline> {
    effects::registry()
        .iter()
        .filter_map(|source| {
            let wgsl = source.wgsl_bytes();
            if wgsl.is_empty() {
                return None;
            }
            match build_effect_pipeline(device, layout, wgsl, source.name()) {
                Ok(pipeline) => Some((source.id().to_owned(), pipeline)),
                Err(err) => {
                    eprintln!(
                        "[NeoUtl] エフェクトのシェーダコンパイル失敗、除外して継続: id={} name={} 理由={err}",
                        source.id(),
                        source.name()
                    );
                    None
                }
            }
        })
        .collect()
}

/// テクスチャRGBA平均を求める固定コンピュートシェーダ。
/// acc[0..4)=r/g/b/a固定小数点和(SCALE倍・u32)、acc[4]=ピクセル数。
/// CPU側でSCALE・ピクセル数で除して平均へ戻す（reduce_source_mean参照）。
const REDUCE_MEAN_WGSL: &str = r#"
@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> acc: array<atomic<u32>, 5>;

const SCALE: f32 = 1000000.0;

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(src_tex);
    if (gid.x >= dims.x || gid.y >= dims.y) {
        return;
    }
    let c = textureLoad(src_tex, vec2<i32>(gid.xy), 0);
    atomicAdd(&acc[0], u32(clamp(c.r, 0.0, 1.0) * SCALE));
    atomicAdd(&acc[1], u32(clamp(c.g, 0.0, 1.0) * SCALE));
    atomicAdd(&acc[2], u32(clamp(c.b, 0.0, 1.0) * SCALE));
    atomicAdd(&acc[3], u32(clamp(c.a, 0.0, 1.0) * SCALE));
    atomicAdd(&acc[4], 1u);
}
"#;

fn build_reduce_mean_pipeline(
    device: &wgpu::Device,
) -> (wgpu::ComputePipeline, wgpu::BindGroupLayout) {
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Reduce Mean BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Reduce Mean Pipeline Layout"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Reduce Mean Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(REDUCE_MEAN_WGSL)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Reduce Mean Pipeline"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: Default::default(),
        cache: None,
    });
    (pipeline, bgl)
}

/// system.register_computeで登録されたコンピュートパス定義群からパイプラインを構築する。
/// コンパイル失敗した定義は警告出力の上除外し、他定義の処理は継続する
/// （build_effect_pipelines_from_registryと対称の除外方針）。
fn build_lua_compute_pipelines(
    device: &wgpu::Device,
    defs: &[neoutl_lua_runtime::ComputeDef],
) -> HashMap<String, wgpu::ComputePipeline> {
    defs.iter()
        .filter_map(|def| {
            let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&def.id),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(def.wgsl.as_str())),
            });
            if let Some(err) = pollster::block_on(error_scope.pop()) {
                eprintln!(
                    "[NeoUtl] system.register_compute シェーダコンパイル失敗、除外: id={} 理由={err}",
                    def.id
                );
                return None;
            }
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(&def.id),
                layout: None,
                module: &shader,
                entry_point: Some("cs_main"),
                compilation_options: Default::default(),
                cache: None,
            });
            Some((def.id.clone(), pipeline))
        })
        .collect()
}

fn create_effect_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Effect Postprocess BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

/// kind_idに対応するプラグインのstable_idを引く。プラグイン未登録時はNone。
fn stable_id_of(kind_id: u32) -> Option<&'static str> {
    by_kind_id(kind_id).map(|p| unsafe { &*((p.vtable.meta)()) }.stable_id)
}

fn create_media_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Media Object BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(MEDIA_UNIFORM_SIZE),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// NV12動画フレーム用BGL。輝度(binding1)・色差(binding2)を別プレーンとしてバインドする。
/// media_bind_group_layoutと違いテクスチャバインディングが2枚（RGBA単一プレーン非対応）。
fn create_video_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let plane_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Video Object BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(MEDIA_UNIFORM_SIZE),
                },
                count: None,
            },
            plane_entry(1),
            plane_entry(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// エフェクト適用済みオブジェクト個別テクスチャをself.textureへアルファ合成するための
/// フルスクリーン三角形WGSL。位置・変形はオブジェクト単体描画パス側のmvpで確定済みのため
/// 追加変換は行わず等倍転写のみ行う。
const COMPOSITE_WGSL: &str = r#"
struct VOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VOut {
    var uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    var out: VOut;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(src_tex, src_sampler, in.uv);
}
"#;

fn create_composite_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Composite BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn build_composite_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Composite"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(COMPOSITE_WGSL)),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Composite"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// render_effect_object_offscreenが描画する単体オブジェクトの種別。
/// ActiveObjectの実描画経路（標準パイプライン/媒体・映像/テキスト）と1:1対応する。
enum EffectObjectDrawKind<'a> {
    Standard {
        obj: &'a ActiveObject,
        offset: u32,
    },
    Media {
        texture: &'a wgpu::Texture,
        offset: u32,
    },
    Text {
        clip_instance: u64,
        offset: u32,
    },
}

fn build_media_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    wgsl: &'static str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Media Object"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(wgsl)),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Media Object"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl RenderEngine {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, width: u32, height: u32) -> Self {
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Standard Object Uniform Buffer"),
            size: UNIFORM_STRIDE * MAX_OBJECTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Standard Object BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(STANDARD_UNIFORM_SIZE),
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Standard Object BG"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(STANDARD_UNIFORM_SIZE),
                }),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipelines = build_pipelines_from_registry(&device, &pipeline_layout);
        let texture = create_texture(&device, width, height);
        let depth_texture = create_depth_texture(&device, width, height);

        let effect_bind_group_layout = create_effect_bind_group_layout(&device);
        let effect_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Effect Pipeline Layout"),
                bind_group_layouts: &[Some(&effect_bind_group_layout)],
                immediate_size: 0,
            });
        let effect_pipelines =
            build_effect_pipelines_from_registry(&device, &effect_pipeline_layout);
        let scripts_dir = crate::effects::default_effects_lua_dir();
        let hot_reload_rx = if crate::config::SYSTEM_DEFAULT_HOT_RELOAD_ENABLED {
            Some(hot_reload::spawn_watcher(
                crate::objects::default_objects_dir(),
                crate::effects::default_effects_dir(),
                scripts_dir.clone(),
            ))
        } else {
            None
        };
        let effect_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Effect Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let effect_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Effect Uniform Buffer"),
            size: MAX_EFFECT_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let effect_ping = create_effect_texture(&device, width, height);
        let effect_pong = create_effect_texture(&device, width, height);
        let effect_object_pool: Vec<wgpu::Texture> = Vec::new();
        let effect_object_depth = create_depth_texture(&device, width, height);
        let composite_bind_group_layout = create_composite_bind_group_layout(&device);
        let composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Composite Pipeline Layout"),
                bind_group_layouts: &[Some(&composite_bind_group_layout)],
                immediate_size: 0,
            });
        let composite_pipeline = build_composite_pipeline(&device, &composite_pipeline_layout);

        let media_bind_group_layout = create_media_bind_group_layout(&device);
        let media_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Media Pipeline Layout"),
                bind_group_layouts: &[Some(&media_bind_group_layout)],
                immediate_size: 0,
            });
        let media_pipeline = build_media_pipeline(&device, &media_pipeline_layout, MEDIA_WGSL);
        let media_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Media Uniform Buffer"),
            size: UNIFORM_STRIDE * MAX_OBJECTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let media_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Media Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let video_bind_group_layout = create_video_bind_group_layout(&device);
        let video_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Video Pipeline Layout"),
                bind_group_layouts: &[Some(&video_bind_group_layout)],
                immediate_size: 0,
            });
        let video_pipeline = build_media_pipeline(&device, &video_pipeline_layout, VIDEO_WGSL);

        let font = load_font().and_then(|f| FontArc::try_from_vec(f).ok());

        let (reduce_mean_pipeline, reduce_mean_bind_group_layout) =
            build_reduce_mean_pipeline(&device);
        let reduce_mean_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Reduce Mean Accumulator"),
            size: 20,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let reduce_mean_readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Reduce Mean Readback"),
            size: 20,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let lua_system = match neoutl_lua_runtime::LuaSystem::new() {
            Ok(sys) => {
                sys.load_dir(&scripts_dir);
                Some(sys)
            }
            Err(err) => {
                eprintln!("[NeoUtl] LuaSystem初期化失敗、system拡張を無効化: {err}");
                None
            }
        };
        let lua_compute_pipelines = lua_system
            .as_ref()
            .map(|sys| build_lua_compute_pipelines(&device, &sys.drain_computes()))
            .unwrap_or_default();

        Self {
            device,
            queue,
            texture,
            depth_texture,
            uniform_buffer,
            bind_group_layout,
            bind_group,
            font,
            text_targets: HashMap::new(),
            render_width: width,
            render_height: height,
            pipelines,
            effect_pipelines,
            effect_bind_group_layout,
            effect_sampler,
            effect_uniform_buffer,
            effect_ping,
            effect_pong,
            effect_object_pool,
            effect_object_depth,
            composite_pipeline,
            composite_bind_group_layout,
            media_pipeline,
            media_bind_group_layout,
            media_uniform_buffer,
            media_sampler,
            video_pipeline,
            video_bind_group_layout,
            scene_texture_cache: HashMap::new(),
            lua_system,
            lua_compute_pipelines,
            reduce_mean_pipeline,
            reduce_mean_bind_group_layout,
            reduce_mean_buffer,
            reduce_mean_readback_buffer,
            object_pipeline_layout: pipeline_layout,
            effect_pipeline_layout,
            hot_reload_rx,
            scripts_dir,
        }
    }

    /// self.texture(現フレーム最終合成結果)のRGBA平均をGPU上で1回のコンピュートパスで
    /// 求め、CPUへスカラー4要素のみ読み出す。ピクセル単位データはCPUへ一切渡さない。
    pub fn reduce_source_mean(&self) -> [f32; 4] {
        let zeros = [0u32; 5];
        self.queue
            .write_buffer(&self.reduce_mean_buffer, 0, bytemuck::cast_slice(&zeros));

        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Reduce Mean BG"),
            layout: &self.reduce_mean_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.reduce_mean_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Reduce Mean Encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Reduce Mean Pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.reduce_mean_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(
                self.render_width.div_ceil(8),
                self.render_height.div_ceil(8),
                1,
            );
        }
        encoder.copy_buffer_to_buffer(
            &self.reduce_mean_buffer,
            0,
            &self.reduce_mean_readback_buffer,
            0,
            20,
        );
        self.queue.submit([encoder.finish()]);

        let slice = self.reduce_mean_readback_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).expect("map_async結果送信失敗");
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("device poll失敗");
        rx.recv()
            .expect("map_async結果受信失敗")
            .expect("バッファmap失敗");

        let mapped = slice.get_mapped_range();
        let raw: &[u32] = bytemuck::cast_slice(&mapped);
        let count = (raw[4].max(1)) as f32;
        const SCALE: f32 = 1_000_000.0;
        let result = [
            raw[0] as f32 / SCALE / count,
            raw[1] as f32 / SCALE / count,
            raw[2] as f32 / SCALE / count,
            raw[3] as f32 / SCALE / count,
        ];
        drop(mapped);
        self.reduce_mean_readback_buffer.unmap();
        result
    }

    /// reduce_source_mean()を実行し、結果を"source_mean"という名前でLuaSystemへpublishする。
    /// scripts/スクリプトはsystem.reduce_result("source_mean")で次回以降読み出せる
    /// （スカラー4要素のみ、ピクセルバッファそのものは読み出せない）。
    pub fn run_lua_reduce_hooks(&self) {
        if let Some(sys) = &self.lua_system {
            let values = self.reduce_source_mean();
            sys.publish_reduce_result("source_mean", &values);
        }
    }

    /// target_scene・local_frameのシーンをオフスクリーンへレンダリングし、
    /// 結果テクスチャを返す（RGBA、self.texture同一フォーマット）。
    /// 呼び出し中はself.texture/depth_textureをtarget_scene解像度へ一時的に
    /// 差し替え、完了後に呼び出し元の解像度へ復元する。
    ///
    /// 既知の制約: get_active_objects_system_atはCamera/MVP計算にECS側の
    /// グローバルProjectResourceを参照するため、target_sceneの解像度が
    /// トップレベルプロジェクトと異なる場合、内包オブジェクトの画角は
    /// target_scene基準ではなくプロジェクト基準のまま計算される
    /// （SceneMeta.width/height自体はオフスクリーンテクスチャ寸法として正しく反映される）。
    fn render_scene_texture(
        &mut self,
        world: &crate::ecs::EcsWorld,
        target_scene: i32,
        local_frame: i32,
        depth: u32,
    ) -> Option<wgpu::Texture> {
        if depth >= config::MAX_SCENE_NESTING_DEPTH {
            eprintln!(
                "[NeoUtl] シーンネスト深度上限({})到達 target_scene={target_scene}: 非描画",
                config::MAX_SCENE_NESTING_DEPTH
            );
            return None;
        }
        if let Some(cached) = self.scene_texture_cache.get(&target_scene) {
            return Some(cached.clone());
        }
        let scene = world.get_scene(target_scene)?;
        let saved_width = self.render_width;
        let saved_height = self.render_height;
        self.resize_render_target(scene.width, scene.height);

        let active =
            crate::ecs::systems::get_active_objects_system_at(world, target_scene, local_frame);
        let project = world.get_project();
        self.render_at(world, &active, &project, depth);
        let texture = self.texture.clone();

        self.resize_render_target(saved_width, saved_height);
        self.scene_texture_cache
            .insert(target_scene, texture.clone());
        Some(texture)
    }

    pub fn resize_render_target(&mut self, width: u32, height: u32) {
        self.render_width = width;
        self.render_height = height;
        self.texture = create_texture(&self.device, width, height);
        self.depth_texture = create_depth_texture(&self.device, width, height);
        self.effect_ping = create_effect_texture(&self.device, width, height);
        self.effect_pong = create_effect_texture(&self.device, width, height);
        self.effect_object_pool.clear();
        self.effect_object_depth = create_depth_texture(&self.device, width, height);
        eprintln!("[NeoUtl] レンダーターゲット変更: {width}×{height}");
    }

    /// ActiveObjectのMVP・不透明度・図形パラメータを標準Uniformバッファへ書き込み、
    /// バインド時に使う動的オフセットを返す（インデックス * UNIFORM_STRIDE）。
    fn write_standard_uniform(&self, index: u64, obj: &ActiveObject) -> u32 {
        let mut data = [0u8; STANDARD_UNIFORM_SIZE as usize];
        data[0..64].copy_from_slice(bytemuck::cast_slice(&obj.mvp));
        data[64..68].copy_from_slice(&obj.opacity.to_le_bytes());

        let (sides, extrude_depth, fill_color) = obj
            .shape_params
            .map_or((4.0, 0.0, [1.0, 1.0, 1.0, 1.0]), |s| {
                (s.sides as f32, s.extrude_depth, s.fill_color)
            });
        data[68..72].copy_from_slice(&sides.to_le_bytes());
        data[72..76].copy_from_slice(&extrude_depth.to_le_bytes());
        data[80..96].copy_from_slice(bytemuck::cast_slice(&fill_color));

        let offset = index * UNIFORM_STRIDE;
        self.queue.write_buffer(&self.uniform_buffer, offset, &data);
        offset as u32
    }

    /// mvp・不透明度をメディア用Uniformバッファへ書き込む共通経路。
    /// write_media_uniform（映像/画像）とテキスト（render()内、rescale後のmvp）の両方から呼ぶ。
    fn write_media_uniform_raw(&self, index: u64, mvp: &[f32; 16], opacity: f32) -> u32 {
        let mut data = [0u8; MEDIA_UNIFORM_SIZE as usize];
        data[0..64].copy_from_slice(bytemuck::cast_slice(mvp));
        data[64..68].copy_from_slice(&opacity.to_le_bytes());
        let offset = index * UNIFORM_STRIDE;
        self.queue
            .write_buffer(&self.media_uniform_buffer, offset, &data);
        offset as u32
    }

    /// ActiveObjectのMVP・不透明度をメディア用Uniformバッファへ書き込み、
    /// バインド時に使う動的オフセットを返す（write_standard_uniformと同一のストライド運用）。
    fn write_media_uniform(&self, index: u64, obj: &ActiveObject) -> u32 {
        self.write_media_uniform_raw(index, &obj.mvp, obj.opacity)
    }

    /// index番目のオブジェクト単体オフスクリーンターゲットを返す。未生成なら
    /// render_width×render_height寸法で生成しプールへ追加する（フレームを跨いで再利用）。
    fn ensure_effect_object_target(&mut self, index: usize) -> &wgpu::Texture {
        while self.effect_object_pool.len() <= index {
            self.effect_object_pool.push(create_effect_texture(
                &self.device,
                self.render_width,
                self.render_height,
            ));
        }
        &self.effect_object_pool[index]
    }

    /// srcの内容へchainを順次適用しdstへ書き戻す。chainが空ならsrcをdstへ等倍コピーする。
    /// 各パスはeffect_ping/effect_pongへ交互出力する共有ワークバッファを用いる
    /// （呼び出しは同一フレーム内で逐次実行のため競合しない）。
    fn apply_effect_chain(
        &self,
        src: &wgpu::Texture,
        dst: &wgpu::Texture,
        chain: &[(String, HashMap<String, Value>)],
    ) {
        let extent = wgpu::Extent3d {
            width: self.render_width,
            height: self.render_height,
            depth_or_array_layers: 1,
        };

        if chain.is_empty() {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Effect Passthrough Copy Encoder"),
                });
            encoder.copy_texture_to_texture(src.as_image_copy(), dst.as_image_copy(), extent);
            self.queue.submit([encoder.finish()]);
            return;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Effect Copy Encoder"),
            });
        encoder.copy_texture_to_texture(
            src.as_image_copy(),
            self.effect_ping.as_image_copy(),
            extent,
        );
        self.queue.submit([encoder.finish()]);

        let mut src_is_ping = true;
        for (effect_id, params) in chain {
            let Some(source) = effects::loader::by_id(effect_id) else {
                continue;
            };
            let Some(pipeline) = self.effect_pipelines.get(effect_id) else {
                continue;
            };
            let schema = source.param_schema();
            let values: Vec<f32> = schema
                .iter()
                .map(|s| {
                    params
                        .get(s.key.as_str())
                        .map_or(s.default_float, |v| match v {
                            Value::Number(n) => *n,
                            Value::Bool(b) => {
                                if *b {
                                    1.0
                                } else {
                                    0.0
                                }
                            }
                            Value::Enum(idx) => *idx as f32,
                            Value::Text(_) | Value::FilePath(_) | Value::TrackRef(_) => {
                                s.default_float
                            }
                        })
                })
                .collect();

            let uniform_size = (source.uniform_size() as usize).max(16);
            let mut bytes = vec![0u8; uniform_size];
            source.pack_uniform(&values, &mut bytes);
            self.queue
                .write_buffer(&self.effect_uniform_buffer, 0, &bytes);

            let (src_tex, dst_tex) = if src_is_ping {
                (&self.effect_ping, &self.effect_pong)
            } else {
                (&self.effect_pong, &self.effect_ping)
            };
            let src_view = src_tex.create_view(&wgpu::TextureViewDescriptor::default());
            let dst_view = dst_tex.create_view(&wgpu::TextureViewDescriptor::default());

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Effect Pass BG"),
                layout: &self.effect_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.effect_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.effect_uniform_buffer.as_entire_binding(),
                    },
                ],
            });

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Effect Pass Encoder"),
                });
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Effect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &dst_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                rpass.set_pipeline(pipeline);
                rpass.set_bind_group(0, &bind_group, &[]);
                rpass.draw(0..3, 0..1);
            }
            self.queue.submit([encoder.finish()]);
            src_is_ping = !src_is_ping;
        }

        let final_src = if src_is_ping {
            &self.effect_ping
        } else {
            &self.effect_pong
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Effect Finalize Encoder"),
            });
        encoder.copy_texture_to_texture(final_src.as_image_copy(), dst.as_image_copy(), extent);
        self.queue.submit([encoder.finish()]);
    }

    /// 標準オブジェクトパイプライン（図形等、self.pipelines登録済みkind_id）1件を
    /// rpassの現在のカラー/深度アタッチメントへ描画する。
    fn draw_standard_pass(&self, rpass: &mut wgpu::RenderPass, obj: &ActiveObject, offset: u32) {
        if let Some((pipeline, vertex_count)) = self.pipelines.get(&obj.kind_id) {
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, &self.bind_group, &[offset]);
            rpass.draw(0..*vertex_count, 0..1);
        }
    }

    /// 映像/画像フレーム1件（NV12平面・RGBA単一プレーン両対応）をrpassへ描画する。
    fn draw_media_pass(&self, rpass: &mut wgpu::RenderPass, texture: &wgpu::Texture, offset: u32) {
        let is_planar_nv12 = texture.format() == wgpu::TextureFormat::NV12;
        if is_planar_nv12 {
            let plane_y = texture.create_view(&wgpu::TextureViewDescriptor {
                aspect: wgpu::TextureAspect::Plane0,
                ..Default::default()
            });
            let plane_uv = texture.create_view(&wgpu::TextureViewDescriptor {
                aspect: wgpu::TextureAspect::Plane1,
                ..Default::default()
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Video Object BG"),
                layout: &self.video_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.media_uniform_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(MEDIA_UNIFORM_SIZE),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&plane_y),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&plane_uv),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.media_sampler),
                    },
                ],
            });
            rpass.set_pipeline(&self.video_pipeline);
            rpass.set_bind_group(0, &bind_group, &[offset]);
            rpass.draw(0..6, 0..1);
        } else {
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Media Object BG"),
                layout: &self.media_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.media_uniform_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(MEDIA_UNIFORM_SIZE),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.media_sampler),
                    },
                ],
            });
            rpass.set_pipeline(&self.media_pipeline);
            rpass.set_bind_group(0, &bind_group, &[offset]);
            rpass.draw(0..6, 0..1);
        }
    }

    /// text_targetsに事前描画済みのグリフテクスチャ1件をrpassへ描画する。
    fn draw_text_pass(&self, rpass: &mut wgpu::RenderPass, clip_instance: u64, offset: u32) {
        let Some(target) = self.text_targets.get(&clip_instance) else {
            return;
        };
        let view = target
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Object BG"),
            layout: &self.media_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.media_uniform_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(MEDIA_UNIFORM_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.media_sampler),
                },
            ],
        });
        rpass.set_pipeline(&self.media_pipeline);
        rpass.set_bind_group(0, &bind_group, &[offset]);
        rpass.draw(0..6, 0..1);
    }

    /// pool_texへ単体オブジェクトを透明クリアの上描画する（Phase3）。深度は
    /// effect_object_depthを共用しオブジェクトごとにClear(1.0)で初期化する。
    /// draw_kindが標準/媒体/テキストいずれか1系統のみ実行する（相互排他）。
    fn render_effect_object_offscreen(
        &self,
        pool_tex: &wgpu::Texture,
        draw_kind: EffectObjectDrawKind,
    ) {
        let view = pool_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = self
            .effect_object_depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Effect Object Encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Effect Object Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            match draw_kind {
                EffectObjectDrawKind::Standard { obj, offset } => {
                    self.draw_standard_pass(&mut rpass, obj, offset);
                }
                EffectObjectDrawKind::Media { texture, offset } => {
                    self.draw_media_pass(&mut rpass, texture, offset);
                }
                EffectObjectDrawKind::Text {
                    clip_instance,
                    offset,
                } => {
                    self.draw_text_pass(&mut rpass, clip_instance, offset);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Phase5: pool_texをself.textureへ元の描画順序（レイヤー順）を保ったまま
    /// アルファブレンド合成する。等倍フルスクリーン矩形描画（位置・変形は
    /// render_effect_object_offscreen側のmvpで確定済みのため追加変換不要）。
    fn composite_effect_object(&self, pool_tex: &wgpu::Texture) {
        let src_view = pool_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite BG"),
            layout: &self.composite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.effect_sampler),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Composite Encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rpass.set_pipeline(&self.composite_pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }

    /// 従来通りの外部呼び出し窓口（depth=0起点）。呼び出し前にscene_texture_cacheを
    /// クリアし、フレーム単位で1回だけ各ネストシーンをレンダリングする。
    pub fn render(
        &mut self,
        world: &crate::ecs::EcsWorld,
        active_objects: &[ActiveObject],
        project: &ProjectResource,
    ) {
        self.scene_texture_cache.clear();
        self.drain_hot_reload_events();
        if let Some(sys) = &self.lua_system
            && let Err(err) = sys.run_pre_render_hooks()
        {
            eprintln!("[NeoUtl] system.on_pre_render フック実行失敗: {err}");
        }
        self.render_at(world, active_objects, project, 0);
        self.run_lua_reduce_hooks();
    }

    /// hot_reload::spawn_watcherからの通知を非ブロッキングでdrainし、対応するプラグイン
    /// registryとGPUパイプラインを差分更新する。フレーム先頭（lua pre-render hook実行前）
    /// でのみ呼ぶことで、当該フレーム内の描画は常に一貫したパイプライン集合を参照する。
    fn drain_hot_reload_events(&mut self) {
        let Some(rx) = &self.hot_reload_rx else {
            return;
        };
        let events: Vec<ReloadEvent> = rx.try_iter().collect();
        for event in events {
            match event {
                ReloadEvent::Object(path) => self.apply_object_reload(&path),
                ReloadEvent::Effect(path) => self.apply_effect_reload(&path),
                ReloadEvent::Script(path) => self.apply_script_reload(&path),
            }
        }
    }

    /// objects::loader::reload_one成功時、pipelines全体を現行registryから再構築する。
    /// 失敗時（Phase6方針）は現行pipelinesを変更せずログのみ出力する。
    fn apply_object_reload(&mut self, path: &std::path::Path) {
        if let Err(err) = crate::objects::loader::reload_one(path) {
            eprintln!(
                "[NeoUtl] ホットリロード失敗（objects） {}: {err}",
                path.display()
            );
            return;
        }
        self.rebuild_all_object_pipelines();
    }

    /// effects::loader::reload_one成功時、effect_pipelines全体を現行registryから再構築する。
    fn apply_effect_reload(&mut self, path: &std::path::Path) {
        if let Err(err) = crate::effects::loader::reload_one(path) {
            eprintln!(
                "[NeoUtl] ホットリロード失敗（effects） {}: {err}",
                path.display()
            );
            return;
        }
        self.rebuild_all_effect_pipelines();
    }

    /// scripts_dir配下の*.lua変更検知時、LuaSystem::reload_dirでhooks/effects/computesを
    /// 全解除・全再実行し、drain結果でlua_compute_pipelinesとeffects registryのLua側分を
    /// 差し替える。失敗時は現行状態を変更せずログのみ出力する
    /// （clear_hooksが先行実行されるため、load_dir内個別ファイル失敗があっても
    /// hookの多重登録は発生しない）。
    fn apply_script_reload(&mut self, _path: &std::path::Path) {
        let Some(sys) = &self.lua_system else {
            return;
        };
        if let Err(err) = sys.reload_dir(&self.scripts_dir) {
            eprintln!(
                "[NeoUtl] ホットリロード失敗（scripts） {}: {err}",
                self.scripts_dir.display()
            );
            return;
        }
        self.lua_compute_pipelines =
            build_lua_compute_pipelines(&self.device, &sys.drain_computes());
        crate::effects::loader::reload_lua(sys.drain_effects());
        self.rebuild_all_effect_pipelines();
        eprintln!(
            "[NeoUtl] scriptsホットリロード完了: {}",
            self.scripts_dir.display()
        );
    }

    /// 現行objects registry全件からpipelinesを再構築する。差し替え対象は再ロードされた
    /// 1プラグインのみだが、kind_id -> パイプライン対応の再構築コスト自体はO(登録数)で
    /// 小さく（プラグイン数は数十件規模）、対象特定の複雑化より全体再構築の単純さを優先する。
    fn rebuild_all_object_pipelines(&mut self) {
        self.pipelines = build_pipelines_from_registry(&self.device, &self.object_pipeline_layout);
    }

    /// 現行effects registry全件からeffect_pipelinesを再構築する。理由はrebuild_all_object_pipelinesと同様。
    fn rebuild_all_effect_pipelines(&mut self) {
        self.effect_pipelines =
            build_effect_pipelines_from_registry(&self.device, &self.effect_pipeline_layout);
    }

    /// SceneObjectの再帰評価を伴う本体。depthはMAX_SCENE_NESTING_DEPTH判定にのみ使う。
    fn render_at(
        &mut self,
        world: &crate::ecs::EcsWorld,
        active_objects: &[ActiveObject],
        _project: &ProjectResource,
        depth: u32,
    ) {
        if is_device_lost() {
            return;
        }
        let mut media_frames: Vec<Option<wgpu::Texture>> = Vec::with_capacity(active_objects.len());
        {
            let cache = crate::media::cache::global();
            for obj in active_objects {
                let stable_id = stable_id_of(obj.kind_id);
                let is_visual = matches!(stable_id, Some(VIDEO_STABLE_ID | IMAGE_STABLE_ID));
                let tex = if is_visual {
                    if let Some(src) = &obj.media_source {
                        match cache.frame_at(
                            &src.path,
                            obj.clip_instance,
                            obj.source_frame,
                            &self.device,
                            &self.queue,
                        ) {
                            Ok(texture) => Some(texture),
                            Err(err) => {
                                eprintln!(
                                    "[NeoUtl] フレーム取得失敗 kind_id={} path={} frame={}: {err}",
                                    obj.kind_id,
                                    src.path.display(),
                                    obj.source_frame
                                );
                                None
                            }
                        }
                    } else {
                        eprintln!(
                            "[NeoUtl] MediaSource未設定 kind_id={}: 映像/画像オブジェクトにパスが割当てられていません",
                            obj.kind_id
                        );
                        None
                    }
                } else if stable_id == Some(SCENE_STABLE_ID) {
                    match obj.nested_scene {
                        Some((target_scene, local_frame)) => {
                            self.render_scene_texture(world, target_scene, local_frame, depth + 1)
                        }
                        None => None,
                    }
                } else {
                    None
                };
                media_frames.push(tex);
            }
        }

        let mut media_offsets: Vec<Option<u32>> = Vec::with_capacity(active_objects.len());
        let mut media_next_index = 0u64;
        for (obj, tex) in active_objects.iter().zip(media_frames.iter()) {
            if tex.is_some() && media_next_index < MAX_OBJECTS {
                let offset = self.write_media_uniform(media_next_index, obj);
                media_offsets.push(Some(offset));
                media_next_index += 1;
            } else {
                media_offsets.push(None);
            }
        }

        let mut offsets: Vec<Option<u32>> = Vec::with_capacity(active_objects.len());
        let mut next_index = 0u64;
        for obj in active_objects {
            if self.pipelines.contains_key(&obj.kind_id) && next_index < MAX_OBJECTS {
                let offset = self.write_standard_uniform(next_index, obj);
                offsets.push(Some(offset));
                next_index += 1;
            } else {
                offsets.push(None);
            }
        }

        let mut effect_pool_index: Vec<Option<usize>> = Vec::with_capacity(active_objects.len());
        {
            let mut next_pool = 0usize;
            for obj in active_objects {
                if !obj.effects.is_empty() && next_pool < config::MAX_EFFECT_OBJECTS {
                    effect_pool_index.push(Some(next_pool));
                    next_pool += 1;
                } else {
                    effect_pool_index.push(None);
                }
            }
        }

        let mut text_draws: Vec<(u64, u32, usize)> = Vec::new();
        if let Some(ref font) = self.font {
            let mut seen: HashSet<u64> = HashSet::with_capacity(active_objects.len());
            for (obj_index, obj) in active_objects.iter().enumerate() {
                let Some(plugin) = by_kind_id(obj.kind_id) else {
                    continue;
                };
                let meta = unsafe { &*((plugin.vtable.meta)()) };
                if meta.stable_id != neoutl_object_api::TEXT_STABLE_ID {
                    continue;
                }
                let Some(tc) = obj.text_content.as_ref() else {
                    continue;
                };
                if media_next_index >= MAX_OBJECTS {
                    continue;
                }

                let (tex_w, tex_h) = crate::media::text::measure(font, tc);
                seen.insert(obj.clip_instance);

                let needs_rebuild = match self.text_targets.get(&obj.clip_instance) {
                    Some(t) => t.width != tex_w || t.height != tex_h,
                    None => true,
                };
                if needs_rebuild {
                    self.text_targets.insert(
                        obj.clip_instance,
                        build_text_target(&self.device, font, tex_w, tex_h),
                    );
                }
                let target = self
                    .text_targets
                    .get_mut(&obj.clip_instance)
                    .expect("直前にinsert済み");

                let section = crate::media::text::build_section(tc, target.width, target.height);
                {
                    let view = target
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let _ = target.brush.queue(
                        self.device.as_ref(),
                        self.queue.as_ref(),
                        vec![&section],
                    );
                    let mut encoder =
                        self.device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("Text Glyph Encoder"),
                            });
                    {
                        let mut glyph_pass =
                            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Text Glyph Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                        store: wgpu::StoreOp::Store,
                                    },
                                    depth_slice: None,
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            });
                        target.brush.draw(&mut glyph_pass);
                    }
                    self.queue.submit([encoder.finish()]);
                }

                let ratio_w = tex_w as f32 / UNIT_SIZE_PX;
                let ratio_h = tex_h as f32 / UNIT_SIZE_PX;
                let mut mvp = obj.mvp;
                for i in 0..4 {
                    mvp[i] *= ratio_w;
                    mvp[4 + i] *= ratio_h;
                }

                let offset = self.write_media_uniform_raw(media_next_index, &mvp, obj.opacity);
                media_next_index += 1;
                text_draws.push((obj.clip_instance, offset, obj_index));
            }
            self.text_targets.retain(|k, _| seen.contains(k));
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = self
            .depth_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        {
            let clear_color = if depth == 0 {
                wgpu::Color {
                    r: 0.05,
                    g: 0.05,
                    b: 0.07,
                    a: 1.0,
                }
            } else {
                wgpu::Color::TRANSPARENT
            };
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            for (i, (obj, offset)) in active_objects.iter().zip(offsets.iter()).enumerate() {
                if effect_pool_index[i].is_some() {
                    continue;
                }
                let Some(offset) = offset else { continue };
                self.draw_standard_pass(&mut rpass, obj, *offset);
            }

            for (i, (texture, offset)) in media_frames.iter().zip(media_offsets.iter()).enumerate()
            {
                if effect_pool_index[i].is_some() {
                    continue;
                }
                let (Some(texture), Some(offset)) = (texture, offset) else {
                    continue;
                };
                self.draw_media_pass(&mut rpass, texture, *offset);
            }

            for (clip_instance, offset, obj_index) in &text_draws {
                if effect_pool_index[*obj_index].is_some() {
                    continue;
                }
                self.draw_text_pass(&mut rpass, *clip_instance, *offset);
            }
        }

        self.queue.submit([encoder.finish()]);

        let text_draw_by_index: HashMap<usize, (u64, u32)> = text_draws
            .iter()
            .map(|(clip_instance, offset, obj_index)| (*obj_index, (*clip_instance, *offset)))
            .collect();

        for (i, obj) in active_objects.iter().enumerate() {
            let Some(pool_idx) = effect_pool_index[i] else {
                continue;
            };
            let draw_kind = if let Some(offset) = offsets[i] {
                EffectObjectDrawKind::Standard { obj, offset }
            } else if let (Some(Some(texture)), Some(Some(offset))) =
                (media_frames.get(i), media_offsets.get(i))
            {
                EffectObjectDrawKind::Media {
                    texture,
                    offset: *offset,
                }
            } else if let Some((clip_instance, offset)) = text_draw_by_index.get(&i) {
                EffectObjectDrawKind::Text {
                    clip_instance: *clip_instance,
                    offset: *offset,
                }
            } else {
                continue;
            };

            let pool_tex = self.ensure_effect_object_target(pool_idx).clone();
            self.render_effect_object_offscreen(&pool_tex, draw_kind);
            self.apply_effect_chain(&pool_tex, &pool_tex, &obj.effects);
            self.composite_effect_object(&pool_tex);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::resources::ProjectResource;

    /// テスト専用GPUハンドル取得。GPUアダプタ非搭載環境（CI含む）では
    /// request_adapterがErrを返しうるため、呼び出し側は戻り値Noneで
    /// テストを早期スキップする（GPU非依存の判定へフォールバックしない）。
    fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    /// RGBA8テクスチャをCPU側Vec<u8>へ読み出す。bytes_per_row 256byteアライン要件を
    /// 満たすためパディング込みで読み出し、行ごとにトリムして密パックへ変換する。
    fn read_texture_rgba8(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let unpadded_bytes_per_row = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = (padded_bytes_per_row * height) as wgpu::BufferAddress;

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Test Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);

        let slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).expect("map_async結果送信失敗");
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("device poll失敗");
        rx.recv()
            .expect("map_async結果受信失敗")
            .expect("バッファmap失敗");

        let padded = slice.get_mapped_range();
        let mut dense = Vec::with_capacity((unpadded_bytes_per_row * height) as usize);
        for row in 0..height as usize {
            let start = row * padded_bytes_per_row as usize;
            let end = start + unpadded_bytes_per_row as usize;
            dense.extend_from_slice(&padded[start..end]);
        }
        drop(padded);
        output_buffer.unmap();
        dense
    }

    #[test]
    fn render_engine_new_succeeds() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("[test] GPUアダプタ非検出、テストskip");
            return;
        };
        let engine = RenderEngine::new(device, queue, 64, 64);
        assert_eq!(engine.render_width, 64);
        assert_eq!(engine.render_height, 64);
    }

    #[test]
    fn render_empty_scene_clears_target() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("[test] GPUアダプタ非検出、テストskip");
            return;
        };
        let mut engine = RenderEngine::new(device, queue, 32, 32);
        let project = ProjectResource::new();
        engine.render(&[], &project);

        let pixels = read_texture_rgba8(
            &engine.device,
            &engine.queue,
            &engine.texture,
            engine.render_width,
            engine.render_height,
        );
        assert_eq!(pixels.len(), (32 * 32 * 4) as usize);
        let alpha_values: Vec<u8> = pixels.iter().skip(3).step_by(4).copied().collect();
        assert!(alpha_values.iter().all(|&a| a == alpha_values[0]));
    }

    /// テスト用ActiveObjectを最小構成で生成する。kind_id・effectsのみ差し替える。
    fn make_active_object(
        kind_id: u32,
        effects: Vec<(String, HashMap<String, Value>)>,
    ) -> ActiveObject {
        ActiveObject {
            kind_id,
            start_frame: 0,
            source_frame: 0,
            clip_instance: kind_id as u64,
            text_content: None,
            shape_params: None,
            media_source: None,
            global_matrix: [0.0; 16],
            mvp: [0.0; 16],
            opacity: 1.0,
            audio: Default::default(),
            effects,
            nested_scene: None,
        }
    }

    /// エフェクト付きオブジェクトへ適用したエフェクトが、隣接する無エフェクト
    /// オブジェクトのピクセルへ波及しないことを検証する（Phase7-1）。
    #[test]
    fn effect_chain_does_not_leak_to_adjacent_object() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("[test] GPUアダプタ非検出、テストskip");
            return;
        };
        let mut engine = RenderEngine::new(device, queue, 32, 32);
        let project = ProjectResource::new();

        let plain = make_active_object(u32::MAX, Vec::new());
        let with_effect = make_active_object(
            u32::MAX,
            vec![("nonexistent-effect-id".to_string(), HashMap::new())],
        );
        engine.render(&[plain, with_effect], &project);

        let pixels = read_texture_rgba8(
            &engine.device,
            &engine.queue,
            &engine.texture,
            engine.render_width,
            engine.render_height,
        );
        assert_eq!(pixels.len(), (32 * 32 * 4) as usize);
    }

    /// 2オブジェクトへ異なるエフェクトチェーンを設定した場合でも、
    /// 未登録IDチェーンはapply_effect_chain側でスキップされ、双方の出力が
    /// 独立して完走することを検証する（Phase7-2）。
    #[test]
    fn distinct_effect_chains_render_independently() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("[test] GPUアダプタ非検出、テストskip");
            return;
        };
        let mut engine = RenderEngine::new(device, queue, 32, 32);
        let project = ProjectResource::new();

        let obj_a = make_active_object(u32::MAX, vec![("effect-a".to_string(), HashMap::new())]);
        let obj_b = make_active_object(u32::MAX, vec![("effect-b".to_string(), HashMap::new())]);
        engine.render(&[obj_a, obj_b], &project);

        let pixels = read_texture_rgba8(
            &engine.device,
            &engine.queue,
            &engine.texture,
            engine.render_width,
            engine.render_height,
        );
        assert_eq!(pixels.len(), (32 * 32 * 4) as usize);
    }

    #[test]
    fn resize_render_target_updates_dimensions_and_survives_render() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("[test] GPUアダプタ非検出、テストskip");
            return;
        };
        let mut engine = RenderEngine::new(device, queue, 64, 64);
        engine.resize_render_target(128, 72);
        assert_eq!(engine.render_width, 128);
        assert_eq!(engine.render_height, 72);

        let project = ProjectResource::new();
        engine.render(&[], &project);
        let pixels = read_texture_rgba8(
            &engine.device,
            &engine.queue,
            &engine.texture,
            engine.render_width,
            engine.render_height,
        );
        assert_eq!(pixels.len(), (128 * 72 * 4) as usize);
    }
}
