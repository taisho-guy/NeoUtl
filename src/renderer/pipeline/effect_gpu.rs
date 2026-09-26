use egui_wgpu::wgpu;
use neoutl_effect_api::EffectRenderContext;
use neoutl_shared_abi::{
    CacheHandle, CacheImageRef, ComputeDispatch, ResourceKind, ResourceRef, StrRef,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use wgpu::util::DeviceExt;

thread_local! {
    static ACTIVE: RefCell<Option<Bridge>> = const { RefCell::new(None) };
}

static CACHES: OnceLock<Mutex<HashMap<(usize, usize, String), wgpu::Texture>>> = OnceLock::new();
static CACHE_HANDLE: CacheHandle = CacheHandle {
    get_image: cache_get,
    create_image: cache_create,
    clear_image: cache_clear,
};

pub(super) struct Bridge {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    object: wgpu::Texture,
    framebuffer: wgpu::Texture,
    images: HashMap<PathBuf, wgpu::Texture>,
    random: wgpu::Texture,
    named: HashMap<String, wgpu::Texture>,
    temps: HashMap<String, wgpu::Texture>,
    namespace: usize,
    width: u32,
    height: u32,
    roi: neoutl_shared_abi::Roi,
}

impl Bridge {
    pub(super) fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        object: &wgpu::Texture,
        framebuffer: &wgpu::Texture,
        images: HashMap<PathBuf, wgpu::Texture>,
        namespace: usize,
        width: u32,
        height: u32,
        roi: neoutl_shared_abi::Roi,
    ) -> Self {
        let random = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Effect Random Resource"),
            size: wgpu::Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut pixels = vec![0u8; 256 * 256 * 4];
        let mut state = 0x9e37_79b9u32;
        for p in pixels.chunks_exact_mut(4) {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            p.copy_from_slice(&[
                (state & 255) as u8,
                ((state >> 8) & 255) as u8,
                ((state >> 16) & 255) as u8,
                255,
            ]);
        }
        queue.write_texture(
            random.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256 * 4),
                rows_per_image: Some(256),
            },
            random.size(),
        );
        Self {
            device,
            queue,
            object: object.clone(),
            framebuffer: framebuffer.clone(),
            images,
            random,
            named: HashMap::new(),
            temps: HashMap::new(),
            namespace,
            width,
            height,
            roi,
        }
    }

    fn create_storage_texture(&self, label: &str, width: u32, height: u32) -> wgpu::Texture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Initialize Effect Storage Texture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
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
        drop(_pass);
        crate::infra::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
        texture
    }

    fn resolve(&mut self, resource: ResourceRef, create: bool) -> Option<wgpu::Texture> {
        let name = unsafe { ref_name(resource.name) }?;
        match resource.kind {
            ResourceKind::Object => Some(self.object.clone()),
            ResourceKind::Framebuffer => Some(self.framebuffer.clone()),
            ResourceKind::Random => Some(self.random.clone()),
            ResourceKind::Image => {
                let path = PathBuf::from(name);
                if let Some(texture) = self.images.get(&path) {
                    return Some(texture.clone());
                }
                let texture = neoutl_media_runtime::cache::global()
                    .frame_at(&path, 0, 0, &self.device, &self.queue)
                    .ok()?;
                self.images.insert(path, texture.clone());
                Some(texture)
            }
            ResourceKind::Named => {
                if create && !self.named.contains_key(&name) {
                    let texture = self.create_storage_texture(
                        "Effect Named Resource",
                        self.width,
                        self.height,
                    );
                    self.named.insert(name.clone(), texture);
                }
                self.named.get(&name).cloned()
            }
            ResourceKind::TempBuffer => {
                let key = if name.is_empty() {
                    "default".to_owned()
                } else {
                    name
                };
                if create && !self.temps.contains_key(&key) {
                    let texture = self.create_storage_texture(
                        "Effect Temporary Resource",
                        self.width,
                        self.height,
                    );
                    self.temps.insert(key.clone(), texture);
                }
                self.temps.get(&key).cloned()
            }
            ResourceKind::Cache => CACHES
                .get_or_init(Default::default)
                .lock()
                .ok()?
                .get(&(
                    self.device.as_ref() as *const _ as usize,
                    self.namespace,
                    name,
                ))
                .cloned(),
        }
    }
}

