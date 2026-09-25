//! ハンドル位置は hit_test::anchor_screen_pos と同一の計算を用いる。
//! 描画のみを扱い、入力判定は持たない。

use super::hit_test::{AnchorKind, anchor_screen_pos};
use super::projection::project_object_origin;
use crate::ecs::EcsWorld;

const HANDLE_RADIUS: f32 = 5.0;
const ORIGIN_RADIUS: f32 = 3.0;

pub fn draw(painter: &egui::Painter, world: &EcsWorld, selected: &[usize], image_rect: egui::Rect) {
    let line_color = egui::Color32::from_rgb(0x8a, 0xab, 0xff);
    let handle_color = egui::Color32::from_rgb(0x3a, 0x6d, 0xf0);
    let origin_color = egui::Color32::from_rgb(0xe8, 0xe8, 0xee);

    for &id in selected {
        let Some(origin) = project_object_origin(world, id, image_rect) else {
            continue;
        };
        painter.circle_filled(origin, ORIGIN_RADIUS, origin_color);

        for kind in [AnchorKind::Scale, AnchorKind::Rotate] {
            let Some(pos) = anchor_screen_pos(world, id, image_rect, kind) else {
                continue;
            };
            painter.line_segment([origin, pos], egui::Stroke::new(1.0, line_color));
            match kind {
                AnchorKind::Scale => {
                    let rect = egui::Rect::from_center_size(
                        pos,
                        egui::vec2(HANDLE_RADIUS * 2.0, HANDLE_RADIUS * 2.0),
                    );
                    painter.rect_filled(rect, 1.0, handle_color);
                }
                AnchorKind::Rotate => {
                    painter.circle_filled(pos, HANDLE_RADIUS, handle_color);
                }
            }
        }
    }
}
