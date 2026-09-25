//! 対応する操作:
//!   - オブジェクト本体のドラッグ           → 選択オブジェクト群の平行移動
//!   - Scaleアンカーのドラッグ              → 単一オブジェクトの拡大率変更 (scale_x, scale_y 同時)
//!   - Rotateアンカーのドラッグ             → 単一オブジェクトの rot_z 変更
//!   - オブジェクト本体のクリック           → 選択
//!   - 空白のクリック                       → 選択解除
//!
//! 対応範囲外:
//!   - 中心座標 (Ctrl+Alt+Drag) : Transform に中心オフセットの概念が存在しないため、
//!     `neoutl_schema::Transform` を含むスキーマ変更が前提になる。本モジュールでは実装しない。
//!   - Scale / Rotate のドラッグ量計算はスクリーン空間の比率・角度で完結しており、
//!     カメラ設定に依存しない。
//!   - Move のドラッグ量計算 (ViewportState 側) はカメラ既定値を前提とする近似。

use super::hit_test::{self, AnchorKind, HitTarget};
use super::projection;
use super::viewport::ViewportState;
use crate::app::state::{self as app_state, SharedAppState};
use crate::ecs::components::ObjectId;
use crate::ecs::history::HistoryCommand;
use crate::ecs::transform::{GlobalMatrix, Transform, compute_global_matrix};
use shipyard::{Get, IntoIter, View, ViewMut, World};

enum ActiveDrag {
    Move {
        object_ids: Vec<usize>,
        start: Vec<(usize, Transform)>,
    },
    Scale {
        object_id: usize,
        start: Transform,
        origin_screen: egui::Pos2,
        start_vec: egui::Vec2,
    },
    Rotate {
        object_id: usize,
        start: Transform,
        origin_screen: egui::Pos2,
        start_angle_deg: f32,
    },
}

pub struct InteractionState {
    active: Option<ActiveDrag>,
}

impl InteractionState {
    pub fn new() -> Self {
        Self { active: None }
    }

    pub fn handle(
        &mut self,
        response: &egui::Response,
        viewport: &ViewportState,
        image_rect: egui::Rect,
        state: &SharedAppState,
    ) {
        if response.drag_started() {
            self.start_drag(response, image_rect, state);
        }
        if response.dragged() && self.active.is_some() {
            self.apply_drag(response, viewport, state);
        }
        if response.drag_stopped() {
            self.commit_drag(state);
        }
        if response.clicked() {
            self.handle_click(response, image_rect, state);
        }
    }

    fn start_drag(
        &mut self,
        response: &egui::Response,
        image_rect: egui::Rect,
        state: &SharedAppState,
    ) {
        let Some(pointer) = response.interact_pointer_pos() else {
            return;
        };
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let selected: Vec<usize> = world.selected_ids().iter().copied().collect();
        let target = hit_test::hit_test(pointer, &selected, &world, image_rect);

        self.active = match target {
            HitTarget::Anchor(id, AnchorKind::Scale) => world
                .get_transform(id)
                .zip(projection::project_object_origin(&world, id, image_rect))
                .map(|(start, origin_screen)| ActiveDrag::Scale {
                    object_id: id,
                    start,
                    origin_screen,
                    start_vec: pointer - origin_screen,
                }),
            HitTarget::Anchor(id, AnchorKind::Rotate) => world
                .get_transform(id)
                .zip(projection::project_object_origin(&world, id, image_rect))
                .map(|(start, origin_screen)| {
                    let v = pointer - origin_screen;
                    ActiveDrag::Rotate {
                        object_id: id,
                        start,
                        origin_screen,
                        start_angle_deg: v.y.atan2(v.x).to_degrees(),
                    }
                }),
            HitTarget::Object(id) => {
                if !selected.contains(&id) {
                    world.select_id(id, true);
                }
                let object_ids = if selected.contains(&id) {
                    selected
                } else {
                    vec![id]
                };
                let start = object_ids
                    .iter()
                    .filter_map(|&oid| world.get_transform(oid).map(|t| (oid, t)))
                    .collect();
                Some(ActiveDrag::Move { object_ids, start })
            }
            HitTarget::None => None,
        };
    }

