use crate::app_state::SharedAppState;
use crate::ecs::EcsWorld;
use crate::ui::export_dialog::ExportDialog;
use crate::ui::keybindings::KeybindingsWindow;
use crate::ui::preview::PreviewPanel;
use crate::ui::project_settings::ProjectSettingsWindow;
use crate::ui::scene_settings::SceneSettingsWindow;
use crate::ui::system_settings::SystemSettingsWindow;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// フェーズ2でegui-native化済みの設定系ダイアログ一式。開閉は各構造体の`open`
/// フィールドまたは`open()`/`open_for_edit()`が単一の起点であり、PreviewPanelの
/// メニューは開要求フラグを立てるのみでダイアログ本体の状態は持たない。
pub struct DialogSet {
    pub system_settings: SystemSettingsWindow,
    pub project_settings: ProjectSettingsWindow,
    pub scene_settings: SceneSettingsWindow,
    pub keybindings: KeybindingsWindow,
    pub export_dialog: ExportDialog,
    /// 未保存プロジェクトタブを閉じる際の確認待ちセッションindex。
    pub confirm_close_session: Option<usize>,
}

impl DialogSet {
    pub fn new(world_holder: Arc<Mutex<EcsWorld>>) -> Self {
        Self {
            system_settings: SystemSettingsWindow::new(&world_holder),
            project_settings: ProjectSettingsWindow::new(),
            scene_settings: SceneSettingsWindow::new(),
            keybindings: KeybindingsWindow::new(),
            export_dialog: ExportDialog::new(),
            confirm_close_session: None,
        }
    }

    /// プロジェクトタブを閉じる唯一の受け口。dirtyでなければ即時close_session、
    /// dirtyならconfirm_close_sessionへ退避しUI側の確認ダイアログ表示を待つ。
    pub fn request_close_session(&mut self, state: &SharedAppState, index: usize) {
        let dirty = state
            .lock()
            .unwrap()
            .sessions
            .get(index)
            .map(|s| s.dirty)
            .unwrap_or(false);
        if dirty {
            self.confirm_close_session = Some(index);
        } else {
            let _ = crate::app_state::close_session(state, index);
        }
    }

    /// タイムラインのシーンタブ右クリック等、外部起点でのシーン設定編集用。
    pub fn open_scene_edit(&mut self, state: &SharedAppState, scene_id: i32) {
        self.scene_settings.open_for_edit(state, scene_id);
    }

    /// タイムラインの「新規シーン」操作起点。
    pub fn open_scene_create(&mut self, state: &SharedAppState) {
        self.scene_settings.open_for_create(state);
    }

    /// Previewメニューからの表示要求だけを消費する。個々のダイアログ描画は
    /// egui_loopの対応するネイティブwinitウィンドウで行う。
    pub fn sync_preview_requests(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
    ) {
        let mut preview = preview_panel.borrow_mut();
        if std::mem::take(&mut preview.open_system_settings) {
            self.system_settings.open = true;
        }
        if std::mem::take(&mut preview.open_project_settings) {
            self.project_settings.open(state);
        }
        if std::mem::take(&mut preview.open_keybindings) {
            self.keybindings.open = true;
        }
        if std::mem::take(&mut preview.open_export) {
            self.export_dialog.open(state);
        }
    }
}
