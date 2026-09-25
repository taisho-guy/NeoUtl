//!   - オブジェクト本体のドラッグ           → 選択オブジェクト群の平行移動
//!   - Scaleアンカーのドラッグ              → 単一オブジェクトの拡大率変更 (scale_x, scale_y 同時)
//!   - Alt+オブジェクトドラッグ             → 単一オブジェクトの拡大率変更
//!   - Alt+Scaleアンカーのドラッグ          → 単一オブジェクトのZ回転
//!   - Rotateアンカーのドラッグ             → 単一オブジェクトの rot_z 変更
//!   - Ctrl+Alt+ドラッグ                    → 単一オブジェクトの中心座標 (center_x, center_y) 変更
//!   - オブジェクト本体のクリック           → 選択
//!   - 空白のクリック                       → 選択解除
//!
//! 対応範囲外:
//!   - Scale / Rotate のドラッグ量計算はスクリーン空間の比率・角度で完結しており、
//!     カメラ設定に依存しない。
//!   - Move / CenterOffset はドラッグ開始時に対象オブジェクトの実効カメラを解決し、
//!     その ViewportState をドラッグ中に再利用する。

use super::hit_test::{self, AnchorKind, HitTarget};
use super::projection;
use super::viewport::ViewportState;
use crate::app::state::{self as app_state, SharedAppState};
use crate::ecs::components::ObjectId;
use crate::ecs::history::HistoryCommand;
use crate::ecs::systems::resolve_object_camera;
use crate::ecs::transform::{Camera, GlobalMatrix, Transform, compute_global_matrix};
use neoutl_object_api::UNIT_SIZE_PX;
use shipyard::{Get, IntoIter, View, ViewMut, World};

enum ActiveDrag {
    Move {
        object_ids: Vec<usize>,
        start: Vec<(usize, Transform)>,
        viewport: ViewportState,
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
    CenterOffset {
        object_id: usize,
        start: Transform,
        viewport: ViewportState,
    },
    Camera {
        object_id: usize,
        start: Camera,
        mode: CameraDragMode,
    },
}

#[derive(Clone, Copy)]
enum CameraDragMode {
    Orbit,
    Dolly,
    Pan,
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
        image_rect: egui::Rect,
        state: &SharedAppState,
    ) {
        if response.drag_started_by(egui::PointerButton::Secondary) {
            self.start_camera_drag(response, state);
            if self.active.is_some() {
                return;
            }
        }
        if response.drag_started() {
            self.start_drag(response, image_rect, state);
        }
        if response.dragged() && self.active.is_some() {
            self.apply_drag(response, state);
        }
        if response.drag_stopped() {
            self.commit_drag(state);
        }
        if response.clicked() {
            self.handle_click(response, image_rect, state);
        }
    }

