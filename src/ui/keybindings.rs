use crate::shortcuts::{self, ALL_COMMANDS, CommandId, OwnedBinding, Scope};
use crate::{KeyBindingRow, KeybindingsWindow};
use slint::{ComponentHandle, ModelRc, VecModel};

fn scope_label(s: Scope) -> &'static str {
    match s {
        Scope::Global => "全体",
        Scope::Timeline => "タイムライン",
        Scope::Properties => "設定ダイアログ",
        Scope::Preview => "プレビュー",
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

fn command_index(command: CommandId) -> i32 {
    ALL_COMMANDS
        .iter()
        .position(|&c| c == command)
        .map_or(0, |i| i as i32)
}

fn build_rows() -> Vec<KeyBindingRow> {
    let keymap = shortcuts::active_keymap().lock().unwrap();
    ALL_COMMANDS
        .iter()
        .map(|&command| {
            let (scope, binding) = keymap.binding_of(command);
            KeyBindingRow {
                command_index: command_index(command),
                label: shortcuts::label(command).into(),
                scope_label: scope_label(scope).into(),
                key_display: key_display(&binding).into(),
                conflict: false,
            }
        })
        .collect()
}

fn sync(window: &KeybindingsWindow) {
    let model: ModelRc<KeyBindingRow> = ModelRc::new(VecModel::from(build_rows()));
    window.set_rows(model);
}

pub fn setup(window: &KeybindingsWindow) {
    sync(window);

    {
        let weak = window.as_weak();
        window.on_capture_binding(move |index, ctrl, shift, alt, key| {
            let Some(&command) = ALL_COMMANDS.get(index as usize) else {
                return;
            };
            if key.is_empty() {
                return;
            }
            let binding = OwnedBinding {
                ctrl,
                shift,
                alt,
                key: key.to_string(),
            };
            let Some(win) = weak.upgrade() else {
                return;
            };
            let mut keymap = shortcuts::active_keymap().lock().unwrap();
            let (scope, _) = keymap.binding_of(command);
            if let Some(other) = keymap.conflict_of(command, scope, &binding) {
                win.set_conflict_message(format!("競合: {}", shortcuts::label(other)).into());
                return;
            }
            win.set_conflict_message("".into());
            keymap.set_binding(command, scope, binding);
            drop(keymap);
            sync(&win);
        });
    }

    {
        let weak = window.as_weak();
        window.on_reset_binding(move |index| {
            let Some(&command) = ALL_COMMANDS.get(index as usize) else {
                return;
            };
            shortcuts::active_keymap()
                .lock()
                .unwrap()
                .reset_to_default(command);
            if let Some(win) = weak.upgrade() {
                sync(&win);
            }
        });
    }

    {
        let weak = window.as_weak();
        window.on_reset_all(move || {
            shortcuts::active_keymap().lock().unwrap().reset_all();
            if let Some(win) = weak.upgrade() {
                sync(&win);
            }
        });
    }

    {
        let weak = window.as_weak();
        window.on_save_settings(move || {
            let result = shortcuts::save_to_disk(&shortcuts::active_keymap().lock().unwrap());
            if let Some(win) = weak.upgrade() {
                win.set_save_status(match result {
                    Ok(()) => "保存完了".into(),
                    Err(_) => "保存失敗".into(),
                });
            }
        });
    }

    {
        let weak = window.as_weak();
        window.on_reload_settings(move || {
            let loaded = shortcuts::load_from_disk().unwrap_or_default();
            *shortcuts::active_keymap().lock().unwrap() = loaded;
            if let Some(win) = weak.upgrade() {
                sync(&win);
                win.set_save_status("再読込完了".into());
            }
        });
    }
}
