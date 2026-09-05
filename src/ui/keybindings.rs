use crate::shortcuts::{self, ALL_COMMANDS, CommandId, OwnedBinding, Scope};
use egui::{Context, Ui};

fn scope_label(s: Scope) -> String {
    match s {
        Scope::Unspecified => t!("未定義"),
        Scope::Global => t!("全体"),
        Scope::Timeline => t!("タイムライン"),
        Scope::Properties => t!("設定ダイアログ"),
        Scope::Preview => t!("プレビュー"),
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
    pending_binding: Option<OwnedBinding>,
}

impl KeybindingsWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            rows: build_rows(),
            save_status: String::new(),
            conflict_message: String::new(),
            capturing: None,
            pending_binding: None,
        }
    }

    fn sync(&mut self) {
        self.rows = build_rows();
    }

    fn apply_capture(&mut self, command: CommandId, binding: OwnedBinding) {
        let mut keymap = shortcuts::active_keymap().lock().unwrap();
        let (scope, _) = keymap.binding_of(command);
        if let Some(other) = keymap.conflict_of(command, scope, &binding) {
            self.conflict_message = t!("競合: {}").replace("{}", &shortcuts::label(other));
            return;
        }
        self.conflict_message.clear();
        keymap.set_binding(command, scope, binding);
        drop(keymap);
        self.sync();
    }

    fn cancel_capture(&mut self) {
        self.capturing = None;
        self.pending_binding = None;
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
        if !open {
            self.cancel_capture();
        }
    }

    pub fn show(&mut self, ctx: &Context, ui: &mut Ui) {
        if !self.open {
            return;
        }

        if let Some(command) = self.capturing {
            let (newly_pressed, released) = ctx.input(|i| {
                let modifiers = i.modifiers;
                let mut newly_pressed = None;
                let mut released = None;
                for e in &i.events {
                    let egui::Event::Key { key, pressed, .. } = e else {
                        continue;
                    };
                    let name = key.name();
                    if *pressed {
                        newly_pressed = Some(OwnedBinding {
                            ctrl: modifiers.ctrl,
                            shift: modifiers.shift,
                            alt: modifiers.alt,
                            key: name.to_string(),
                        });
                    } else if self
                        .pending_binding
                        .as_ref()
                        .is_some_and(|b| b.key.eq_ignore_ascii_case(name))
                    {
                        released = self.pending_binding.clone();
                    }
                }
                (newly_pressed, released)
            });
            if let Some(binding) = newly_pressed {
                self.pending_binding = Some(binding);
            }
            if let Some(binding) = released {
                self.capturing = None;
                self.pending_binding = None;
                self.apply_capture(command, binding);
            }
        }

        egui::CentralPanel::default().show(ui, |ui| {
            if !self.conflict_message.is_empty() {
                ui.colored_label(egui::Color32::RED, &self.conflict_message);
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let row_height = 24.0;
                    let key_btn_w = 130.0;
                    let reset_btn_w = 90.0;

                    for (i, row) in self.rows.clone().into_iter().enumerate() {
                        let fill = if i % 2 == 1 {
                            ui.visuals().faint_bg_color
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        egui::Frame::default().fill(fill).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(&row.label);
                                ui.label(&row.scope_label);

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add_sized(
                                                [reset_btn_w, row_height],
                                                egui::Button::new(t!("既定へ")),
                                            )
                                            .clicked()
                                        {
                                            shortcuts::active_keymap()
                                                .lock()
                                                .unwrap()
                                                .reset_to_default(row.command);
                                            self.sync();
                                        }

                                        let capturing = self.capturing == Some(row.command);
                                        let text = if capturing {
                                            t!("入力待ち…")
                                        } else {
                                            row.key_display.clone()
                                        };
                                        if ui
                                            .add_sized(
                                                [key_btn_w, row_height],
                                                egui::Button::new(text),
                                            )
                                            .clicked()
                                        {
                                            self.capturing = Some(row.command);
                                            self.pending_binding = None;
                                        }
                                    },
                                );
                            });
                        });
                    }
                });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(t!("全て既定へ")).clicked() {
                    shortcuts::active_keymap().lock().unwrap().reset_all();
                    self.sync();
                }
                if ui.button(t!("保存")).clicked() {
                    let result =
                        shortcuts::save_to_disk(&shortcuts::active_keymap().lock().unwrap());
                    self.save_status = match result {
                        Ok(()) => t!("保存完了"),
                        Err(_) => t!("保存失敗"),
                    };
                }
                if ui.button(t!("再読込")).clicked() {
                    let loaded = shortcuts::load_from_disk().unwrap_or_default();
                    *shortcuts::active_keymap().lock().unwrap() = loaded;
                    self.sync();
                    self.save_status = t!("再読込完了");
                }
                ui.label(&self.save_status);
            });
        });
    }
}
