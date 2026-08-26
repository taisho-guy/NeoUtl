use super::sections::{clip_bounds, float_row};
use crate::ecs::EcsWorld;
use crate::ecs::TimelineData;
use crate::ecs::effects::{find_effect, param_schema};
use crate::ecs::types::Value;
use crate::localization::effect_param_label;
use neoutl_shared_abi::ParamKind;
use std::collections::HashMap;
use std::sync::Mutex;

static GROUP_OPEN_STATE: Mutex<Option<HashMap<(usize, i32, String), bool>>> = Mutex::new(None);

fn is_group_open(object_id: usize, effect_index: i32, label: &str, initial_open: bool) -> bool {
    let mut guard = GROUP_OPEN_STATE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    *map.entry((object_id, effect_index, label.to_owned()))
        .or_insert(initial_open)
}

fn toggle_group_open(object_id: usize, effect_index: i32, label: &str) {
    let mut guard = GROUP_OPEN_STATE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    let entry = map
        .entry((object_id, effect_index, label.to_owned()))
        .or_insert(true);
    *entry = !*entry;
}

pub fn effects_sidebar(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    let effects = world.get_effects(id);
    let plugins = world.get_plugin_chain(id).unwrap_or_default();
    if effects.is_empty() && plugins.is_empty() {
        ui.weak(t!("エフェクトはありません"));
        return;
    }
    let card = elegance::Theme::current(ui.ctx()).palette.card;
    if !effects.is_empty() {
        let last = effects.len() - 1;
        for (index, inst) in effects.into_iter().enumerate() {
            ui.push_id(("effect_sidebar_row", id, index), |ui| {
                egui::Frame::default()
                    .fill(card)
                    .corner_radius(3.0)
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut enabled = inst.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                world.set_effect_enabled(id, index, enabled);
                            }
                            ui.add(egui::Label::new(&inst.effect_id).truncate());
                            ui.add_enabled_ui(index > 0, |ui| {
                                if ui.small_button("↑").clicked() {
                                    world.reorder_effect(id, index, index - 1);
                                }
                            });
                            ui.add_enabled_ui(index < last, |ui| {
                                if ui.small_button("↓").clicked() {
                                    world.reorder_effect(id, index, index + 1);
                                }
                            });
                            if ui.small_button("✕").clicked() {
                                world.remove_effect(id, index);
                            }
                        });
                    });
            });
        }
    }

    if !plugins.is_empty() {
        ui.add_space(4.0);
        ui.colored_label(
            egui::Color32::from_rgb(0x8a, 0xab, 0xff),
            t!("音声エフェクト"),
        );
        let last = plugins.len() - 1;
        for (index, inst) in plugins.into_iter().enumerate() {
            ui.push_id(("plugin_sidebar_row", id, index), |ui| {
                egui::Frame::default()
                    .fill(card)
                    .corner_radius(3.0)
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut active = !inst.bypass;
                            if ui.checkbox(&mut active, "").changed() {
                                world.set_audio_plugin_bypass(id, index, !active);
                            }
                            let name = inst
                                .path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or(&inst.plugin_id);
                            ui.add(egui::Label::new(name).truncate());
                            ui.add_enabled_ui(index > 0, |ui| {
                                if ui.small_button("↑").clicked() {
                                    world.reorder_audio_plugin(id, index, index - 1);
                                }
                            });
                            ui.add_enabled_ui(index < last, |ui| {
                                if ui.small_button("↓").clicked() {
                                    world.reorder_audio_plugin(id, index, index + 1);
                                }
                            });
                            if ui.small_button("✕").clicked() {
                                world.remove_audio_plugin(id, index);
                            }
                        });
                    });
            });
        }
    }
}

