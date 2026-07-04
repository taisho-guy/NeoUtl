// src/renderer/pipeline.rs
use crate::ecs::resources::ProjectResource;
use crate::ecs::systems::ActiveObject;
use crate::objects::{by_kind_id, registry};
use slint::wgpu_29::wgpu;
use std::collections::HashMap;
use std::sync::Arc;
use wgpu_text::glyph_brush::{Section as TextSection, Text, ab_glyph::FontArc};
use wgpu_text::{BrushBuilder, TextBrush};

pub struct RenderEngine {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub texture: wgpu::Texture,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub angle: f32,
    pub text_brush: Option<TextBrush>,
    pub render_width: u32,
    pub render_height: u32,
    pipelines: HashMap<u32, (wgpu::RenderPipeline, u32)>,
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
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
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

impl RenderEngine {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, width: u32, height: u32) -> Self {
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform BG"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipelines = build_pipelines_from_registry(&device, &pipeline_layout);
        let texture = create_texture(&device, width, height);
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
            uniform_buffer,
            bind_group,
            angle: 0.0,
            text_brush,
            render_width: width,
            render_height: height,
            pipelines,
        }
    }

    pub fn resize_render_target(&mut self, width: u32, height: u32) {
        self.render_width = width;
        self.render_height = height;
        self.texture = create_texture(&self.device, width, height);
        if let Some(ref mut b) = self.text_brush {
            b.resize_view(width as f32, height as f32, &self.queue);
        }
        eprintln!("[NeoUtl] レンダーターゲット変更: {width}×{height}");
    }

    pub fn render(&mut self, active_objects: &[ActiveObject], _project: &ProjectResource) {
        self.angle += 0.02;
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[self.angle]));

        let text_sections: Vec<TextSection<'_>> = active_objects
            .iter()
            .filter_map(|obj| {
                let plugin = by_kind_id(obj.kind_id)?;
                if plugin.name != "Text" {
                    return None;
                }
                let tc = obj.text_content.as_ref()?;
                Some(
                    TextSection::default()
                        .with_screen_position((
                            tc.x * self.render_width as f32,
                            tc.y * self.render_height as f32,
                        ))
                        .add_text(
                            Text::new(&tc.text)
                                .with_scale(tc.font_size)
                                .with_color(tc.color),
                        ),
                )
            })
            .collect();

        if let Some(ref mut brush) = self.text_brush {
            if !text_sections.is_empty() {
                let refs: Vec<&TextSection<'_>> = text_sections.iter().collect();
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
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            for obj in active_objects {
                if let Some((pipeline, vertex_count)) = self.pipelines.get(&obj.kind_id) {
                    rpass.set_pipeline(pipeline);
                    rpass.set_bind_group(0, &self.bind_group, &[]);
                    rpass.draw(0..*vertex_count, 0..1);
                }
            }

            if let Some(ref mut brush) = self.text_brush {
                brush.draw(&mut rpass);
            }
        }

        self.queue.submit([encoder.finish()]);
    }
}
