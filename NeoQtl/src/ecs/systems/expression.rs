use crate::ecs::components::{ObjectId, PropertyExpressions};
use crate::ecs::effects::EffectStack;
use crate::ecs::transform::Transform;
use neoutl_expression_api::{
    ExpressionEvalContext, ExpressionHostVTable, STANDARD_EXPRESSION_ENGINE_VTABLE,
    bind_expression_host,
};
use neoutl_shared_abi::StrRef;
use shipyard::{Get, IntoIter, View, ViewMut, World};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Once;

#[derive(Default)]
pub struct HostContextData {
    pub frame: i32,
    pub fps: f32,
    pub time_seconds: f64,
    pub properties: HashMap<(usize, String), f32>,
    pub object_layers: HashMap<usize, i32>,
}

thread_local! {
    static CURRENT_CONTEXT: RefCell<Option<HostContextData>> = const { RefCell::new(None) };
}

fn with_context<R>(f: impl FnOnce(&HostContextData) -> R) -> R {
    CURRENT_CONTEXT.with(|ctx| {
        let borrow = ctx.borrow();
        if let Some(c) = borrow.as_ref() {
            f(c)
        } else {
            f(&HostContextData::default())
        }
    })
}

unsafe extern "C" fn host_get_property(object_id: usize, prop_name: StrRef, fallback: f32) -> f32 {
    let name = unsafe { prop_name.as_str() };
    with_context(|ctx| {
        ctx.properties
            .get(&(object_id, name.to_string()))
            .copied()
            .unwrap_or(fallback)
    })
}

unsafe extern "C" fn host_get_time_seconds() -> f64 {
    with_context(|ctx| ctx.time_seconds)
}

unsafe extern "C" fn host_get_frame() -> i32 {
    with_context(|ctx| ctx.frame)
}

unsafe extern "C" fn host_get_fps() -> f32 {
    with_context(|ctx| ctx.fps)
}

unsafe extern "C" fn host_get_object_layer(object_id: usize) -> i32 {
    with_context(|ctx| ctx.object_layers.get(&object_id).copied().unwrap_or(0))
}

static HOST_VTABLE: ExpressionHostVTable = ExpressionHostVTable {
    get_property: host_get_property,
    get_time_seconds: host_get_time_seconds,
    get_frame: host_get_frame,
    get_fps: host_get_fps,
    get_object_layer: host_get_object_layer,
};

static INIT_ONCE: Once = Once::new();

pub fn ensure_expression_host_bound() {
    INIT_ONCE.call_once(|| {
        bind_expression_host(&STANDARD_EXPRESSION_ENGINE_VTABLE, &HOST_VTABLE);
    });
}