pub fn effects_section(
    ui: &mut egui::Ui,
    world: &mut EcsWorld,
    id: usize,
    objects: &[TimelineData],
) {
    let effects = world.get_effects(id);
    let plugins = world.get_plugin_chain(id).unwrap_or_default();
    if effects.is_empty() && plugins.is_empty() {
        ui.label(t!("エフェクトはありません"));
        return;
    }
    let (clip_start, clip_end) = clip_bounds(world, id);
    let current_frame = world.current_frame();

    if !plugins.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(0x8a, 0xab, 0xff),
            t!("音声エフェクト"),
        );
        let last = plugins.len() - 1;
        for (index, inst) in plugins.into_iter().enumerate() {
            ui.push_id(("audio_plugin_row", id, index), |ui| {
                ui.horizontal(|ui| {
                    let mut active = !inst.bypass;
                    if ui.checkbox(&mut active, "").changed() {
                        world.set_audio_plugin_bypass(id, index, !active);
                    }
                    let name = inst
                        .path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&inst.plugin_id);
                    ui.strong(name);
                    ui.colored_label(
                        egui::Color32::from_rgb(0x88, 0x88, 0x90),
                        format!("({:?})", inst.format),
                    );
                    ui.add_enabled_ui(index > 0, |ui| {
                        if ui.small_button("↑").clicked() {
                            world.reorder_audio_plugin(id, index, index - 1);
                        }
                    });
                    ui.add_enabled_ui(index < last, |ui| {
                        if ui.small_button("↓").clicked() {
                            world.reorder_audio_plugin(id, index, index + 1);
                        }
                    });
                    if ui.small_button("✕").clicked() {
                        world.remove_audio_plugin(id, index);
                    }
                });

                ui.indent(("plugin_info", index), |ui| {
                    if inst.param_info.is_empty() {
                        ui.small(t!("(パラメータ情報がありません)"));
                    } else {
                        for info in &inst.param_info {
                            let val = inst.params.get(&info.id).copied().unwrap_or(info.default);
                            let segment = super::segment::Segment {
                                start_frame: clip_start,
                                end_frame: clip_end,
                                start_value: val as f32,
                                end_value: val as f32,
                            };
                            let outcome = super::row::property_row(
                                ui,
                                (id, "audio_plugin", index, info.id),
                                &info.name,
                                segment,
                                info.min as f32,
                                info.max as f32,
                            );
                            if let Some(v) = outcome.start_value {
                                world.set_audio_plugin_param(id, index, info.id, v as f64);
                            }
                            if let Some(v) = outcome.end_value {
                                world.set_audio_plugin_param(id, index, info.id, v as f64);
                            }
                        }
                    }
                });
                ui.separator();
            });
        }
    }

    if !effects.is_empty() {
        let last = effects.len() - 1;
        for (index, inst) in effects.into_iter().enumerate() {
            ui.push_id(("effect_row", id, index), |ui| {
                ui.horizontal(|ui| {
                    let mut enabled = inst.enabled;
                    if ui.checkbox(&mut enabled, "").changed() {
                        world.set_effect_enabled(id, index, enabled);
                    }
                    ui.label(&inst.effect_id);
                    ui.add_enabled_ui(index > 0, |ui| {
                        if ui.small_button("↑").clicked() {
                            world.reorder_effect(id, index, index - 1);
                        }
                    });
                    ui.add_enabled_ui(index < last, |ui| {
                        if ui.small_button("↓").clicked() {
                            world.reorder_effect(id, index, index + 1);
                        }
                    });
                    if ui.small_button("✕").clicked() {
                        world.remove_effect(id, index);
                    }
                });

                let Some(source) = find_effect(&inst.effect_id) else {
                    ui.small(t!("(エフェクト定義が見つかりません)"));
                    return;
                };
                let schema = param_schema(&source);
                let mut collapsed = false;

                for s in &schema {
                    if s.kind == ParamKind::Group {
                        let initial_open = s.default_float != 0.0;
                        let open = is_group_open(id, index as i32, &s.label, initial_open);
                        if ui
                            .selectable_label(open, format!("▸ {}", effect_param_label(&s.label)))
                            .clicked()
                        {
                            toggle_group_open(id, index as i32, &s.label);
                        }
                        collapsed = !is_group_open(id, index as i32, &s.label, initial_open);
                        continue;
                    }
                    if collapsed {
                        continue;
                    }
                    if s.kind == ParamKind::Separator {
                        ui.separator();
                        continue;
                    }

                    let current = inst.params.get(&s.key).map(|p| &p.static_value);

                    match s.kind {
                        ParamKind::Float | ParamKind::Color => {
                            let base = match current {
                                Some(Value::Number(v)) => *v,
                                _ => s.default_float,
                            };
                            let min = if s.kind == ParamKind::Color {
                                0.0
                            } else {
                                s.min
                            };
                            let max = if s.kind == ParamKind::Color {
                                1.0
                            } else {
                                s.max
                            };
                            let track = world.get_effect_keyframes(id, index, &s.key);
                            let key_set = s.key.clone();
                            let key_rm = s.key.clone();
                            float_row(
                                ui,
                                world,
                                (id, index, &s.key),
                                super::easing_editor::TrackTarget::Effect {
                                    object_id: id,
                                    effect_index: index,
                                    key: s.key.clone(),
                                },
                                &s.label,
                                min,
                                max,
                                clip_start,
                                clip_end,
                                current_frame,
                                base,
                                &track,
                                move |w, f, v, e, p| {
                                    w.set_effect_keyframe(id, index, &key_set, f, v, e, p)
                                },
                                move |w, f| w.remove_effect_keyframe(id, index, &key_rm, f),
                            );
                        }
                        _ => {
                            if let Some(v) = param_widget(ui, id, index, s, current, objects) {
                                apply_effect_value(world, id, index, &s.key, v);
                            }
                        }
                    }
                }
            });
            ui.separator();
        }
    }
}

