use crate::ecs::EcsWorld;
use crate::ecs::components::ParamAccess;

#[derive(Clone, Debug)]
pub struct ParamProperty {
    pub key: String,
    pub label: String,
    pub current_value: f32,
    pub min: f32,
    pub max: f32,
    pub has_keyframes: bool,
}

pub fn get_transform_params(world: &EcsWorld, id: usize) -> Vec<ParamProperty> {
    let mut params = Vec::new();
    let transform = world.get_transform(id);
    for (key, label, min, max) in [
        ("x", "X", -3840.0, 3840.0),
        ("y", "Y", -2160.0, 2160.0),
        ("z", "Z", -10000.0, 10000.0),
        ("scale_x", "拡大率X", 0.0, 1000.0),
        ("scale_y", "拡大率Y", 0.0, 1000.0),
        ("rotation_z", "回転", -3600.0, 3600.0),
        ("opacity", "透明度", 0.0, 100.0),
    ] {
        let current_value = transform
            .as_ref()
            .and_then(|t| t.get_param(key))
            .unwrap_or(0.0);
        let keyframes = world.get_keyframes(id, key);
        params.push(ParamProperty {
            key: key.into(),
            label: label.into(),
            current_value,
            min,
            max,
            has_keyframes: !keyframes.is_empty(),
        });
    }
    params
}

pub fn set_object_param(world: &mut EcsWorld, id: usize, key: &str, val: f32) {
    if let Some(mut t) = world.get_transform(id) {
        t.set_param(key, val);
        world.set_transform(id, t);
    }
}
