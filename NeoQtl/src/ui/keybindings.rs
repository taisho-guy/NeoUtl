use crate::shortcuts::{self, ALL_COMMANDS, OwnedBinding, Scope};

fn scope_label(s: Scope) -> String {
    match s {
        Scope::Unspecified => "未定義".into(),
        Scope::Global => "全体".into(),
        Scope::Timeline => "タイムライン".into(),
        Scope::Properties => "設定ダイアログ".into(),
        Scope::Preview => "プレビュー".into(),
    }
}

fn key_display(b: &OwnedBinding) -> String {
    let mut parts = Vec::new();
    if b.ctrl {
        parts.push("Ctrl");
    }
    if b.shift {
        parts.push("Shift");
    }
    if b.alt {
        parts.push("Alt");
    }
    parts.push(b.key.as_str());
    parts.join("+")
}

#[derive(Clone, Debug)]
pub struct KeybindingRow {
    pub command_id: u32,
    pub label: String,
    pub scope_label: String,
    pub key_display: String,
}

pub struct KeybindingsWindow {
    pub open: bool,
    pub save_status: String,
    pub conflict_message: String,
}

impl KeybindingsWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            save_status: String::new(),
            conflict_message: String::new(),
        }
    }

    pub fn get_rows(&self) -> Vec<KeybindingRow> {
        let keymap = shortcuts::active_keymap().lock().unwrap();
        ALL_COMMANDS
            .iter()
            .map(|&command| {
                let (scope, binding) = keymap.binding_of(command);
                KeybindingRow {
                    command_id: command as u32,
                    label: shortcuts::label(command).into(),
                    scope_label: scope_label(scope),
                    key_display: key_display(&binding),
                }
            })
            .collect()
    }

    pub fn reset_defaults(&mut self) {
        let mut keymap = shortcuts::active_keymap().lock().unwrap();
        keymap.reset_all();
        let _ = shortcuts::save_to_disk(&keymap);
        self.save_status = "初期化しました".into();
    }
}