fn param_widget(
    ui: &mut egui::Ui,
    object_id: usize,
    effect_index: usize,
    s: &neoutl_shared_abi::ParamRowOwned,
    current: Option<&Value>,
    objects: &[TimelineData],
) -> Option<Value> {
    ui.horizontal(|ui| {
        ui.label(effect_param_label(&s.label));
        match s.kind {
            ParamKind::Bool => {
                let mut b = match current {
                    Some(Value::Bool(b)) => *b,
                    _ => s.default_float != 0.0,
                };
                ui.checkbox(&mut b, "").changed().then_some(Value::Bool(b))
            }
            ParamKind::Enum => {
                let mut index = match current {
                    Some(Value::Enum(i)) => *i,
                    _ => s.default_float as u32,
                };
                let current_label = s
                    .enum_options
                    .get(index as usize)
                    .map(|o| effect_param_label(o))
                    .unwrap_or_default();
                let mut changed = false;
                egui::ComboBox::from_id_salt(("effect_enum", object_id, effect_index, &s.key))
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        for (i, opt) in s.enum_options.iter().enumerate() {
                            if ui
                                .selectable_value(&mut index, i as u32, effect_param_label(opt))
                                .changed()
                            {
                                changed = true;
                            }
                        }
                    });
                changed.then_some(Value::Enum(index))
            }
            ParamKind::Text => {
                let mut t = match current {
                    Some(Value::Text(t)) => t.clone(),
                    _ => String::new(),
                };
                ui.text_edit_singleline(&mut t)
                    .changed()
                    .then_some(Value::Text(t))
            }
            ParamKind::FilePath | ParamKind::Folder => {
                let mut t = match current {
                    Some(Value::FilePath(t)) => t.clone(),
                    _ => String::new(),
                };
                let mut changed = false;
                if ui.text_edit_singleline(&mut t).changed() {
                    changed = true;
                }
                if ui.button(t!("参照…")).clicked() {
                    let dialog = rfd::FileDialog::new();
                    let picked = if s.kind == ParamKind::Folder {
                        dialog.pick_folder()
                    } else {
                        dialog.pick_file()
                    };
                    if let Some(path) = picked {
                        t = path.to_string_lossy().into_owned();
                        changed = true;
                    }
                }
                changed.then_some(Value::FilePath(t))
            }
            ParamKind::Track => {
                let mut track_ref = match current {
                    Some(Value::TrackRef(i)) => *i,
                    _ => -1,
                };
                let current_label = if track_ref < 0 {
                    t!("未選択")
                } else {
                    format!("Object {track_ref}")
                };
                let mut changed = false;
                egui::ComboBox::from_id_salt(("effect_track", object_id, effect_index, &s.key))
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(&mut track_ref, -1, t!("未選択"))
                            .changed()
                        {
                            changed = true;
                        }
                        for o in objects {
                            if o.id as usize == object_id {
                                continue;
                            }
                            let label = format!("Object {}", o.id);
                            if ui.selectable_value(&mut track_ref, o.id, label).changed() {
                                changed = true;
                            }
                        }
                    });
                changed.then_some(Value::TrackRef(track_ref))
            }
            ParamKind::Group | ParamKind::Separator | ParamKind::Float | ParamKind::Color => None,
        }
    })
    .inner
}

fn apply_effect_value(
    world: &mut EcsWorld,
    object_id: usize,
    index: usize,
    key: &str,
    value: Value,
) {
    match value {
        Value::Number(v) => world.set_effect_param(object_id, index, key, v),
        Value::Bool(b) => world.set_effect_param_bool(object_id, index, key, b),
        Value::Text(t) => world.set_effect_param_text(object_id, index, key, t),
        Value::FilePath(p) => world.set_effect_param_path(object_id, index, key, p),
        Value::Enum(e) => world.set_effect_param_enum(object_id, index, key, e),
        Value::TrackRef(t) => world.set_effect_param_track_ref(object_id, index, key, t),
    }
}
