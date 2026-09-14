pub mod easing_editor;
pub mod effect_list;
pub mod row;
pub mod sections;
pub mod segment;
pub mod track;

use crate::app_state::{self, SharedAppState};
use crate::ui::effect_add_dialog::EffectAddDialog;
use crate::ui::effect_catalog::EffectCatalogState;

pub struct PropertiesPanel {
    pub open: bool,
    pub effect_add: EffectAddDialog,
    pub selected: Option<usize>,
}

impl PropertiesPanel {
    pub fn new() -> Self {
        Self {
            open: true,
            effect_add: EffectAddDialog::new(),
            selected: None,
        }
    }

    pub fn select_object(&mut self, id: usize) {
        self.selected = Some(id);
    }

    pub fn add_effect(&mut self, state: &SharedAppState, effect_id: &str) {
        let Some(id) = self.selected else { return };
        let holder = app_state::active_world(state);
        let mut world = holder.lock().unwrap();

        if let Some(plugin_entry) = crate::audio::plugin_registry::find_by_id_or_path(effect_id) {
            let mixer_holder = crate::app_state::active_audio_mixer(state);
            let mut mixer = mixer_holder.lock().unwrap();
            let param_info = mixer.probe_plugin_param_info(
                plugin_entry.format,
                &plugin_entry.path,
                &plugin_entry.plugin_id,
            );
            drop(mixer);
            world.add_audio_plugin(id, &plugin_entry, param_info);
        } else {
            world.add_effect(id, effect_id);
        }
        crate::ui::effect_catalog::mark_effect_used(effect_id);
    }
}
