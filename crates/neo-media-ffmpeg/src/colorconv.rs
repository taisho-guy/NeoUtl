use neo_media_core::PixelFormat;
use neo_media_swscale::SwscaleUniforms;

const OPS_DISPATCH_WGSL: &str = include_str!("shaders/ops_dispatch.wgsl");
const OPS_DISPATCH_ENTRY: &str = "cs_main";

pub struct P0xxGpuResources {
    pub pipeline: wgpu::ComputePipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub uniform_buffer: wgpu::Buffer,
    pub tap_buffer_h: wgpu::Buffer,
    pub tap_buffer_v: wgpu::Buffer,
}

impl P0xxGpuResources {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ops_dispatch"),
            source: wgpu::ShaderSource::Wgsl(OPS_DISPATCH_WGSL.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ops_dispatch_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ops_dispatch_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ops_dispatch_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some(OPS_DISPATCH_ENTRY),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ops_dispatch_uniform_buffer"),
            size: std::mem::size_of::<SwscaleUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let tap_buffer_h = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ops_dispatch_tap_buffer_h"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tap_buffer_v = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ops_dispatch_tap_buffer_v"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
            tap_buffer_h,
            tap_buffer_v,
        }
    }
}

fn upload_plane_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
    stride: u32,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
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
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
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
pub fn composite_p0xx_to_rgba(
    res: &mut P0xxGpuResources,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cache: &neo_media_cache::NeoMediaCache,
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
    let storage_format = if bit_depth == 8 {
        wgpu::TextureFormat::R8Uint
    } else {
        wgpu::TextureFormat::R16Uint
    };
    let uv_format = if bit_depth == 8 {
        wgpu::TextureFormat::Rg8Uint
    } else {
        wgpu::TextureFormat::Rg16Uint
    };

    let plane_y = upload_plane_texture(
        device,
        queue,
        y_bytes,
        y_stride,
        width,
        height,
        storage_format,
        "swscale_plane_y",
    );
    let plane_uv = upload_plane_texture(
        device,
        queue,
        uv_bytes,
        uv_stride,
        width.div_ceil(2),
        height.div_ceil(2),
        uv_format,
        "swscale_plane_uv",
    );

    let uniforms = SwscaleUniforms {
        color_matrix,
        color_range,
        bit_depth,
        storage_bits: if bit_depth == 8 { 8 } else { 16 },
        src_width: width,
        src_height: height,
        dst_width: width,
        dst_height: height,
        tap_count_h: 1,
        tap_count_v: 1,
        _pad: [0, 0],
    };
    queue.write_buffer(&res.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

    let dst_texture = cache
        .acquire_for_write_as(
            neo_media_cache::KIND_PLAYBACK,
            PixelFormat::Rgba8,
            width,
            height,
        )
        .map_err(|e| format!("{e:?}"))?;
    let dst_view = dst_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let y_view = plane_y.create_view(&wgpu::TextureViewDescriptor::default());
    let uv_view = plane_uv.create_view(&wgpu::TextureViewDescriptor::default());

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ops_dispatch_p0xx_ram"),
        layout: &res.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: res.uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&y_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&uv_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: res.tap_buffer_h.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: res.tap_buffer_v.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&dst_view),
            },
        ],
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("swscale_p0xx_ram"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("ops_dispatch"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&res.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
    let submission_index = queue.submit(Some(encoder.finish()));
    cache.mark_ready(
        PixelFormat::Rgba8,
        width,
        height,
        &dst_texture,
        submission_index,
    );
    Ok(dst_texture)
}

#[allow(clippy::too_many_arguments)]
pub fn composite_yuv420p_to_rgba(
    res: &mut P0xxGpuResources,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cache: &neo_media_cache::NeoMediaCache,
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
    let chroma_w = width.div_ceil(2);
    let chroma_h = height.div_ceil(2);
    let mut uv_interleaved = vec![0u8; (chroma_w * chroma_h * 2) as usize];
    for row in 0..chroma_h {
        for col in 0..chroma_w {
            let u_val = u_bytes[(row * u_stride + col) as usize];
            let v_val = v_bytes[(row * v_stride + col) as usize];
            let idx = ((row * chroma_w + col) * 2) as usize;
            uv_interleaved[idx] = u_val;
            uv_interleaved[idx + 1] = v_val;
        }
    }
    let _ = y_stride;
    composite_p0xx_to_rgba(
        res,
        device,
        queue,
        cache,
        y_bytes,
        width,
        &uv_interleaved,
        chroma_w * 2,
        width,
        height,
        8,
        color_matrix,
        color_range,
    )
}