unsafe fn ref_name(name: StrRef) -> Option<String> {
    if name.len == 0 {
        return Some(String::new());
    }
    if name.ptr.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(name.ptr, name.len) };
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

fn with_context<R>(bridge: Bridge, f: impl FnOnce(&EffectRenderContext) -> R) -> R {
    ACTIVE.with(|slot| {
        let namespace = bridge.namespace;
        let previous = slot.replace(Some(bridge));
        let context = EffectRenderContext {
            cache: &CACHE_HANDLE,
            cache_identifier: namespace as *const (),
            exec_pixelshader,
            exec_computeshader,
            get_resource_size,
            get_resource_data,
            set_resource_data,
            release_resource,
        };
        struct Restore(*const RefCell<Option<Bridge>>, Option<Bridge>);
        impl Drop for Restore {
            fn drop(&mut self) {
                unsafe {
                    (*self.0).replace(self.1.take());
                }
            }
        }
        let restore = Restore(slot as *const _, previous);
        let result = f(&context);
        drop(restore);
        result
    })
}

fn with_active<R>(f: impl FnOnce(&mut Bridge) -> Result<R, u32>) -> Result<R, u32> {
    ACTIVE.with(|slot| {
        let mut active = slot.try_borrow_mut().map_err(|_| 1u32)?;
        f(active.as_mut().ok_or(1u32)?)
    })
}

unsafe extern "C" fn cache_get(identifier: *const (), name: StrRef) -> CacheImageRef {
    let original_name = name;
    let Some(name) = (unsafe { ref_name(name) }) else {
        return CacheImageRef::empty();
    };
    with_active(|bridge| {
        let key = (
            bridge.device.as_ref() as *const _ as usize,
            identifier as usize,
            name.clone(),
        );
        let caches = CACHES
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| 1u32)?;
        let Some(texture) = caches.get(&key) else {
            return Ok(CacheImageRef::empty());
        };
        Ok(CacheImageRef {
            resource: ResourceRef::cache(original_name),
            width: texture.width(),
            height: texture.height(),
            valid: 1,
        })
    })
    .unwrap_or_else(|_| CacheImageRef::empty())
}

unsafe extern "C" fn cache_create(
    identifier: *const (),
    name: StrRef,
    width: u32,
    height: u32,
) -> CacheImageRef {
    let original_name = name;
    let Some(name) = (unsafe { ref_name(name) }) else {
        return CacheImageRef::empty();
    };
    with_active(|bridge| {
        let key = (
            bridge.device.as_ref() as *const _ as usize,
            identifier as usize,
            name.clone(),
        );
        let mut caches = CACHES
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| 1u32)?;
        let texture = match caches.get(&key) {
            Some(texture)
                if texture.width() == width.max(1) && texture.height() == height.max(1) =>
            {
                texture.clone()
            }
            _ => {
                let texture = bridge.create_storage_texture("Effect VRAM Cache", width, height);
                caches.insert(key, texture.clone());
                texture
            }
        };
        Ok(CacheImageRef {
            resource: ResourceRef::cache(original_name),
            width: texture.width(),
            height: texture.height(),
            valid: 1,
        })
    })
    .unwrap_or_else(|_| CacheImageRef::empty())
}

unsafe extern "C" fn cache_clear(identifier: *const (), name: StrRef) {
    let Some(name) = (unsafe { ref_name(name) }) else {
        return;
    };
    let _ = with_active(|bridge| {
        CACHES
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| 1u32)?
            .remove(&(
                bridge.device.as_ref() as *const _ as usize,
                identifier as usize,
                name,
            ));
        Ok(())
    });
}

unsafe extern "C" fn exec_pixelshader(
    data: *const u8,
    len: usize,
    target: ResourceRef,
    resources: *const ResourceRef,
    resource_count: u32,
    constants: *const u8,
    constant_len: usize,
) -> u32 {
    if data.is_null()
        || (resource_count > 0 && resources.is_null())
        || (constant_len > 0 && constants.is_null())
    {
        return 2;
    }
    if resource_count > 2 {
        return 3;
    }
    let wgsl = unsafe { std::slice::from_raw_parts(data, len) };
    let refs = if resource_count == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(resources, resource_count as usize) }
    };
    let constants = if constant_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(constants, constant_len) }
    };
    with_active(|bridge| render_pixel(bridge, wgsl, target, refs, constants)).map_or(4, |_| 0)
}

