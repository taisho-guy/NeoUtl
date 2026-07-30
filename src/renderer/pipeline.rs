use crate::config;
use crate::ecs::resources::ProjectResource;
use crate::ecs::systems::ActiveObject;
use crate::ecs::types::Value;
use crate::effects;
use crate::objects::{by_kind_id, registry};
use neoutl_object_api::{IMAGE_STABLE_ID, UNIT_SIZE_PX, VIDEO_STABLE_ID};
use slint::wgpu_29::wgpu;
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
    media_pipeline: wgpu::RenderPipeline,
    media_bind_group_layout: wgpu::BindGroupLayout,
    media_uniform_buffer: wgpu::Buffer,
    media_sampler: wgpu::Sampler,
    video_pipeline: wgpu::RenderPipeline,
    video_bind_group_layout: wgpu::BindGroupLayout,
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
        .filter_map(|plugin| {
            let src = unsafe { (plugin.vtable.wgsl)() };
            if src.ptr.is_null() {
                return None;
            }
            let wgsl = unsafe { std::slice::from_raw_parts(src.ptr, src.len) };
            match build_effect_pipeline(device, layout, wgsl, &plugin.name) {
                Ok(pipeline) => Some((plugin.id.clone(), pipeline)),
                Err(err) => {
                    eprintln!(
                        "[NeoUtl] エフェクトプラグインのシェーダコンパイル失敗、除外して継続: id={} name={} 理由={err}",
                        plugin.id, plugin.name
                    );
                    None
                }
            }
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
            media_pipeline,
            media_bind_group_layout,
            media_uniform_buffer,
            media_sampler,
            video_pipeline,
            video_bind_group_layout,
        }
    }

    pub fn resize_render_target(&mut self, width: u32, height: u32) {
        self.render_width = width;
        self.render_height = height;
        self.texture = create_texture(&self.device, width, height);
        self.depth_texture = create_depth_texture(&self.device, width, height);
        self.effect_ping = create_effect_texture(&self.device, width, height);
        self.effect_pong = create_effect_texture(&self.device, width, height);
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

    /// ActiveObject.effectsを連結した順序付きエフェクトチェーンを、
    /// self.textureへポストプロセス適用する（Phase2/8: WGSL実処理接続）。
    /// 各パスはeffect_ping/effect_pongへ交互出力し、最終結果をself.textureへ書き戻す。
    fn apply_effect_chain(&self, chain: &[(String, HashMap<String, Value>)]) {
        if chain.is_empty() {
            return;
        }
        let extent = wgpu::Extent3d {
            width: self.render_width,
            height: self.render_height,
            depth_or_array_layers: 1,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Effect Copy Encoder"),
            });
        encoder.copy_texture_to_texture(
            self.texture.as_image_copy(),
            self.effect_ping.as_image_copy(),
            extent,
        );
        self.queue.submit([encoder.finish()]);

        let mut src_is_ping = true;
        for (effect_id, params) in chain {
            let Some(plugin) = effects::loader::by_id(effect_id) else {
                continue;
            };
            let Some(pipeline) = self.effect_pipelines.get(effect_id) else {
                continue;
            };
            let Some(meta) = crate::ecs::effects::find_effect(effect_id) else {
                continue;
            };
            let schema = crate::ecs::effects::param_schema(meta);
            let values: Vec<f32> = schema
                .iter()
                .map(|s| {
                    let key = unsafe { s.key.as_str() };
                    params.get(key).map_or(s.default_float, |v| match v {
                        Value::Number(n) => *n,
                        Value::Bool(b) => {
                            if *b {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        Value::Enum(idx) => *idx as f32,
                        Value::Text(_) | Value::FilePath(_) | Value::TrackRef(_) => s.default_float,
                    })
                })
                .collect();

            let uniform_size = (unsafe { (plugin.vtable.uniform_size)() } as usize).max(16);
            let mut bytes = vec![0u8; uniform_size];
            unsafe {
                (plugin.vtable.pack_uniform)(
                    values.as_ptr(),
                    values.len() as u32,
                    bytes.as_mut_ptr(),
                );
            }
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
        encoder.copy_texture_to_texture(
            final_src.as_image_copy(),
            self.texture.as_image_copy(),
            extent,
        );
        self.queue.submit([encoder.finish()]);
    }

    pub fn render(&mut self, active_objects: &[ActiveObject], _project: &ProjectResource) {
        if is_device_lost() {
            return;
        }
        let mut media_frames: Vec<Option<wgpu::Texture>> = Vec::with_capacity(active_objects.len());
        {
            let cache = crate::media::cache::global();
            for obj in active_objects {
                let is_visual = matches!(
                    stable_id_of(obj.kind_id),
                    Some(VIDEO_STABLE_ID | IMAGE_STABLE_ID)
                );
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

        let mut text_draws: Vec<(u64, u32)> = Vec::new();
        if let Some(ref font) = self.font {
            let mut seen: HashSet<u64> = HashSet::with_capacity(active_objects.len());
            for obj in active_objects {
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
                text_draws.push((obj.clip_instance, offset));
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
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.07,
                            a: 1.0,
                        }),
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

            for (obj, offset) in active_objects.iter().zip(offsets.iter()) {
                let Some(offset) = offset else { continue };
                if let Some((pipeline, vertex_count)) = self.pipelines.get(&obj.kind_id) {
                    rpass.set_pipeline(pipeline);
                    rpass.set_bind_group(0, &self.bind_group, &[*offset]);
                    rpass.draw(0..*vertex_count, 0..1);
                }
            }

            for (_obj, (texture, offset)) in active_objects
                .iter()
                .zip(media_frames.iter().zip(media_offsets.iter()))
            {
                let (Some(texture), Some(offset)) = (texture, offset) else {
                    continue;
                };
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
                    rpass.set_bind_group(0, &bind_group, &[*offset]);
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
                    rpass.set_bind_group(0, &bind_group, &[*offset]);
                    rpass.draw(0..6, 0..1);
                }
            }

            for (clip_instance, offset) in &text_draws {
                let Some(target) = self.text_targets.get(clip_instance) else {
                    continue;
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
                rpass.set_bind_group(0, &bind_group, &[*offset]);
                rpass.draw(0..6, 0..1);
            }
        }

        self.queue.submit([encoder.finish()]);

        let chain: Vec<(String, HashMap<String, Value>)> = active_objects
            .iter()
            .flat_map(|obj| obj.effects.iter().cloned())
            .collect();
        self.apply_effect_chain(&chain);
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
