// src/ecs/systems.rs
use super::EcsWorld;
use crate::ecs::components::{AudioParams, KindId, SceneId, TextContent, TimeRange};
use crate::ecs::effects::{EffectStack, compute_effect_params_at};
use crate::ecs::resources::{SceneResource, TimelineResource};
use crate::ecs::transform::{GlobalMatrix, Transform};
use crate::ecs::types::Value;
use shipyard::{Get, IntoIter, UniqueView, View};
use std::collections::HashMap;

pub struct ActiveObject {
    pub kind_id: u32,
    pub text_content: Option<TextContent>,
    pub global_matrix: [f32; 16],
    pub opacity: f32,
    pub audio: AudioParams,
    pub effects: Vec<(String, HashMap<String, Value>)>,
}

pub fn get_active_objects_system(world: &EcsWorld) -> Vec<ActiveObject> {
    world.world.run(
        |timeline: UniqueView<TimelineResource>,
         scenes: UniqueView<SceneResource>,
         time_ranges: View<TimeRange>,
         kind_ids: View<KindId>,
         scene_ids: View<SceneId>,
         text_contents: View<TextContent>,
         transforms: View<Transform>,
         global_matrices: View<GlobalMatrix>,
         audio_params: View<AudioParams>,
         effect_stacks: View<EffectStack>| {
            let current = timeline.current_frame;
            let active_scene = scenes.active_scene;
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
                let matrix = global_matrices
                    .get(id)
                    .map(|m| m.0)
                    .unwrap_or(super::transform::IDENTITY_MATRIX);
                let opacity = transforms.get(id).map(|t| t.opacity).unwrap_or(1.0);
                let audio = audio_params.get(id).copied().unwrap_or_default();
                let effects = effect_stacks
                    .get(id)
                    .map(|stack| compute_effect_params_at(stack, current))
                    .unwrap_or_default();

                active.push(ActiveObject {
                    kind_id: kind.0,
                    text_content,
                    global_matrix: matrix,
                    opacity,
                    audio,
                    effects,
                });
            }
            active
        },
    )
}
