use crate::ecs::EcsWorld;
use crate::ecs::audio_plugins;
use crate::ecs::audio_plugins::PluginChain;
use shipyard::{AddComponent, Get, View, ViewMut};
use std::collections::HashMap;

impl EcsWorld {
    pub fn get_plugin_chain(
        &self,
        object_id: usize,
    ) -> Option<Vec<audio_plugins::PluginInstanceRef>> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|chains: View<PluginChain>| chains.get(entity).ok().map(|c| c.0.clone()))
    }

    pub fn add_audio_plugin(
        &mut self,
        object_id: usize,
        entry: &maolan_host_adapter::PluginCatalogEntry,
        param_info: Vec<maolan_host_adapter::PluginParamInfo>,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            let instance = audio_plugins::PluginInstanceRef {
                instance_uid: 0,
                format: entry.format,
                path: entry.path.clone(),
                plugin_id: entry.plugin_id.clone(),
                bypass: false,
                params: HashMap::new(),
                param_info,
            };
            if let Ok(mut chain) = (&mut chains).get(entity) {
                chain.0.push(instance);
                chain.repair_instance_uids();
            } else {
                let mut chain = PluginChain(vec![instance]);
                chain.repair_instance_uids();
                chains.add_component_unchecked(entity, chain);
            }
        });
    }

    pub fn remove_audio_plugin(&mut self, object_id: usize, index: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                if index < chain.0.len() {
                    chain.0.remove(index);
                }
            }
        });
    }

    pub fn set_audio_plugin_bypass(&mut self, object_id: usize, index: usize, bypass: bool) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                if let Some(inst) = chain.0.get_mut(index) {
                    inst.bypass = bypass;
                }
            }
        });
    }

    pub fn reorder_audio_plugin(&mut self, object_id: usize, from: usize, to: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                if from < chain.0.len() && to < chain.0.len() {
                    let item = chain.0.remove(from);
                    chain.0.insert(to, item);
                }
            }
        });
    }

    pub fn set_audio_plugin_param(
        &mut self,
        object_id: usize,
        index: usize,
        param_id: u32,
        value: f64,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                if let Some(inst) = chain.0.get_mut(index) {
                    inst.params.insert(param_id, value);
                }
            }
        });
    }
}
