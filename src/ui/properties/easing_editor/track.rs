use crate::ecs::EcsWorld;
use crate::ecs::components::{KeyframeTracks, ObjectId, ParamAccess};
use crate::ecs::effects::EffectStack;
use crate::ecs::history::HistoryCommand;
use crate::ecs::types::{EffectParam, Keyframe, Value, next_edit_seq};
use neoutl_easing_standard::{EasingPayload, encode_payload};
use shipyard::{Get, IntoIter, View, ViewMut, World};

pub const ENGINE_ID: &str = "neoutl-easing-standard";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TrackTarget {
    Object {
        object_id: usize,
        key: String,
    },
    Effect {
        object_id: usize,
        effect_index: usize,
        key: String,
    },
}

impl TrackTarget {
    pub fn object_id(&self) -> usize {
        match self {
            Self::Object { object_id, .. } | Self::Effect { object_id, .. } => *object_id,
        }
    }
}

pub fn read(world: &EcsWorld, target: &TrackTarget) -> Vec<Keyframe> {
    match target {
        TrackTarget::Object { object_id, key } => world.get_keyframes(*object_id, key),
        TrackTarget::Effect {
            object_id,
            effect_index,
            key,
        } => world.get_effect_keyframes(*object_id, *effect_index, key),
    }
}

fn assign(world: &mut World, target: &TrackTarget, keys: &[Keyframe]) {
    let id = target.object_id();
    let Some(entity) = world.run(|ids: View<ObjectId>| {
        ids.iter()
            .with_id()
            .find(|(_, o)| o.0 == id)
            .map(|(e, _)| e)
    }) else {
        return;
    };
    match target {
        TrackTarget::Object { key, .. } => {
            let mut tracks = world
                .run(|v: View<KeyframeTracks>| v.get(entity).ok().cloned())
                .unwrap_or_default();
            tracks.0.insert(key.clone(), keys.to_vec());
            world.add_component(entity, tracks);
        }
        TrackTarget::Effect {
            effect_index, key, ..
        } => world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity)
                && let Some(effect) = stack.0.get_mut(*effect_index)
            {
                effect
                    .params
                    .entry(key.clone())
                    .or_insert_with(|| {
                        EffectParam::new(Value::Number(keys.first().map_or(0.0, |k| k.value)))
                    })
                    .keyframes = keys.to_vec();
            }
        }),
    }
}

pub fn write(world: &mut EcsWorld, target: &TrackTarget, keys: &[Keyframe]) {
    assign(&mut world.world, target, keys);
    world.touch();
}

pub fn same(a: &[Keyframe], b: &[Keyframe]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.frame == y.frame
                && x.value == y.value
                && x.engine_id == y.engine_id
                && x.engine_payload == y.engine_payload
        })
}

struct TrackEdit {
    target: TrackTarget,
    before: Vec<Keyframe>,
    after: Vec<Keyframe>,
}

impl HistoryCommand for TrackEdit {
    fn description(&self) -> &str {
        "Edit easing"
    }
    fn apply(&mut self, world: &mut World) {
        assign(world, &self.target, &self.after);
    }
    fn revert(&mut self, world: &mut World) {
        assign(world, &self.target, &self.before);
    }
}

pub fn push_history(
    world: &mut EcsWorld,
    target: &TrackTarget,
    before: Vec<Keyframe>,
    after: Vec<Keyframe>,
) {
    if !same(&before, &after) {
        world.push_history_command(Box::new(TrackEdit {
            target: target.clone(),
            before,
            after,
        }));
    }
}

pub fn new_keyframe(frame: i32, value: f32, payload: &EasingPayload) -> Keyframe {
    Keyframe {
        frame,
        value,
        engine_id: ENGINE_ID.to_owned(),
        engine_payload: encode_payload(payload),
        edit_seq: next_edit_seq(),
        apply_mode: Default::default(),
    }
}

pub fn clip_bounds(world: &EcsWorld, target: &TrackTarget) -> (i32, i32) {
    let (start, end) = world
        .get_timeline_objects()
        .into_iter()
        .find(|o| o.id as usize == target.object_id())
        .map(|o| (o.start_frame, o.end_frame))
        .unwrap_or((0, 1));
    (start, end.max(start + 1))
}

fn base_value(world: &EcsWorld, target: &TrackTarget) -> f32 {
    match target {
        TrackTarget::Object { object_id, key } => world
            .get_transform(*object_id)
            .and_then(|v| v.get_param(key))
            .or_else(|| {
                world
                    .get_audio_params(*object_id)
                    .and_then(|v| v.get_param(key))
            })
            .or_else(|| world.get_text(*object_id).and_then(|v| v.get_param(key)))
            .or_else(|| world.get_shape(*object_id).and_then(|v| v.get_param(key)))
            .unwrap_or_default(),
        TrackTarget::Effect {
            object_id,
            effect_index,
            key,
        } => world
            .get_effects(*object_id)
            .get(*effect_index)
            .and_then(|effect| effect.params.get(key))
            .and_then(|param| match param.static_value {
                Value::Number(value) => Some(value),
                _ => None,
            })
            .unwrap_or_default(),
    }
}

pub fn start_only(world: &EcsWorld, target: &TrackTarget) -> Vec<Keyframe> {
    let (start, _) = clip_bounds(world, target);
    vec![new_keyframe(
        start,
        base_value(world, target),
        &EasingPayload::linear(),
    )]
}

pub fn with_end(
    world: &EcsWorld,
    target: &TrackTarget,
    track: &[Keyframe],
    playhead: i32,
) -> Option<Vec<Keyframe>> {
    let first = track.first()?;
    let (_, clip_end) = clip_bounds(world, target);
    let frame = if playhead > first.frame && playhead <= clip_end {
        playhead
    } else {
        clip_end
    };
    if frame <= first.frame {
        return None;
    }
    let mut keys = track.to_vec();
    keys.push(new_keyframe(frame, first.value, &EasingPayload::linear()));
    Some(keys)
}
