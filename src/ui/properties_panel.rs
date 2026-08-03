use crate::app_state::{self, SharedAppState};
use crate::ecs::components::ParamAccess;
use crate::ecs::effects::{find_effect, param_schema};
use crate::ecs::object_schema::{
    AUDIO_SCHEMA, SHAPE_SCHEMA, TEXT_SCHEMA, TRANSFORM_SCHEMA, is_visible, resolve_range,
};
use crate::ecs::types::{Keyframe, Value};
use crate::ui::effect_add_dialog::EffectAddDialog;
use crate::ui::effect_catalog::EffectCatalogState;
use egui_plot::{Line, Plot, PlotPoints};
use neoutl_shared_abi::ParamKind;
use std::collections::HashMap;
use std::sync::Mutex;

/// 折り畳み見出し(kind==Group)の開閉状態。Slint版`GROUP_OPEN_STATE`と同じく
/// プロジェクトファイルへは非保存のホストUIローカル状態。
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

pub struct PropertiesPanel {
    pub open: bool,
    pub effect_add: EffectAddDialog,
    selected: Option<usize>,
    /// 2a: レジストリ走査は起動コストがあるためパネル生成時に一度だけ構築する
    /// （Slint版 EffectCatalogState::build()と同じ方針）。
    catalog: EffectCatalogState,
}

impl PropertiesPanel {
    pub fn new() -> Self {
        Self {
            open: true,
            effect_add: EffectAddDialog::new(),
            selected: None,
            catalog: EffectCatalogState::build(),
        }
    }

    /// `WindowKind::EffectAdd`ネイティブウィンドウから毎フレーム呼ばれる。
    /// Slint版 properties.rs の `wire_effect_add_dialog` に相当する接続点。
    pub fn show_effect_add(&mut self, ctx: &egui::Context, state: &SharedAppState) {
        if let Some(effect_id) = self.effect_add.show(ctx, &self.catalog) {
            if let Some(id) = self.selected {
                let holder = app_state::active_world(state);
                holder.lock().unwrap().add_effect(id, &effect_id);
            }
            crate::ui::effect_catalog::mark_effect_used(&effect_id);
        }
    }

