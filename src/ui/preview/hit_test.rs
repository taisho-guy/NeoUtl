//! アンカーはオブジェクトの外形 (SourceSizeCache から解決した4隅) を基準に配置する。
//! SourceSizeCache に未登録のオブジェクト (シェイプ・テキスト等、外形未解決の種別、
//! またはまだ1フレームも `get_active_objects_system` を通っていないオブジェクト) は、
//! 原点からの固定オフセットへフォールバックする。

use super::projection::{project_object_corners, project_object_origin};
use crate::ecs::EcsWorld;
use crate::ecs::resources::source_size_cache;
use crate::ecs::systems::get_active_objects_system;

pub const ANCHOR_OFFSET_PX: f32 = 48.0;
pub const ANCHOR_HIT_RADIUS_PX: f32 = 8.0;
pub const OBJECT_PICK_RADIUS_PX: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorKind {
    Scale,
    Rotate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitTarget {
    Anchor(usize, AnchorKind),
    Object(usize),
    None,
}

/// 凸四角形の内外判定 (2Dクロス積による符号一致判定)。
/// `corners` は `project_object_corners` の出力 (時計回り/反時計回りいずれでも可) を想定する。
fn point_in_quad(p: egui::Pos2, corners: &[egui::Pos2; 4]) -> bool {
    let mut sign: Option<bool> = None;
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        let s = cross > 0.0;
        match sign {
            None => sign = Some(s),
            Some(prev) if prev != s => return false,
            _ => {}
        }
    }
    true
}

pub fn anchor_screen_pos(
    world: &EcsWorld,
    object_id: usize,
    image_rect: egui::Rect,
    kind: AnchorKind,
) -> Option<egui::Pos2> {
    let origin = project_object_origin(world, object_id, image_rect)?;

    if let Some((sw, sh)) = source_size_cache::global().get(object_id) {
        if let Some(corners) = project_object_corners(world, object_id, sw, sh, image_rect) {
            return Some(match kind {
                AnchorKind::Scale => corners[2],
                AnchorKind::Rotate => {
                    let mid_top = egui::pos2(
                        (corners[0].x + corners[1].x) * 0.5,
                        (corners[0].y + corners[1].y) * 0.5,
                    );
                    let dir = (mid_top - origin).normalized();
                    mid_top + dir * 16.0
                }
            });
        }
    }

    Some(match kind {
        AnchorKind::Scale => origin + egui::vec2(ANCHOR_OFFSET_PX, ANCHOR_OFFSET_PX),
        AnchorKind::Rotate => origin + egui::vec2(0.0, -ANCHOR_OFFSET_PX),
    })
}

pub fn hit_test(
    pointer: egui::Pos2,
    selected: &[usize],
    world: &EcsWorld,
    image_rect: egui::Rect,
) -> HitTarget {
    for &id in selected {
        for kind in [AnchorKind::Rotate, AnchorKind::Scale] {
            if let Some(pos) = anchor_screen_pos(world, id, image_rect, kind) {
                if pos.distance(pointer) <= ANCHOR_HIT_RADIUS_PX {
                    return HitTarget::Anchor(id, kind);
                }
            }
        }
    }

    let (active, _captured) = get_active_objects_system(world);
    let mut nearest: Option<(usize, f32)> = None;
    for obj in &active {
        let id = obj.clip_instance as usize;

        if let Some((sw, sh)) = source_size_cache::global().get(id) {
            if let Some(corners) = project_object_corners(world, id, sw, sh, image_rect) {
                if point_in_quad(pointer, &corners) {
                    nearest = Some((id, 0.0));
                    continue;
                }
            }
        }

        let Some(pos) = project_object_origin(world, id, image_rect) else {
            continue;
        };
        let d = pos.distance(pointer);
        if d <= OBJECT_PICK_RADIUS_PX && nearest.map_or(true, |(_, nd)| d < nd) {
            nearest = Some((id, d));
        }
    }
    nearest.map_or(HitTarget::None, |(id, _)| HitTarget::Object(id))
}
