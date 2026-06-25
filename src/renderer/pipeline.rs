// src/renderer/pipeline.rs
use crate::objects::{RenderKind, cube, tetrahedron};
use slint::wgpu_29::wgpu;
use std::sync::Arc;

pub struct RenderEngine {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub tetrahedron_pipeline: wgpu::RenderPipeline,
    pub cube_pipeline: wgpu::RenderPipeline,
    pub texture: wgpu::Texture,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub angle: f32,
}

impl RenderEngine {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Bind Group Layout"),
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
            label: Some("Uniform Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let tetra_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tetrahedron Shader"),
            source: wgpu::ShaderSource::Wgsl(tetrahedron::WGSL_SHADER.into()),
        });
        let tetrahedron_pipeline = Self::create_pipeline(&device, &pipeline_layout, &tetra_shader);

        let cube_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cube Shader"),
            source: wgpu::ShaderSource::Wgsl(cube::WGSL_SHADER.into()),
        });
        let cube_pipeline = Self::create_pipeline(&device, &pipeline_layout, &cube_shader);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Render Target Texture"),
            size: wgpu::Extent3d {
                width: 800,
                height: 600,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        Self {
            device,
            queue,
            tetrahedron_pipeline,
            cube_pipeline,
            texture,
            uniform_buffer,
            bind_group,
            angle: 0.0,
        }
    }

    fn create_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
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

    pub fn render(&mut self, active_kinds: &[RenderKind]) {
        self.angle += 0.02;
        let angle_array = [self.angle];
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&angle_array));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

            for &kind in active_kinds {
                match kind {
                    RenderKind::Tetrahedron => {
                        render_pass.set_pipeline(&self.tetrahedron_pipeline);
                        render_pass.set_bind_group(0, &self.bind_group, &[]);
                        render_pass.draw(0..tetrahedron::VERTEX_COUNT, 0..1);
                    }
                    RenderKind::Cube => {
                        render_pass.set_pipeline(&self.cube_pipeline);
                        render_pass.set_bind_group(0, &self.bind_group, &[]);
                        render_pass.draw(0..cube::VERTEX_COUNT, 0..1);
                    }
                }
            }
        }
        self.queue.submit([encoder.finish()]);
    }
}
