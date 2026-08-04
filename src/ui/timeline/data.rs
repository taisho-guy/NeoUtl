use super::TimelineWindow;
use crate::objects::registry;
use crate::ui::types::TimelineObject;
use egui::Color32;

/// EcsWorldの`TimelineData`をUI用`TimelineObject`へ変換する（毎フレーム再構築）。
/// 波形テクスチャのみ`waveform_cache`により永続化する。
impl TimelineWindow {
    pub(super) fn to_egui(
        &mut self,
        ctx: &egui::Context,
        data: &crate::ecs::TimelineData,
        fps: f64,
    ) -> TimelineObject {
        let registry_snapshot = registry();
        let plugin = registry_snapshot.get(data.kind as usize);
        let waveform = data
            .media_path
            .as_deref()
            .and_then(|path| self.waveform_texture(ctx, path));
        let waveform_duration_frames = data
            .media_path
            .as_deref()
            .and_then(|path| {
                crate::media::cache::global()
                    .load_audio(path)
                    .ok()
                    .map(|audio| {
                        (audio.frame_count() as f64 / audio.sample_rate as f64 * fps).ceil() as i32
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
            label: plugin.map_or("Unknown", |p| p.name.as_str()).to_string(),
            selected: false,
            keyframe_frames: Vec::new(),
            waveform: waveform.map(|h| h.id()),
            has_waveform: waveform_duration_frames > 0,
            waveform_origin_frame: -data.media_trim_in_frame as i32,
            waveform_duration_frames,
        }
    }

    /// 波形テクスチャは音声デコード＋波形生成を伴うため、パス単位でegui::TextureHandleを
    /// 保持し続ける（毎フレーム再構築は行わない）。ハンドルを保持しない限りegui側で
    /// 参照カウントが尽き解放されるため、waveform_cacheが唯一の保持元となる。
    pub(super) fn waveform_texture(
        &mut self,
        ctx: &egui::Context,
        path: &std::path::Path,
    ) -> Option<egui::TextureHandle> {
        let key = path.to_path_buf();
        if let Some(handle) = self.waveform_cache.get(&key) {
            return Some(handle.clone());
        }
        let audio = crate::media::cache::global().load_audio(path).ok()?;
        let asset = crate::media::waveform::get(path).unwrap_or_else(|| {
            let asset = crate::media::waveform::build(path, &audio);
            crate::media::waveform::insert(asset.clone());
            asset
        });
        let peaks = crate::media::waveform::level_for_columns(&asset, 512);
        let visible_peaks = peaks.as_ref();
        let width = 512usize;
        let height = 48usize;
        let mut pixels = vec![Color32::TRANSPARENT; width * height];
        for x in 0..width {
            let Some(peak) = visible_peaks.get(x * visible_peaks.len() / width) else {
                continue;
            };
            let center = height as i32 / 2;
            let top = ((1.0 - peak.max.clamp(-1.0, 1.0)) * center as f32).round() as i32;
            let bottom = ((1.0 - peak.min.clamp(-1.0, 1.0)) * center as f32).round() as i32;
            for y in top.max(0)..bottom.min(height as i32) {
                if let Some(px) = pixels.get_mut(y as usize * width + x) {
                    *px = Color32::from_rgba_unmultiplied(92, 177, 255, 210);
                }
            }
        }
        let image = egui::ColorImage {
            size: [width, height],
            source_size: egui::vec2(width as f32, height as f32),
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
