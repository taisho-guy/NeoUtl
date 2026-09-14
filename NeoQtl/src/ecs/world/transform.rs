use crate::ecs::EcsWorld;
use crate::ecs::components::{ObjectId, ParentRef};
use crate::ecs::transform::{Camera, GlobalMatrix, Transform, compute_global_matrix, mat4_mul};
use shipyard::{EntityId, Get, IntoIter, UniqueViewMut, View, ViewMut};
use std::collections::HashMap;

impl EcsWorld {
    #[cfg(test)]
    pub fn set_transform_param(&mut self, object_id: usize, key: &str, value: f32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(
            |mut transforms: ViewMut<Transform>, mut matrices: ViewMut<GlobalMatrix>| {
                if let Ok(mut slot) = (&mut transforms).get(entity) {
                    crate::ecs::components::ParamAccess::set_param(&mut *slot, key, value);
                    if let Ok(mut matrix) = (&mut matrices).get(entity) {
                        *matrix = compute_global_matrix(&slot);
                    }
                }
            },
        );
    }

    pub fn get_transform(&self, object_id: usize) -> Option<Transform> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|transforms: View<Transform>| transforms.get(entity).ok().copied())
    }

    pub fn set_transform(&mut self, object_id: usize, t: Transform) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(
            |mut transforms: ViewMut<Transform>, mut matrices: ViewMut<GlobalMatrix>| {
                if let Ok(mut slot) = (&mut transforms).get(entity) {
                    *slot = t;
                }
                if let Ok(mut matrix) = (&mut matrices).get(entity) {
                    *matrix = compute_global_matrix(&t);
                }
            },
        );
        self.touch();
    }

    pub fn recompute_global_matrices(&mut self) {
        self.world.run(
            |transforms: View<Transform>, mut matrices: ViewMut<GlobalMatrix>| {
                for (entity, t) in transforms.iter().with_id() {
                    if let Ok(mut matrix) = (&mut matrices).get(entity) {
                        *matrix = compute_global_matrix(t);
                    }
                }
            },
        );
        self.apply_parent_chains();
    }

    fn apply_parent_chains(&mut self) {
        const MAX_DEPTH: u32 = 16;
        let local: HashMap<EntityId, [f32; 16]> = self.world.run(|matrices: View<GlobalMatrix>| {
            matrices.iter().with_id().map(|(e, m)| (e, m.0)).collect()
        });
        let object_ids: HashMap<EntityId, usize> = self
            .world
            .run(|ids: View<ObjectId>| ids.iter().with_id().map(|(e, id)| (e, id.0)).collect());
        let entities_by_object_id: HashMap<usize, EntityId> =
            object_ids.iter().map(|(&e, &id)| (id, e)).collect();
        let parents: HashMap<EntityId, EntityId> =
            self.world.run(|parent_refs: View<ParentRef>| {
                parent_refs
                    .iter()
                    .with_id()
                    .filter_map(|(e, p)| entities_by_object_id.get(&p.0).map(|&pe| (e, pe)))
                    .collect()
            });

        let mut resolved: HashMap<EntityId, [f32; 16]> = HashMap::new();
        fn resolve(
            entity: EntityId,
            local: &HashMap<EntityId, [f32; 16]>,
            parents: &HashMap<EntityId, EntityId>,
            resolved: &mut HashMap<EntityId, [f32; 16]>,
            depth: u32,
        ) -> [f32; 16] {
            if let Some(m) = resolved.get(&entity) {
                return *m;
            }
            let own = local.get(&entity).copied().unwrap_or([
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ]);
            let world_matrix = match parents.get(&entity) {
                Some(&parent) if depth < MAX_DEPTH => {
                    let parent_world = resolve(parent, local, parents, resolved, depth + 1);
                    mat4_mul(&parent_world, &own)
                }
                _ => own,
            };
            resolved.insert(entity, world_matrix);
            world_matrix
        }

        for &entity in parents.keys() {
            resolve(entity, &local, &parents, &mut resolved, 0);
        }

        if resolved.is_empty() {
            return;
        }
        self.world.run(|mut matrices: ViewMut<GlobalMatrix>| {
            for (entity, m) in &resolved {
                if let Ok(mut slot) = (&mut matrices).get(*entity) {
                    slot.0 = *m;
                }
            }
        });
    }

    pub fn set_camera(&mut self, camera: Camera) {
        self.world
            .run(|mut slot: UniqueViewMut<Camera>| *slot = camera);
    }
}
