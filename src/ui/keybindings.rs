use crate::localization::tr;
use crate::shortcuts::{self, ALL_COMMANDS, CommandId, OwnedBinding, Scope};
use egui::{Context, Ui};

fn scope_label(s: Scope) -> String {
    match s {
        Scope::Global => tr("全体"),
        Scope::Timeline => tr("タイムライン"),
        Scope::Properties => tr("設定ダイアログ"),
        Scope::Preview => tr("プレビュー"),
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

#[derive(Clone)]
struct Row {
    command: CommandId,
    label: String,
    scope_label: String,
    key_display: String,
}

fn build_rows() -> Vec<Row> {
    let keymap = shortcuts::active_keymap().lock().unwrap();
    ALL_COMMANDS
        .iter()
        .map(|&command| {
            let (scope, binding) = keymap.binding_of(command);
            Row {
                command,
                label: shortcuts::label(command).into(),
                scope_label: scope_label(scope),
                key_display: key_display(&binding),
            }
        })
        .collect()
}

pub struct KeybindingsWindow {
    pub open: bool,
    rows: Vec<Row>,
    save_status: String,
    conflict_message: String,
    capturing: Option<CommandId>,
}

impl KeybindingsWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            rows: build_rows(),
            save_status: String::new(),
            conflict_message: String::new(),
            capturing: None,
        }
    }

    fn sync(&mut self) {
        self.rows = build_rows();
    }

    fn apply_capture(&mut self, command: CommandId, binding: OwnedBinding) {
        let mut keymap = shortcuts::active_keymap().lock().unwrap();
        let (scope, _) = keymap.binding_of(command);
        if let Some(other) = keymap.conflict_of(command, scope, &binding) {
            self.conflict_message = tr("競合: {}").replace("{}", &shortcuts::label(other));
            return;
        }
        self.conflict_message.clear();
        keymap.set_binding(command, scope, binding);
        drop(keymap);
        self.sync();
    }

    pub fn show(&mut self, ctx: &Context, ui: &mut Ui) {
        if !self.open {
            return;
        }

        if let Some(command) = self.capturing {
            let binding = ctx.input(|i| {
                let modifiers = i.modifiers;
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key, pressed: true, ..
                    } => Some(OwnedBinding {
                        ctrl: modifiers.ctrl,
                        shift: modifiers.shift,
                        alt: modifiers.alt,
                        key: key.name().to_string(),
                    }),
                    _ => None,
                })
            });
            if let Some(binding) = binding {
                self.capturing = None;
                self.apply_capture(command, binding);
            }
        }

        egui::CentralPanel::default().show(ui, |ui| {
            if !self.conflict_message.is_empty() {
                ui.colored_label(egui::Color32::RED, &self.conflict_message);
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("keybindings_rows")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        for row in self.rows.clone() {
                            ui.label(&row.label);
                            ui.label(&row.scope_label);

                            let capturing = self.capturing == Some(row.command);
                            let text = if capturing {
                                tr("入力待ち…")
                            } else {
                                row.key_display.clone()
                            };
                            if ui.button(text).clicked() {
                                self.capturing = Some(row.command);
                            }

                            if ui.button(tr("既定へ")).clicked() {
                                shortcuts::active_keymap()
                                    .lock()
                                    .unwrap()
                                    .reset_to_default(row.command);
                                self.sync();
                            }
                            ui.end_row();
                        }
                    });
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(tr("全て既定へ")).clicked() {
                    shortcuts::active_keymap().lock().unwrap().reset_all();
                    self.sync();
                }
                if ui.button(tr("保存")).clicked() {
                    let result =
                        shortcuts::save_to_disk(&shortcuts::active_keymap().lock().unwrap());
                    self.save_status = match result {
                        Ok(()) => tr("保存完了"),
                        Err(_) => tr("保存失敗"),
                    };
                }
                if ui.button(tr("再読込")).clicked() {
                    let loaded = shortcuts::load_from_disk().unwrap_or_default();
                    *shortcuts::active_keymap().lock().unwrap() = loaded;
                    self.sync();
                    self.save_status = tr("再読込完了");
                }
                ui.label(&self.save_status);
            });
        });
    }
}
