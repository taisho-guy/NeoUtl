use super::curve_view::fmt;
use crate::ecs::types::Keyframe;
use egui::{Rect, Sense, Stroke, pos2, vec2};
use neoutl_easing_standard::{CurveKind, ease, parse_payload};

pub fn show(
    ui: &mut egui::Ui,
    track: &[Keyframe],
    selected: usize,
    playhead: i32,
) -> Option<usize> {
    let (rect, response) = ui.allocate_exact_size(
        vec2(super::finite_width(ui).max(80.0), 64.0),
        Sense::click(),
    );
    let visuals = ui.visuals().clone();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, visuals.extreme_bg_color);
    let body = Rect::from_min_max(
        rect.min + vec2(4.0, 4.0),
        pos2(rect.max.x - 4.0, rect.max.y - 18.0),
    );
    let first = track.first()?.frame;
    let span = (track.last()?.frame - first).max(1) as f32;
    let x_of = |frame: i32| body.left() + body.width() * (frame - first) as f32 / span;
    let accent = visuals.selection.stroke.color;
    let mut picked = None;
    let mut last_label_x = f32::MIN;
    for (i, w) in track.windows(2).enumerate() {
        let seg = Rect::from_min_max(
            pos2(x_of(w[0].frame), body.top()),
            pos2(x_of(w[1].frame), body.bottom()),
        );
        if i == selected {
            painter.rect_filled(seg, 2.0, visuals.selection.bg_fill.linear_multiply(0.5));
        }
        painter.rect_stroke(
            seg,
            2.0,
            Stroke::new(
                if i == selected { 2.0 } else { 1.0 },
                if i == selected {
                    accent
                } else {
                    visuals.widgets.noninteractive.bg_stroke.color
                },
            ),
            egui::StrokeKind::Inside,
        );
        let payload = parse_payload(&w[0].engine_payload);
        let inner = seg.shrink2(vec2(2.0, 4.0));
        let is_script = matches!(payload.kind, CurveKind::Script { .. });
        let line: Vec<egui::Pos2> = (0..=24)
            .map(|k| {
                let t = k as f32 / 24.0;
                let y = if is_script { t } else { ease(&payload, t) };
                pos2(
                    inner.left() + inner.width() * t,
                    inner.bottom() - inner.height() * y.clamp(-0.2, 1.2),
                )
            })
            .collect();
        painter.with_clip_rect(seg).add(egui::Shape::line(
            line,
            Stroke::new(1.5, visuals.text_color()),
        ));
        let x = x_of(w[0].frame);
        if x - last_label_x >= 34.0 {
            painter.text(
                pos2(x + 2.0, rect.bottom() - 2.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{}", w[0].frame),
                egui::FontId::proportional(10.0),
                visuals.weak_text_color(),
            );
            last_label_x = x;
        }
        if let Some(pos) = response.hover_pos()
            && seg.x_range().contains(pos.x)
        {
            if response.clicked() {
                picked = Some(i);
            }
            response.clone().on_hover_text(format!(
                "区間 {}: f{} → f{} ({}f)\n値 {} → {}",
                i + 1,
                w[0].frame,
                w[1].frame,
                w[1].frame - w[0].frame,
                fmt(w[0].value),
                fmt(w[1].value)
            ));
        }
    }
    if (first..=first + span as i32).contains(&playhead) {
        let x = x_of(playhead);
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom() - 14.0)],
            Stroke::new(2.0, visuals.warn_fg_color),
        );
    }
    picked
}
