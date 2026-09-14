use crate::ecs::EcsWorld;
use crate::ecs::components::{ParamAccess, ShapeParams, TextContent};
use crate::ecs::effects::EffectStack;
use crate::ecs::types::EffectInstance;
use shipyard::{Get, View, ViewMut};

impl EcsWorld {
    pub fn get_effects(&self, object_id: usize) -> Vec<EffectInstance> {
        let Some(entity) = self.find_entity(object_id) else {
            return Vec::new();
        };
        self.world.run(|stacks: View<EffectStack>| {
            stacks.get(entity).map(|s| s.0.clone()).unwrap_or_default()
        })
    }

    pub fn get_text(&self, object_id: usize) -> Option<TextContent> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|texts: View<TextContent>| texts.get(entity).ok().cloned())
    }

    pub fn set_text(&mut self, object_id: usize, text: String, font_size: f32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut texts: ViewMut<TextContent>| {
            if let Ok(mut slot) = (&mut texts).get(entity) {
                slot.text = text;
                slot.font_size = font_size;
            }
        });
        self.touch();
    }

    pub fn set_text_param(&mut self, object_id: usize, key: &str, value: f32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut texts: ViewMut<TextContent>| {
            if let Ok(mut slot) = (&mut texts).get(entity) {
                ParamAccess::set_param(&mut *slot, key, value);
            }
        });
        self.touch();
    }

    pub fn set_text_font_stack(&mut self, object_id: usize, stack: Vec<String>) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut texts: ViewMut<TextContent>| {
            if let Ok(mut slot) = (&mut texts).get(entity) {
                slot.font_family_stack = if stack.is_empty() {
                    vec![String::new()]
                } else {
                    stack
                };
            }
        });
        self.touch();
    }

    pub fn get_shape(&self, object_id: usize) -> Option<ShapeParams> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|shapes: View<ShapeParams>| shapes.get(entity).ok().copied())
    }
}
