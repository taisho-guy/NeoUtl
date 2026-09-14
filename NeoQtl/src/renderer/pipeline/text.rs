use super::*;

pub(super) fn load_font_bytes(family: &str, bold: bool, italic: bool) -> Option<Vec<u8>> {
    use font_kit::family_name::FamilyName;
    use font_kit::properties::{Properties, Style, Weight};
    use font_kit::source::SystemSource;
    let requested = if family.is_empty() {
        FamilyName::SansSerif
    } else {
        FamilyName::Title(family.to_owned())
    };
    let mut properties = Properties::new();
    properties.weight(if bold { Weight::BOLD } else { Weight::NORMAL });
    properties.style(if italic { Style::Italic } else { Style::Normal });
    let source = SystemSource::new();
    let handle = source
        .select_best_match(&[requested, FamilyName::SansSerif], &properties)
        .ok()?;
    let font = handle.load().ok()?;
    let data = font.copy_font_data()?;
    eprintln!(
        "{}",
        t!(
            "[NeoUtl] フォント解決: %{arg0}",
            arg0 = format!("{family} bold={bold} italic={italic}")
        )
    );
    Some(data.to_vec())
}

impl RenderEngine {
    fn resolve_font(&mut self, family: &str, bold: bool, italic: bool) -> Option<FontArc> {
        let key = (family.to_owned(), bold, italic);
        if let Some(font) = self.fonts.get(&key) {
            return Some(font.clone());
        }
        let bytes = load_font_bytes(family, bold, italic)?;
        let font = FontArc::try_from_vec(bytes).ok()?;
        self.fonts.insert(key, font.clone());
        Some(font)
    }

    pub(super) fn resolve_font_stack(
        &mut self,
        stack: &[String],
        text: &str,
        bold: bool,
        italic: bool,
    ) -> Option<FontArc> {
        let mut fallback: Option<FontArc> = None;
        for family in stack {
            let Some(font) = self.resolve_font(family, bold, italic) else {
                continue;
            };
            let covers_all = text.chars().all(|c| c == '\n' || font.glyph_id(c).0 != 0);
            if covers_all {
                return Some(font);
            }
            fallback = Some(font);
        }
        fallback.or_else(|| self.resolve_font("", bold, italic))
    }

    pub(super) fn apply_text_outline(
        &self,
        target: &TextRenderTarget,
        tc: &crate::ecs::components::TextContent,
    ) {
        let Some(source) = effects::loader::by_id("text_outline") else {
            return;
        };
        let Some(pipeline) = self.effect_pipelines.get("text_outline") else {
            return;
        };
        let values = [
            tc.outline_width,
            tc.outline_color[0],
            tc.outline_color[1],
            tc.outline_color[2],
            tc.outline_color[3],
        ];
        let uniform_size = (source.uniform_size() as usize).max(16);
        let mut bytes = vec![0u8; uniform_size];
        source.pack_uniform(&values, &mut bytes);
        self.queue
            .write_buffer(&self.effect_uniform_buffer, 0, &bytes);

        let src_view = target
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = target
            .outline_scratch
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Outline BG"),
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
                    resource: wgpu::BindingResource::TextureView(&self.dummy_map_texture_view),
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
                label: Some("Text Outline Encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Text Outline Pass"),
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
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_texture(
            target.outline_scratch.as_image_copy(),
            target.texture.as_image_copy(),
            wgpu::Extent3d {
                width: target.width,
                height: target.height,
                depth_or_array_layers: 1,
            },
        );
        crate::gpu_shared::locked_submit(&self.queue, [encoder.finish()]);
    }
}
