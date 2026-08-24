use super::TimelineWindow;
use crate::app_state::SharedAppState;
use crate::ui::dialogs::DialogSet;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::SceneTabItem;
use egui_dock::{
    AllowedSplits, DockArea, DockState, NodeIndex, Style, SurfaceIndex, TabIndex, TabViewer,
};
use std::cell::RefCell;
use std::rc::Rc;

struct SceneTabViewer<'a> {
    switch_target: &'a mut Option<i32>,
    rename_target: &'a mut Option<i32>,
    settings_target: &'a mut Option<i32>,
    close_target: &'a mut Option<i32>,
    add_clicked: &'a mut bool,
}

impl<'a> TabViewer for SceneTabViewer<'a> {
    type Tab = SceneTabItem;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new("scene-tab").with(tab.id)
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        (&tab.name).into()
    }

    fn ui(&mut self, _ui: &mut egui::Ui, _tab: &mut Self::Tab) {}

    fn on_tab_button(&mut self, tab: &mut Self::Tab, response: &egui::Response) {
        if response.clicked() {
            *self.switch_target = Some(tab.id);
        }
        if response.double_clicked() {
            *self.rename_target = Some(tab.id);
        }
        if response.secondary_clicked() {
            *self.settings_target = Some(tab.id);
        }
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        tab.id != 0
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> egui_dock::widgets::tab_viewer::OnCloseResponse {
        *self.close_target = Some(tab.id);
        egui_dock::widgets::tab_viewer::OnCloseResponse::Ignore
    }

    fn on_add(&mut self, _node_path: egui_dock::NodePath) {
        *self.add_clicked = true;
    }
}

impl TimelineWindow {
    pub(super) fn scene_tab_bar(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        scene_tabs: &[SceneTabItem],
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        let mut dock_state = DockState::new(scene_tabs.to_vec());
        if let Some(active_idx) = scene_tabs.iter().position(|t| t.active) {
            let _ = dock_state.set_active_tab(egui_dock::TabPath::new(
                SurfaceIndex::main(),
                NodeIndex::root(),
                TabIndex(active_idx),
            ));
        }

        let mut switch_target: Option<i32> = None;
        let mut rename_target: Option<i32> = None;
        let mut settings_target: Option<i32> = None;
        let mut close_target: Option<i32> = None;
        let mut add_clicked = false;
        let mut viewer = SceneTabViewer {
            switch_target: &mut switch_target,
            rename_target: &mut rename_target,
            settings_target: &mut settings_target,
            close_target: &mut close_target,
            add_clicked: &mut add_clicked,
        };

        ui.allocate_ui_with_layout(
            egui::Vec2::new(ui.available_width(), super::SCENE_TAB_HEIGHT),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                DockArea::new(&mut dock_state)
                    .id(egui::Id::new("timeline-scene-tabs"))
                    .style(Style::from_egui(ui.style().as_ref()))
                    .show_add_buttons(true)
                    .show_close_buttons(true)
                    .draggable_tabs(false)
                    .allowed_splits(AllowedSplits::None)
                    .show_inside(ui, &mut viewer);
            },
        );

        if let Some(id) = switch_target {
            self.switch_scene_tab(state, preview_panel, id);
        }
        if let Some(id) = rename_target {
            dialogs.borrow_mut().open_scene_edit(state, id);
        }
        if let Some(id) = settings_target {
            dialogs.borrow_mut().open_scene_edit(state, id);
        }
        if let Some(id) = close_target {
            self.close_scene_tab(state, preview_panel, id);
        }
        if add_clicked {
            dialogs.borrow_mut().open_scene_create(state);
        }
    }
}
