use neoutl_shared_abi::PropertyWriteback;
#[allow(unused_imports)]
use shipyard::{Get, IntoIter, Unique, UniqueView, UniqueViewMut, View, ViewMut, World};

pub trait HistoryCommand: Send + Sync {
    #[allow(dead_code)]
    fn description(&self) -> &str {
        "Command"
    }
    fn apply(&mut self, world: &mut World);
    fn revert(&mut self, world: &mut World);
}

#[derive(Unique)]
pub struct HistoryStack {
    done: Vec<Box<dyn HistoryCommand>>,
    undone: Vec<Box<dyn HistoryCommand>>,
    max_depth: usize,
}

#[allow(dead_code)]
impl HistoryStack {
    pub fn new() -> Self {
        Self::with_max_depth(crate::config::UNDO_HISTORY_LIMIT)
    }

    pub fn with_max_depth(max_depth: usize) -> Self {
        Self {
            done: Vec::new(),
            undone: Vec::new(),
            max_depth: max_depth.max(1),
        }
    }

    pub fn push(&mut self, command: Box<dyn HistoryCommand>) {
        self.done.push(command);
        if self.done.len() > self.max_depth {
            self.done.remove(0);
        }
        self.undone.clear();
    }

    pub fn pop_done(&mut self) -> Option<Box<dyn HistoryCommand>> {
        self.done.pop()
    }

    pub fn pop_undone(&mut self) -> Option<Box<dyn HistoryCommand>> {
        self.undone.pop()
    }

    pub fn push_done(&mut self, command: Box<dyn HistoryCommand>) {
        self.done.push(command);
        if self.done.len() > self.max_depth {
            self.done.remove(0);
        }
    }

    pub fn push_undone(&mut self, command: Box<dyn HistoryCommand>) {
        self.undone.push(command);
    }

