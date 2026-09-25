//! アンカーはオブジェクトの外形ではなく、原点からの固定オフセットで配置する。
//! 理由: オブジェクトの描画サイズ解決 (メディア寸法・シェイプ寸法等、種別ごとに異なる)
//! はレンダラ内部 (`rescale_for_source` 等) に閉じており、
//! UI層から重複実装せずに正しく再取得する経路が現状存在しないため。

use super::projection::project_object_origin;
use crate::ecs::EcsWorld;
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

pub fn anchor_screen_pos(
    world: &EcsWorld,
    object_id: usize,
    image_rect: egui::Rect,
    kind: AnchorKind,
) -> Option<egui::Pos2> {
    let origin = project_object_origin(world, object_id, image_rect)?;
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
