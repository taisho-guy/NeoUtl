// src/ecs/systems.rs
use super::EcsWorld;
use crate::ecs::components::{AudioParams, KindId, SceneId, ShapeParams, TextContent, TimeRange};
use crate::ecs::effects::{EffectStack, compute_effect_params_at};
use crate::ecs::resources::{ProjectResource, SceneResource, TimelineResource};
use crate::ecs::transform::{Camera, GlobalMatrix, Projection, Transform, compute_mvp};
use crate::ecs::types::Value;
use crate::objects;
use neoutl_object_api::Dimensionality;
use shipyard::{Get, IntoIter, UniqueView, View};
use std::collections::HashMap;

pub struct ActiveObject {
    pub kind_id: u32,
    pub text_content: Option<TextContent>,
    pub shape_params: Option<ShapeParams>,
    pub global_matrix: [f32; 16],
    pub mvp: [f32; 16],
    pub opacity: f32,
    pub audio: AudioParams,
    pub effects: Vec<(String, HashMap<String, Value>)>,
}

/// kind_idのdimensionalityからOrtho/Perspectiveを選択する。
/// プラグイン未登録（kind_id不明）時は2D既定のOrthoにフォールバックする。
fn projection_for(kind_id: u32) -> Projection {
    match objects::by_kind_id(kind_id).map(|p| unsafe { &*((p.vtable.meta)()) }.dimensionality) {
        Some(Dimensionality::ThreeD) | Some(Dimensionality::Both) => {
            Projection::Perspective { fov_deg: 45.0 }
        }
        _ => Projection::Ortho,
    }
}

pub fn get_active_objects_system(world: &EcsWorld) -> Vec<ActiveObject> {
    world.world.run(
        |(timeline, scenes, project, camera): (
            UniqueView<TimelineResource>,
            UniqueView<SceneResource>,
            UniqueView<ProjectResource>,
            UniqueView<Camera>,
        ),
         (time_ranges, kind_ids, scene_ids, text_contents, shape_params): (
            View<TimeRange>,
            View<KindId>,
            View<SceneId>,
            View<TextContent>,
            View<ShapeParams>,
        ),
         (transforms, global_matrices, audio_params, effect_stacks): (
            View<Transform>,
            View<GlobalMatrix>,
            View<AudioParams>,
            View<EffectStack>,
        )| {
            let current = timeline.current_frame;
            let active_scene = scenes.active_scene;
            let aspect = project.width.max(1) as f32 / project.height.max(1) as f32;
            let mut active = Vec::new();

            for (id, (range, kind, scene)) in (&time_ranges, &kind_ids, &scene_ids).iter().with_id()
            {
                if scene.0 != active_scene {
                    continue;
                }
                if current < range.start_frame || current >= range.end_frame {
                    continue;
                }
                let text_content = text_contents.get(id).ok().cloned();
                let shape = shape_params.get(id).ok().copied();
                let matrix = global_matrices.get(id).copied().unwrap_or_default();
                let mvp = compute_mvp(&matrix, &camera, aspect, projection_for(kind.0));
                let opacity = transforms.get(id).map(|t| t.opacity).unwrap_or(1.0);
                let audio = audio_params.get(id).copied().unwrap_or_default();
                let effects = effect_stacks
                    .get(id)
                    .map(|stack| compute_effect_params_at(stack, current))
                    .unwrap_or_default();

                active.push(ActiveObject {
                    kind_id: kind.0,
                    text_content,
                    shape_params: shape,
                    global_matrix: matrix.0,
                    mvp,
                    opacity,
                    audio,
                    effects,
                });
            }
            active
        },
    )
}