    fn apply_drag(
        &mut self,
        response: &egui::Response,
        viewport: &ViewportState,
        state: &SharedAppState,
    ) {
        let Some(drag) = &self.active else {
            return;
        };
        let delta = response.drag_delta();
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();

        match drag {
            ActiveDrag::Move { object_ids, .. } => {
                if delta == egui::Vec2::ZERO {
                    return;
                }
                let (dx, dy) = viewport.screen_delta_to_scene(delta);
                for &id in object_ids {
                    if let Some(mut t) = world.get_transform(id) {
                        t.x += dx;
                        t.y += dy;
                        world.set_transform(id, t);
                    }
                }
            }
            ActiveDrag::Scale {
                object_id,
                start,
                origin_screen,
                start_vec,
            } => {
                let Some(pointer) = response.interact_pointer_pos() else {
                    return;
                };
                let current_vec = pointer - *origin_screen;
                let start_len = start_vec.length().max(1.0);
                let ratio = (current_vec.length() / start_len).max(0.01);
                let mut t = *start;
                t.scale_x = start.scale_x * ratio;
                t.scale_y = start.scale_y * ratio;
                world.set_transform(*object_id, t);
            }
            ActiveDrag::Rotate {
                object_id,
                start,
                origin_screen,
                start_angle_deg,
            } => {
                let Some(pointer) = response.interact_pointer_pos() else {
                    return;
                };
                let v = pointer - *origin_screen;
                let angle_deg = v.y.atan2(v.x).to_degrees();
                let mut t = *start;
                t.rot_z = start.rot_z + (angle_deg - start_angle_deg);
                world.set_transform(*object_id, t);
            }
        }
    }

    fn commit_drag(&mut self, state: &SharedAppState) {
        let Some(drag) = self.active.take() else {
            return;
        };
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();

        let changes: Vec<(usize, Transform, Transform)> = match drag {
            ActiveDrag::Move { start, .. } => start
                .into_iter()
                .filter_map(|(id, old)| world.get_transform(id).map(|new| (id, old, new)))
                .filter(|(_, old, new)| !transforms_equal(old, new))
                .collect(),
            ActiveDrag::Scale {
                object_id, start, ..
            } => world
                .get_transform(object_id)
                .filter(|new| !transforms_equal(&start, new))
                .map(|new| vec![(object_id, start, new)])
                .unwrap_or_default(),
            ActiveDrag::Rotate {
                object_id, start, ..
            } => world
                .get_transform(object_id)
                .filter(|new| !transforms_equal(&start, new))
                .map(|new| vec![(object_id, start, new)])
                .unwrap_or_default(),
        };

        if !changes.is_empty() {
            world.push_history_command(Box::new(TransformChangeCommand { changes }));
        }
    }

    fn handle_click(
        &mut self,
        response: &egui::Response,
        image_rect: egui::Rect,
        state: &SharedAppState,
    ) {
        let Some(pointer) = response.interact_pointer_pos() else {
            return;
        };
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let selected: Vec<usize> = world.selected_ids().iter().copied().collect();
        match hit_test::hit_test(pointer, &selected, &world, image_rect) {
            HitTarget::Object(id) => world.select_id(id, true),
            HitTarget::None => world.clear_selection(),
            HitTarget::Anchor(..) => {}
        }
    }
}

fn transforms_equal(a: &Transform, b: &Transform) -> bool {
    a.x == b.x
        && a.y == b.y
        && a.scale_x == b.scale_x
        && a.scale_y == b.scale_y
        && a.rot_z == b.rot_z
}

struct TransformChangeCommand {
    changes: Vec<(usize, Transform, Transform)>,
}

impl HistoryCommand for TransformChangeCommand {
    fn description(&self) -> &str {
        "Transform Object"
    }

    fn apply(&mut self, world: &mut World) {
        for (id, _old, new) in &self.changes {
            set_transform_raw(world, *id, *new);
        }
    }

    fn revert(&mut self, world: &mut World) {
        for (id, old, _new) in &self.changes {
            set_transform_raw(world, *id, *old);
        }
    }
}

fn set_transform_raw(world: &mut World, object_id: usize, t: Transform) {
    world.run(
        |ids: View<ObjectId>,
         mut transforms: ViewMut<Transform>,
         mut matrices: ViewMut<GlobalMatrix>| {
            for (entity, id) in ids.iter().with_id() {
                if id.0 == object_id {
                    if let Ok(mut slot) = (&mut transforms).get(entity) {
                        *slot = t;
                    }
                    if let Ok(mut matrix) = (&mut matrices).get(entity) {
                        *matrix = compute_global_matrix(&t);
                    }
                    break;
                }
            }
        },
    );
}
