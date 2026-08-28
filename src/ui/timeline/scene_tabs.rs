use super::TimelineWindow;
use crate::app_state::SharedAppState;
use crate::ui::dialogs::DialogSet;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::SceneTabItem;
use elegance::{BrowserTab, BrowserTabs, BrowserTabsEvent};
use std::cell::RefCell;
use std::rc::Rc;

const ROOT_SCENE_ID: i32 = 0;

impl TimelineWindow {
    pub(super) fn scene_tab_bar(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        scene_tabs: &[SceneTabItem],
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        let mut tabs = BrowserTabs::new("timeline-scene-tabs").show_new_button(true);
        for tab in scene_tabs {
            tabs = tabs.with_tab(BrowserTab::new(tab.id.to_string(), tab.name.clone()));
        }
        if let Some(active) = scene_tabs.iter().find(|tab| tab.active) {
            tabs.set_selected(active.id.to_string());
        }

        let response = tabs.show(ui);
        let hovered = response.hovered();
        let double_clicked = ui.input(|input| {
            input
                .pointer
                .button_double_clicked(egui::PointerButton::Primary)
        });

        let mut switch_target: Option<i32> = None;
        let mut close_target: Option<i32> = None;
        let mut add_clicked = false;
        for event in tabs.take_events() {
            match event {
                BrowserTabsEvent::Activated(id) => switch_target = id.parse().ok(),
                BrowserTabsEvent::Closed(id) => close_target = id.parse().ok(),
                BrowserTabsEvent::NewRequested => add_clicked = true,
            }
        }

        let active_id = tabs.selected().and_then(|id| id.parse::<i32>().ok());
        let rename_target = if hovered && double_clicked {
            active_id
        } else {
            None
        };

        if let Some(id) = switch_target {
            self.switch_scene_tab(state, preview_panel, id);
        }
        if let Some(id) = rename_target {
            dialogs.borrow_mut().open_scene_edit(state, id);
        }
        if let Some(id) = close_target {
            if id != ROOT_SCENE_ID {
                self.close_scene_tab(state, preview_panel, id);
            }
        }
        if add_clicked {
            dialogs.borrow_mut().open_scene_create(state);
        }
    }
}
