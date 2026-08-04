pub mod easing_editor;
mod effect_list;
mod row;
mod sections;
mod segment;
mod track;

use crate::app_state::{self, SharedAppState};
use crate::ui::effect_add_dialog::EffectAddDialog;
use crate::ui::effect_catalog::EffectCatalogState;

pub struct PropertiesPanel {
    pub open: bool,
    pub effect_add: EffectAddDialog,
    selected: Option<usize>,
    catalog: EffectCatalogState,
}

impl PropertiesPanel {
    pub fn new() -> Self {
        Self {
            open: true,
            effect_add: EffectAddDialog::new(),
            selected: None,
            catalog: EffectCatalogState::build(),
        }
    }

    pub fn show_effect_add(&mut self, ctx: &egui::Context, state: &SharedAppState) {
        if let Some(effect_id) = self.effect_add.show(ctx, &self.catalog) {
            if let Some(id) = self.selected {
                let holder = app_state::active_world(state);
                holder.lock().unwrap().add_effect(id, &effect_id);
            }
            crate::ui::effect_catalog::mark_effect_used(&effect_id);
        }
    }

    pub fn show(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui, state: &SharedAppState) {
        if !self.open {
            return;
        }
        let holder = app_state::active_world(state);
        let mut world = holder.lock().unwrap();
        let objects = world.get_timeline_objects();
        if self.selected.is_none() || !self.selected.is_some_and(|id| world.object_exists(id)) {
            self.selected = objects.first().map(|o| o.id as usize);
        }
        let Some(id) = self.selected else {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading("プロパティ");
                ui.label("オブジェクトを選択してください");
            });
            return;
        };

        egui::Panel::left("properties_effect_sidebar")
            .resizable(true)
            .default_size(180.0)
            .size_range(140.0..=320.0)
            .frame(
                egui::Frame::default()
                    .fill(egui::Color32::from_rgb(0x13, 0x13, 0x18))
                    .inner_margin(6.0),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), "エフェクト");
                    if ui.small_button("＋追加").clicked() {
                        self.effect_add.open();
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("properties_effect_sidebar_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        effect_list::effects_sidebar(ui, &mut world, id);
                    });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("properties_main_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.heading("プロパティ");
                    ui.small(format!("Object {id} / frame {}", world.current_frame()));
                    ui.separator();

                    sections::transform_section(ui, &mut world, id);
                    sections::text_section(ui, &mut world, id);
                    sections::shape_section(ui, &mut world, id);
                    sections::audio_section(ui, &mut world, id);

                    ui.separator();
                    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), "エフェクト詳細");
                    effect_list::effects_section(ui, &mut world, id, &objects);
                });
        });
    }
}
