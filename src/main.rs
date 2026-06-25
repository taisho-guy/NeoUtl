use std::sync::{Arc, Mutex};
use slint::ComponentHandle;

use slint::wgpu_29::wgpu;

slint::include_modules!();

#[derive(Clone, Copy, Debug)]
struct VideoObject {
    id: usize,
    start_frame: i32,
    end_frame: i32,
}

struct TimelineState {
    current_frame: i32,
    total_frames: i32,
    objects: Vec<VideoObject>,
    next_id: usize,
}

impl TimelineState {
    fn new() -> Self {
        Self {
            current_frame: 0,
            total_frames: 300,
            objects: Vec::new(),
            next_id: 1,
        }
    }
    fn update_total_frames(&mut self) {
        let max_end = self.objects.iter().map(|o| o.end_frame).max().unwrap_or(0);
        self.total_frames = max_end.max(300);
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
        .require_wgpu_29(slint::wgpu_29::WGPUConfiguration::default())
        .select()?;

    let app = MainWindow::new()?;
    let app_weak = app.as_weak();

    let timeline_state = Arc::new(Mutex::new(TimelineState::new()));
    let engine_holder: Arc<Mutex<Option<RenderEngine>>> = Arc::new(Mutex::new(None));
    let engine_setup = engine_holder.clone();

    app.window().set_rendering_notifier(move |state, graphics_api| {
        if let (slint::RenderingState::RenderingSetup, slint::GraphicsAPI::WGPU29 { device, queue, .. }) = (state, graphics_api) {
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
                    bind_group_layouts: &[Some(&bind_group_layout)],
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

    let state_ctrl = timeline_state.clone();
    let app_weak_ctrl = app.as_weak();
    app.on_seek_timeline(move |ratio| {
        if let Some(app) = app_weak_ctrl.upgrade() {
            let mut state = state_ctrl.lock().unwrap();
            let frame = (ratio * state.total_frames as f32) as i32;
            state.current_frame = frame.clamp(0, state.total_frames);
            app.set_current_frame(state.current_frame);
        }
    });

    let state_ctrl = timeline_state.clone();
    let app_weak_ctrl = app.as_weak();
    app.on_add_object_at(move |ratio| {
        if let Some(app) = app_weak_ctrl.upgrade() {
            let mut state = state_ctrl.lock().unwrap();
            let start = (ratio * state.total_frames as f32) as i32;
            let new_obj = VideoObject { id: state.next_id, start_frame: start, end_frame: start + 90 };
            state.next_id += 1;
            state.objects.push(new_obj);
            state.update_total_frames();

            app.set_total_frames(state.total_frames);
            let slint_objs: Vec<TimelineObject> = state.objects.iter().map(|o| {
                TimelineObject { id: o.id as i32, start_frame: o.start_frame, end_frame: o.end_frame }
            }).collect();
            app.set_objects(slint::ModelRc::new(slint::VecModel::from(slint_objs)));
        }
    });

    let state_ctrl = timeline_state.clone();
    let app_weak_ctrl = app.as_weak();
    app.on_delete_object(move |id| {
        if let Some(app) = app_weak_ctrl.upgrade() {
            let mut state = state_ctrl.lock().unwrap();
            state.objects.retain(|o| o.id != id as usize);
            state.update_total_frames();

            if state.current_frame > state.total_frames {
                state.current_frame = state.total_frames;
                app.set_current_frame(state.current_frame);
            }

            app.set_total_frames(state.total_frames);
            let slint_objs: Vec<TimelineObject> = state.objects.iter().map(|o| {
                TimelineObject { id: o.id as i32, start_frame: o.start_frame, end_frame: o.end_frame }
            }).collect();
            app.set_objects(slint::ModelRc::new(slint::VecModel::from(slint_objs)));
        }
    });

    let engine_render = engine_holder.clone();
    let state_render = timeline_state.clone();
    app.on_request_render(move || {
        let mut engine_lock = engine_render.lock().unwrap();
        if let Some(ref mut engine) = *engine_lock {
            
            let has_object = {
                let state = state_render.lock().unwrap();
                state.objects.iter().any(|o| state.current_frame >= o.start_frame && state.current_frame < o.end_frame)
            };

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
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.05, b: 0.07, a: 1.0 }),
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

                if has_object {
                    render_pass.set_pipeline(&engine.render_pipeline);
                    render_pass.set_bind_group(0, &engine.bind_group, &[]);
                    render_pass.draw(0..12, 0..1);
                }
            }

            engine.queue.submit([encoder.finish()]);

            let imported_image = slint::Image::try_from(engine.texture.clone()).unwrap();
            if let Some(app) = app_weak.upgrade() {
                app.set_video_frame(imported_image);
            }
        }
    });

    app.run()?;
    Ok(())
}