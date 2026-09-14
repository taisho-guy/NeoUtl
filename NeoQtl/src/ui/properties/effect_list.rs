use crate::ecs::EcsWorld;

#[derive(Clone, Debug)]
pub struct EffectItem {
    pub index: usize,
    pub effect_id: String,
    pub name: String,
    pub enabled: bool,
}

pub fn get_object_effects(world: &EcsWorld, object_id: usize) -> Vec<EffectItem> {
    let effects = world.get_effects(object_id);
    effects
        .iter()
        .enumerate()
        .map(|(index, inst)| EffectItem {
            index,
            effect_id: inst.effect_id.clone(),
            name: inst.effect_id.clone(),
            enabled: inst.enabled,
        })
        .collect()
}

pub fn toggle_effect_enabled(world: &mut EcsWorld, object_id: usize, effect_index: usize) {
    let effects = world.get_effects(object_id);
    if let Some(e) = effects.get(effect_index) {
        let new_state = !e.enabled;
        world.set_effect_enabled(object_id, effect_index, new_state);
    }
}

pub fn remove_effect(world: &mut EcsWorld, object_id: usize, effect_index: usize) {
    world.remove_effect(object_id, effect_index);
}