#[allow(dead_code)]
pub fn evaluate_expressions_for_world(world: &mut World, frame: i32, fps: f32) -> usize {
    ensure_expression_host_bound();

    let fps = if fps <= 0.0 { 30.0 } else { fps };
    let time_seconds = frame as f64 / fps as f64;

    let mut ctx_data = HostContextData {
        frame,
        fps,
        time_seconds,
        properties: HashMap::new(),
        object_layers: HashMap::new(),
    };

    world.run(
        |ids: View<ObjectId>,
         transforms: View<Transform>,
         layers: View<crate::ecs::components::Layer>,
         stacks: View<EffectStack>| {
            for (entity, id) in ids.iter().with_id() {
                let obj_id = id.0;
                if let Ok(layer) = layers.get(entity) {
                    ctx_data.object_layers.insert(obj_id, layer.0);
                }
                if let Ok(tr) = transforms.get(entity) {
                    ctx_data.properties.insert((obj_id, "X".to_string()), tr.x);
                    ctx_data.properties.insert((obj_id, "Y".to_string()), tr.y);
                    ctx_data.properties.insert((obj_id, "Z".to_string()), tr.z);
                    ctx_data
                        .properties
                        .insert((obj_id, "Rotation".to_string()), tr.rot_z);
                    ctx_data
                        .properties
                        .insert((obj_id, "ScaleX".to_string()), tr.scale_x);
                    ctx_data
                        .properties
                        .insert((obj_id, "ScaleY".to_string()), tr.scale_y);
                    ctx_data
                        .properties
                        .insert((obj_id, "ScaleZ".to_string()), 1.0);
                }
                if let Ok(stack) = stacks.get(entity) {
                    for inst in &stack.0 {
                        for (key, param) in &inst.params {
                            if let crate::ecs::types::Value::Number(val) = param.static_value {
                                ctx_data.properties.insert((obj_id, key.clone()), val);
                            }
                        }
                    }
                }
            }
        },
    );

    let mut evaluated_count = 0;

    CURRENT_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some(ctx_data);
    });

    let mut updates: Vec<(usize, String, f32)> = Vec::new();

    world.run(
        |ids: View<ObjectId>, expressions: View<PropertyExpressions>| {
            for (entity, id) in ids.iter().with_id() {
                let obj_id = id.0;
                if let Ok(exprs) = expressions.get(entity) {
                    for (prop_key, binding) in &exprs.bindings {
                        if !binding.enabled || binding.compiled_handle == 0 {
                            continue;
                        }

                        let cur_val = with_context(|c| {
                            c.properties
                                .get(&(obj_id, prop_key.clone()))
                                .copied()
                                .unwrap_or(0.0)
                        });

                        let eval_ctx = ExpressionEvalContext {
                            object_id: obj_id,
                            frame,
                            time_seconds,
                            current_value: cur_val,
                        };

                        let new_val = unsafe {
                            (STANDARD_EXPRESSION_ENGINE_VTABLE.evaluate)(
                                binding.compiled_handle,
                                &eval_ctx,
                            )
                        };

                        updates.push((obj_id, prop_key.clone(), new_val));
                    }
                }
            }
        },
    );

    CURRENT_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = None;
    });

    for (obj_id, prop_key, new_val) in updates {
        evaluated_count += 1;
        world.run(
            |ids: View<ObjectId>,
             mut transforms: ViewMut<Transform>,
             mut stacks: ViewMut<EffectStack>| {
                for (entity, id) in ids.iter().with_id() {
                    if id.0 == obj_id {
                        if let Ok(mut tr) = (&mut transforms).get(entity) {
                            match prop_key.as_str() {
                                "X" | "x" => tr.x = new_val,
                                "Y" | "y" => tr.y = new_val,
                                "Z" | "z" => tr.z = new_val,
                                "Rotation" | "rot_z" => tr.rot_z = new_val,
                                "ScaleX" | "scale_x" => tr.scale_x = new_val,
                                "ScaleY" | "scale_y" => tr.scale_y = new_val,
                                _ => {}
                            }
                        }

                        if let Ok(mut stack) = (&mut stacks).get(entity) {
                            for inst in &mut stack.0 {
                                if let Some(param) = inst.params.get_mut(&prop_key) {
                                    param.static_value = crate::ecs::types::Value::Number(new_val);
                                }
                            }
                        }
                        break;
                    }
                }
            },
        );
    }

    evaluated_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::EcsWorld;
    use crate::ecs::components::PropertyExpressions;
    use shipyard::AddComponent;

    #[test]
    fn test_expression_system_evaluates_and_updates_transform() {
        let mut ecs_world = EcsWorld::new();
        let obj_id = ecs_world.add_shape_object(0, 0, 100, 0, Default::default());

        ecs_world.set_transform(
            obj_id,
            Transform {
                x: 10.0,
                y: 20.0,
                ..Default::default()
            },
        );

        ecs_world
            .world
            .run(|mut exprs: ViewMut<PropertyExpressions>| {
                let entity = ecs_world.find_entity(obj_id).unwrap();
                let mut prop_expr = PropertyExpressions::new();
                prop_expr.set_expression("Y", "prop('X') * 2.5 + time * 10", true);
                let _ = exprs.add_component_unchecked(entity, prop_expr);
            });

        let count = evaluate_expressions_for_world(&mut ecs_world.world, 60, 30.0);
        assert_eq!(count, 1);

        let tr = ecs_world.get_transform(obj_id).unwrap();
        assert_eq!(tr.x, 10.0);
        assert!((tr.y - 45.0).abs() < 1e-4);
    }

    #[test]
    fn test_expression_system_evaluates_and_updates_effect_params() {
        let mut ecs_world = EcsWorld::new();
        let obj_id = ecs_world.add_shape_object(0, 0, 100, 0, Default::default());

        ecs_world.world.run(|mut stacks: ViewMut<EffectStack>| {
            let entity = ecs_world.find_entity(obj_id).unwrap();
            let mut inst = crate::ecs::types::EffectInstance::new("test.blur");
            inst.params.insert(
                "Radius".to_string(),
                crate::ecs::types::EffectParam::new(crate::ecs::types::Value::Number(5.0)),
            );
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.0.push(inst);
            }
        });

        ecs_world
            .world
            .run(|mut exprs: ViewMut<PropertyExpressions>| {
                let entity = ecs_world.find_entity(obj_id).unwrap();
                let mut prop_expr = PropertyExpressions::new();
                prop_expr.set_expression("Radius", "sin(time) * 20 + 25", true);
                let _ = exprs.add_component_unchecked(entity, prop_expr);
            });

        let count = evaluate_expressions_for_world(&mut ecs_world.world, 0, 30.0);
        assert_eq!(count, 1);
        let val = ecs_world.effect_param_f32(obj_id, 0, "Radius").unwrap();
        assert!((val - 25.0).abs() < 1e-4);
    }
}