    fn start_camera_drag(&mut self, response: &egui::Response, state: &SharedAppState) {
        let modifiers = response.ctx.input(|i| i.modifiers);
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let object_id = world
            .selected_ids()
            .iter()
            .find(|&&id| world.get_camera_params(id).is_some())
            .copied();
        let Some(object_id) = object_id else {
            return;
        };
        let Some(start) = world.get_camera_params(object_id) else {
            return;
        };
        let mode = if modifiers.ctrl {
            CameraDragMode::Dolly
        } else if modifiers.shift {
            CameraDragMode::Pan
        } else {
            CameraDragMode::Orbit
        };
        self.active = Some(ActiveDrag::Camera {
            object_id,
            start,
            mode,
        });
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
        let modifiers = response.ctx.input(|i| i.modifiers);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let selected: Vec<usize> = world.selected_ids().iter().copied().collect();

        if modifiers.ctrl && modifiers.alt {
            let target = hit_test::hit_test(pointer, &selected, &world, image_rect);
            if let HitTarget::Object(id) = target {
                if let Some(start) = world.get_transform(id) {
                    let project = world.get_project();
                    let camera = resolve_object_camera(&world, id);
                    self.active = Some(ActiveDrag::CenterOffset {
                        object_id: id,
                        start,
                        viewport: ViewportState::new(
                            image_rect,
                            project.width,
                            project.height,
                            &camera,
                        ),
                    });
                    return;
                }
            }
            self.active = None;
            return;
        }

        let target = hit_test::hit_test(pointer, &selected, &world, image_rect);

        self.active = match target {
            HitTarget::Anchor(id, AnchorKind::Scale) => world
                .get_transform(id)
                .zip(projection::project_object_origin(&world, id, image_rect))
                .map(|(start, origin_screen)| {
                    let start_vec = pointer - origin_screen;
                    if modifiers.alt {
                        ActiveDrag::Rotate {
                            object_id: id,
                            start,
                            origin_screen,
                            start_angle_deg: start_vec.y.atan2(start_vec.x).to_degrees(),
                        }
                    } else {
                        ActiveDrag::Scale {
                            object_id: id,
                            start,
                            origin_screen,
                            start_vec,
                        }
                    }
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
                if modifiers.alt {
                    world
                        .get_transform(id)
                        .zip(projection::project_object_origin(&world, id, image_rect))
                        .map(|(start, origin_screen)| ActiveDrag::Scale {
                            object_id: id,
                            start,
                            origin_screen,
                            start_vec: pointer - origin_screen,
                        })
                } else {
                    let object_ids = if selected.contains(&id) {
                        selected
                    } else {
                        vec![id]
                    };
                    let start = object_ids
                        .iter()
                        .filter_map(|&oid| world.get_transform(oid).map(|t| (oid, t)))
                        .collect();
                    let project = world.get_project();
                    let camera = resolve_object_camera(&world, id);
                    Some(ActiveDrag::Move {
                        object_ids,
                        start,
                        viewport: ViewportState::new(
                            image_rect,
                            project.width,
                            project.height,
                            &camera,
                        ),
                    })
                }
            }
            HitTarget::None => None,
        };
    }

    fn apply_drag(&mut self, response: &egui::Response, state: &SharedAppState) {
        let Some(drag) = &self.active else {
            return;
        };
        let delta = response.drag_delta();
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();

        match drag {
            ActiveDrag::Move {
                object_ids,
                start,
                viewport,
            } => {
                if delta == egui::Vec2::ZERO {
                    return;
                }
                let (dx, dy) = viewport.screen_delta_to_scene(delta);
                for &id in object_ids {
                    if let Some((_, initial)) = start.iter().find(|(start_id, _)| *start_id == id) {
                        let mut t = *initial;
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
            ActiveDrag::CenterOffset {
                object_id,
                start,
                viewport,
            } => {
                if delta == egui::Vec2::ZERO {
                    return;
                }
                let (dx, dy) = viewport.screen_delta_to_scene(delta);
                let object_id = *object_id;
                let mut t = *start;
                t.center_x += dx / UNIT_SIZE_PX;
                t.center_y += dy / UNIT_SIZE_PX;
                world.set_transform(object_id, t);
            }
            ActiveDrag::Camera {
                object_id,
                start,
                mode,
            } => {
                let drag_delta = response.drag_delta();
                let next = camera_after_drag(*start, drag_delta, *mode);
                set_camera(&mut world, *object_id, next);
            }
        }
    }

    fn commit_drag(&mut self, state: &SharedAppState) {
        let Some(drag) = self.active.take() else {
            return;
        };
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();

        if let ActiveDrag::Camera {
            object_id, start, ..
        } = &drag
        {
            if let Some(new) = world.get_camera_params(*object_id) {
                if new != *start {
                    world.push_history_command(Box::new(CameraChangeCommand {
                        object_id: *object_id,
                        old: *start,
                        new,
                    }));
                }
            }
            return;
        }

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
            ActiveDrag::CenterOffset {
                object_id, start, ..
            } => world
                .get_transform(object_id)
                .filter(|new| !transforms_equal(&start, new))
                .map(|new| vec![(object_id, start, new)])
                .unwrap_or_default(),
            ActiveDrag::Camera { .. } => unreachable!(),
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

fn camera_after_drag(start: Camera, delta: egui::Vec2, mode: CameraDragMode) -> Camera {
    let mut camera = start;
    let dx = delta.x * 0.35;
    let dy = delta.y * 0.35;
    let offset = [
        start.pos_x - start.target_x,
        start.pos_y - start.target_y,
        start.pos_z - start.target_z,
    ];
    let radius = (offset[0] * offset[0] + offset[1] * offset[1] + offset[2] * offset[2])
        .sqrt()
        .max(1.0);

    match mode {
        CameraDragMode::Orbit => {
            let yaw = offset[0].atan2(offset[2]) + dx.to_radians();
            let pitch = (offset[1] / radius).asin() + dy.to_radians();
            let pitch = pitch.clamp(-1.5, 1.5);
            let horizontal = radius * pitch.cos();
            camera.pos_x = start.target_x + horizontal * yaw.sin();
            camera.pos_y = start.target_y + radius * pitch.sin();
            camera.pos_z = start.target_z + horizontal * yaw.cos();
        }
        CameraDragMode::Dolly => {
            let distance = (radius + dy * 4.0).max(1.0);
            let scale = distance / radius;
            camera.pos_x = start.target_x + offset[0] * scale;
            camera.pos_y = start.target_y + offset[1] * scale;
            camera.pos_z = start.target_z + offset[2] * scale;
        }
        CameraDragMode::Pan => {
            let forward = normalize3([
                start.target_x - start.pos_x,
                start.target_y - start.pos_y,
                start.target_z - start.pos_z,
            ]);
            let right = normalize3([forward[2], 0.0, -forward[0]]);
            let up = normalize3([
                right[1] * forward[2] - right[2] * forward[1],
                right[2] * forward[0] - right[0] * forward[2],
                right[0] * forward[1] - right[1] * forward[0],
            ]);
            let movement = [
                (right[0] * dx - up[0] * dy) * 2.0,
                (right[1] * dx - up[1] * dy) * 2.0,
                (right[2] * dx - up[2] * dy) * 2.0,
            ];
            camera.pos_x += movement[0];
            camera.pos_y += movement[1];
            camera.pos_z += movement[2];
            camera.target_x += movement[0];
            camera.target_y += movement[1];
            camera.target_z += movement[2];
        }
    }
    camera
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / length, v[1] / length, v[2] / length]
}

fn set_camera(world: &mut crate::ecs::EcsWorld, object_id: usize, camera: Camera) {
    world.set_camera_params(object_id, camera);
}

fn transforms_equal(a: &Transform, b: &Transform) -> bool {
    a.x == b.x
        && a.y == b.y
        && a.scale_x == b.scale_x
        && a.scale_y == b.scale_y
        && a.rot_z == b.rot_z
        && a.center_x == b.center_x
        && a.center_y == b.center_y
        && a.center_z == b.center_z
}

struct TransformChangeCommand {
    changes: Vec<(usize, Transform, Transform)>,
}

struct CameraChangeCommand {
    object_id: usize,
    old: Camera,
    new: Camera,
}

impl HistoryCommand for CameraChangeCommand {
    fn description(&self) -> &str {
        "Move Camera"
    }

    fn apply(&mut self, world: &mut World) {
        set_camera_raw(world, self.object_id, self.new);
    }

    fn revert(&mut self, world: &mut World) {
        set_camera_raw(world, self.object_id, self.old);
    }
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

fn set_camera_raw(world: &mut World, object_id: usize, camera: Camera) {
    world.run(|ids: View<ObjectId>, mut cameras: ViewMut<Camera>| {
        for (entity, id) in ids.iter().with_id() {
            if id.0 == object_id {
                if let Ok(mut slot) = (&mut cameras).get(entity) {
                    *slot = camera;
                }
                break;
            }
        }
    });
}
