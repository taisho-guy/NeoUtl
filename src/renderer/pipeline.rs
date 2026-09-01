use crate::config;
use crate::ecs::resources::ProjectResource;
use crate::ecs::systems::{ActiveObject, CapturedObjects};
use crate::ecs::types::Value;
use crate::effects;
use crate::hot_reload::{self, ReloadEvent};
use crate::objects::{by_kind_id, registry};
use egui_wgpu::wgpu;
use neoutl_object_api::{IMAGE_STABLE_ID, UNIT_SIZE_PX, VIDEO_STABLE_ID};
use shipyard::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ComposeCacheKey {
    Scene(i32),
    FrameBuffer(EntityId),
    EffectMapScene(i32),
}
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wgpu_text::glyph_brush::ab_glyph::{Font, FontArc};
use wgpu_text::{BrushBuilder, TextBrush};

pub static DEVICE_LOST: AtomicBool = AtomicBool::new(false);

pub fn is_device_lost() -> bool {
    DEVICE_LOST.load(Ordering::Relaxed)
}

fn mark_device_lost(reason: &str) {
    eprintln!(
        "{}",
        t!(
            "[NeoUtl] GPUデバイスロスト検知: %{arg0}",
            arg0 = format!("{reason}")
        )
    );
    DEVICE_LOST.store(true, Ordering::Relaxed);
}

pub fn install_device_lost_watcher(device: &wgpu::Device) {
    device.set_device_lost_callback(|reason, message| {
        mark_device_lost(&format!("{reason:?}: {message}"));
    });
}

const STANDARD_UNIFORM_SIZE: u64 = 96;
const UNIFORM_STRIDE: u64 = config::UNIFORM_STRIDE_BYTES;
const MAX_OBJECTS: u64 = config::MAX_SCENE_OBJECTS;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MAX_EFFECT_UNIFORM_SIZE: u64 = config::MAX_EFFECT_UNIFORM_BYTES;
const MEDIA_UNIFORM_SIZE: u64 = 80;
static MEDIA_WGSL: &str = include_str!(concat!(env!("OUT_DIR"), "/media.wgsl"));
static VIDEO_WGSL: &str = include_str!(concat!(env!("OUT_DIR"), "/media_video.wgsl"));

pub struct RenderEngine {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub texture: wgpu::Texture,
    pub depth_texture: wgpu::Texture,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    fonts: HashMap<(String, bool, bool), FontArc>,
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
    effect_object_pool: Vec<wgpu::Texture>,
    effect_object_depth: wgpu::Texture,
    composite_pipeline: wgpu::RenderPipeline,
    composite_bind_group_layout: wgpu::BindGroupLayout,
    clip_composite_pipeline: wgpu::RenderPipeline,
    clip_composite_bind_group_layout: wgpu::BindGroupLayout,
    clip_uniform_buffer: wgpu::Buffer,
    media_pipeline: wgpu::RenderPipeline,
    media_bind_group_layout: wgpu::BindGroupLayout,
    media_uniform_buffer: wgpu::Buffer,
    media_sampler: wgpu::Sampler,
    video_pipeline: wgpu::RenderPipeline,
    video_bind_group_layout: wgpu::BindGroupLayout,
    lua_system: Option<neoutl_lua_runtime::LuaSystem>,
    lua_compute_pipelines: HashMap<String, wgpu::ComputePipeline>,
    reduce_mean_pipeline: wgpu::ComputePipeline,
    reduce_mean_bind_group_layout: wgpu::BindGroupLayout,
    reduce_mean_buffer: wgpu::Buffer,
    reduce_mean_readback_buffer: wgpu::Buffer,
    scene_texture_cache: HashMap<ComposeCacheKey, wgpu::Texture>,
    map_texture_cache: HashMap<std::path::PathBuf, wgpu::Texture>,
    dummy_map_texture_view: wgpu::TextureView,
    object_pipeline_layout: wgpu::PipelineLayout,
    effect_pipeline_layout: wgpu::PipelineLayout,
    hot_reload_rx: Option<std::sync::mpsc::Receiver<ReloadEvent>>,
    scripts_dir: std::path::PathBuf,
}

struct TextRenderTarget {
    texture: wgpu::Texture,
    outline_scratch: wgpu::Texture,
    brush: TextBrush,
    width: u32,
    height: u32,
}

fn build_text_target(
    device: &wgpu::Device,
    font: &FontArc,
    width: u32,
    height: u32,
) -> TextRenderTarget {
    let width = width.max(1);
    let height = height.max(1);
    let texture = create_effect_texture(device, width, height);
    let outline_scratch = create_effect_texture(device, width, height);
    let brush = BrushBuilder::using_font(font.clone()).build(
        device,
        width,
        height,
        wgpu::TextureFormat::Rgba16Float,
    );
    TextRenderTarget {
        texture,
        outline_scratch,
        brush,
        width,
        height,
    }
}

