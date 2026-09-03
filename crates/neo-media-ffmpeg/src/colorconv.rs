use std::sync::{Arc, OnceLock};

use neo_media_cache::{KIND_PLAYBACK, NeoMediaCache};
use neo_media_core::PixelFormat;

static YUV420P_WGSL: &str = include_str!(concat!(env!("OUT_DIR"), "/media_yuv420p.wgsl"));
static P010_WGSL: &str = include_str!(concat!(env!("OUT_DIR"), "/media_p010.wgsl"));

struct Yuv420pCompositor {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

fn compositor(device: &wgpu::Device) -> &'static Yuv420pCompositor {
    static COMPOSITOR: OnceLock<Yuv420pCompositor> = OnceLock::new();
    COMPOSITOR.get_or_init(|| build_compositor(device))
}

fn build_compositor(device: &wgpu::Device) -> Yuv420pCompositor {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("YUV420P Composite Shader"),
        source: wgpu::ShaderSource::Wgsl(YUV420P_WGSL.into()),
    });

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
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("YUV420P Composite BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            },
            plane_entry(1),
            plane_entry(2),
            plane_entry(3),
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("YUV420P Composite Pipeline Layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("YUV420P Composite Pipeline"),
        layout: Some(&pipeline_layout),
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
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("YUV420P Composite Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    Yuv420pCompositor {
        pipeline,
        bind_group_layout,
        sampler,
    }
}

fn upload_plane(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
    stride: u32,
    width: u32,
    height: u32,
    label: &str,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(stride),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture
}

struct P010Compositor {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

fn p010_compositor(device: &wgpu::Device) -> &'static P010Compositor {
    static COMPOSITOR: OnceLock<P010Compositor> = OnceLock::new();
    COMPOSITOR.get_or_init(|| build_p010_compositor(device))
}

fn build_p010_compositor(device: &wgpu::Device) -> P010Compositor {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("P010 Composite Shader"),
        source: wgpu::ShaderSource::Wgsl(P010_WGSL.into()),
    });

    let plane_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Uint,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("P010 Composite BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            },
            plane_entry(1),
            plane_entry(2),
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("P010 Composite Pipeline Layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("P010 Composite Pipeline"),
        layout: Some(&pipeline_layout),
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
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    P010Compositor {
        pipeline,
        bind_group_layout,
    }
}

fn upload_plane_r16(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
    stride: u32,
    width: u32,
    height: u32,
    label: &str,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R16Uint,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(stride),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture
}

fn upload_plane_rg16(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
    stride: u32,
    width: u32,
    height: u32,
    label: &str,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg16Uint,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(stride),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture
}

#[allow(clippy::too_many_arguments)]
pub fn composite_p010_to_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cache: &Arc<NeoMediaCache>,
    y_bytes: &[u8],
    y_stride: u32,
    uv_bytes: &[u8],
    uv_stride: u32,
    width: u32,
    height: u32,
    color_matrix: u32,
    color_range: u32,
) -> Result<wgpu::Texture, String> {
    let chroma_width = width.div_ceil(2);
    let chroma_height = height.div_ceil(2);

    let y_tex = upload_plane_r16(device, queue, y_bytes, y_stride, width, height, "P010 Y");
    let uv_tex = upload_plane_rg16(
        device,
        queue,
        uv_bytes,
        uv_stride,
        chroma_width,
        chroma_height,
        "P010 UV",
    );

    let output = cache
        .acquire_for_write_as(KIND_PLAYBACK, PixelFormat::Rgba8, width, height)
        .map_err(|err| format!("P010合成先テクスチャacquire失敗: {err:?}"))?;

    let comp = p010_compositor(device);

    let mut uniform_bytes = [0u8; 16];
    uniform_bytes[0..4].copy_from_slice(&color_matrix.to_le_bytes());
    uniform_bytes[4..8].copy_from_slice(&color_range.to_le_bytes());
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("P010 Composite Uniform"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

    let y_view = y_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let uv_view = uv_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("P010 Composite BG"),
        layout: &comp.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&y_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&uv_view),
            },
        ],
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("P010 Composite Encoder"),
    });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("P010 Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rpass.set_pipeline(&comp.pipeline);
        rpass.set_bind_group(0, &bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }
    let submission_index = queue.submit(std::iter::once(encoder.finish()));
    cache.mark_ready(PixelFormat::Rgba8, width, height, &output, submission_index);

    Ok(output)
}

#[allow(clippy::too_many_arguments)]
pub fn composite_yuv420p_to_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cache: &Arc<NeoMediaCache>,
    y_bytes: &[u8],
    y_stride: u32,
    u_bytes: &[u8],
    u_stride: u32,
    v_bytes: &[u8],
    v_stride: u32,
    width: u32,
    height: u32,
    color_matrix: u32,
    color_range: u32,
) -> Result<wgpu::Texture, String> {
    let chroma_width = width.div_ceil(2);
    let chroma_height = height.div_ceil(2);

    let y_tex = upload_plane(device, queue, y_bytes, y_stride, width, height, "YUV420P Y");
    let u_tex = upload_plane(
        device,
        queue,
        u_bytes,
        u_stride,
        chroma_width,
        chroma_height,
        "YUV420P U",
    );
    let v_tex = upload_plane(
        device,
        queue,
        v_bytes,
        v_stride,
        chroma_width,
        chroma_height,
        "YUV420P V",
    );

    let output = cache
        .acquire_for_write_as(KIND_PLAYBACK, PixelFormat::Rgba8, width, height)
        .map_err(|err| format!("YUV420P合成先テクスチャacquire失敗: {err:?}"))?;

    let comp = compositor(device);

    let mut uniform_bytes = [0u8; 16];
    uniform_bytes[0..4].copy_from_slice(&color_matrix.to_le_bytes());
    uniform_bytes[4..8].copy_from_slice(&color_range.to_le_bytes());
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("YUV420P Composite Uniform"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

    let y_view = y_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let u_view = u_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let v_view = v_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("YUV420P Composite BG"),
        layout: &comp.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&y_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&u_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&v_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&comp.sampler),
            },
        ],
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("YUV420P Composite Encoder"),
    });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("YUV420P Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rpass.set_pipeline(&comp.pipeline);
        rpass.set_bind_group(0, &bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }
    let submission_index = queue.submit(std::iter::once(encoder.finish()));
    cache.mark_ready(PixelFormat::Rgba8, width, height, &output, submission_index);

    Ok(output)
}