fn render_pixel(
    bridge: &mut Bridge,
    wgsl: &[u8],
    target: ResourceRef,
    resources: &[ResourceRef],
    constants: &[u8],
) -> Result<(), u32> {
    let target_tex = bridge.resolve(target, true).ok_or(2u32)?;
    if matches!(target.kind, ResourceKind::Object) {
        return Err(3);
    }
    let first = if let Some(r) = resources.first() {
        bridge.resolve(*r, false).ok_or(2u32)?
    } else {
        bridge.object.clone()
    };
    let second = if let Some(r) = resources.get(1) {
        bridge.resolve(*r, false).ok_or(2u32)?
    } else {
        bridge.random.clone()
    };
    if resources.iter().any(|r| {
        r.kind == target.kind && unsafe { ref_name(r.name) } == unsafe { ref_name(target.name) }
    }) {
        return Err(3);
    }
    let source = std::str::from_utf8(wgsl).map_err(|_| 4u32)?;
    let module = bridge
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Effect Custom Pixel Shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    let layout = bridge
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Effect Custom Pixel Layout"),
            bind_group_layouts: &[Some(&pixel_bgl(&bridge.device))],
            immediate_size: 0,
        });
    let pipeline = bridge
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Effect Custom Pixel Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_tex.format(),
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
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
    let sampler = bridge
        .device
        .create_sampler(&wgpu::SamplerDescriptor::default());
    let mut padded = constants.to_vec();
    padded.resize(padded.len().max(16).div_ceil(16) * 16, 0);
    let uniform = bridge
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Effect Custom Constants"),
            contents: &padded,
            usage: wgpu::BufferUsages::UNIFORM,
        });
    let first_view = first.create_view(&Default::default());
    let second_view = second.create_view(&Default::default());
    let target_view = target_tex.create_view(&Default::default());
    let bgl = pixel_bgl(&bridge.device);
    let bind = bridge.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Effect Custom Pixel Bind Group"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&first_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&second_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let mut encoder = bridge.device.create_command_encoder(&Default::default());
    if target.kind == ResourceKind::Framebuffer {
        copy_input_to_framebuffer(bridge, &mut encoder)?;
    } else if target_tex.width() == first.width()
        && target_tex.height() == first.height()
        && target_tex.format() == first.format()
    {
        encoder.copy_texture_to_texture(
            first.as_image_copy(),
            target_tex.as_image_copy(),
            first.size(),
        );
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Effect Custom Pixel Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let sx = bridge.roi.x.floor().max(0.0).min(target_tex.width() as f32) as u32;
        let sy = bridge
            .roi
            .y
            .floor()
            .max(0.0)
            .min(target_tex.height() as f32) as u32;
        let ex = (bridge.roi.x + bridge.roi.w)
            .ceil()
            .max(0.0)
            .min(target_tex.width() as f32) as u32;
        let ey = (bridge.roi.y + bridge.roi.h)
            .ceil()
            .max(0.0)
            .min(target_tex.height() as f32) as u32;
        if ex <= sx || ey <= sy {
            return Ok(());
        }
        pass.set_scissor_rect(sx, sy, ex - sx, ey - sy);
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
    crate::infra::gpu_shared::locked_submit(&bridge.queue, [encoder.finish()]);
    Ok(())
}

