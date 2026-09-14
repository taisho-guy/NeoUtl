use crate::ecs::EcsWorld;
use crate::ecs::components::{BlendMode, ClipMode, ClipTarget, Layer, ParentRef, TimeRemap};
use shipyard::{Get, View};

impl EcsWorld {
    pub fn set_parent(&mut self, object_id: usize, parent_id: Option<usize>) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        match parent_id {
            Some(pid) => {
                if pid == object_id || self.creates_parent_cycle(object_id, pid) {
                    return;
                }
                self.world.add_component(entity, ParentRef(pid));
            }
            None => {
                self.world.remove::<(ParentRef,)>(entity);
            }
        }
        self.recompute_global_matrices();
        self.touch();
    }

    pub fn parent_of(&self, object_id: usize) -> Option<usize> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|parents: View<ParentRef>| parents.get(entity).ok().map(|p| p.0))
    }

    fn creates_parent_cycle(&self, object_id: usize, candidate_parent: usize) -> bool {
        const MAX_CHAIN: u32 = 4096;
        let mut current = Some(candidate_parent);
        let mut depth = 0u32;
        while let Some(id) = current {
            if id == object_id {
                return true;
            }
            depth += 1;
            if depth > MAX_CHAIN {
                return true;
            }
            current = self.parent_of(id);
        }
        false
    }

    pub fn set_track_matte_by_id(
        &mut self,
        target_id: usize,
        source_id: Option<usize>,
        mode: ClipMode,
    ) {
        let Some(source_id) = source_id else {
            self.set_clip_target(
                target_id,
                ClipTarget {
                    enabled: false,
                    ..ClipTarget::default()
                },
            );
            return;
        };
        let Some((target_entity, source_entity)) =
            self.find_entity(target_id).zip(self.find_entity(source_id))
        else {
            return;
        };
        let Some((target_layer, source_layer)) = self.world.run(|layers: View<Layer>| {
            layers
                .get(target_entity)
                .ok()
                .map(|l| l.0)
                .zip(layers.get(source_entity).ok().map(|l| l.0))
        }) else {
            return;
        };
        let mut ct = ClipTarget {
            enabled: true,
            mode,
            ..ClipTarget::default()
        };
        if source_layer > target_layer {
            ct.layer_count_down = (source_layer - target_layer) as u32;
            ct.layer_count_up = 0;
        } else {
            ct.layer_count_up = (target_layer - source_layer) as u32;
            ct.layer_count_down = 0;
        }
        self.set_clip_target(target_id, ct);
    }

    pub fn set_blend_mode(&mut self, object_id: usize, mode: BlendMode) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.add_component(entity, mode);
        self.touch();
    }

    pub fn blend_mode_of(&self, object_id: usize) -> BlendMode {
        let Some(entity) = self.find_entity(object_id) else {
            return BlendMode::default();
        };
        self.world
            .run(|modes: View<BlendMode>| modes.get(entity).copied().unwrap_or_default())
    }

    pub fn set_time_remap(&mut self, object_id: usize, remap: TimeRemap) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.add_component(entity, remap);
        self.touch();
    }

    pub fn time_remap_of(&self, object_id: usize) -> TimeRemap {
        let Some(entity) = self.find_entity(object_id) else {
            return TimeRemap::default();
        };
        self.world
            .run(|remaps: View<TimeRemap>| remaps.get(entity).cloned().unwrap_or_default())
    }
}