    pub fn show(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui, state: &SharedAppState) {
        if !self.open {
            return;
        }
        let holder = app_state::active_world(state);
        let mut world = holder.lock().unwrap();
        let objects = world.get_timeline_objects();
        if self.selected.is_none() || !self.selected.is_some_and(|id| world.object_exists(id)) {
            self.selected = objects.first().map(|o| o.id as usize);
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("プロパティ");
            let Some(id) = self.selected else {
                ui.label("オブジェクトを選択してください");
                return;
            };
            ui.small(format!("Object {id} / frame {}", world.current_frame()));
            if ui.button("＋エフェクト追加").clicked() {
                self.effect_add.open();
            }
            ui.separator();

            self.transform_section(ui, &mut world, id);
            self.text_section(ui, &mut world, id);
            self.shape_section(ui, &mut world, id);
            self.audio_section(ui, &mut world, id);

            ui.separator();
            ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), "エフェクト");
            self.effects_section(ui, &mut world, id, &objects);
        });
    }

    fn transform_section(&self, ui: &mut egui::Ui, world: &mut crate::ecs::EcsWorld, id: usize) {
        let Some(mut transform) = world.get_transform(id) else {
            return;
        };
        for schema in TRANSFORM_SCHEMA {
            if !matches!(schema.kind, ParamKind::Float | ParamKind::Bool) {
                continue;
            }
            let Some(mut value) = transform.get_param(schema.key) else {
                continue;
            };
            let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
            ui.horizontal(|ui| {
                ui.label(schema.label);
                let changed = if schema.kind == ParamKind::Bool {
                    let mut b = value > 0.5;
                    let r = ui.checkbox(&mut b, "");
                    value = if b { 1.0 } else { 0.0 };
                    r.changed()
                } else {
                    ui.add(
                        egui::DragValue::new(&mut value)
                            .range(min..=max)
                            .speed((max - min) / 1000.0),
                    )
                    .changed()
                };
                if changed {
                    transform.set_param(schema.key, value);
                    world.set_transform(id, transform);
                }
            });
            let track = world.get_keyframes(id, schema.key);
            if !track.is_empty() {
                easing_plot(ui, &track, schema.label);
            }
            ui.horizontal(|ui| {
                if ui.small_button("＋KF").clicked() {
                    let (e, p) = track
                        .last()
                        .map(|k| (k.engine_id.clone(), k.engine_payload.clone()))
                        .unwrap_or(("neoutl-easing-standard".into(), Vec::new()));
                    let f = world.current_frame();
                    world.set_keyframe(id, schema.key, f, value, e, p);
                }
                if ui.small_button("−KF").clicked() {
                    let f = world.current_frame();
                    world.remove_keyframe(id, schema.key, f);
                }
            });
        }
    }

    /// テキストオブジェクト専用ネイティブパラメータ（TEXT_SCHEMA）。
    /// "text"のみString型のためParamAccess非対応、set_textへ直接委譲する。
    fn text_section(&self, ui: &mut egui::Ui, world: &mut crate::ecs::EcsWorld, id: usize) {
        let Some(mut content) = world.get_text(id) else {
            return;
        };
        ui.separator();
        ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), "テキスト");
        for schema in TEXT_SCHEMA {
            ui.horizontal(|ui| {
                ui.label(schema.label);
                match schema.kind {
                    ParamKind::Text => {
                        if ui.text_edit_multiline(&mut content.text).changed() {
                            world.set_text(id, content.text.clone(), content.font_size);
                        }
                    }
                    ParamKind::Float => {
                        let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
                        let mut value = content.get_param(schema.key).unwrap_or(0.0);
                        if ui
                            .add(
                                egui::DragValue::new(&mut value)
                                    .range(min..=max)
                                    .speed((max - min).max(0.001) / 1000.0),
                            )
                            .changed()
                        {
                            content.set_param(schema.key, value);
                            world.set_text(id, content.text.clone(), content.font_size);
                        }
                    }
                    _ => {}
                }
            });
            if schema.kind == ParamKind::Float {
                let track = world.get_keyframes(id, schema.key);
                if !track.is_empty() {
                    easing_plot(ui, &track, schema.label);
                }
                let value = content.get_param(schema.key).unwrap_or(0.0);
                ui.horizontal(|ui| {
                    if ui.small_button("＋KF").clicked() {
                        let (e, p) = track
                            .last()
                            .map(|k| (k.engine_id.clone(), k.engine_payload.clone()))
                            .unwrap_or(("neoutl-easing-standard".into(), Vec::new()));
                        let f = world.current_frame();
                        world.set_keyframe(id, schema.key, f, value, e, p);
                    }
                    if ui.small_button("−KF").clicked() {
                        let f = world.current_frame();
                        world.remove_keyframe(id, schema.key, f);
                    }
                });
            }
        }
    }

    /// 図形オブジェクト専用ネイティブパラメータ（SHAPE_SCHEMA）。全キーf32のため
    /// ParamAccess経由でget_shape/set_shapeへ委譲する。
    fn shape_section(&self, ui: &mut egui::Ui, world: &mut crate::ecs::EcsWorld, id: usize) {
        let Some(mut shape) = world.get_shape(id) else {
            return;
        };
        ui.separator();
        ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), "図形");
        for schema in SHAPE_SCHEMA {
            let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
            let mut value = shape.get_param(schema.key).unwrap_or(0.0);
            ui.horizontal(|ui| {
                ui.label(schema.label);
                if ui
                    .add(
                        egui::DragValue::new(&mut value)
                            .range(min..=max)
                            .speed((max - min).max(0.001) / 1000.0),
                    )
                    .changed()
                {
                    shape.set_param(schema.key, value);
                    world.set_shape(id, shape);
                }
            });
            let track = world.get_keyframes(id, schema.key);
            if !track.is_empty() {
                easing_plot(ui, &track, schema.label);
            }
            ui.horizontal(|ui| {
                if ui.small_button("＋KF").clicked() {
                    let (e, p) = track
                        .last()
                        .map(|k| (k.engine_id.clone(), k.engine_payload.clone()))
                        .unwrap_or(("neoutl-easing-standard".into(), Vec::new()));
                    let f = world.current_frame();
                    world.set_keyframe(id, schema.key, f, value, e, p);
                }
                if ui.small_button("−KF").clicked() {
                    let f = world.current_frame();
                    world.remove_keyframe(id, schema.key, f);
                }
            });
        }
    }

    /// オーディオオブジェクト専用ネイティブパラメータ（AUDIO_SCHEMA）。
    /// "pan"は"mute"==falseの時のみ表示する（depends_on）。
    fn audio_section(&self, ui: &mut egui::Ui, world: &mut crate::ecs::EcsWorld, id: usize) {
        let Some(mut audio) = world.get_audio_params(id) else {
            return;
        };
        ui.separator();
        ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), "オーディオ");
        for schema in AUDIO_SCHEMA {
            if !is_visible(schema, |k| audio.get_param(k).unwrap_or(0.0)) {
                continue;
            }
            ui.horizontal(|ui| {
                ui.label(schema.label);
                match schema.kind {
                    ParamKind::Bool => {
                        let mut b = audio.get_param(schema.key).unwrap_or(0.0) > 0.5;
                        if ui.checkbox(&mut b, "").changed() {
                            audio.set_param(schema.key, if b { 1.0 } else { 0.0 });
                            world.set_audio_params(id, audio.volume, audio.pan, audio.mute);
                        }
                    }
                    ParamKind::Float => {
                        let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
                        let mut value = audio.get_param(schema.key).unwrap_or(0.0);
                        if ui
                            .add(
                                egui::DragValue::new(&mut value)
                                    .range(min..=max)
                                    .speed((max - min).max(0.001) / 1000.0),
                            )
                            .changed()
                        {
                            audio.set_param(schema.key, value);
                            world.set_audio_params(id, audio.volume, audio.pan, audio.mute);
                        }
                    }
                    _ => {}
                }
            });
        }
    }

    /// 2b: エフェクトスタック（Slint版`EffectRow`一覧）の表示・追加・削除・並び替え。
    /// 追加自体は`effect_add`ダイアログ確定時（`show_effect_add`）に行われ、
    /// ここでは既存スタックの操作のみを扱う。
    fn effects_section(
        &self,
        ui: &mut egui::Ui,
        world: &mut crate::ecs::EcsWorld,
        id: usize,
        objects: &[crate::ecs::TimelineData],
    ) {
        let effects = world.get_effects(id);
        if effects.is_empty() {
            ui.label("エフェクトはありません");
            return;
        }
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
                    ui.small("(エフェクト定義が見つかりません)");
                    return;
                };
                let schema = param_schema(&source);
                let mut collapsed = false;
                egui::Grid::new(("effect_params", id, index))
                    .num_columns(2)
                    .show(ui, |ui| {
                        for s in &schema {
                            if s.kind == ParamKind::Group {
                                let initial_open = s.default_float != 0.0;
                                let open = is_group_open(id, index as i32, &s.label, initial_open);
                                if ui
                                    .selectable_label(open, format!("▸ {}", s.label))
                                    .clicked()
                                {
                                    toggle_group_open(id, index as i32, &s.label);
                                }
                                ui.end_row();
                                collapsed =
                                    !is_group_open(id, index as i32, &s.label, initial_open);
                                continue;
                            }
                            if collapsed {
                                continue;
                            }
                            if s.kind == ParamKind::Separator {
                                ui.separator();
                                ui.end_row();
                                continue;
                            }

                            let current = inst.params.get(&s.key).map(|p| &p.static_value);
                            ui.label(&s.label);
                            let changed_value =
                                self.param_widget(ui, s, current, objects, id, index);
                            if let Some(v) = changed_value {
                                apply_effect_value(world, id, index, &s.key, v);
                            }
                            ui.end_row();

                            if s.kind == ParamKind::Float {
                                let track = world.get_effect_keyframes(id, index, &s.key);
                                if !track.is_empty() {
                                    easing_plot(ui, &track, &s.label);
                                    ui.end_row();
                                }
                                let base = match current {
                                    Some(Value::Number(v)) => *v,
                                    _ => s.default_float,
                                };
                                ui.horizontal(|ui| {
                                    if ui.small_button("＋KF").clicked() {
                                        let (e, p) = track
                                            .last()
                                            .map(|k| {
                                                (k.engine_id.clone(), k.engine_payload.clone())
                                            })
                                            .unwrap_or((
                                                "neoutl-easing-standard".into(),
                                                Vec::new(),
                                            ));
                                        let f = world.current_frame();
                                        world.set_effect_keyframe(id, index, &s.key, f, base, e, p);
                                    }
                                    if ui.small_button("−KF").clicked() {
                                        let f = world.current_frame();
                                        world.remove_effect_keyframe(id, index, &s.key, f);
                                    }
                                });
                                ui.end_row();
                            }
                        }
                    });
            });
            ui.separator();
        }
    }

    /// 2c: `ParamKind`種別ごとの入力ウィジェット分岐。
    /// 変更があった場合のみ`Some(Value)`を返す（呼び出し側がworldへ書き戻す）。
    fn param_widget(
        &self,
        ui: &mut egui::Ui,
        s: &neoutl_shared_abi::ParamRowOwned,
        current: Option<&Value>,
        objects: &[crate::ecs::TimelineData],
        object_id: usize,
        effect_index: usize,
    ) -> Option<Value> {
        match s.kind {
            ParamKind::Float | ParamKind::Color => {
                let mut v = match current {
                    Some(Value::Number(v)) => *v,
                    _ => s.default_float,
                };
                let r = ui.add(
                    egui::DragValue::new(&mut v)
                        .range(s.min..=s.max)
                        .speed(((s.max - s.min).max(0.001)) / 1000.0),
                );
                r.changed().then_some(Value::Number(v))
            }
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
                    .cloned()
                    .unwrap_or_default();
                let mut changed = false;
                egui::ComboBox::from_id_salt(("effect_enum", object_id, effect_index, &s.key))
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        for (i, opt) in s.enum_options.iter().enumerate() {
                            if ui.selectable_value(&mut index, i as u32, opt).changed() {
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
                ui.horizontal(|ui| {
                    if ui.text_edit_singleline(&mut t).changed() {
                        changed = true;
                    }
                    if ui.button("参照…").clicked() {
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
                });
                changed.then_some(Value::FilePath(t))
            }
            ParamKind::Track => {
                let mut track_ref = match current {
                    Some(Value::TrackRef(i)) => *i,
                    _ => -1,
                };
                let current_label = if track_ref < 0 {
                    "未選択".to_string()
                } else {
                    format!("Object {track_ref}")
                };
                let mut changed = false;
                egui::ComboBox::from_id_salt(("effect_track", object_id, effect_index, &s.key))
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut track_ref, -1, "未選択").changed() {
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
            ParamKind::Group | ParamKind::Separator => None,
        }
    }
}

fn apply_effect_value(
    world: &mut crate::ecs::EcsWorld,
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

fn easing_plot(ui: &mut egui::Ui, k: &[Keyframe], label: &str) {
    let p: PlotPoints = k.iter().map(|x| [x.frame as f64, x.value as f64]).collect();
    Plot::new(format!("easing_{label}"))
        .height(70.0)
        .show_axes([false, false])
        .show(ui, |u| u.line(Line::new(label, p)));
}
