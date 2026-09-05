use crate::config;
use crate::ecs::resources::ProjectResource;
use crate::ecs::systems::{ActiveObject, CapturedObjects};
use crate::ecs::types::Value;
use crate::effects;
use crate::hot_reload::{self as hot_reload_crate, ReloadEvent};
use crate::objects::{by_kind_id, registry};
use egui_wgpu::wgpu;
use neoutl_object_api::{IMAGE_STABLE_ID, UNIT_SIZE_PX, VIDEO_STABLE_ID};
use shipyard::EntityId;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wgpu_text::glyph_brush::ab_glyph::{Font, FontArc};
use wgpu_text::{BrushBuilder, TextBrush};

mod bind_layouts;
mod device;
mod draw;
mod frame;
mod hot_reload;
mod shaders;
#[cfg(test)]
mod tests;
mod text;
mod textures;

use bind_layouts::{
    create_clip_composite_bind_group_layout, create_composite_bind_group_layout,
    create_effect_bind_group_layout, create_media_bind_group_layout,
    create_video_bind_group_layout,
};
pub use device::{install_device_lost_watcher, is_device_lost};
use shaders::{
    build_clip_composite_pipeline, build_composite_pipeline, build_effect_pipelines_from_registry,
    build_lua_compute_pipelines, build_media_pipeline, build_pipelines_from_registry,
    build_reduce_mean_pipeline,
};
use textures::{
    TextRenderTarget, build_text_target, create_depth_texture, create_dummy_map_texture_view,
    create_effect_texture, create_texture, stable_id_of,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ComposeCacheKey {
    Scene(i32),
    FrameBuffer(EntityId),
    EffectMapScene(i32),
}
const STANDARD_UNIFORM_SIZE: u64 = 96;
const UNIFORM_STRIDE: u64 = config::UNIFORM_STRIDE_BYTES;
const MAX_OBJECTS: u64 = config::MAX_SCENE_OBJECTS;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MAX_EFFECT_UNIFORM_SIZE: u64 = config::MAX_EFFECT_UNIFORM_BYTES;
const MEDIA_UNIFORM_SIZE: u64 = 80;
static MEDIA_WGSL: &str = include_str!(concat!(env!("OUT_DIR"), "/media.wgsl"));
static VIDEO_WGSL: &str = include_str!(concat!(env!("OUT_DIR"), "/media_video.wgsl"));
static COMPOSITE_WGSL: &str = include_str!("wgsl/composite.wgsl");
static CLIP_COMPOSITE_WGSL: &str = include_str!("wgsl/clip_composite.wgsl");
static REDUCE_MEAN_WGSL: &str = include_str!("wgsl/reduce_mean.wgsl");
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
            Some(hot_reload_crate::spawn_watcher(
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
        let composite_pipeline =
            build_composite_pipeline(&device, &composite_pipeline_layout, COMPOSITE_WGSL);

        let clip_composite_bind_group_layout = create_clip_composite_bind_group_layout(&device);
        let clip_composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Clip Composite Pipeline Layout"),
                bind_group_layouts: &[Some(&clip_composite_bind_group_layout)],
                immediate_size: 0,
            });
        let clip_composite_pipeline = build_clip_composite_pipeline(
            &device,
            &clip_composite_pipeline_layout,
            CLIP_COMPOSITE_WGSL,
        );
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
            build_reduce_mean_pipeline(&device, REDUCE_MEAN_WGSL);
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

    fn write_media_uniform(
        &self,
        index: u64,
        obj: &ActiveObject,
        color_meta: neoutl_media_api::ColorMeta,
    ) -> u32 {
        self.write_video_uniform_raw(
            index,
            &obj.mvp,
            obj.opacity,
            color_meta.color_matrix,
            color_meta.color_range,
        )
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
}
