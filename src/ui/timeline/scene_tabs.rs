use super::TimelineWindow;
use crate::app_state::SharedAppState;
use crate::ui::dialogs::DialogSet;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::SceneTabItem;
use std::cell::RefCell;
use std::rc::Rc;

/// Slint `timeline.slint` シーンタブバー相当。
impl TimelineWindow {
    pub(super) fn scene_tab_bar(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        scene_tabs: &[SceneTabItem],
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        ui.horizontal(|ui| {
            for tab in scene_tabs {
                ui.horizontal(|ui| {
                    if ui.selectable_label(tab.active, &tab.name).clicked() {
                        self.switch_scene_tab(state, preview_panel, tab.id);
                    }
                    if ui.small_button("⚙").clicked() {
                        dialogs.borrow_mut().open_scene_edit(state, tab.id);
                    }
                    if ui.small_button("✕").clicked() {
                        self.close_scene_tab(state, preview_panel, tab.id);
                    }
                });
            }
            if ui.button("＋").clicked() {
                dialogs.borrow_mut().open_scene_create(state);
            }
        });
    }
}