    pub fn undo(&mut self, world: &mut World) -> bool {
        if let Some(mut command) = self.done.pop() {
            command.revert(world);
            self.undone.push(command);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, world: &mut World) -> bool {
        if let Some(mut command) = self.undone.pop() {
            command.apply(world);
            self.done.push(command);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    pub fn done_len(&self) -> usize {
        self.done.len()
    }

    pub fn undone_len(&self) -> usize {
        self.undone.len()
    }

    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }
}

impl Default for HistoryStack {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct PropertyChangeCommand {
    pub object_id: usize,
    pub effect_index: usize,
    pub key: String,
    pub old_value: f32,
    pub new_value: f32,
    #[allow(dead_code)]
    pub desc: String,
}

impl PropertyChangeCommand {
    pub fn new(
        object_id: usize,
        effect_index: usize,
        key: impl Into<String>,
        old_value: f32,
        new_value: f32,
    ) -> Self {
        let key = key.into();
        let desc = format!("Change {} to {}", key, new_value);
        Self {
            object_id,
            effect_index,
            key,
            old_value,
            new_value,
            desc,
        }
    }
}

impl HistoryCommand for PropertyChangeCommand {
    fn description(&self) -> &str {
        &self.desc
    }

    fn apply(&mut self, world: &mut World) {
        set_param_internal(
            world,
            self.object_id,
            self.effect_index,
            &self.key,
            self.new_value,
        );
    }

    fn revert(&mut self, world: &mut World) {
        set_param_internal(
            world,
            self.object_id,
            self.effect_index,
            &self.key,
            self.old_value,
        );
    }
}

use crate::ecs::components::ObjectId;

fn set_param_internal(
    world: &mut World,
    object_id: usize,
    effect_index: usize,
    key: &str,
    value: f32,
) {
    world.run(
        |ids: View<ObjectId>, mut stacks: ViewMut<crate::ecs::effects::EffectStack>| {
            for (entity, id) in ids.iter().with_id() {
                if id.0 == object_id {
                    if let Ok(mut stack) = (&mut stacks).get(entity) {
                        if let Some(inst) = stack.0.get_mut(effect_index) {
                            if let Some(param) = inst.params.get_mut(key) {
                                param.static_value = crate::ecs::types::Value::Number(value);
                            }
                        }
                    }
                    break;
                }
            }
        },
    );
}

#[allow(dead_code)]
pub fn process_writeback_items(
    world: &mut World,
    object_id: usize,
    effect_index: usize,
    items: &[PropertyWriteback],
) -> usize {
    let mut pushed_count = 0;

    for item in items {
        let key_str = unsafe { item.key.as_str() };
        if key_str.is_empty() {
            continue;
        }

        let old_val = world.run(
            |ids: View<ObjectId>, stacks: View<crate::ecs::effects::EffectStack>| {
                for (entity, id) in ids.iter().with_id() {
                    if id.0 == object_id {
                        if let Ok(stack) = stacks.get(entity) {
                            if let Some(inst) = stack.0.get(effect_index) {
                                if let Some(param) = inst.params.get(key_str) {
                                    if let crate::ecs::types::Value::Number(val) =
                                        param.static_value
                                    {
                                        return Some(val);
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
                None
            },
        );

        let Some(old_val) = old_val else {
            continue;
        };

        if (old_val - item.value).abs() <= 1e-6 {
            continue;
        }

        set_param_internal(world, object_id, effect_index, key_str, item.value);

        if item.is_user_action != 0 {
            let cmd = Box::new(PropertyChangeCommand::new(
                object_id,
                effect_index,
                key_str,
                old_val,
                item.value,
            ));
            world.run(|mut stack: UniqueViewMut<HistoryStack>| {
                stack.push(cmd);
            });
            pushed_count += 1;
        }
    }

    pushed_count
}

#[allow(dead_code)]
pub fn poll_all_effect_writebacks(world: &mut World) -> usize {
    let targets: Vec<(usize, usize, String)> = world.run(
        |ids: View<ObjectId>, stacks: View<crate::ecs::effects::EffectStack>| {
            let mut list = Vec::new();
            for (entity, id) in ids.iter().with_id() {
                if let Ok(stack) = stacks.get(entity) {
                    for (eff_idx, inst) in stack.0.iter().enumerate() {
                        list.push((id.0, eff_idx, inst.effect_id.clone()));
                    }
                }
            }
            list
        },
    );

    let mut total_pushed = 0;
    const CAPACITY: usize = 16;
    let mut buffer = [PropertyWriteback {
        key: neoutl_shared_abi::StrRef::empty(),
        value: 0.0,
        is_user_action: 0,
    }; CAPACITY];

    for (object_id, effect_index, effect_id) in targets {
        let Some(source) = crate::effects::loader::by_id(&effect_id) else {
            continue;
        };
        let poll_fn = match source.as_ref() {
            crate::effects::loader::EffectSource::Native(plugin) => plugin.vtable.poll_writeback,
            crate::effects::loader::EffectSource::Lua(_) => None,
        };

        let Some(poll_fn) = poll_fn else {
            continue;
        };

        let count = unsafe { poll_fn(buffer.as_mut_ptr(), CAPACITY as u32) };
        if count > 0 {
            let valid_count = (count as usize).min(CAPACITY);
            let pushed =
                process_writeback_items(world, object_id, effect_index, &buffer[..valid_count]);
            total_pushed += pushed;
        }
    }

    total_pushed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::EcsWorld;

    #[test]
    fn history_stack_push_undo_redo_max_depth() {
        let mut world = World::new();
        let mut stack = HistoryStack::with_max_depth(2);

        #[derive(Default, Unique)]
        struct Counter(i32);
        world.add_unique(Counter(0));

        struct AddCmd(i32);
        impl HistoryCommand for AddCmd {
            fn apply(&mut self, world: &mut World) {
                world.run(|mut c: UniqueViewMut<Counter>| c.0 += self.0);
            }
            fn revert(&mut self, world: &mut World) {
                world.run(|mut c: UniqueViewMut<Counter>| c.0 -= self.0);
            }
        }

        let mut cmd1 = AddCmd(10);
        cmd1.apply(&mut world);
        stack.push(Box::new(cmd1));

        let mut cmd2 = AddCmd(20);
        cmd2.apply(&mut world);
        stack.push(Box::new(cmd2));

        assert_eq!(world.run(|c: UniqueView<Counter>| c.0), 30);
        assert_eq!(stack.done_len(), 2);

        assert!(stack.undo(&mut world));
        assert_eq!(world.run(|c: UniqueView<Counter>| c.0), 10);
        assert_eq!(stack.done_len(), 1);
        assert_eq!(stack.undone_len(), 1);

        assert!(stack.redo(&mut world));
        assert_eq!(world.run(|c: UniqueView<Counter>| c.0), 30);
        assert_eq!(stack.done_len(), 2);
        assert_eq!(stack.undone_len(), 0);

        let mut cmd3 = AddCmd(30);
        cmd3.apply(&mut world);
        stack.push(Box::new(cmd3));

        let mut cmd4 = AddCmd(40);
        cmd4.apply(&mut world);
        stack.push(Box::new(cmd4));

        assert_eq!(stack.done_len(), 2);
    }

    #[test]
    fn poll_writeback_records_user_action_and_skips_non_user_action() {
        let mut ecs_world = EcsWorld::new();
        let obj_id = ecs_world.add_shape_object(0, 0, 100, 0, Default::default());

        let effect_id = "test.effect.writeback";
        ecs_world
            .world
            .run(|mut stacks: ViewMut<crate::ecs::effects::EffectStack>| {
                if let Some(entity) = ecs_world.find_entity(obj_id) {
                    if let Ok(mut stack) = (&mut stacks).get(entity) {
                        let mut inst = crate::ecs::types::EffectInstance::new(effect_id);
                        inst.params.insert(
                            "Blur".to_string(),
                            crate::ecs::types::EffectParam::new(crate::ecs::types::Value::Number(
                                5.0,
                            )),
                        );
                        inst.params.insert(
                            "Gain".to_string(),
                            crate::ecs::types::EffectParam::new(crate::ecs::types::Value::Number(
                                1.0,
                            )),
                        );
                        stack.0.push(inst);
                    }
                }
            });

        let non_user_item = PropertyWriteback {
            key: neoutl_shared_abi::StrRef::from_str("Gain"),
            value: 2.5,
            is_user_action: 0,
        };
        let pushed = process_writeback_items(&mut ecs_world.world, obj_id, 0, &[non_user_item]);
        assert_eq!(pushed, 0);
        assert_eq!(ecs_world.effect_param_f32(obj_id, 0, "Gain"), Some(2.5));
        assert_eq!(ecs_world.history_done_len(), 0);

        let user_item = PropertyWriteback {
            key: neoutl_shared_abi::StrRef::from_str("Blur"),
            value: 15.0,
            is_user_action: 1,
        };
        let pushed_user = process_writeback_items(&mut ecs_world.world, obj_id, 0, &[user_item]);
        assert_eq!(pushed_user, 1);
        assert_eq!(ecs_world.effect_param_f32(obj_id, 0, "Blur"), Some(15.0));
        assert_eq!(ecs_world.history_done_len(), 1);

        assert!(ecs_world.undo());
        assert_eq!(ecs_world.effect_param_f32(obj_id, 0, "Blur"), Some(5.0));
        assert_eq!(ecs_world.effect_param_f32(obj_id, 0, "Gain"), Some(2.5));
        assert_eq!(ecs_world.history_done_len(), 0);
        assert_eq!(ecs_world.history_undone_len(), 1);

        assert!(ecs_world.redo());
        assert_eq!(ecs_world.effect_param_f32(obj_id, 0, "Blur"), Some(15.0));
        assert_eq!(ecs_world.history_done_len(), 1);
        assert_eq!(ecs_world.history_undone_len(), 0);
    }
}
