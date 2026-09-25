use super::TimelineWindow;
use crate::objects::registry;
use crate::ui::types::TimelineObject;
use egui::Color32;

impl TimelineWindow {
    pub(super) fn as_egui(
        &mut self,
        ctx: &egui::Context,
        data: &crate::ecs::TimelineData,
        fps: f64,
    ) -> TimelineObject {
        let registry_snapshot = registry();
        let plugin = usize::try_from(data.kind)
            .ok()
            .and_then(|kind| registry_snapshot.get(kind));
        let is_audio = plugin.is_some_and(|p| p.name == "Audio");
        let waveform = data
            .media_path
            .as_deref()
            .filter(|_| is_audio)
            .and_then(|path| self.waveform_texture(ctx, path));
        let waveform_duration_frames = data
            .media_path
            .as_deref()
            .filter(|_| is_audio)
            .and_then(|path| {
                neoutl_media_runtime::cache::global()
                    .load_audio(path)
                    .ok()
                    .map(|audio| {
                        let frame_count = audio
                            .frame_count()
                            .to_string()
                            .parse::<f64>()
                            .unwrap_or(f64::MAX);
                        let sample_rate = f64::from(audio.sample_rate.max(1));
                        (frame_count / sample_rate * fps)
                            .ceil()
                            .to_string()
                            .parse::<i32>()
                            .unwrap_or(i32::MAX)
                    })
            })
            .unwrap_or(0);
        TimelineObject {
            id: data.id,
            start_frame: data.start_frame,
            end_frame: data.end_frame,
            kind: data.kind,
            kind_known: plugin.is_some(),
            layer: data.layer,
            label: plugin.map_or_else(
                || crate::infra::localization::tr("Unknown"),
                |p| crate::infra::localization::object_name(&p.name),
            ),
            selected: false,
            keyframe_frames: Vec::new(),
            waveform: waveform.map(|h| h.id()),
            has_waveform: waveform_duration_frames > 0,
            waveform_origin_frame: i64_to_i32(data.media_trim_in_frame).saturating_neg(),
            waveform_duration_frames,
            group_layer_count_down: data.group_layer_count_down,
            group_layer_count_up: data.group_layer_count_up,
            clip_layer_count_down: data.clip_layer_count_down,
            clip_layer_count_up: data.clip_layer_count_up,
        }
    }

    pub(super) fn waveform_texture(
        &mut self,
        ctx: &egui::Context,
        path: &std::path::Path,
    ) -> Option<egui::TextureHandle> {
        let key = path.to_path_buf();
        if let Some(handle) = self.waveform_cache.get(&key) {
            return Some(handle.clone());
        }
        let audio = neoutl_media_runtime::cache::global()
            .load_audio(path)
            .ok()?;
        let asset = neoutl_media_runtime::waveform::get(path).unwrap_or_else(|| {
            let asset = neoutl_media_runtime::waveform::build(path, &audio);
            neoutl_media_runtime::waveform::insert(asset.clone());
            asset
        });
        let peaks = neoutl_media_runtime::waveform::level_for_columns(&asset, 512);
        let visible_peaks = peaks.as_ref();
        let width = 512usize;
        let height = 48usize;
        let wave_color = ctx.style_of(ctx.theme()).visuals.selection.bg_fill;
        let mut pixels = vec![Color32::TRANSPARENT; width * height];
        for x in 0..width {
            let peak_index = x
                .saturating_mul(visible_peaks.len())
                .checked_div(width)
                .unwrap_or(0);
            let Some(peak) = visible_peaks.get(peak_index) else {
                continue;
            };
            let center = 24.0;
            let top = f32_to_i32((1.0 - peak.max.clamp(-1.0, 1.0)) * center);
            let bottom = f32_to_i32((1.0 - peak.min.clamp(-1.0, 1.0)) * center);
            for y in top.max(0)..bottom.min(48) {
                let pixel_index = usize::try_from(y)
                    .unwrap_or(0)
                    .saturating_mul(width)
                    .saturating_add(x);
                if let Some(px) = pixels.get_mut(pixel_index) {
                    *px = wave_color.gamma_multiply(0.82);
                }
            }
        }
        let image = egui::ColorImage {
            size: [width, height],
            source_size: egui::vec2(
                f32::from(u16::try_from(width).unwrap_or(u16::MAX)),
                f32::from(u16::try_from(height).unwrap_or(u16::MAX)),
            ),
            pixels,
        };
        let handle = ctx.load_texture(
            format!("waveform-{}", path.display()),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.waveform_cache.insert(key, handle.clone());
        Some(handle)
    }
}

fn i64_to_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or_else(|_| {
        if value.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}

fn f32_to_i32(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().to_string().parse().unwrap_or_else(|_| {
        if value.is_sign_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}
