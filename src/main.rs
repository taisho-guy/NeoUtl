use std::sync::{Arc, Mutex};
use slint::ComponentHandle;

slint::slint! {
    export component MainWindow inherits Window {
        preferred-width: 800px;
        preferred-height: 600px;
        title: "NeoUtl - wgpu 3D Rotation Test";

        in-out property <image> video-frame;
        
        callback request-render();

        Timer {
            interval: 16ms;
            running: true;
            triggered() => {
                root.request-render();
            }
        }

        VerticalLayout {
            padding: 20px;
            Text {
                text: "NeoUtl: wgpu Render Pipeline Test (Rotating Tetrahedron)";
                font-size: 20px;
                horizontal-alignment: center;
            }
            Rectangle {
                background: #1e1e1e;
                Image {
                    source: root.video-frame;
                    width: 100%;
                    height: 100%;
                }
            }
        }
    }
}

struct RenderEngine {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    render_pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    angle: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    slint::BackendSelector::new()
        .require_wgpu_28(slint::wgpu_28::WGPUConfiguration::default())
        .select()?;

    let app = MainWindow::new()?;
    let app_weak = app.as_weak();

    let engine_holder: Arc<Mutex<Option<RenderEngine>>> = Arc::new(Mutex::new(None));
    let engine_setup = engine_holder.clone();

    app.window().set_rendering_notifier(move |state, graphics_api| {
        if let (slint::RenderingState::RenderingSetup, slint::GraphicsAPI::WGPU28 { device, queue, .. }) = (state, graphics_api) {
            let mut engine_lock = engine_setup.lock().unwrap();
            if engine_lock.is_none() {
                let device = Arc::new(device.clone());
                let queue = Arc::new(queue.clone());

                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Tetrahedron Shader"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
                });

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
                    bind_group_layouts: &[&bind_group_layout],
                    immediate_size: 0, 
                });

                let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline"),
                    layout: Some(&pipeline_layout),
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
                });

                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Render Target Texture"),
                    size: wgpu::Extent3d { width: 800, height: 600, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });

                *engine_lock = Some(RenderEngine {
                    device,
                    queue,
                    render_pipeline,
                    texture,
                    uniform_buffer,
                    bind_group,
                    angle: 0.0,
                });
            }
        }
    })?;

    let engine_render = engine_holder.clone();
    app.on_request_render(move || {
        let mut engine_lock = engine_render.lock().unwrap();
        if let Some(ref mut engine) = *engine_lock {
            engine.angle += 0.03;
            let angle_array = [engine.angle];
            let bytes = bytemuck::cast_slice(&angle_array);
            engine.queue.write_buffer(&engine.uniform_buffer, 0, bytes);

            let mut encoder = engine.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });
            let view = engine.texture.create_view(&wgpu::TextureViewDescriptor::default());

            {
                let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.05, b: 0.1, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })];

                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass"),
                    color_attachments: color_attachments.as_slice(),
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                render_pass.set_pipeline(&engine.render_pipeline);
                render_pass.set_bind_group(0, &engine.bind_group, &[]);
                render_pass.draw(0..12, 0..1);
            }

            engine.queue.submit(std::iter::once(encoder.finish()));

            let imported_image = slint::Image::try_from(engine.texture.clone()).unwrap();
            if let Some(app) = app_weak.upgrade() {
                app.set_video_frame(imported_image);
            }
        }
    });

    app.run()?;
    Ok(())
}