use super::fields::{choice_field, int_field, toggle_field};
use super::window::SystemSettingsWindow;
use crate::ecs::EcsWorld;
use elegance::ThemeSwitcher;
use std::sync::{Arc, Mutex};

impl SystemSettingsWindow {
    pub(super) fn page_general(&mut self, ui: &mut egui::Ui, world_holder: &Arc<Mutex<EcsWorld>>) {
        let mut autosave_enabled = self.autosave_enabled;
        let mut autosave_interval_sec = self.autosave_interval_sec;
        let changed = toggle_field(ui, "自動保存を有効化", &mut autosave_enabled)
            | int_field(
                ui,
                "自動保存間隔（秒）",
                &mut autosave_interval_sec,
                10,
                3600,
            );
        if changed {
            self.autosave_enabled = autosave_enabled;
            self.autosave_interval_sec = autosave_interval_sec.clamp(10, 86_400);
            let (enabled, interval) = (self.autosave_enabled, self.autosave_interval_sec);
            self.persist(world_holder, |s| {
                s.autosave_enabled = enabled;
                s.autosave_interval_sec = interval;
            });
        }
    }

    pub(super) fn page_appearance(
        &mut self,
        ui: &mut egui::Ui,
        world_holder: &Arc<Mutex<EcsWorld>>,
    ) {
        ui.label(t!("テーマ"));
        let resp = ui.add(ThemeSwitcher::new(&mut self.theme_choice).auto_install(false));
        if resp.changed() {
            crate::app::theme::set(self.theme_choice);
            let id = crate::app::theme::id_of(self.theme_choice).to_string();
            self.persist(world_holder, |s| s.theme_id = id);
        }
        ui.end_row();

        let mut ui_scale_percent = self.ui_scale_percent;
        if int_field(ui, "UIスケール（%）", &mut ui_scale_percent, 50, 200) {
            self.ui_scale_percent = ui_scale_percent;
            let scale = self.ui_scale_percent;
            self.persist(world_holder, |s| s.ui_scale_percent = scale);
        }

        let mut easing_engine_index = self.easing_engine_index;
        if choice_field(
            ui,
            "イージングエンジン",
            &self.easing_engine_names,
            &mut easing_engine_index,
        ) {
            self.easing_engine_index = easing_engine_index;
            if let Some(id) = self
                .easing_engine_ids
                .get(easing_engine_index as usize)
                .cloned()
            {
                self.persist(world_holder, |s| s.easing_engine_id.clone_from(&id));
            }
        }
    }
}
