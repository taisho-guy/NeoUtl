pub struct TrackOutcome {
    pub point_clicked: Option<i32>,
    pub add_point: Option<i32>,
    pub remove_point: Option<i32>,
    pub drag_committed: Option<(i32, i32)>,
}

impl TrackOutcome {
    fn empty() -> Self {
        Self {
            point_clicked: None,
            add_point: None,
            remove_point: None,
            drag_committed: None,
        }
    }
}

const POINT_RADIUS: f32 = 4.0;
const HEIGHT: f32 = 12.0;

static DRAG_ORIGIN: std::sync::Mutex<Option<std::collections::HashMap<egui::Id, i32>>> =
    std::sync::Mutex::new(None);

static CONTEXT_HIT: std::sync::Mutex<
    Option<std::collections::HashMap<egui::Id, (Option<(usize, i32, f32)>, f32)>>,
> = std::sync::Mutex::new(None);

pub fn keyframe_track(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash + std::fmt::Debug,
    boundary_frames: &[i32],
    clip_start: i32,
    clip_end: i32,
    current_frame: i32,
    segment_start: i32,
    segment_end: i32,
) -> TrackOutcome {
    let track_id = ui.make_persistent_id(id_source);
    let mut out = TrackOutcome::empty();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), HEIGHT),
        egui::Sense::click_and_drag(),
    );
    let span = (clip_end - clip_start).max(1) as f32;
    let frac = |f: i32| ((f - clip_start) as f32 / span).clamp(0.0, 1.0);
    let x_at = |f: i32| rect.left() + rect.width() * frac(f);
    let frame_at = |x: f32| -> i32 {
        let t = ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
        clip_start + (t * span + 0.5) as i32
    };

    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(0x13, 0x13, 0x18));
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(0x24, 0x24, 0x2c)),
        egui::StrokeKind::Outside,
    );

    let seg_rect = egui::Rect::from_min_max(
        egui::pos2(x_at(segment_start), rect.top()),
        egui::pos2(x_at(segment_end), rect.bottom()),
    );
    painter.rect_filled(
        seg_rect,
        0.0,
        egui::Color32::from_rgba_unmultiplied(0x2a, 0x4d, 0xb8, 90),
    );

    let cx = x_at(current_frame);
    painter.line_segment(
        [egui::pos2(cx, rect.top()), egui::pos2(cx, rect.bottom())],
        egui::Stroke::new(1.0, egui::Color32::from_rgb(0xe8, 0xe8, 0xee)),
    );

    let last = boundary_frames.len().saturating_sub(1);
    let mut origins = DRAG_ORIGIN.lock().unwrap();
    let origins = origins.get_or_insert_with(std::collections::HashMap::new);
    if !response.dragged() && !response.drag_started() && !response.drag_stopped() {
        origins.remove(&track_id);
    }
    let dragging_origin = origins.get(&track_id).copied();
    let drag_preview_x = dragging_origin.and_then(|_| {
        if response.dragged() {
            response
                .interact_pointer_pos()
                .map(|p| p.x.clamp(rect.left(), rect.right()))
        } else {
            None
        }
    });

    let mut nearest: Option<(usize, i32, f32)> = None;
    for (idx, &f) in boundary_frames.iter().enumerate() {
        let is_endpoint = idx == 0 || idx == last;
        let is_dragged_point = dragging_origin == Some(f);
        let px = if is_dragged_point {
            drag_preview_x.unwrap_or_else(|| x_at(f))
        } else {
            x_at(f)
        };
        let color = if is_endpoint {
            egui::Color32::from_rgb(0x6a, 0x6a, 0x76)
        } else if is_dragged_point {
            egui::Color32::from_rgb(0x8a, 0xab, 0xff)
        } else {
            egui::Color32::from_rgb(0x3a, 0x6d, 0xf0)
        };
        painter.circle_filled(egui::pos2(px, rect.center().y), POINT_RADIUS, color);
        if dragging_origin.is_none() {
            if let Some(pos) = response.hover_pos().or(response.interact_pointer_pos()) {
                let d = (pos.x - px).abs();
                if d <= POINT_RADIUS * 2.5 && nearest.map(|(_, _, nd)| d < nd).unwrap_or(true) {
                    nearest = Some((idx, f, d));
                }
            }
        }
    }
    let hit_endpoint = |idx: usize| idx == 0 || idx == last;

    if response.drag_started() {
        if let Some((idx, f, _)) = nearest {
            if !hit_endpoint(idx) {
                origins.insert(track_id, f);
            }
        }
    }
    if response.drag_stopped() {
        if let (Some(origin), Some(pos)) =
            (origins.remove(&track_id), response.interact_pointer_pos())
        {
            let to = frame_at(pos.x).clamp(clip_start, clip_end);
            if to != origin {
                out.drag_committed = Some((origin, to));
            }
        }
    }
    if response.clicked() && !origins.contains_key(&track_id) {
        if let Some(pos) = response.interact_pointer_pos() {
            out.point_clicked = match nearest {
                Some((_, f, _)) => Some(f),
                None => Some(frame_at(pos.x)),
            };
        }
    }

    let add_cell = std::cell::Cell::new(None);
    let remove_cell = std::cell::Cell::new(None);

    if response.secondary_clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let mut hits = CONTEXT_HIT.lock().unwrap();
            let hits = hits.get_or_insert_with(std::collections::HashMap::new);
            hits.insert(track_id, (nearest, pos.x));
        }
    }

    elegance::ContextMenu::new(track_id).show(&response, |ui| {
        let hits = CONTEXT_HIT.lock().unwrap();
        let Some(&(fixed_nearest, pos_x)) = hits.as_ref().and_then(|m| m.get(&track_id)) else {
            return;
        };
        match fixed_nearest {
            Some((idx, f, d)) if d <= POINT_RADIUS * 2.5 && !hit_endpoint(idx) => {
                if ui
                    .add(elegance::Button::new(t!("キーフレーム削除")))
                    .clicked()
                {
                    remove_cell.set(Some(f));
                    ui.close();
                }
            }
            _ => {
                let f = frame_at(pos_x);
                if ui
                    .add(elegance::Button::new(t!("キーフレーム追加")))
                    .clicked()
                {
                    add_cell.set(Some(f));
                    ui.close();
                }
            }
        }
    });
    out.add_point = add_cell.get();
    out.remove_point = remove_cell.get();
    out
}
