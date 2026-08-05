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
        ui.allocate_ui_with_layout(
            egui::Vec2::new(ui.available_width(), super::SCENE_TAB_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_height(super::SCENE_TAB_HEIGHT);
                for tab in scene_tabs {
                    ui.horizontal(|ui| {
                        let label = ui.selectable_label(tab.active, &tab.name);
                        if label.clicked() {
                            self.switch_scene_tab(state, preview_panel, tab.id);
                        }
                        if label.double_clicked() {
                            dialogs.borrow_mut().open_scene_edit(state, tab.id);
                        }
                        if tab.id != 0 && ui.small_button("✕").clicked() {
                            self.close_scene_tab(state, preview_panel, tab.id);
                        }
                    });
                }
                if ui
                    .add_sized([40.0, super::SCENE_TAB_HEIGHT], egui::Button::new("＋"))
                    .clicked()
                {
                    dialogs.borrow_mut().open_scene_create(state);
                }
            },
        );
    }
}
