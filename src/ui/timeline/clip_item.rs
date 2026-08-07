use super::util::{brighten, darken, readable_text_color};
use super::{
    ClipDrag, DragMode, HANDLE_WIDTH, KEYFRAME_SIZE, KeyframeDrag, LAYER_HEIGHT, TimelineWindow,
};
use crate::app_state::SharedAppState;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::TimelineObject;
use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, Vec2};
use std::cell::RefCell;
use std::rc::Rc;

/// Slint `timeline/clip-item.slint` 相当。クリップ本体・波形・キーフレーム
/// マーカーの描画と、移動/リサイズ/キーフレームドラッグの入力処理を担う。
impl TimelineWindow {
    pub(super) fn clip_ui(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        view_rect: Rect,
        state: &SharedAppState,
        _preview_panel: &Rc<RefCell<PreviewPanel>>,
        _props_weak: &(),
        obj: &TimelineObject,
        locked: bool,
        total_frames: i32,
    ) {
        let _ = total_frames;
        let (preview_start, preview_end, preview_layer) = match &self.drag {
            Some(d) if d.id == obj.id => (d.preview_start, d.preview_end, d.preview_layer),
            _ => (obj.start_frame, obj.end_frame, obj.layer),
        };

        let x = view_rect.min.x + self.frame_to_x(preview_start);
        let h = LAYER_HEIGHT * 0.75;
        let y = view_rect.min.y + self.layer_to_y(preview_layer) + (LAYER_HEIGHT - h) * 0.5;
        let w = ((preview_end - preview_start) as f32 * self.zoom_scale).max(4.0);
        let clip_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
        if clip_rect.max.x < view_rect.min.x || clip_rect.min.x > view_rect.max.x {
            return;
        }
        if clip_rect.max.y < view_rect.min.y || clip_rect.min.y > view_rect.max.y {
            return;
        }

        let palette = [
            Color32::from_rgb(0x2a, 0x4d, 0xb8),
            Color32::from_rgb(0x25, 0x6e, 0x3c),
            Color32::from_rgb(0x6a, 0x2d, 0xb8),
            Color32::from_rgb(0xb8, 0x86, 0x2a),
            Color32::from_rgb(0x2a, 0xb8, 0xa8),
            Color32::from_rgb(0xb8, 0x2a, 0x5f),
        ];
        let base = if !obj.kind_known {
            Color32::from_rgb(0x5a, 0x5a, 0x5a)
        } else {
            palette[(obj.kind.max(0) as usize) % palette.len()]
        };
        let color = if obj.selected {
            brighten(base, 0.5)
        } else {
            base
        };
        let visuals = ui.visuals().clone();
        let label_color = readable_text_color(color);
        let keyframe_color = visuals.warn_fg_color;

        if obj.selected {
            painter.rect_filled(clip_rect, 0.0, color);
        } else {
            let mut mesh = egui::Mesh::default();
            let left_color = darken(color, 0.5);
            mesh.colored_vertex(clip_rect.left_top(), left_color);
            mesh.colored_vertex(clip_rect.left_bottom(), left_color);
            mesh.colored_vertex(clip_rect.right_top(), color);
            mesh.colored_vertex(clip_rect.right_bottom(), color);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(1, 3, 2);
            painter.add(mesh);
        }

        if obj.has_waveform && self.show_waveform {
            if let Some(tex) = obj.waveform {
                let wx = clip_rect.min.x
                    + 3.0
                    + (obj.waveform_origin_frame - preview_start) as f32 * self.zoom_scale;
                let ww = (obj.waveform_duration_frames as f32 * self.zoom_scale).max(1.0);
                let wave_rect = Rect::from_min_size(
                    Pos2::new(wx, clip_rect.min.y + 3.0),
                    Vec2::new(ww, (clip_rect.height() - 6.0).max(1.0)),
                );
                let tint = if obj.selected {
                    Color32::from_white_alpha(230)
                } else {
                    Color32::from_white_alpha(166)
                };
                let clipped = painter.with_clip_rect(clip_rect.intersect(view_rect));
                clipped.image(
                    tex,
                    wave_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    tint,
                );
            }
        }

        {
            let text_clip = clip_rect.intersect(view_rect);
            if text_clip.width() > 0.0 && text_clip.height() > 0.0 {
                let label_x = clip_rect.min.x.max(view_rect.min.x) + 6.0;
                let label_x = label_x.min(clip_rect.max.x - 2.0);
                painter.with_clip_rect(text_clip).text(
                    Pos2::new(label_x, clip_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &obj.label,
                    egui::FontId::proportional(10.0),
                    label_color,
                );
            }
        }

        for &f in &obj.keyframe_frames {
            let dragged_delta = match &self.kdrag {
                Some(k) if k.id == obj.id && k.frame == f => k.delta_frames,
                _ => 0,
            };
            let span = (obj.end_frame - obj.start_frame).max(1) as f32;
            let kx = clip_rect.min.x
                + (f + dragged_delta - obj.start_frame) as f32 * clip_rect.width() / span;
            let ky = clip_rect.center().y;
            let marker_rect = Rect::from_center_size(Pos2::new(kx, ky), Vec2::splat(KEYFRAME_SIZE));
            painter.rect_filled(marker_rect, 0.0, keyframe_color);
            painter.rect_stroke(
                marker_rect,
                0.0,
                Stroke::new(1.0, darken(keyframe_color, 0.4)),
                egui::StrokeKind::Outside,
            );

            let kid = ui.id().with(("keyframe", obj.id, f));
            let kresp = ui.interact(marker_rect, kid, Sense::click_and_drag());
            if !locked {
                if kresp.drag_started() {
                    if let Some(pos) = kresp.interact_pointer_pos() {
                        self.kdrag = Some(KeyframeDrag {
                            id: obj.id,
                            frame: f,
                            press: pos,
                            delta_frames: 0,
                        });
                    }
                }
                if kresp.dragged() {
                    if let (Some(pos), Some(kdrag)) =
                        (kresp.interact_pointer_pos(), &mut self.kdrag)
                    {
                        kdrag.delta_frames =
                            ((pos.x - kdrag.press.x) / self.zoom_scale).floor() as i32;
                    }
                }
                if kresp.drag_stopped() {
                    if let Some(kdrag) = self.kdrag.take() {
                        if kdrag.delta_frames != 0 {
                            let new_frame =
                                (f + kdrag.delta_frames).clamp(obj.start_frame, obj.end_frame);
                            self.keyframe_moved(state, obj.id, f, new_frame);
                        } else if kresp.clicked() {
                        }
                    }
                }
            }
        }

        let body_rect = Rect::from_min_size(
            Pos2::new(clip_rect.min.x + HANDLE_WIDTH, clip_rect.min.y),
            Vec2::new(
                (clip_rect.width() - 2.0 * HANDLE_WIDTH).max(0.0),
                clip_rect.height(),
            ),
        );
        let left_rect =
            Rect::from_min_size(clip_rect.min, Vec2::new(HANDLE_WIDTH, clip_rect.height()));
        let right_rect = Rect::from_min_size(
            Pos2::new(clip_rect.max.x - HANDLE_WIDTH, clip_rect.min.y),
            Vec2::new(HANDLE_WIDTH, clip_rect.height()),
        );

        let left_id = ui.id().with(("clip-left", obj.id));
        let left_resp = ui.interact(left_rect, left_id, Sense::click_and_drag());
        let right_id = ui.id().with(("clip-right", obj.id));
        let right_resp = ui.interact(right_rect, right_id, Sense::click_and_drag());
        let body_id = ui.id().with(("clip-body", obj.id));
        let body_resp = ui.interact(body_rect, body_id, Sense::click_and_drag());

        if locked {
            return;
        }

        if left_resp.drag_started() {
            if let Some(pos) = left_resp.interact_pointer_pos() {
                self.drag = Some(ClipDrag {
                    id: obj.id,
                    mode: DragMode::ResizeLeft,
                    press: pos,
                    start_frame: obj.start_frame,
                    end_frame: obj.end_frame,
                    layer: obj.layer,
                    duration: obj.end_frame - obj.start_frame,
                    preview_start: obj.start_frame,
                    preview_end: obj.end_frame,
                    preview_layer: obj.layer,
                });
            }
        }
        if right_resp.drag_started() {
            if let Some(pos) = right_resp.interact_pointer_pos() {
                self.drag = Some(ClipDrag {
                    id: obj.id,
                    mode: DragMode::ResizeRight,
                    press: pos,
                    start_frame: obj.start_frame,
                    end_frame: obj.end_frame,
                    layer: obj.layer,
                    duration: obj.end_frame - obj.start_frame,
                    preview_start: obj.start_frame,
                    preview_end: obj.end_frame,
                    preview_layer: obj.layer,
                });
            }
        }
        if body_resp.drag_started() {
            self.select_object(state, &(), obj.id, false);
            if let Some(pos) = body_resp.interact_pointer_pos() {
                self.drag = Some(ClipDrag {
                    id: obj.id,
                    mode: DragMode::Move,
                    press: pos,
                    start_frame: obj.start_frame,
                    end_frame: obj.end_frame,
                    layer: obj.layer,
                    duration: obj.end_frame - obj.start_frame,
                    preview_start: obj.start_frame,
                    preview_end: obj.end_frame,
                    preview_layer: obj.layer,
                });
            }
        }
        if body_resp.clicked() && self.drag.is_none() {
            self.select_object(state, &(), obj.id, false);
        }
        if body_resp.secondary_clicked() {
            self.select_object(state, &(), obj.id, false);
            if let Some(pos) = body_resp.interact_pointer_pos() {
                self.open_context_menu(ui, state, pos, obj.start_frame, obj.layer, obj.id);
            }
        }

        if left_resp.drag_stopped() {
            if let Some(drag) = self
                .drag
                .take_if(|d| d.id == obj.id && d.mode == DragMode::ResizeLeft)
            {
                let holder = crate::app_state::active_world(state);
                holder.lock().unwrap().resize_object(
                    obj.id as usize,
                    drag.preview_start,
                    drag.preview_end,
                );
            }
        }
        if right_resp.drag_stopped() {
            if let Some(drag) = self
                .drag
                .take_if(|d| d.id == obj.id && d.mode == DragMode::ResizeRight)
            {
                let holder = crate::app_state::active_world(state);
                holder.lock().unwrap().resize_object(
                    obj.id as usize,
                    drag.preview_start,
                    drag.preview_end,
                );
            }
        }
        if body_resp.drag_stopped() {
            if let Some(drag) = self
                .drag
                .take_if(|d| d.id == obj.id && d.mode == DragMode::Move)
            {
                let holder = crate::app_state::active_world(state);
                holder.lock().unwrap().move_object(
                    obj.id as usize,
                    drag.preview_start,
                    drag.preview_layer,
                );
            }
        }

        if let Some(drag) = &mut self.drag {
            if drag.id == obj.id {
                match drag.mode {
                    DragMode::Move => {
                        if let Some(pos) = body_resp
                            .interact_pointer_pos()
                            .or(ui.ctx().pointer_hover_pos())
                        {
                            let dx = ((pos.x - drag.press.x) / self.zoom_scale).floor() as i32;
                            let dy = ((pos.y - drag.press.y) / LAYER_HEIGHT).floor() as i32;
                            drag.preview_start = (drag.start_frame + dx).max(0);
                            drag.preview_end = drag.preview_start + drag.duration;
                            drag.preview_layer = (drag.layer + dy).max(0);
                        }
                    }
                    DragMode::ResizeLeft => {
                        if let Some(pos) = left_resp
                            .interact_pointer_pos()
                            .or(ui.ctx().pointer_hover_pos())
                        {
                            let dx = ((pos.x - drag.press.x) / self.zoom_scale).floor() as i32;
                            drag.preview_start =
                                (drag.start_frame + dx).clamp(0, drag.end_frame - 1);
                        }
                    }
                    DragMode::ResizeRight => {
                        if let Some(pos) = right_resp
                            .interact_pointer_pos()
                            .or(ui.ctx().pointer_hover_pos())
                        {
                            let dx = ((pos.x - drag.press.x) / self.zoom_scale).floor() as i32;
                            drag.preview_end = (drag.end_frame + dx).max(drag.start_frame + 1);
                        }
                    }
                }
            }
        }
    }
}
