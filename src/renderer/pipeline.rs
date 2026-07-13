// src/renderer/pipeline.rs
use crate::ecs::resources::ProjectResource;
use crate::ecs::systems::ActiveObject;
use crate::ecs::types::Value;
use crate::effects;
use crate::objects::{by_kind_id, registry};
use slint::wgpu_29::wgpu;
use std::collections::HashMap;
use std::sync::Arc;
use wgpu_text::glyph_brush::ab_glyph::FontArc;
use wgpu_text::{BrushBuilder, TextBrush};

/// 全ObjectVTable実装が共有する標準Uniform契約（shape.wgsl等のUniforms構造体と一致させること）。
/// mat4x4<f32>(64) + opacity(4) + sides(4) + extrude_depth(4) + _pad0(4) + fill_color(16) = 96 bytes
const STANDARD_UNIFORM_SIZE: u64 = 96;
/// wgpuのmin_uniform_buffer_offset_alignment既定値。動的オフセットの単位ストライドとして採用する。
const UNIFORM_STRIDE: u64 = 256;
const MAX_OBJECTS: u64 = 512;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// エフェクトUniformsバッファの確保上限（array<vec4<f32>, 8> = 128byte相当まで対応）。
/// 現行16エフェクトの最大パラメータ数(clipping=5件→uniform_size_std=32byte)を十分に上回る。
const MAX_EFFECT_UNIFORM_SIZE: u64 = 128;

pub struct RenderEngine {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub texture: wgpu::Texture,
    pub depth_texture: wgpu::Texture,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub text_brush: Option<TextBrush>,
    pub render_width: u32,
    pub render_height: u32,
    pipelines: HashMap<u32, (wgpu::RenderPipeline, u32)>,
    effect_pipelines: HashMap<String, wgpu::RenderPipeline>,
    effect_bind_group_layout: wgpu::BindGroupLayout,
    effect_sampler: wgpu::Sampler,
    effect_uniform_buffer: wgpu::Buffer,
    effect_ping: wgpu::Texture,
    effect_pong: wgpu::Texture,
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
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

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    wgsl: &str,
    label: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
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

fn build_pipelines_from_registry(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
) -> HashMap<u32, (wgpu::RenderPipeline, u32)> {
    registry()
        .iter()
        .filter_map(|plugin| {
            let vertex_count = unsafe { ((*plugin.vtable).vertex_count)() };
            if vertex_count == 0 {
                return None;
            }
            let src = unsafe { ((*plugin.vtable).wgsl)() };
            if src.ptr.is_null() {
                return None;
            }
            let wgsl = unsafe {
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(src.ptr, src.len))
            };
            Some((
                plugin.kind_id,
                (
                    build_pipeline(device, layout, wgsl, &plugin.name),
                    vertex_count,
                ),
            ))
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
    wgsl: &str,
    label: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
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

/// effects::loader::registry()の全プラグインからポストプロセスパイプラインを構築する。
/// エフェクトIDをキーとし、ActiveObject.effectsの並び順に都度引いて適用する。
fn build_effect_pipelines_from_registry(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
) -> HashMap<String, wgpu::RenderPipeline> {
    effects::registry()
        .iter()
        .filter_map(|plugin| {
            let src = unsafe { ((*plugin.vtable).wgsl)() };
            if src.ptr.is_null() {
                return None;
            }
            let wgsl = unsafe {
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(src.ptr, src.len))
            };
            Some((
                plugin.id.clone(),
                build_effect_pipeline(device, layout, wgsl, &plugin.name),
            ))
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

        let text_brush = load_font().and_then(|f| {
            FontArc::try_from_vec(f).ok().map(|fa| {
                BrushBuilder::using_font(fa).build(
                    &device,
                    width,
                    height,
                    wgpu::TextureFormat::Rgba8Unorm,
                )
            })
        });

        Self {
            device,
            queue,
            texture,
            depth_texture,
            uniform_buffer,
            bind_group_layout,
            bind_group,
            text_brush,
            render_width: width,
            render_height: height,
            pipelines,
            effect_pipelines,
            effect_bind_group_layout,
            effect_sampler,
            effect_uniform_buffer,
            effect_ping,
            effect_pong,
        }
    }

    pub fn resize_render_target(&mut self, width: u32, height: u32) {
        self.render_width = width;
        self.render_height = height;
        self.texture = create_texture(&self.device, width, height);
        self.depth_texture = create_depth_texture(&self.device, width, height);
        self.effect_ping = create_effect_texture(&self.device, width, height);
        self.effect_pong = create_effect_texture(&self.device, width, height);
        if let Some(ref mut b) = self.text_brush {
            b.resize_view(width as f32, height as f32, &self.queue);
        }
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
            .map(|s| (s.sides as f32, s.extrude_depth, s.fill_color))
            .unwrap_or((4.0, 0.0, [1.0, 1.0, 1.0, 1.0]));
        data[68..72].copy_from_slice(&sides.to_le_bytes());
        data[72..76].copy_from_slice(&extrude_depth.to_le_bytes());
        // 76..80 は _pad0（未使用）
        data[80..96].copy_from_slice(bytemuck::cast_slice(&fill_color));

        let offset = index * UNIFORM_STRIDE;
        self.queue.write_buffer(&self.uniform_buffer, offset, &data);
        offset as u32
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
                    params
                        .get(key)
                        .map(|v| match v {
                            Value::Number(n) => *n,
                            Value::Bool(b) => {
                                if *b {
                                    1.0
                                } else {
                                    0.0
                                }
                            }
                            Value::Text(_) => s.default_float,
                        })
                        .unwrap_or(s.default_float)
                })
                .collect();

            let uniform_size = (unsafe { (plugin.vtable.uniform_size)() } as usize).max(16);
            let mut bytes = vec![0u8; uniform_size];
            unsafe {
                (plugin.vtable.pack_uniform)(
                    values.as_ptr(),
                    values.len() as u32,
                    bytes.as_mut_ptr(),
                )
            };
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
        // 描画対象のみ標準Uniformバッファへ事前書き込みする（テキストはbuffer不要のためスキップ）。
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

        let text_sections: Vec<_> = active_objects
            .iter()
            .filter_map(|obj| {
                let plugin = by_kind_id(obj.kind_id)?;
                let meta = unsafe { &*((plugin.vtable.meta)()) };
                if meta.stable_id != neoutl_object_api::TEXT_STABLE_ID {
                    return None;
                }
                let tc = obj.text_content.as_ref()?;
                Some(crate::media::text::build_section(
                    tc,
                    self.render_width,
                    self.render_height,
                ))
            })
            .collect();

        if let Some(ref mut brush) = self.text_brush {
            if !text_sections.is_empty() {
                let refs: Vec<&_> = text_sections.iter().collect();
                let _ = brush.queue(self.device.as_ref(), self.queue.as_ref(), refs);
            }
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

            if let Some(ref mut brush) = self.text_brush {
                brush.draw(&mut rpass);
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