fn load_font_bytes(family: &str, bold: bool, italic: bool) -> Option<Vec<u8>> {
    use font_kit::family_name::FamilyName;
    use font_kit::properties::{Properties, Style, Weight};
    use font_kit::source::SystemSource;
    let requested = if family.is_empty() {
        FamilyName::SansSerif
    } else {
        FamilyName::Title(family.to_owned())
    };
    let mut properties = Properties::new();
    properties.weight(if bold { Weight::BOLD } else { Weight::NORMAL });
    properties.style(if italic { Style::Italic } else { Style::Normal });
    let source = SystemSource::new();
    let handle = source
        .select_best_match(&[requested, FamilyName::SansSerif], &properties)
        .ok()?;
    let font = handle.load().ok()?;
    let data = font.copy_font_data()?;
    eprintln!(
        "{}",
        t!(
            "[NeoUtl] フォント解決: %{arg0}",
            arg0 = format!("{family} bold={bold} italic={italic}")
        )
    );
    Some(data.to_vec())
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
        format: wgpu::TextureFormat::Rgba16Float,
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn try_create_shader_module(
    device: &wgpu::Device,
    wgsl: &[u8],
    label: &str,
) -> Result<wgpu::ShaderModule, String> {
    let text = std::str::from_utf8(wgsl)
        .map_err(|err| t!("WGSLソースが非UTF-8: %{arg0}", arg0 = format!("{err}")).to_string())?;
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
                    format: wgpu::TextureFormat::Rgba16Float,
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
            if src.is_empty() {
                return None;
            }
            let wgsl = unsafe { src.as_slice() };
            match build_pipeline(device, layout, wgsl, &plugin.name) {
                Ok(pipeline) => Some((plugin.kind_id, (pipeline, vertex_count))),
                Err(err) => {
                    eprintln!("{}", t!("[NeoUtl] オブジェクトプラグインのシェーダコンパイル失敗、除外して継続: kind_id=%{arg0} name=%{arg1} 理由=%{arg2}", arg0 = format!("{}", plugin.kind_id), arg1 = format!("{}", plugin.name), arg2 = format!("{err}")));
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
        format: wgpu::TextureFormat::Rgba16Float,
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
                    format: wgpu::TextureFormat::Rgba16Float,
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
                    eprintln!("{}", t!("[NeoUtl] エフェクトのシェーダコンパイル失敗、除外して継続: id=%{arg0} name=%{arg1} 理由=%{arg2}", arg0 = format!("{}", source.id()), arg1 = format!("{}", source.name()), arg2 = format!("{err}")));
                    None
                }
            }
        })
        .collect()
}

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
                eprintln!("{}", t!("[NeoUtl] system.register_compute シェーダコンパイル失敗、除外: id=%{arg0} 理由=%{arg1}", arg0 = format!("{}", def.id), arg1 = format!("{err}")));
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
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_dummy_map_texture_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Displacement Map Dummy Texture"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &[0u8; 8],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

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
@group(0) @binding(2) var src_depth: texture_depth_2d;

struct FOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(in: VOut) -> FOut {
    var out: FOut;
    out.color = textureSample(src_tex, src_sampler, in.uv);
    out.depth = textureLoad(src_depth, vec2<i32>(in.position.xy), 0);
    return out;
}
"#;

const CLIP_COMPOSITE_WGSL: &str = r#"
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

struct ClipUniform {
    mode: u32,
    chroma_hue: f32,
    chroma_tolerance: f32,
    blend_edge: u32,
};

@group(0) @binding(0) var content_tex: texture_2d<f32>;
@group(0) @binding(1) var mold_tex: texture_2d<f32>;
@group(0) @binding(2) var clip_sampler: sampler;
@group(0) @binding(3) var content_depth: texture_depth_2d;
@group(0) @binding(4) var<uniform> u: ClipUniform;

fn rgb_to_hue(c: vec3<f32>) -> f32 {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    let delta = mx - mn;
    if (delta == 0.0) {
        return 0.0;
    }
    if (mx == c.r) {
        return 60.0 * (((c.g - c.b) / delta) % 6.0);
    }
    if (mx == c.g) {
        return 60.0 * ((c.b - c.r) / delta + 2.0);
    }
    return 60.0 * ((c.r - c.g) / delta + 4.0);
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let d = ((a - b) % 360.0 + 360.0) % 360.0;
    return min(d, 360.0 - d);
}

struct FOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(in: VOut) -> FOut {
    let content = textureSample(content_tex, clip_sampler, in.uv);
    let mold = textureSample(mold_tex, clip_sampler, in.uv);
    var mask: f32;
    switch (u.mode) {
        case 0u: { mask = mold.a; }
        case 1u: { mask = 1.0 - mold.a; }
        case 2u: { mask = dot(mold.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)); }
        case 3u: { mask = 1.0 - dot(mold.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)); }
        default: {
            let d = hue_distance(rgb_to_hue(mold.rgb), u.chroma_hue);
            mask = select(0.0, 1.0, d > u.chroma_tolerance);
        }
    }
    if (u.blend_edge == 0u) {
        mask = select(0.0, 1.0, mask > 0.5);
    }
    var out: FOut;
    out.color = vec4<f32>(content.rgb, content.a * mask);
    out.depth = textureLoad(content_depth, vec2<i32>(in.position.xy), 0);
    return out;
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
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

