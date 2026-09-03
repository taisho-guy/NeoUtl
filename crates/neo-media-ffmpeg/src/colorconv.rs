use std::sync::{Arc, OnceLock};

use neo_media_cache::{KIND_PLAYBACK, NeoMediaCache};
use neo_media_core::PixelFormat;

static P0XX_WGSL: &str = include_str!(concat!(env!("OUT_DIR"), "/media_p010.wgsl"));

struct P0xxCompositor {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

fn p0xx_compositor(device: &wgpu::Device) -> &'static P0xxCompositor {
    static COMPOSITOR: OnceLock<P0xxCompositor> = OnceLock::new();
    COMPOSITOR.get_or_init(|| build_p0xx_compositor(device))
}

fn build_p0xx_compositor(device: &wgpu::Device) -> P0xxCompositor {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("P0xx Composite Shader"),
        source: wgpu::ShaderSource::Wgsl(P0XX_WGSL.into()),
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
        label: Some("P0xx Composite BGL"),
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
        label: Some("P0xx Composite Pipeline Layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("P0xx Composite Pipeline"),
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

    P0xxCompositor {
        pipeline,
        bind_group_layout,
    }
}

fn luma_texture_format(bit_depth: u32) -> wgpu::TextureFormat {
    if bit_depth <= 8 {
        wgpu::TextureFormat::R8Uint
    } else {
        wgpu::TextureFormat::R16Uint
    }
}

fn chroma_texture_format(bit_depth: u32) -> wgpu::TextureFormat {
    if bit_depth <= 8 {
        wgpu::TextureFormat::Rg8Uint
    } else {
        wgpu::TextureFormat::Rg16Uint
    }
}

fn create_plane_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    label: &str,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn write_plane(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    bytes: &[u8],
    stride: u32,
    width: u32,
    height: u32,
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
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
}

struct P0xxGpuState {
    y_tex: wgpu::Texture,
    uv_tex: wgpu::Texture,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    bit_depth: u32,
}

#[derive(Default)]
pub struct P0xxGpuResources {
    state: Option<P0xxGpuState>,
}

impl P0xxGpuResources {
    pub fn new() -> Self {
        Self::default()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn composite_p0xx_to_rgba(
    resources: &mut P0xxGpuResources,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cache: &Arc<NeoMediaCache>,
    y_bytes: &[u8],
    y_stride: u32,
    uv_bytes: &[u8],
    uv_stride: u32,
    width: u32,
    height: u32,
    bit_depth: u32,
    color_matrix: u32,
    color_range: u32,
) -> Result<wgpu::Texture, String> {
    let chroma_width = width.div_ceil(2);
    let chroma_height = height.div_ceil(2);

    let needs_rebuild = match &resources.state {
        Some(s) => s.width != width || s.height != height || s.bit_depth != bit_depth,
        None => true,
    };

    let comp = p0xx_compositor(device);

    if needs_rebuild {
        let y_tex = create_plane_texture(
            device,
            width,
            height,
            "P0xx Y",
            luma_texture_format(bit_depth),
        );
        let uv_tex = create_plane_texture(
            device,
            chroma_width,
            chroma_height,
            "P0xx UV",
            chroma_texture_format(bit_depth),
        );
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("P0xx Composite Uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let y_view = y_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let uv_view = uv_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("P0xx Composite BG"),
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
        resources.state = Some(P0xxGpuState {
            y_tex,
            uv_tex,
            uniform_buffer,
            bind_group,
            width,
            height,
            bit_depth,
        });
    }

    let state = resources
        .state
        .as_ref()
        .expect("直前でrebuildにより必ず設定済み");

    write_plane(queue, &state.y_tex, y_bytes, y_stride, width, height);
    write_plane(
        queue,
        &state.uv_tex,
        uv_bytes,
        uv_stride,
        chroma_width,
        chroma_height,
    );

    let storage_bits: u32 = if bit_depth <= 8 { 8 } else { 16 };
    let mut uniform_bytes = [0u8; 16];
    uniform_bytes[0..4].copy_from_slice(&color_matrix.to_le_bytes());
    uniform_bytes[4..8].copy_from_slice(&color_range.to_le_bytes());
    uniform_bytes[8..12].copy_from_slice(&bit_depth.to_le_bytes());
    uniform_bytes[12..16].copy_from_slice(&storage_bits.to_le_bytes());
    queue.write_buffer(&state.uniform_buffer, 0, &uniform_bytes);

    let output = cache
        .acquire_for_write_as(KIND_PLAYBACK, PixelFormat::Rgba8, width, height)
        .map_err(|err| format!("P0xx合成先テクスチャacquire失敗: {err:?}"))?;
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("P0xx Composite Encoder"),
    });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("P0xx Composite Pass"),
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
        rpass.set_bind_group(0, &state.bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }
    let submission_index = queue.submit(std::iter::once(encoder.finish()));
    cache.mark_ready(PixelFormat::Rgba8, width, height, &output, submission_index);

    Ok(output)
}
