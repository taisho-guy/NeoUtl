use super::shaders::EffectObjectDrawKind;
use super::*;

impl RenderEngine {
    pub fn render(
        &mut self,
        world: &crate::ecs::EcsWorld,
        active_objects: &[ActiveObject],
        captured: &CapturedObjects,
        project: &ProjectResource,
    ) {
        self.scene_texture_cache.clear();
        self.drain_hot_reload_events();
        if let Some(sys) = &self.lua_system
            && let Err(err) = sys.run_pre_render_hooks()
        {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] system.on_pre_render フック実行失敗: %{arg0}",
                    arg0 = format!("{err}")
                )
            );
        }
        self.render_at(world, active_objects, captured, project, 0, None);
        self.run_lua_reduce_hooks();
    }

    pub fn read_frame_rgba8(&self) -> Vec<u8> {
        let width = self.render_width;
        let height = self.render_height;
        let unpadded_bytes_per_row = width * 8;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = (padded_bytes_per_row * height) as wgpu::BufferAddress;

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Export Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);

        let slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).expect(&t!("map_async結果送信失敗"));
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect(&t!("device poll失敗"));
        rx.recv()
            .expect(&t!("map_async結果受信失敗"))
            .expect(&t!("バッファmap失敗"));

        let padded = slice.get_mapped_range().expect(&t!("get_mapped_range失敗"));
        let mut rgba8 = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height as usize {
            let start = row * padded_bytes_per_row as usize;
            for pixel in padded[start..start + unpadded_bytes_per_row as usize].chunks_exact(8) {
                for channel in pixel.chunks_exact(2) {
                    let v = half::f16::from_le_bytes([channel[0], channel[1]]).to_f32();
                    rgba8.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
                }
            }
        }
        drop(padded);
        output_buffer.unmap();
        rgba8
    }

    pub(super) fn render_at(
        &mut self,
        world: &crate::ecs::EcsWorld,
        active_objects: &[ActiveObject],
        captured: &CapturedObjects,
        _project: &ProjectResource,
        depth: u32,
        clear_override: Option<wgpu::Color>,
    ) {
        if is_device_lost() {
            return;
        }
        let mut media_frames: Vec<Option<wgpu::Texture>> = Vec::with_capacity(active_objects.len());
        let mut media_color_metas: Vec<neoutl_media_api::ColorMeta> =
            Vec::with_capacity(active_objects.len());
        {
            let cache = neoutl_media_runtime::cache::global();
            for obj in active_objects {
                let stable_id = stable_id_of(obj.kind_id);
                let is_visual = matches!(stable_id, Some(VIDEO_STABLE_ID | IMAGE_STABLE_ID));
                let tex = if is_visual {
                    if let Some(src) = &obj.media_source {
                        let color_meta =
                            cache.color_meta_at(&src.path, obj.clip_instance, obj.source_frame);
                        media_color_metas.push(color_meta);
                        match cache.frame_at(
                            &src.path,
                            obj.clip_instance,
                            obj.source_frame,
                            &self.device,
                            &self.queue,
                        ) {
                            Ok(texture) => Some(texture),
                            Err(err) => {
                                eprintln!(
                                    "{}",
                                    t!(
                                        "[NeoUtl] フレーム取得失敗 kind_id=%{arg0} clip_instance=%{arg4} path=%{arg1} frame=%{arg2}: %{arg3}",
                                        arg0 = format!("{}", obj.kind_id),
                                        arg1 = format!("{}", src.path.display()),
                                        arg2 = format!("{}", obj.source_frame),
                                        arg3 = format!("{err}"),
                                        arg4 = format!("{}", obj.clip_instance)
                                    )
                                );
                                None
                            }
                        }
                    } else {
                        media_color_metas.push(neoutl_media_api::ColorMeta::default());
                        None
                    }
                } else {
                    media_color_metas.push(neoutl_media_api::ColorMeta::default());
                    match obj.compose_source {
                        Some(crate::ecs::systems::ComposeSource::NestedScene {
                            target_scene,
                            local_frame,
                        }) => world.get_scene(target_scene).and_then(|scene| {
                            let (nested, nested_captured) =
                                crate::ecs::systems::get_active_objects_system_at(
                                    world,
                                    target_scene,
                                    local_frame,
                                );
                            self.render_composed_texture(
                                world,
                                &nested,
                                &nested_captured,
                                scene.width,
                                scene.height,
                                ComposeCacheKey::Scene(target_scene),
                                depth + 1,
                                None,
                            )
                        }),
                        Some(crate::ecs::systems::ComposeSource::FrameBuffer {
                            controller,
                            kind: crate::ecs::systems::FrameBufferKind::Group,
                        }) => {
                            let empty = Vec::new();
                            let objects = captured.get(&controller).unwrap_or(&empty);
                            self.render_composed_texture(
                                world,
                                objects,
                                captured,
                                self.render_width,
                                self.render_height,
                                ComposeCacheKey::FrameBuffer(controller),
                                depth + 1,
                                None,
                            )
                        }
                        None => None,
                    }
                };
                media_frames.push(tex);
            }
        }
        let mut mold_frames: Vec<Option<wgpu::Texture>> = Vec::with_capacity(active_objects.len());
        for obj in active_objects {
            let tex = match obj.clip_target {
                Some(info) => {
                    let empty = Vec::new();
                    let objects = captured.get(&info.controller).unwrap_or(&empty);
                    self.render_composed_texture(
                        world,
                        objects,
                        captured,
                        self.render_width,
                        self.render_height,
                        ComposeCacheKey::FrameBuffer(info.controller),
                        depth + 1,
                        None,
                    )
                }
                None => None,
            };
            mold_frames.push(tex);
        }
        let mut media_offsets: Vec<Option<u32>> = Vec::with_capacity(active_objects.len());
        let mut media_next_index = 0u64;
        for ((obj, tex), color_meta) in active_objects
            .iter()
            .zip(media_frames.iter())
            .zip(media_color_metas.iter())
        {
            if tex.is_some() && media_next_index < MAX_OBJECTS {
                let offset = self.write_media_uniform(media_next_index, obj, *color_meta);
                media_offsets.push(Some(offset));
                media_next_index += 1;
            } else {
                media_offsets.push(None);
            }
        }

        let mut offsets: Vec<Option<u32>> = Vec::with_capacity(active_objects.len());
        let mut next_index = 0u64;
        for obj in active_objects {
            if self.pipelines.contains_key(&obj.kind_id) && next_index < MAX_OBJECTS {
                let offset = self.write_standard_uniform(next_index, obj);
                offsets.push(Some(offset));
                next_index += 1;
            } else {
                offsets.push(None);
            }
        }

        let mut effect_pool_index: Vec<Option<usize>> = Vec::with_capacity(active_objects.len());
        {
            let mut next_pool = 0usize;
            for obj in active_objects {
                if (!obj.effects.is_empty() || obj.clip_target.is_some())
                    && next_pool < config::MAX_EFFECT_OBJECTS
                {
                    effect_pool_index.push(Some(next_pool));
                    next_pool += 1;
                } else {
                    effect_pool_index.push(None);
                }
            }
        }

        let mut text_draws: Vec<(u64, u32, usize)> = Vec::new();
        {
            let mut seen: HashSet<u64> = HashSet::with_capacity(active_objects.len());
            for (obj_index, obj) in active_objects.iter().enumerate() {
                let Some(plugin) = by_kind_id(obj.kind_id) else {
                    continue;
                };
                let meta = unsafe { &*((plugin.vtable.meta)()) };
                if meta.stable_id != neoutl_object_api::TEXT_STABLE_ID {
                    continue;
                }
                let Some(tc) = obj.text_content.as_ref() else {
                    continue;
                };
                if media_next_index >= MAX_OBJECTS {
                    continue;
                }
                let Some(font) =
                    self.resolve_font_stack(&tc.font_family_stack, &tc.text, tc.bold, tc.italic)
                else {
                    continue;
                };

                let text_layout = neoutl_media_runtime::text::layout(
                    &font,
                    &tc.text,
                    tc.font_size,
                    tc.line_height,
                );
                let (tex_w, tex_h) = (text_layout.width, text_layout.height);
                seen.insert(obj.clip_instance);

                let needs_rebuild = match self.text_targets.get(&obj.clip_instance) {
                    Some(t) => t.width != tex_w || t.height != tex_h,
                    None => true,
                };
                if needs_rebuild {
                    self.text_targets.insert(
                        obj.clip_instance,
                        build_text_target(&self.device, &font, tex_w, tex_h),
                    );
                }
                let target = self
                    .text_targets
                    .get_mut(&obj.clip_instance)
                    .expect(&t!("直前にinsert済み"));

                let h_align = match tc.align {
                    crate::ecs::components::TextAlign::Left => {
                        neoutl_media_runtime::text::HAlign::Left
                    }
                    crate::ecs::components::TextAlign::Center => {
                        neoutl_media_runtime::text::HAlign::Center
                    }
                    crate::ecs::components::TextAlign::Right => {
                        neoutl_media_runtime::text::HAlign::Right
                    }
                };
                let sections = neoutl_media_runtime::text::build_sections(
                    tc.color,
                    tc.font_size,
                    h_align,
                    &text_layout,
                    target.width,
                    target.height,
                );
                let section_refs: Vec<&_> = sections.iter().collect();
                {
                    let view = target
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let _ =
                        target
                            .brush
                            .queue(self.device.as_ref(), self.queue.as_ref(), section_refs);
                    let mut encoder =
                        self.device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("Text Glyph Encoder"),
                            });
                    {
                        let mut glyph_pass =
                            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Text Glyph Pass"),
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
                        target.brush.draw(&mut glyph_pass);
                    }
                    crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
                }

                if tc.outline_width > 0.0 {
                    if let Some(t) = self.text_targets.get(&obj.clip_instance) {
                        self.apply_text_outline(t, tc);
                    }
                }

                let ratio_w = tex_w as f32 / UNIT_SIZE_PX;
                let ratio_h = tex_h as f32 / UNIT_SIZE_PX;
                let mut mvp = obj.mvp;
                for i in 0..4 {
                    mvp[i] *= ratio_w;
                    mvp[4 + i] *= ratio_h;
                }

                let offset = self.write_media_uniform_raw(media_next_index, &mvp, obj.opacity);
                media_next_index += 1;
                text_draws.push((obj.clip_instance, offset, obj_index));
            }
            self.text_targets.retain(|k, _| seen.contains(k));
        }

        let text_draw_by_index: HashMap<usize, (u64, u32)> = text_draws
            .iter()
            .map(|(clip_instance, offset, obj_index)| (*obj_index, (*clip_instance, *offset)))
            .collect();

        let clear_color = clear_override.unwrap_or(if depth == 0 {
            wgpu::Color {
                r: 0.05,
                g: 0.05,
                b: 0.07,
                a: 1.0,
            }
        } else {
            wgpu::Color::TRANSPARENT
        });

        let object_count = active_objects.len();
        let mut drawn_any = false;
        let mut idx = 0usize;
        while idx < object_count {
            if let Some(pool_idx) = effect_pool_index[idx] {
                let obj = &active_objects[idx];
                let draw_kind = if let Some(offset) = offsets[idx] {
                    EffectObjectDrawKind::Standard { obj, offset }
                } else if let (Some(texture), Some(offset)) =
                    (&media_frames[idx], media_offsets[idx])
                {
                    EffectObjectDrawKind::Media { texture, offset }
                } else if let Some((clip_instance, offset)) = text_draw_by_index.get(&idx) {
                    EffectObjectDrawKind::Text {
                        clip_instance: *clip_instance,
                        offset: *offset,
                    }
                } else {
                    idx += 1;
                    continue;
                };

                let pool_tex = self.ensure_effect_object_target(pool_idx).clone();
                self.render_effect_object_offscreen(&pool_tex, draw_kind);
                if !obj.effects.is_empty() {
                    self.apply_effect_chain(
                        world,
                        active_objects,
                        captured,
                        depth,
                        &pool_tex,
                        &pool_tex,
                        &obj.effects,
                    );
                }
                match obj.clip_target {
                    Some(info) => {
                        let mold_tex = mold_frames[idx].as_ref().unwrap_or(&pool_tex);
                        self.composite_clipped_object(
                            &pool_tex,
                            mold_tex,
                            info.mode,
                            info.chroma_hue,
                            info.chroma_tolerance,
                            info.blend_edge,
                            if drawn_any { None } else { Some(clear_color) },
                        );
                    }
                    None => {
                        self.composite_effect_object(
                            &pool_tex,
                            if drawn_any { None } else { Some(clear_color) },
                        );
                    }
                }
                drawn_any = true;
                idx += 1;
                continue;
            }

            let start = idx;
            while idx < object_count && effect_pool_index[idx].is_none() {
                idx += 1;
            }

            let color_load = if drawn_any {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(clear_color)
            };
            let depth_load = if drawn_any {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(1.0)
            };

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Segment Encoder"),
                });
            {
                let view = self
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let depth_view = self
                    .depth_texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass Segment"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: color_load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: depth_load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                for i in start..idx {
                    if let Some(offset) = offsets[i] {
                        self.draw_standard_pass(&mut rpass, &active_objects[i], offset);
                    }
                    if let (Some(texture), Some(offset)) = (&media_frames[i], media_offsets[i]) {
                        self.draw_media_pass(&mut rpass, texture, offset);
                    }
                    if let Some((clip_instance, offset)) = text_draw_by_index.get(&i) {
                        self.draw_text_pass(&mut rpass, *clip_instance, *offset);
                    }
                }
            }
            crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
            drawn_any = true;
        }

        if !drawn_any {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Clear Encoder"),
                });
            {
                let view = self
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let depth_view = self
                    .depth_texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear_color),
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
            }
            crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
        }
    }
}
