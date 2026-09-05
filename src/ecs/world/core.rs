use crate::ecs::EcsWorld;
use crate::ecs::resources;
use crate::ecs::resources::{
    LayerStates, ProjectResource, SceneResource, SystemSettingsResource, TimelineResource,
};
use crate::ecs::transform::Camera;
use shipyard::World;

impl EcsWorld {
    pub fn new() -> Self {
        let world = World::new();
        world.add_unique(TimelineResource::new());
        world.add_unique(ProjectResource::new());
        world.add_unique(LayerStates::new(resources::DEFAULT_LAYER_COUNT));
        world.add_unique(SceneResource::new());
        world.add_unique(SystemSettingsResource::new());
        world.add_unique(Camera::default());
        Self {
            world,
            selected_ids: std::collections::HashSet::new(),
            revision: 0,
        }
    }

    pub(crate) fn touch(&mut self) {
        self.revision += 1;
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn set_selected_ids(&mut self, ids: std::collections::HashSet<usize>) {
        self.selected_ids = ids;
    }

    pub fn is_selected(&self, id: usize) -> bool {
        self.selected_ids.contains(&id)
    }
}