fn copy_input_to_framebuffer(
    bridge: &Bridge,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<(), u32> {
    if bridge.object.format() == bridge.framebuffer.format() {
        encoder.copy_texture_to_texture(
            bridge.object.as_image_copy(),
            bridge.framebuffer.as_image_copy(),
            bridge.object.size(),
        );
        return Ok(());
    }
    const WGSL: &str = r#"
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->VOut { var p=array<vec2<f32>,3>(vec2<f32>(-1.0,1.0),vec2<f32>(3.0,1.0),vec2<f32>(-1.0,-3.0)); var o:VOut; o.position=vec4<f32>(p[i],0.0,1.0); o.uv=vec2<f32>((p[i].x+1.0)*0.5,(1.0-p[i].y)*0.5); return o; }
@group(0) @binding(0) var src:texture_2d<f32>; @group(0) @binding(1) var smp:sampler;
@fragment fn fs_main(in:VOut)->@location(0) vec4<f32> { return textureSample(src,smp,in.uv); }
"#;
    let module = bridge
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Effect Framebuffer Preserve"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
    let bgl = bridge
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Effect Framebuffer Preserve BGL"),
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
            ],
        });
    let layout = bridge
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Effect Framebuffer Preserve Layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
    let pipeline = bridge
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Effect Framebuffer Preserve"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: bridge.framebuffer.format(),
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
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
    let src_view = bridge.object.create_view(&Default::default());
    let dst_view = bridge.framebuffer.create_view(&Default::default());
    let sampler = bridge
        .device
        .create_sampler(&wgpu::SamplerDescriptor::default());
    let bind = bridge.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Effect Framebuffer Preserve BG"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&src_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Effect Framebuffer Preserve Pass"),
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
    pass.set_pipeline(&pipeline);
    pass.set_bind_group(0, &bind, &[]);
    pass.draw(0..3, 0..1);
    drop(pass);
    Ok(())
}

fn pixel_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Effect Custom Pixel BGL"),
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
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

unsafe extern "C" fn exec_computeshader(
    data: *const u8,
    len: usize,
    targets: *const ResourceRef,
    target_count: u32,
    resources: *const ResourceRef,
    resource_count: u32,
    constants: *const u8,
    constant_len: usize,
    dispatch: ComputeDispatch,
) -> u32 {
    if data.is_null()
        || targets.is_null()
        || target_count == 0
        || (resource_count > 0 && resources.is_null())
        || (constant_len > 0 && constants.is_null())
    {
        return 2;
    }
    let wgsl = unsafe { std::slice::from_raw_parts(data, len) };
    let ts = unsafe { std::slice::from_raw_parts(targets, target_count as usize) };
    let rs = if resource_count == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(resources, resource_count as usize) }
    };
    let cs = if constant_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(constants, constant_len) }
    };
    with_active(|bridge| dispatch_compute(bridge, wgsl, ts, rs, cs, dispatch)).map_or(4, |_| 0)
}

fn dispatch_compute(
    bridge: &mut Bridge,
    wgsl: &[u8],
    targets: &[ResourceRef],
    resources: &[ResourceRef],
    constants: &[u8],
    dispatch: ComputeDispatch,
) -> Result<(), u32> {
    if dispatch.group_count_x == 0 || dispatch.group_count_y == 0 || dispatch.group_count_z == 0 {
        return Err(3);
    }
    let target_textures: Vec<_> = targets
        .iter()
        .map(|r| bridge.resolve(*r, true).ok_or(2u32))
        .collect::<Result<_, _>>()?;
    if targets.iter().any(|t| {
        matches!(
            t.kind,
            ResourceKind::Object
                | ResourceKind::Framebuffer
                | ResourceKind::Image
                | ResourceKind::Random
        )
    }) {
        return Err(3);
    }
    if target_textures
        .iter()
        .any(|t| t.format() != wgpu::TextureFormat::Rgba8Unorm)
    {
        return Err(3);
    }
    if targets.iter().any(|target| {
        resources.iter().any(|resource| {
            target.kind == resource.kind
                && unsafe { ref_name(target.name) } == unsafe { ref_name(resource.name) }
        })
    }) {
        return Err(3);
    }
    let resource_textures: Vec<_> = resources
        .iter()
        .map(|r| bridge.resolve(*r, false).ok_or(2u32))
        .collect::<Result<_, _>>()?;
    let text = std::str::from_utf8(wgsl).map_err(|_| 4u32)?;
    let mut entries = Vec::new();
    for i in 0..targets.len() {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: i as u32,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::Rgba8Unorm,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        });
    }
    for i in 0..resources.len() {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: (targets.len() + i) as u32,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }
    let uniform_binding = (targets.len() + resources.len()) as u32;
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: uniform_binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    });
    let bgl = bridge
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Effect Compute BGL"),
            entries: &entries,
        });
    let layout = bridge
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Effect Compute Layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
    let module = bridge
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Effect Compute WGSL"),
            source: wgpu::ShaderSource::Wgsl(text.into()),
        });
    let pipeline = bridge
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Effect Compute Pipeline"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });
    let mut data = constants.to_vec();
    data.resize(data.len().max(16).div_ceil(16) * 16, 0);
    let uniform = bridge
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Effect Compute Uniform"),
            contents: &data,
            usage: wgpu::BufferUsages::UNIFORM,
        });
    let target_views: Vec<_> = target_textures
        .iter()
        .map(|t| t.create_view(&Default::default()))
        .collect();
    let resource_views: Vec<_> = resource_textures
        .iter()
        .map(|t| t.create_view(&Default::default()))
        .collect();
    let mut bindings: Vec<_> = target_views
        .iter()
        .enumerate()
        .map(|(i, v)| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: wgpu::BindingResource::TextureView(v),
        })
        .collect();
    bindings.extend(
        resource_views
            .iter()
            .enumerate()
            .map(|(i, v)| wgpu::BindGroupEntry {
                binding: (targets.len() + i) as u32,
                resource: wgpu::BindingResource::TextureView(v),
            }),
    );
    bindings.push(wgpu::BindGroupEntry {
        binding: uniform_binding,
        resource: uniform.as_entire_binding(),
    });
    let bind = bridge.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Effect Compute Bind Group"),
        layout: &bgl,
        entries: &bindings,
    });
    let mut encoder = bridge.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Effect Compute Pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(
            dispatch.group_count_x,
            dispatch.group_count_y,
            dispatch.group_count_z,
        );
    }
    crate::infra::gpu_shared::locked_submit(&bridge.queue, [encoder.finish()]);
    Ok(())
}

