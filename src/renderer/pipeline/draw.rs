use super::shaders::EffectObjectDrawKind;
use super::*;
use crate::ecs::components::BlendMode;

impl RenderEngine {
    pub(super) fn apply_effect_chain(
        &mut self,
        world: &crate::ecs::EcsWorld,
        objects: &[ActiveObject],
        captured: &CapturedObjects,
        depth: u32,
        src: &wgpu::Texture,
        dst: &wgpu::Texture,
        chain: &[(String, HashMap<String, Value>)],
        timeline_frame: i32,
        fps: u32,
        initial_roi: neoutl_shared_abi::Roi,
    ) {
        let extent = wgpu::Extent3d {
            width: self.render_width,
            height: self.render_height,
            depth_or_array_layers: 1,
        };

        if chain.is_empty() {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Effect Passthrough Copy Encoder"),
                });
            encoder.copy_texture_to_texture(src.as_image_copy(), dst.as_image_copy(), extent);
            crate::infra::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
            return;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Effect Copy Encoder"),
            });
        encoder.copy_texture_to_texture(
            src.as_image_copy(),
            self.effect_ping.as_image_copy(),
            extent,
        );
        crate::infra::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);

        let mut src_is_ping = true;
        let mut roi = initial_roi;
        for (effect_id, params) in chain {
            let Some(source) = effects::loader::by_id(effect_id) else {
                continue;
            };
            if matches!(source.kind(), neoutl_effect_api::EffectKind::Audio) {
                continue;
            }
            let schema = source.param_schema();
            let values: Vec<f32> = schema
                .iter()
                .map(|s| {
                    params
                        .get(s.key.as_str())
                        .map_or(s.default_float, |v| match v {
                            Value::Number(n) => *n,
                            Value::Bool(b) => {
                                if *b {
                                    1.0
                                } else {
                                    0.0
                                }
                            }
                            Value::Enum(idx) => *idx as f32,
                            Value::Text(_) | Value::FilePath(_) | Value::TrackRef(_) => {
                                s.default_float
                            }
                        })
                })
                .collect();
            if let effects::loader::EffectSource::Native(plugin) = source.as_ref()
                && let Some(needs_frame) = plugin.vtable.is_need_render_frame
                && unsafe {
                    needs_frame(
                        values.as_ptr(),
                        values.len() as u32,
                        i64::from(timeline_frame) * 1_000_000 / i64::from(fps.max(1)),
                    )
                } == 0
            {
                continue;
            }

            if let effects::loader::EffectSource::Native(plugin) = source.as_ref()
                && let Some(calc_roi) = plugin.vtable.calc_roi
            {
                let next = unsafe {
                    calc_roi(
                        roi,
                        values.as_ptr(),
                        values.len() as u32,
                        i64::from(timeline_frame) * 1_000_000 / i64::from(fps.max(1)),
                        self.render_width as f32,
                        self.render_height as f32,
                    )
                };
                if [next.x, next.y, next.w, next.h]
                    .iter()
                    .all(|v| v.is_finite())
                    && next.w > 0.0
                    && next.h > 0.0
                {
                    roi = next;
                }
            }
            let uniform_size = (source.uniform_size() as usize).max(16);
            let mut bytes = vec![0u8; uniform_size];
            source.pack_uniform(&values, &mut bytes);
            self.queue
                .write_buffer(&self.effect_uniform_buffer, 0, &bytes);

            let requires_tex_idx = source.requires_texture_param_index();
            let resolved_scene_tex: Option<wgpu::Texture> = if let Some(idx) = requires_tex_idx {
                let scene_ref = schema.get(idx as usize).and_then(|s| {
                    params.get(s.key.as_str()).and_then(|v| match v {
                        Value::TrackRef(id) => Some(*id),
                        _ => None,
                    })
                });
                if let Some(scene_id) = scene_ref {
                    self.render_composed_texture(
                        world,
                        objects,
                        captured,
                        self.render_width,
                        self.render_height,
                        ComposeCacheKey::EffectMapScene(scene_id),
                        depth + 1,
                        None,
                        timeline_frame,
                    )
                } else {
                    None
                }
            } else {
                None
            };
            let map_view: wgpu::TextureView = if let Some(t) = &resolved_scene_tex {
                t.create_view(&wgpu::TextureViewDescriptor::default())
            } else if let Some(idx) = requires_tex_idx {
                let path = schema.get(idx as usize).and_then(|s| {
                    params.get(s.key.as_str()).and_then(|v| match v {
                        Value::FilePath(p) => Some(p.clone()),
                        _ => None,
                    })
                });
                match path.and_then(|p| {
                    self.map_texture_cache
                        .get(std::path::Path::new(&p))
                        .cloned()
                }) {
                    Some(t) => t.create_view(&wgpu::TextureViewDescriptor::default()),
                    None => self.dummy_map_texture_view.clone(),
                }
            } else {
                self.dummy_map_texture_view.clone()
            };

            let (src_tex, dst_tex) = if src_is_ping {
                (&self.effect_ping, &self.effect_pong)
            } else {
                (&self.effect_pong, &self.effect_ping)
            };
            let native_vtable = match source.as_ref() {
                effects::loader::EffectSource::Native(plugin) => Some(plugin.vtable),
                effects::loader::EffectSource::Lua(_) => None,
            };
            let namespace = native_vtable.map_or_else(
                || effect_id.as_ptr() as usize,
                |v| unsafe { (v.meta)() as usize },
            );
            let bridge = || {
                super::effect_gpu::Bridge::new(
                    self.device.clone(),
                    self.queue.clone(),
                    src_tex,
                    dst_tex,
                    self.map_texture_cache.clone(),
                    namespace,
                    self.render_width,
                    self.render_height,
                    roi,
                )
            };
            if let Some(render) = native_vtable.and_then(|v| v.custom_render) {
                let status = super::effect_gpu::run_custom(bridge(), |ctx| unsafe {
                    render(ctx as *const _, values.as_ptr(), values.len() as u32)
                });
                if status != 0 {
                    eprintln!(
                        "[NeoUtl] effect custom_render failed: id={effect_id}, code={status}"
                    );
                    continue;
                }
                src_is_ping = !src_is_ping;
                continue;
            }
            if let Some(vtable) = native_vtable
                && let Some(get_shader) = vtable.compute_wgsl
            {
                let shader = unsafe { get_shader() };
                let shader = unsafe { shader.as_slice() };
                if !shader.is_empty() {
                    let dispatch = vtable.compute_dispatch.map_or(
                        neoutl_shared_abi::ComputeDispatch {
                            group_count_x: self.render_width.div_ceil(8),
                            group_count_y: self.render_height.div_ceil(8),
                            group_count_z: 1,
                        },
                        |f| unsafe {
                            f(
                                values.as_ptr(),
                                values.len() as u32,
                                self.render_width,
                                self.render_height,
                            )
                        },
                    );
                    match super::effect_gpu::run_compute(bridge(), shader, &bytes, dispatch) {
                        Ok(()) => src_is_ping = !src_is_ping,
                        Err(code) => eprintln!(
                            "[NeoUtl] effect compute_wgsl failed: id={effect_id}, code={code}"
                        ),
                    }
                    continue;
                }
            }
            let Some((pipeline, vertex_pipeline)) = self.effect_pipelines.get(effect_id).cloned()
            else {
                continue;
            };
            let pipeline = vertex_pipeline.unwrap_or(pipeline);
            let src_view = src_tex.create_view(&wgpu::TextureViewDescriptor::default());
            let dst_view = dst_tex.create_view(&wgpu::TextureViewDescriptor::default());

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Effect Pass BG"),
                layout: &self.effect_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.effect_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.effect_uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&map_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&self.effect_sampler),
                    },
                ],
            });

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Effect Pass Encoder"),
                });
            encoder.copy_texture_to_texture(
                src_tex.as_image_copy(),
                dst_tex.as_image_copy(),
                extent,
            );
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Effect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &dst_view,
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
                let sx = roi.x.floor().max(0.0).min(self.render_width as f32) as u32;
                let sy = roi.y.floor().max(0.0).min(self.render_height as f32) as u32;
                let ex = (roi.x + roi.w)
                    .ceil()
                    .max(0.0)
                    .min(self.render_width as f32) as u32;
                let ey = (roi.y + roi.h)
                    .ceil()
                    .max(0.0)
                    .min(self.render_height as f32) as u32;
                if ex <= sx || ey <= sy {
                    continue;
                }
                rpass.set_scissor_rect(sx, sy, ex - sx, ey - sy);
                rpass.set_pipeline(&pipeline);
                rpass.set_bind_group(0, &bind_group, &[]);
                rpass.draw(0..3, 0..1);
            }
            crate::infra::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
            src_is_ping = !src_is_ping;
        }

        let final_src = if src_is_ping {
            &self.effect_ping
        } else {
            &self.effect_pong
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Effect Finalize Encoder"),
            });
        encoder.copy_texture_to_texture(final_src.as_image_copy(), dst.as_image_copy(), extent);
        crate::infra::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
    }

    pub(super) fn draw_standard_pass(
        &self,
        rpass: &mut wgpu::RenderPass,
        obj: &ActiveObject,
        offset: u32,
    ) {
        if let Some((pipelines, vertex_count)) = self.pipelines.get(&obj.kind_id) {
            rpass.set_pipeline(&pipelines[obj.blend_mode.pipeline_index() as usize]);
            rpass.set_bind_group(0, &self.bind_group, &[offset]);
            rpass.draw(0..*vertex_count, 0..1);
        }
    }

    pub(super) fn draw_object_hook(
        &self,
        rpass: &mut wgpu::RenderPass,
        obj: &ActiveObject,
        bind_group: &wgpu::BindGroup,
        depth_enabled: bool,
        vertex_count_override: Option<u32>,
    ) {
        let Some(plugin) = by_kind_id(obj.kind_id) else {
            return;
        };
        let vertex_count =
            vertex_count_override.unwrap_or_else(|| unsafe { (plugin.vtable.vertex_count)() });
        let context = neoutl_object_api::RenderContext {
            version: 1,
            render_pass_ptr: (rpass as *mut wgpu::RenderPass<'_>).cast(),
            bind_group_ptr: (bind_group as *const wgpu::BindGroup).cast(),
            vertex_count,
            mvp_matrix: obj.mvp,
            opacity: obj.opacity,
            depth_enabled,
            ref_layer_texture_ptr: std::ptr::null(),
            ref_layer_texture_count: 0,
        };
        unsafe { (plugin.vtable.render)(&context) };
    }

    pub(super) fn draw_media_pass_with_hook(
        &self,
        rpass: &mut wgpu::RenderPass,
        texture: &wgpu::Texture,
        offset: u32,
        blend_mode: BlendMode,
        obj: &ActiveObject,
        depth_enabled: bool,
    ) {
        let variant = blend_mode.pipeline_index() as usize;
        let is_planar = matches!(texture.format(), wgpu::TextureFormat::NV12);
        if is_planar {
            let plane_y = texture.create_view(&wgpu::TextureViewDescriptor {
                aspect: wgpu::TextureAspect::Plane0,
                ..Default::default()
            });
            let plane_uv = texture.create_view(&wgpu::TextureViewDescriptor {
                aspect: wgpu::TextureAspect::Plane1,
                ..Default::default()
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Video Object BG"),
                layout: &self.video_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.media_uniform_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(MEDIA_UNIFORM_SIZE),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&plane_y),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&plane_uv),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.media_sampler),
                    },
                ],
            });
            rpass.set_pipeline(&self.video_pipeline[variant]);
            rpass.set_bind_group(0, &bind_group, &[offset]);
            rpass.draw(0..6, 0..1);
            self.draw_object_hook(rpass, obj, &bind_group, depth_enabled, Some(6));
        } else {
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Media Object BG"),
                layout: &self.media_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.media_uniform_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(MEDIA_UNIFORM_SIZE),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.media_sampler),
                    },
                ],
            });
            rpass.set_pipeline(&self.media_pipeline[variant]);
            rpass.set_bind_group(0, &bind_group, &[offset]);
            rpass.draw(0..6, 0..1);
            self.draw_object_hook(rpass, obj, &bind_group, depth_enabled, Some(6));
        }
    }

    pub(super) fn draw_text_pass_with_hook(
        &self,
        rpass: &mut wgpu::RenderPass,
        clip_instance: u64,
        offset: u32,
        blend_mode: BlendMode,
        obj: &ActiveObject,
        depth_enabled: bool,
    ) {
        let Some(target) = self.text_targets.get(&clip_instance) else {
            return;
        };
        let view = target
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Object BG"),
            layout: &self.media_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.media_uniform_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(MEDIA_UNIFORM_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.media_sampler),
                },
            ],
        });
        rpass.set_pipeline(&self.media_pipeline[blend_mode.pipeline_index() as usize]);
        rpass.set_bind_group(0, &bind_group, &[offset]);
        rpass.draw(0..6, 0..1);
        self.draw_object_hook(rpass, obj, &bind_group, depth_enabled, Some(6));
    }

    pub(super) fn render_effect_object_offscreen(
        &self,
        pool_tex: &wgpu::Texture,
        draw_kind: EffectObjectDrawKind,
        obj: &ActiveObject,
    ) {
        let view = pool_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = self
            .effect_object_depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Effect Object Encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Effect Object Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            match draw_kind {
                EffectObjectDrawKind::Standard { obj, offset } => {
                    self.draw_standard_pass(&mut rpass, obj, offset);
                    self.draw_object_hook(&mut rpass, obj, &self.bind_group, false, None);
                }
                EffectObjectDrawKind::Media {
                    texture,
                    offset,
                    blend_mode,
                } => {
                    self.draw_media_pass_with_hook(
                        &mut rpass, texture, offset, blend_mode, obj, false,
                    );
                }
                EffectObjectDrawKind::Text {
                    clip_instance,
                    offset,
                    blend_mode,
                } => {
                    self.draw_text_pass_with_hook(
                        &mut rpass,
                        clip_instance,
                        offset,
                        blend_mode,
                        obj,
                        false,
                    );
                }
            }
        }
        crate::infra::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
    }

    pub(super) fn composite_effect_object(
        &self,
        pool_tex: &wgpu::Texture,
        clear_color: Option<wgpu::Color>,
        blend_mode: BlendMode,
    ) {
        let src_view = pool_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let src_depth_view = self
            .effect_object_depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let dst_depth_view = self
            .depth_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite BG"),
            layout: &self.composite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.effect_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&src_depth_view),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Composite Encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: match clear_color {
                            Some(c) => wgpu::LoadOp::Clear(c),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &dst_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: match clear_color {
                            Some(_) => wgpu::LoadOp::Clear(1.0),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rpass.set_pipeline(&self.composite_pipeline[blend_mode.pipeline_index() as usize]);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }
        crate::infra::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
    }

    pub(super) fn composite_clipped_object(
        &self,
        content_pool_tex: &wgpu::Texture,
        mold_pool_tex: &wgpu::Texture,
        mode: crate::ecs::components::ClipMode,
        chroma_hue: f32,
        chroma_tolerance: f32,
        blend_edge: bool,
        clear_color: Option<wgpu::Color>,
    ) {
        let content_view = content_pool_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let mold_view = mold_pool_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let content_depth_view = self
            .effect_object_depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let dst_depth_view = self
            .depth_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let uniform_data: [u32; 4] = [
            mode as u8 as u32,
            chroma_hue.to_bits(),
            chroma_tolerance.to_bits(),
            u32::from(blend_edge),
        ];
        self.queue.write_buffer(
            &self.clip_uniform_buffer,
            0,
            bytemuck::cast_slice(&uniform_data),
        );

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Clip Composite BG"),
            layout: &self.clip_composite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&content_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&mold_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.effect_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&content_depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.clip_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Clip Composite Encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clip Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: match clear_color {
                            Some(c) => wgpu::LoadOp::Clear(c),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &dst_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: match clear_color {
                            Some(_) => wgpu::LoadOp::Clear(1.0),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rpass.set_pipeline(&self.clip_composite_pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }
        crate::infra::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
    }
}