fn create_clip_composite_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Clip Composite BGL"),
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
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
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
                format: wgpu::TextureFormat::Rgba16Float,
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

fn build_clip_composite_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Clip Composite"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(CLIP_COMPOSITE_WGSL)),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Clip Composite"),
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
                format: wgpu::TextureFormat::Rgba16Float,
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
                format: wgpu::TextureFormat::Rgba16Float,
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
            depth_write_enabled: Some(false),
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

        let clip_composite_bind_group_layout = create_clip_composite_bind_group_layout(&device);
        let clip_composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Clip Composite Pipeline Layout"),
                bind_group_layouts: &[Some(&clip_composite_bind_group_layout)],
                immediate_size: 0,
            });
        let clip_composite_pipeline =
            build_clip_composite_pipeline(&device, &clip_composite_pipeline_layout);
        let clip_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Clip Uniform Buffer"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

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
                eprintln!(
                    "{}",
                    t!(
                        "[NeoUtl] LuaSystem初期化失敗、system拡張を無効化: %{arg0}",
                        arg0 = format!("{err}")
                    )
                );
                None
            }
        };
        let lua_compute_pipelines = lua_system
            .as_ref()
            .map(|sys| build_lua_compute_pipelines(&device, &sys.drain_computes()))
            .unwrap_or_default();

        let dummy_map_texture_view = create_dummy_map_texture_view(&device, &queue);

        Self {
            device,
            queue,
            texture,
            depth_texture,
            uniform_buffer,
            bind_group,
            fonts: HashMap::new(),
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
            clip_composite_pipeline,
            clip_composite_bind_group_layout,
            clip_uniform_buffer,
            media_pipeline,
            media_bind_group_layout,
            media_uniform_buffer,
            media_sampler,
            video_pipeline,
            video_bind_group_layout,
            scene_texture_cache: HashMap::new(),
            map_texture_cache: HashMap::new(),
            dummy_map_texture_view,
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
        crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);

        let slice = self.reduce_mean_readback_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).expect(&t!("map_async結果送信失敗"));
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect(&t!("device poll失敗"));
        rx.recv()
            .expect(&t!("map_async結果受信失敗"))
            .expect(&t!("バッファmap失敗"));

        let mapped = slice.get_mapped_range().expect(&t!("get_mapped_range失敗"));
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

    pub fn run_lua_reduce_hooks(&self) {
        if let Some(sys) = &self.lua_system {
            let values = self.reduce_source_mean();
            sys.publish_reduce_result("source_mean", &values);
        }
    }

    fn render_composed_texture(
        &mut self,
        world: &crate::ecs::EcsWorld,
        objects: &[ActiveObject],
        captured: &CapturedObjects,
        width: u32,
        height: u32,
        cache_key: ComposeCacheKey,
        depth: u32,
        clear_override: Option<wgpu::Color>,
    ) -> Option<wgpu::Texture> {
        if depth >= config::MAX_SCENE_NESTING_DEPTH {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] 合成ネスト深度上限(%{arg0})到達: 非描画",
                    arg0 = format!("{}", config::MAX_SCENE_NESTING_DEPTH)
                )
            );
            return None;
        }
        if let Some(cached) = self.scene_texture_cache.get(&cache_key) {
            return Some(cached.clone());
        }
        let saved_width = self.render_width;
        let saved_height = self.render_height;
        let saved_texture = self.texture.clone();
        let saved_depth_texture = self.depth_texture.clone();
        let saved_effect_ping = self.effect_ping.clone();
        let saved_effect_pong = self.effect_pong.clone();
        let saved_effect_object_pool = std::mem::take(&mut self.effect_object_pool);
        let saved_effect_object_depth = self.effect_object_depth.clone();

        self.render_width = width;
        self.render_height = height;
        self.texture = create_texture(&self.device, width, height);
        self.depth_texture = create_depth_texture(&self.device, width, height);
        self.effect_ping = create_effect_texture(&self.device, width, height);
        self.effect_pong = create_effect_texture(&self.device, width, height);
        self.effect_object_pool.clear();
        self.effect_object_depth = create_depth_texture(&self.device, width, height);

        let project = world.get_project();
        self.render_at(world, objects, captured, &project, depth, clear_override);
        let texture = self.texture.clone();

        self.render_width = saved_width;
        self.render_height = saved_height;
        self.texture = saved_texture;
        self.depth_texture = saved_depth_texture;
        self.effect_ping = saved_effect_ping;
        self.effect_pong = saved_effect_pong;
        self.effect_object_pool = saved_effect_object_pool;
        self.effect_object_depth = saved_effect_object_depth;

        self.scene_texture_cache.insert(cache_key, texture.clone());
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
        eprintln!(
            "{}",
            t!(
                "[NeoUtl] レンダーターゲット変更: %{arg0}×%{arg1}",
                arg0 = format!("{width}"),
                arg1 = format!("{height}")
            )
        );
    }

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

    const COLOR_MATRIX_BT709: u32 = 1;
    const COLOR_RANGE_LIMITED: u32 = 0;

    fn write_media_uniform_raw(&self, index: u64, mvp: &[f32; 16], opacity: f32) -> u32 {
        self.write_video_uniform_raw(
            index,
            mvp,
            opacity,
            Self::COLOR_MATRIX_BT709,
            Self::COLOR_RANGE_LIMITED,
        )
    }

    fn write_video_uniform_raw(
        &self,
        index: u64,
        mvp: &[f32; 16],
        opacity: f32,
        color_matrix: u32,
        color_range: u32,
    ) -> u32 {
        let mut data = [0u8; MEDIA_UNIFORM_SIZE as usize];
        data[0..64].copy_from_slice(bytemuck::cast_slice(mvp));
        data[64..68].copy_from_slice(&opacity.to_le_bytes());
        data[68..72].copy_from_slice(&color_matrix.to_le_bytes());
        data[72..76].copy_from_slice(&color_range.to_le_bytes());
        let offset = index * UNIFORM_STRIDE;
        self.queue
            .write_buffer(&self.media_uniform_buffer, offset, &data);
        offset as u32
    }

    fn write_media_uniform(&self, index: u64, obj: &ActiveObject) -> u32 {
        self.write_media_uniform_raw(index, &obj.mvp, obj.opacity)
    }

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

    fn apply_effect_chain(
        &mut self,
        world: &crate::ecs::EcsWorld,
        objects: &[ActiveObject],
        captured: &CapturedObjects,
        depth: u32,
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
            crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
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
        crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);

        let mut src_is_ping = true;
        for (effect_id, params) in chain {
            let Some(source) = effects::loader::by_id(effect_id) else {
                continue;
            };
            let Some(pipeline) = self.effect_pipelines.get(effect_id).cloned() else {
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

            let requires_tex_idx = source.requires_texture_param_index();
            let resolved_scene_tex: Option<wgpu::Texture> = if let Some(idx) = requires_tex_idx {
                let scene_ref = schema.get(idx as usize).and_then(|s| {
                    params.get(s.key.as_str()).and_then(|v| match v {
                        Value::TrackRef(id) => Some(*id),
                        _ => None,
                    })
                });
                if let Some(scene_id) = scene_ref {
                    self.render_composed_texture(
                        world,
                        objects,
                        captured,
                        self.render_width,
                        self.render_height,
                        ComposeCacheKey::EffectMapScene(scene_id),
                        depth + 1,
                        None,
                    )
                } else {
                    None
                }
            } else {
                None
            };
            let map_view: wgpu::TextureView = if let Some(t) = &resolved_scene_tex {
                t.create_view(&wgpu::TextureViewDescriptor::default())
            } else if let Some(idx) = requires_tex_idx {
                let path = schema.get(idx as usize).and_then(|s| {
                    params.get(s.key.as_str()).and_then(|v| match v {
                        Value::FilePath(p) => Some(p.clone()),
                        _ => None,
                    })
                });
                match path.and_then(|p| {
                    self.map_texture_cache
                        .get(std::path::Path::new(&p))
                        .cloned()
                }) {
                    Some(t) => t.create_view(&wgpu::TextureViewDescriptor::default()),
                    None => self.dummy_map_texture_view.clone(),
                }
            } else {
                self.dummy_map_texture_view.clone()
            };

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
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&map_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&self.effect_sampler),
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
                rpass.set_pipeline(&pipeline);
                rpass.set_bind_group(0, &bind_group, &[]);
                rpass.draw(0..3, 0..1);
            }
            crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
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
        crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
    }

    fn draw_standard_pass(&self, rpass: &mut wgpu::RenderPass, obj: &ActiveObject, offset: u32) {
        if let Some((pipeline, vertex_count)) = self.pipelines.get(&obj.kind_id) {
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, &self.bind_group, &[offset]);
            rpass.draw(0..*vertex_count, 0..1);
        }
    }

    fn draw_media_pass(&self, rpass: &mut wgpu::RenderPass, texture: &wgpu::Texture, offset: u32) {
        let is_planar = matches!(
            texture.format(),
            wgpu::TextureFormat::NV12 | wgpu::TextureFormat::P010
        );
        if is_planar {
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

    fn resolve_font(&mut self, family: &str, bold: bool, italic: bool) -> Option<FontArc> {
        let key = (family.to_owned(), bold, italic);
        if let Some(font) = self.fonts.get(&key) {
            return Some(font.clone());
        }
        let bytes = load_font_bytes(family, bold, italic)?;
        let font = FontArc::try_from_vec(bytes).ok()?;
        self.fonts.insert(key, font.clone());
        Some(font)
    }

    fn resolve_font_stack(
        &mut self,
        stack: &[String],
        text: &str,
        bold: bool,
        italic: bool,
    ) -> Option<FontArc> {
        let mut fallback: Option<FontArc> = None;
        for family in stack {
            let Some(font) = self.resolve_font(family, bold, italic) else {
                continue;
            };
            let covers_all = text.chars().all(|c| c == '\n' || font.glyph_id(c).0 != 0);
            if covers_all {
                return Some(font);
            }
            fallback = Some(font);
        }
        fallback.or_else(|| self.resolve_font("", bold, italic))
    }

    fn apply_text_outline(
        &self,
        target: &TextRenderTarget,
        tc: &crate::ecs::components::TextContent,
    ) {
        let Some(source) = effects::loader::by_id("text_outline") else {
            return;
        };
        let Some(pipeline) = self.effect_pipelines.get("text_outline") else {
            return;
        };
        let values = [
            tc.outline_width,
            tc.outline_color[0],
            tc.outline_color[1],
            tc.outline_color[2],
            tc.outline_color[3],
        ];
        let uniform_size = (source.uniform_size() as usize).max(16);
        let mut bytes = vec![0u8; uniform_size];
        source.pack_uniform(&values, &mut bytes);
        self.queue
            .write_buffer(&self.effect_uniform_buffer, 0, &bytes);

        let src_view = target
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = target
            .outline_scratch
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Outline BG"),
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.dummy_map_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.effect_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Text Outline Encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Text Outline Pass"),
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
        encoder.copy_texture_to_texture(
            target.outline_scratch.as_image_copy(),
            target.texture.as_image_copy(),
            wgpu::Extent3d {
                width: target.width,
                height: target.height,
                depth_or_array_layers: 1,
            },
        );
        crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
    }

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
                        store: wgpu::StoreOp::Store,
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
        crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
    }

    fn composite_effect_object(&self, pool_tex: &wgpu::Texture, clear_color: Option<wgpu::Color>) {
        let src_view = pool_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let src_depth_view = self
            .effect_object_depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let dst_depth_view = self
            .depth_texture
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&src_depth_view),
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
                        load: match clear_color {
                            Some(c) => wgpu::LoadOp::Clear(c),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &dst_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: match clear_color {
                            Some(_) => wgpu::LoadOp::Clear(1.0),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rpass.set_pipeline(&self.composite_pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }
        crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
    }

    fn composite_clipped_object(
        &self,
        content_pool_tex: &wgpu::Texture,
        mold_pool_tex: &wgpu::Texture,
        mode: crate::ecs::components::ClipMode,
        chroma_hue: f32,
        chroma_tolerance: f32,
        blend_edge: bool,
        clear_color: Option<wgpu::Color>,
    ) {
        let content_view = content_pool_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let mold_view = mold_pool_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let content_depth_view = self
            .effect_object_depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let dst_depth_view = self
            .depth_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let uniform_data: [u32; 4] = [
            mode as u8 as u32,
            chroma_hue.to_bits(),
            chroma_tolerance.to_bits(),
            u32::from(blend_edge),
        ];
        self.queue.write_buffer(
            &self.clip_uniform_buffer,
            0,
            bytemuck::cast_slice(&uniform_data),
        );

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Clip Composite BG"),
            layout: &self.clip_composite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&content_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&mold_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.effect_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&content_depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.clip_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Clip Composite Encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clip Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: match clear_color {
                            Some(c) => wgpu::LoadOp::Clear(c),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &dst_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: match clear_color {
                            Some(_) => wgpu::LoadOp::Clear(1.0),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rpass.set_pipeline(&self.clip_composite_pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }
        crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
    }

    pub fn render(
        &mut self,
        world: &crate::ecs::EcsWorld,
        active_objects: &[ActiveObject],
        captured: &CapturedObjects,
        project: &ProjectResource,
    ) {
        self.scene_texture_cache.clear();
        self.drain_hot_reload_events();
        if let Some(sys) = &self.lua_system
            && let Err(err) = sys.run_pre_render_hooks()
        {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] system.on_pre_render フック実行失敗: %{arg0}",
                    arg0 = format!("{err}")
                )
            );
        }
        self.render_at(world, active_objects, captured, project, 0, None);
        self.run_lua_reduce_hooks();
    }

    pub fn read_frame_rgba8(&self) -> Vec<u8> {
        let width = self.render_width;
        let height = self.render_height;
        let unpadded_bytes_per_row = width * 8;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = (padded_bytes_per_row * height) as wgpu::BufferAddress;

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Export Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
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
        crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);

        let slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).expect(&t!("map_async結果送信失敗"));
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect(&t!("device poll失敗"));
        rx.recv()
            .expect(&t!("map_async結果受信失敗"))
            .expect(&t!("バッファmap失敗"));

        let padded = slice.get_mapped_range().expect(&t!("get_mapped_range失敗"));
        let mut rgba8 = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height as usize {
            let start = row * padded_bytes_per_row as usize;
            for pixel in padded[start..start + unpadded_bytes_per_row as usize].chunks_exact(8) {
                for channel in pixel.chunks_exact(2) {
                    let v = half::f16::from_le_bytes([channel[0], channel[1]]).to_f32();
                    rgba8.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
                }
            }
        }
        drop(padded);
        output_buffer.unmap();
        rgba8
    }

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

    fn apply_object_reload(&mut self, path: &std::path::Path) {
        if let Err(err) = crate::objects::loader::reload_one(path) {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] ホットリロード失敗（objects） %{arg0}: %{arg1}",
                    arg0 = format!("{}", path.display()),
                    arg1 = format!("{err}")
                )
            );
            return;
        }
        self.rebuild_all_object_pipelines();
    }

    fn apply_effect_reload(&mut self, path: &std::path::Path) {
        if let Err(err) = crate::effects::loader::reload_one(path) {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] ホットリロード失敗（effects） %{arg0}: %{arg1}",
                    arg0 = format!("{}", path.display()),
                    arg1 = format!("{err}")
                )
            );
            return;
        }
        self.rebuild_all_effect_pipelines();
    }

    fn apply_script_reload(&mut self, _path: &std::path::Path) {
        let Some(sys) = &self.lua_system else {
            return;
        };
        if let Err(err) = sys.reload_dir(&self.scripts_dir) {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] ホットリロード失敗（scripts） %{arg0}: %{arg1}",
                    arg0 = format!("{}", self.scripts_dir.display()),
                    arg1 = format!("{err}")
                )
            );
            return;
        }
        self.lua_compute_pipelines =
            build_lua_compute_pipelines(&self.device, &sys.drain_computes());
        crate::effects::loader::reload_lua(sys.drain_effects());
        self.rebuild_all_effect_pipelines();
        eprintln!(
            "{}",
            t!(
                "[NeoUtl] scriptsホットリロード完了: %{arg0}",
                arg0 = format!("{}", self.scripts_dir.display())
            )
        );
    }

    fn rebuild_all_object_pipelines(&mut self) {
        self.pipelines = build_pipelines_from_registry(&self.device, &self.object_pipeline_layout);
    }

    fn rebuild_all_effect_pipelines(&mut self) {
        self.effect_pipelines =
            build_effect_pipelines_from_registry(&self.device, &self.effect_pipeline_layout);
    }

    fn render_at(
        &mut self,
        world: &crate::ecs::EcsWorld,
        active_objects: &[ActiveObject],
        captured: &CapturedObjects,
        _project: &ProjectResource,
        depth: u32,
        clear_override: Option<wgpu::Color>,
    ) {
        if is_device_lost() {
            return;
        }
        let mut media_frames: Vec<Option<wgpu::Texture>> = Vec::with_capacity(active_objects.len());
        {
            let cache = neoutl_media_runtime::cache::global();
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
                                    "{}",
                                    t!(
                                        "[NeoUtl] フレーム取得失敗 kind_id=%{arg0} clip_instance=%{arg4} path=%{arg1} frame=%{arg2}: %{arg3}",
                                        arg0 = format!("{}", obj.kind_id),
                                        arg1 = format!("{}", src.path.display()),
                                        arg2 = format!("{}", obj.source_frame),
                                        arg3 = format!("{err}"),
                                        arg4 = format!("{}", obj.clip_instance)
                                    )
                                );
                                None
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    match obj.compose_source {
                        Some(crate::ecs::systems::ComposeSource::NestedScene {
                            target_scene,
                            local_frame,
                        }) => world.get_scene(target_scene).and_then(|scene| {
                            let (nested, nested_captured) =
                                crate::ecs::systems::get_active_objects_system_at(
                                    world,
                                    target_scene,
                                    local_frame,
                                );
                            self.render_composed_texture(
                                world,
                                &nested,
                                &nested_captured,
                                scene.width,
                                scene.height,
                                ComposeCacheKey::Scene(target_scene),
                                depth + 1,
                                None,
                            )
                        }),
                        Some(crate::ecs::systems::ComposeSource::FrameBuffer {
                            controller,
                            kind: crate::ecs::systems::FrameBufferKind::Group,
                        }) => {
                            let empty = Vec::new();
                            let objects = captured.get(&controller).unwrap_or(&empty);
                            self.render_composed_texture(
                                world,
                                objects,
                                captured,
                                self.render_width,
                                self.render_height,
                                ComposeCacheKey::FrameBuffer(controller),
                                depth + 1,
                                None,
                            )
                        }
                        None => None,
                    }
                };
                media_frames.push(tex);
            }
        }
        let mut mold_frames: Vec<Option<wgpu::Texture>> = Vec::with_capacity(active_objects.len());
        for obj in active_objects {
            let tex = match obj.clip_target {
                Some(info) => {
                    let empty = Vec::new();
                    let objects = captured.get(&info.controller).unwrap_or(&empty);
                    self.render_composed_texture(
                        world,
                        objects,
                        captured,
                        self.render_width,
                        self.render_height,
                        ComposeCacheKey::FrameBuffer(info.controller),
                        depth + 1,
                        None,
                    )
                }
                None => None,
            };
            mold_frames.push(tex);
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
                if (!obj.effects.is_empty() || obj.clip_target.is_some())
                    && next_pool < config::MAX_EFFECT_OBJECTS
                {
                    effect_pool_index.push(Some(next_pool));
                    next_pool += 1;
                } else {
                    effect_pool_index.push(None);
                }
            }
        }

        let mut text_draws: Vec<(u64, u32, usize)> = Vec::new();
        {
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
                let Some(font) =
                    self.resolve_font_stack(&tc.font_family_stack, &tc.text, tc.bold, tc.italic)
                else {
                    continue;
                };

                let text_layout = neoutl_media_runtime::text::layout(
                    &font,
                    &tc.text,
                    tc.font_size,
                    tc.line_height,
                );
                let (tex_w, tex_h) = (text_layout.width, text_layout.height);
                seen.insert(obj.clip_instance);

                let needs_rebuild = match self.text_targets.get(&obj.clip_instance) {
                    Some(t) => t.width != tex_w || t.height != tex_h,
                    None => true,
                };
                if needs_rebuild {
                    self.text_targets.insert(
                        obj.clip_instance,
                        build_text_target(&self.device, &font, tex_w, tex_h),
                    );
                }
                let target = self
                    .text_targets
                    .get_mut(&obj.clip_instance)
                    .expect(&t!("直前にinsert済み"));

                let h_align = match tc.align {
                    crate::ecs::components::TextAlign::Left => {
                        neoutl_media_runtime::text::HAlign::Left
                    }
                    crate::ecs::components::TextAlign::Center => {
                        neoutl_media_runtime::text::HAlign::Center
                    }
                    crate::ecs::components::TextAlign::Right => {
                        neoutl_media_runtime::text::HAlign::Right
                    }
                };
                let sections = neoutl_media_runtime::text::build_sections(
                    tc.color,
                    tc.font_size,
                    h_align,
                    &text_layout,
                    target.width,
                    target.height,
                );
                let section_refs: Vec<&_> = sections.iter().collect();
                {
                    let view = target
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let _ =
                        target
                            .brush
                            .queue(self.device.as_ref(), self.queue.as_ref(), section_refs);
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
                    crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
                }

                if tc.outline_width > 0.0 {
                    if let Some(t) = self.text_targets.get(&obj.clip_instance) {
                        self.apply_text_outline(t, tc);
                    }
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

        let text_draw_by_index: HashMap<usize, (u64, u32)> = text_draws
            .iter()
            .map(|(clip_instance, offset, obj_index)| (*obj_index, (*clip_instance, *offset)))
            .collect();

        let clear_color = clear_override.unwrap_or(if depth == 0 {
            wgpu::Color {
                r: 0.05,
                g: 0.05,
                b: 0.07,
                a: 1.0,
            }
        } else {
            wgpu::Color::TRANSPARENT
        });

        let object_count = active_objects.len();
        let mut drawn_any = false;
        let mut idx = 0usize;
        while idx < object_count {
            if let Some(pool_idx) = effect_pool_index[idx] {
                let obj = &active_objects[idx];
                let draw_kind = if let Some(offset) = offsets[idx] {
                    EffectObjectDrawKind::Standard { obj, offset }
                } else if let (Some(texture), Some(offset)) =
                    (&media_frames[idx], media_offsets[idx])
                {
                    EffectObjectDrawKind::Media { texture, offset }
                } else if let Some((clip_instance, offset)) = text_draw_by_index.get(&idx) {
                    EffectObjectDrawKind::Text {
                        clip_instance: *clip_instance,
                        offset: *offset,
                    }
                } else {
                    idx += 1;
                    continue;
                };

                let pool_tex = self.ensure_effect_object_target(pool_idx).clone();
                self.render_effect_object_offscreen(&pool_tex, draw_kind);
                if !obj.effects.is_empty() {
                    self.apply_effect_chain(
                        world,
                        active_objects,
                        captured,
                        depth,
                        &pool_tex,
                        &pool_tex,
                        &obj.effects,
                    );
                }
                match obj.clip_target {
                    Some(info) => {
                        let mold_tex = mold_frames[idx].as_ref().unwrap_or(&pool_tex);
                        self.composite_clipped_object(
                            &pool_tex,
                            mold_tex,
                            info.mode,
                            info.chroma_hue,
                            info.chroma_tolerance,
                            info.blend_edge,
                            if drawn_any { None } else { Some(clear_color) },
                        );
                    }
                    None => {
                        self.composite_effect_object(
                            &pool_tex,
                            if drawn_any { None } else { Some(clear_color) },
                        );
                    }
                }
                drawn_any = true;
                idx += 1;
                continue;
            }

            let start = idx;
            while idx < object_count && effect_pool_index[idx].is_none() {
                idx += 1;
            }

            let color_load = if drawn_any {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(clear_color)
            };
            let depth_load = if drawn_any {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(1.0)
            };

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Segment Encoder"),
                });
            {
                let view = self
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let depth_view = self
                    .depth_texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass Segment"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: color_load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: depth_load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                for i in start..idx {
                    if let Some(offset) = offsets[i] {
                        self.draw_standard_pass(&mut rpass, &active_objects[i], offset);
                    }
                    if let (Some(texture), Some(offset)) = (&media_frames[i], media_offsets[i]) {
                        self.draw_media_pass(&mut rpass, texture, offset);
                    }
                    if let Some((clip_instance, offset)) = text_draw_by_index.get(&i) {
                        self.draw_text_pass(&mut rpass, *clip_instance, *offset);
                    }
                }
            }
            crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
            drawn_any = true;
        }

        if !drawn_any {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Clear Encoder"),
                });
            {
                let view = self
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let depth_view = self
                    .depth_texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear Pass"),
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
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            }
            crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::resources::ProjectResource;

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
            apply_limit_buckets: false,
        }))
        .ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    fn read_texture_rgba16f(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Vec<f32> {
        let unpadded_bytes_per_row = width * 8;
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
        crate::gpu_shared::locked_submit(queue, [encoder.finish()]);

        let slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).expect(&t!("map_async結果送信失敗"));
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect(&t!("device poll失敗"));
        rx.recv()
            .expect(&t!("map_async結果受信失敗"))
            .expect(&t!("バッファmap失敗"));

        let padded = slice.get_mapped_range().expect(&t!("get_mapped_range失敗"));
        let mut dense = Vec::with_capacity((unpadded_bytes_per_row * height) as usize);
        for row in 0..height as usize {
            let start = row * padded_bytes_per_row as usize;
            let end = start + unpadded_bytes_per_row as usize;
            dense.extend_from_slice(&padded[start..end]);
        }
        drop(padded);
        output_buffer.unmap();
        dense
            .chunks_exact(2)
            .map(|b| half::f16::from_le_bytes([b[0], b[1]]).to_f32())
            .collect()
    }

    #[test]
    fn render_engine_new_succeeds() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("{}", t!("[test] GPUアダプタ非検出、テストskip"));
            return;
        };
        let engine = RenderEngine::new(device, queue, 64, 64);
        assert_eq!(engine.render_width, 64);
        assert_eq!(engine.render_height, 64);
    }

    #[test]
    fn render_empty_scene_clears_target() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("{}", t!("[test] GPUアダプタ非検出、テストskip"));
            return;
        };
        let mut engine = RenderEngine::new(device, queue, 32, 32);
        let project = ProjectResource::new();
        let world = crate::ecs::EcsWorld::new();
        let captured = std::collections::HashMap::new();
        engine.render(&world, &[], &captured, &project);

        let pixels = read_texture_rgba16f(
            &engine.device,
            &engine.queue,
            &engine.texture,
            engine.render_width,
            engine.render_height,
        );
        assert_eq!(pixels.len(), (32 * 32 * 4) as usize);
        let alpha_values: Vec<f32> = pixels.iter().skip(3).step_by(4).copied().collect();
        assert!(alpha_values.iter().all(|&a| a == alpha_values[0]));
    }

    fn make_active_object(
        kind_id: u32,
        effects: Vec<(String, HashMap<String, Value>)>,
    ) -> ActiveObject {
        ActiveObject {
            kind_id,
            source_frame: 0,
            clip_instance: kind_id as u64,
            text_content: None,
            shape_params: None,
            media_source: None,
            mvp: [0.0; 16],
            opacity: 1.0,
            effects,
            compose_source: None,
            layer: 0,
            clip_target: None,
        }
    }

    #[test]
    fn effect_chain_does_not_leak_to_adjacent_object() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("{}", t!("[test] GPUアダプタ非検出、テストskip"));
            return;
        };
        let mut engine = RenderEngine::new(device, queue, 32, 32);
        let project = ProjectResource::new();
        let world = crate::ecs::EcsWorld::new();

        let plain = make_active_object(u32::MAX, Vec::new());
        let with_effect = make_active_object(
            u32::MAX,
            vec![("nonexistent-effect-id".to_string(), HashMap::new())],
        );
        let captured = std::collections::HashMap::new();
        engine.render(&world, &[plain, with_effect], &captured, &project);

        let pixels = read_texture_rgba16f(
            &engine.device,
            &engine.queue,
            &engine.texture,
            engine.render_width,
            engine.render_height,
        );
        assert_eq!(pixels.len(), (32 * 32 * 4) as usize);
    }

    #[test]
    fn distinct_effect_chains_render_independently() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("{}", t!("[test] GPUアダプタ非検出、テストskip"));
            return;
        };
        let mut engine = RenderEngine::new(device, queue, 32, 32);
        let project = ProjectResource::new();
        let world = crate::ecs::EcsWorld::new();

        let obj_a = make_active_object(u32::MAX, vec![("effect-a".to_string(), HashMap::new())]);
        let obj_b = make_active_object(u32::MAX, vec![("effect-b".to_string(), HashMap::new())]);
        let captured = std::collections::HashMap::new();
        engine.render(&world, &[obj_a, obj_b], &captured, &project);

        let pixels = read_texture_rgba16f(
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
            eprintln!("{}", t!("[test] GPUアダプタ非検出、テストskip"));
            return;
        };
        let mut engine = RenderEngine::new(device, queue, 64, 64);
        engine.resize_render_target(128, 72);
        assert_eq!(engine.render_width, 128);
        assert_eq!(engine.render_height, 72);

        let project = ProjectResource::new();
        let world = crate::ecs::EcsWorld::new();
        let captured = std::collections::HashMap::new();
        engine.render(&world, &[], &captured, &project);
        let pixels = read_texture_rgba16f(
            &engine.device,
            &engine.queue,
            &engine.texture,
            engine.render_width,
            engine.render_height,
        );
        assert_eq!(pixels.len(), (128 * 72 * 4) as usize);
    }
}