unsafe extern "C" fn get_resource_size(
    resource: ResourceRef,
    width: *mut u32,
    height: *mut u32,
) -> u32 {
    if width.is_null() || height.is_null() {
        return 2;
    }
    with_active(|bridge| {
        let texture = bridge.resolve(resource, false).ok_or(2u32)?;
        unsafe {
            *width = texture.width();
            *height = texture.height();
        }
        Ok(())
    })
    .map_or(1, |_| 0)
}
unsafe extern "C" fn get_resource_data(
    resource: ResourceRef,
    buffer: *mut u8,
    width: u32,
    height: u32,
    pitch: u32,
) -> u32 {
    if buffer.is_null() || width == 0 || height == 0 {
        return 2;
    }
    with_active(|bridge| read_texture(bridge, resource, buffer, width, height, pitch))
        .map_or(4, |_| 0)
}
fn read_texture(
    bridge: &mut Bridge,
    resource: ResourceRef,
    buffer: *mut u8,
    width: u32,
    height: u32,
    pitch: u32,
) -> Result<(), u32> {
    let texture = bridge.resolve(resource, false).ok_or(2u32)?;
    let bpp = 4u32;
    let row = width.checked_mul(bpp).ok_or(3u32)?;
    if pitch < row || texture.width() < width || texture.height() < height {
        return Err(3);
    }
    let padded =
        row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buf = bridge.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Effect Resource Readback"),
        size: u64::from(padded) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = bridge.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    crate::infra::gpu_shared::locked_submit(&bridge.queue, [enc.finish()]);
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    bridge
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|_| 4u32)?;
    rx.recv().map_err(|_| 4u32)?.map_err(|_| 4u32)?;
    let mapped = slice.get_mapped_range().map_err(|_| 4u32)?;
    for y in 0..height as usize {
        let src_row = mapped.as_ptr().wrapping_add(y * padded as usize);
        let dst_row = unsafe { buffer.add(y * pitch as usize) };
        match texture.format() {
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => unsafe {
                std::ptr::copy_nonoverlapping(src_row, dst_row, row as usize);
            },
            wgpu::TextureFormat::Rgba16Float => {
                for x in 0..width as usize {
                    for c in 0..4 {
                        let off = (x * 4 + c) * 2;
                        let v = half::f16::from_le_bytes([unsafe { *src_row.add(off) }, unsafe {
                            *src_row.add(off + 1)
                        }])
                        .to_f32()
                        .clamp(0.0, 1.0);
                        unsafe {
                            *dst_row.add(x * 4 + c) = (v * 255.0).round() as u8;
                        }
                    }
                }
            }
            _ => return Err(3),
        }
    }
    drop(mapped);
    buf.unmap();
    Ok(())
}
unsafe extern "C" fn set_resource_data(
    resource: ResourceRef,
    buffer: *const u8,
    width: u32,
    height: u32,
    pitch: u32,
) -> u32 {
    if buffer.is_null() || width == 0 || height == 0 {
        return 2;
    }
    with_active(|bridge| {
        let texture = bridge.resolve(resource, true).ok_or(2u32)?;
        let row = width.checked_mul(4).ok_or(3u32)?;
        if pitch < row || texture.width() < width || texture.height() < height {
            return Err(3);
        }
        if !matches!(
            texture.format(),
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Rgba16Float
        ) {
            return Err(3);
        }
        let length = (pitch as usize).checked_mul(height as usize).ok_or(3u32)?;
        let bytes = unsafe { std::slice::from_raw_parts(buffer, length) };
        let upload = if texture.format() == wgpu::TextureFormat::Rgba16Float {
            let mut converted = Vec::with_capacity(width as usize * height as usize * 8);
            for y in 0..height as usize {
                for px in
                    bytes[y * pitch as usize..y * pitch as usize + row as usize].chunks_exact(4)
                {
                    for channel in px {
                        converted.extend_from_slice(
                            &half::f16::from_f32(*channel as f32 / 255.0).to_le_bytes(),
                        );
                    }
                }
            }
            converted
        } else {
            bytes.to_vec()
        };
        let upload_pitch = if texture.format() == wgpu::TextureFormat::Rgba16Float {
            width * 8
        } else {
            pitch
        };
        bridge.queue.write_texture(
            texture.as_image_copy(),
            &upload,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(upload_pitch),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    })
    .map_or(4, |_| 0)
}
unsafe extern "C" fn release_resource(resource: ResourceRef) -> u32 {
    let name = unsafe { ref_name(resource.name) }.unwrap_or_default();
    with_active(|bridge| match resource.kind {
        ResourceKind::Named => {
            bridge.named.remove(&name);
            Ok(())
        }
        ResourceKind::TempBuffer => {
            bridge.temps.remove(&name);
            Ok(())
        }
        ResourceKind::Cache => {
            CACHES
                .get_or_init(Default::default)
                .lock()
                .map_err(|_| 1u32)?
                .remove(&(
                    bridge.device.as_ref() as *const _ as usize,
                    bridge.namespace,
                    name,
                ));
            Ok(())
        }
        _ => Err(3),
    })
    .map_or(1, |_| 0)
}

pub(super) fn run_custom<R>(bridge: Bridge, f: impl FnOnce(&EffectRenderContext) -> R) -> R {
    with_context(bridge, f)
}

pub(super) fn run_compute(
    bridge: Bridge,
    shader: &[u8],
    constants: &[u8],
    dispatch: ComputeDispatch,
) -> Result<(), u32> {
    with_context(bridge, |ctx| unsafe {
        let output = ResourceRef::named(StrRef::from_str("compute-output"));
        let input = ResourceRef::object();
        let status = (ctx.exec_computeshader)(shader.as_ptr(), shader.len(), &output, 1,
            &input, 1, constants.as_ptr(), constants.len(), dispatch);
        if status != 0 { return Err(status); }
        const BLIT: &[u8] = br"
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> VOut { var p = array<vec2<f32>,3>(vec2(-1.0,1.0),vec2(3.0,1.0),vec2(-1.0,-3.0)); var o: VOut; o.position=vec4(p[i],0.0,1.0); o.uv=vec2((p[i].x+1.0)*0.5,(1.0-p[i].y)*0.5); return o; }
@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var image_sampler: sampler; @group(0) @binding(2) var<uniform> unused: vec4<f32>; @group(0) @binding(3) var map: texture_2d<f32>; @group(0) @binding(4) var map_sampler: sampler;
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> { return textureSample(image,image_sampler,in.uv); }
";
        let target = ResourceRef::framebuffer();
        let source = ResourceRef::named(StrRef::from_str("compute-output"));
        let zero = [0u8;16];
        Ok((ctx.exec_pixelshader)(BLIT.as_ptr(), BLIT.len(), target, &source, 1, zero.as_ptr(), zero.len()))
    }).and_then(|code| if code == 0 { Ok(()) } else { Err(code) })
}
