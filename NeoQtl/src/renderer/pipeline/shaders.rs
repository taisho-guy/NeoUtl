use super::*;
use crate::ecs::components::BlendMode;

pub(super) const BLEND_VARIANT_COUNT: usize = 9;

pub(super) fn blend_state_for(mode: BlendMode) -> wgpu::BlendState {
    let alpha = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    };
    let color = match mode {
        BlendMode::Normal => wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        BlendMode::Add => wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        BlendMode::Multiply => wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Dst,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        BlendMode::Screen => wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::OneMinusDst,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        BlendMode::Overlay => wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        BlendMode::Darken => wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Min,
        },
        BlendMode::Lighten => wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        },
        BlendMode::Difference => wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::ReverseSubtract,
        },
        BlendMode::Exclusion => wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::OneMinusDst,
            dst_factor: wgpu::BlendFactor::OneMinusSrc,
            operation: wgpu::BlendOperation::Add,
        },
    };
    wgpu::BlendState { color, alpha }
}

pub(super) fn blend_variants<T, F: Fn(usize, wgpu::BlendState) -> T>(
    build: F,
) -> [T; BLEND_VARIANT_COUNT] {
    std::array::from_fn(|i| {
        let mode = match i {
            0 => BlendMode::Normal,
            1 => BlendMode::Add,
            2 => BlendMode::Multiply,
            3 => BlendMode::Screen,
            4 => BlendMode::Overlay,
            5 => BlendMode::Darken,
            6 => BlendMode::Lighten,
            7 => BlendMode::Difference,
            _ => BlendMode::Exclusion,
        };
        build(i, blend_state_for(mode))
    })
}

pub(super) fn try_create_shader_module(
    device: &wgpu::Device,
    wgsl: &[u8],
    label: &str,
) -> Result<wgpu::ShaderModule, String> {
    let text = std::str::from_utf8(wgsl)
        .map_err(|err| t!("WGSLソースが非UTF-8: %{arg0}", arg0 = format!("{err}")).to_string())?;
    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(text)),
    });
    match pollster::block_on(error_scope.pop()) {
        Some(err) => Err(format!("{err}")),
        None => Ok(shader),
    }
}

pub(super) fn build_pipeline_with_blend(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    blend: wgpu::BlendState,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
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

pub(super) fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    wgsl: &[u8],
    label: &str,
) -> Result<[wgpu::RenderPipeline; BLEND_VARIANT_COUNT], String> {
    let shader = try_create_shader_module(device, wgsl, label)?;
    Ok(blend_variants(|i, blend| {
        build_pipeline_with_blend(device, layout, &shader, &format!("{label}#{i}"), blend)
    }))
}

pub(super) fn build_pipelines_from_registry(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
) -> HashMap<u32, ([wgpu::RenderPipeline; BLEND_VARIANT_COUNT], u32)> {
    registry()
        .iter()
        .filter_map(|plugin| {
            let vertex_count = unsafe { (plugin.vtable.vertex_count)() };
            if vertex_count == 0 {
                return None;
            }
            let src = unsafe { (plugin.vtable.wgsl)() };
            if src.is_empty() {
                return None;
            }
            let wgsl = unsafe { src.as_slice() };
            match build_pipeline(device, layout, wgsl, &plugin.name) {
                Ok(pipelines) => Some((plugin.kind_id, (pipelines, vertex_count))),
                Err(err) => {
                    eprintln!("{}", t!("[NeoUtl] オブジェクトプラグインのシェーダコンパイル失敗、除外して継続: kind_id=%{arg0} name=%{arg1} 理由=%{arg2}", arg0 = format!("{}", plugin.kind_id), arg1 = format!("{}", plugin.name), arg2 = format!("{err}")));
                    None
                }
            }
        })
        .collect()
}

pub(super) fn build_effect_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    wgsl: &[u8],
    label: &str,
) -> Result<wgpu::RenderPipeline, String> {
    let shader = try_create_shader_module(device, wgsl, label)?;
    Ok(
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
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
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
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
        }),
    )
}

pub(super) fn build_effect_pipelines_from_registry(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
) -> HashMap<String, wgpu::RenderPipeline> {
    effects::registry()
        .iter()
        .filter_map(|source| {
            let wgsl = source.wgsl_bytes();
            if wgsl.is_empty() {
                return None;
            }
            match build_effect_pipeline(device, layout, wgsl, source.name()) {
                Ok(pipeline) => Some((source.id().to_owned(), pipeline)),
                Err(err) => {
                    eprintln!("{}", t!("[NeoUtl] エフェクトのシェーダコンパイル失敗、除外して継続: id=%{arg0} name=%{arg1} 理由=%{arg2}", arg0 = format!("{}", source.id()), arg1 = format!("{}", source.name()), arg2 = format!("{err}")));
                    None
                }
            }
        })
        .collect()
}

pub(super) fn build_reduce_mean_pipeline(
    device: &wgpu::Device,
    wgsl: &'static str,
) -> (wgpu::ComputePipeline, wgpu::BindGroupLayout) {
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Reduce Mean BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Reduce Mean Pipeline Layout"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Reduce Mean Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(wgsl)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Reduce Mean Pipeline"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: Default::default(),
        cache: None,
    });
    (pipeline, bgl)
}

pub(super) fn build_lua_compute_pipelines(
    device: &wgpu::Device,
    defs: &[neoutl_lua_runtime::ComputeDef],
) -> HashMap<String, wgpu::ComputePipeline> {
    defs.iter()
        .filter_map(|def| {
            let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&def.id),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(def.wgsl.as_str())),
            });
            if let Some(err) = pollster::block_on(error_scope.pop()) {
                eprintln!("{}", t!("[NeoUtl] system.register_compute シェーダコンパイル失敗、除外: id=%{arg0} 理由=%{arg1}", arg0 = format!("{}", def.id), arg1 = format!("{err}")));
                return None;
            }
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(&def.id),
                layout: None,
                module: &shader,
                entry_point: Some("cs_main"),
                compilation_options: Default::default(),
                cache: None,
            });
            Some((def.id.clone(), pipeline))
        })
        .collect()
}

pub(super) fn build_composite_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    wgsl: &'static str,
) -> [wgpu::RenderPipeline; BLEND_VARIANT_COUNT] {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Composite"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(wgsl)),
    });
    blend_variants(|i, blend| {
        build_pipeline_with_blend(device, layout, &shader, &format!("Composite#{i}"), blend)
    })
}

pub(super) fn build_clip_composite_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    wgsl: &'static str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Clip Composite"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(wgsl)),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Clip Composite"),
        layout: Some(layout),
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
                format: wgpu::TextureFormat::Rgba16Float,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
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

pub(super) enum EffectObjectDrawKind<'a> {
    Standard {
        obj: &'a ActiveObject,
        offset: u32,
    },
    Media {
        texture: &'a wgpu::Texture,
        offset: u32,
        blend_mode: BlendMode,
    },
    Text {
        clip_instance: u64,
        offset: u32,
        blend_mode: BlendMode,
    },
}

pub(super) fn build_media_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    wgsl: &'static str,
) -> [wgpu::RenderPipeline; BLEND_VARIANT_COUNT] {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Media Object"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(wgsl)),
    });
    blend_variants(|i, blend| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("Media Object#{i}")),
            layout: Some(layout),
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
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    })
}
