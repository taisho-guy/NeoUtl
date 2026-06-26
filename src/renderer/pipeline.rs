// src/renderer/pipeline.rs
use crate::ecs::resources::ProjectResource;
use crate::ecs::systems::ActiveObject;
use crate::objects::{RenderKind, cube, tetrahedron};
use slint::wgpu_29::wgpu;
use std::sync::Arc;
use wgpu_text::glyph_brush::{Section as TextSection, Text, ab_glyph::FontArc};
use wgpu_text::{BrushBuilder, TextBrush};

pub struct RenderEngine {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub tetrahedron_pipeline: wgpu::RenderPipeline,
    pub cube_pipeline: wgpu::RenderPipeline,
    pub texture: wgpu::Texture,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub angle: f32,
    /// wgpu_text ブラシ。フォントが見つからない場合は None。
    pub text_brush: Option<TextBrush>,
    /// 現在のレンダーターゲット幅
    pub render_width: u32,
    /// 現在のレンダーターゲット高さ
    pub render_height: u32,
}

// ── フォント読み込み ──────────────────────────────────────────────────────

fn load_font() -> Option<Vec<u8>> {
    let candidates = [
        // プロジェクトローカル（最優先）
        "assets/font.ttf",
        // Linux (DejaVu / Noto)
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        // macOS
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        // Windows
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/calibri.ttf",
    ];

    for path in &candidates {
        if let Ok(bytes) = std::fs::read(path) {
            eprintln!("[NeoUtl] フォント読み込み: {path}");
            return Some(bytes);
        }
    }

    eprintln!("[NeoUtl] 警告: フォントが見つかりません。テキスト描画は無効です。");
    eprintln!("[NeoUtl] assets/font.ttf に .ttf/.otf ファイルを配置してください。");
    None
}

// ── テクスチャ作成ヘルパー ─────────────────────────────────────────────

fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Render Target Texture"),
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

// ── RenderEngine 実装 ─────────────────────────────────────────────────

impl RenderEngine {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, width: u32, height: u32) -> Self {
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // ── ユニフォームバッファ（回転角度） ──
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

        // ── シェーダーとパイプライン ──
        let tetra_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tetrahedron Shader"),
            source: wgpu::ShaderSource::Wgsl(tetrahedron::WGSL_SHADER.into()),
        });
        let tetrahedron_pipeline =
            Self::create_shape_pipeline(&device, &pipeline_layout, &tetra_shader);

        let cube_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cube Shader"),
            source: wgpu::ShaderSource::Wgsl(cube::WGSL_SHADER.into()),
        });
        let cube_pipeline = Self::create_shape_pipeline(&device, &pipeline_layout, &cube_shader);

        // ── レンダーターゲットテクスチャ ──
        let texture = create_texture(device.as_ref(), width, height);

        // ── wgpu_text ブラシ ──
        let text_brush = load_font().and_then(|font| {
            FontArc::try_from_vec(font).ok().map(|font_arc| {
                BrushBuilder::using_font(font_arc).build(
                    device.as_ref(),
                    width,
                    height,
                    wgpu::TextureFormat::Rgba8Unorm,
                )
            })
        });

        if text_brush.is_some() {
            eprintln!("[NeoUtl] TextBrush 初期化成功 ({width}×{height})");
        }

        Self {
            device,
            queue,
            tetrahedron_pipeline,
            cube_pipeline,
            texture,
            uniform_buffer,
            bind_group,
            angle: 0.0,
            text_brush,
            render_width: width,
            render_height: height,
        }
    }

    fn create_shape_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shape Pipeline"),
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

    /// 解像度変更時: テクスチャを再生成し TextBrush のビューも更新する。
    pub fn resize_render_target(&mut self, width: u32, height: u32) {
        self.render_width = width;
        self.render_height = height;
        self.texture = create_texture(self.device.as_ref(), width, height);

        if let Some(ref mut brush) = self.text_brush {
            brush.resize_view(width as f32, height as f32, self.queue.as_ref());
        }

        eprintln!("[NeoUtl] レンダーターゲット変更: {width}×{height}");
    }

    /// 1 フレーム分のレンダリングを実行する。
    pub fn render(&mut self, active_objects: &[ActiveObject], _project: &ProjectResource) {
        // ── 角度更新 ──
        self.angle += 0.02;
        let angle_data = [self.angle];
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&angle_data));

        // ── テキストセクション構築 ──
        // render pass の開始前に queue() を呼ぶ必要があるため先に収集する。
        let text_sections: Vec<TextSection<'_>> = active_objects
            .iter()
            .filter_map(|obj| {
                if let (RenderKind::Text, Some(tc)) = (obj.kind, &obj.text_content) {
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
                } else {
                    None
                }
            })
            .collect();

        // ── テキストをブラシへキュー ──
        // NLL により text_brush と device/queue は別フィールドとして同時借用可能。
        if let Some(ref mut brush) = self.text_brush {
            if !text_sections.is_empty() {
                let refs: Vec<&TextSection<'_>> = text_sections.iter().collect();
                let _ = brush.queue(self.device.as_ref(), self.queue.as_ref(), refs);
            }
        }

        // ── コマンドエンコーダ & レンダーパス ──
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

            // 図形オブジェクトを描画
            for obj in active_objects {
                match obj.kind {
                    RenderKind::Tetrahedron => {
                        rpass.set_pipeline(&self.tetrahedron_pipeline);
                        rpass.set_bind_group(0, &self.bind_group, &[]);
                        rpass.draw(0..tetrahedron::VERTEX_COUNT, 0..1);
                    }
                    RenderKind::Cube => {
                        rpass.set_pipeline(&self.cube_pipeline);
                        rpass.set_bind_group(0, &self.bind_group, &[]);
                        rpass.draw(0..cube::VERTEX_COUNT, 0..1);
                    }
                    // テキストは text_brush.draw() で描画するためここでは何もしない
                    RenderKind::Text => {}
                }
            }

            // テキストを最前面に描画
            if let Some(ref mut brush) = self.text_brush {
                brush.draw(&mut rpass);
            }
        }

        self.queue.submit([encoder.finish()]);
    }
}
