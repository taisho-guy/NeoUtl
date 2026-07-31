use crate::app_state::{self, SharedAppState};
use crate::ecs::types::Keyframe;
use crate::ecs::{
    EcsWorld,
    components::{ParamAccess, ShapeParams},
    effects::{find_effect, param_schema},
    object_schema::{
        AUDIO_GROUP, AUDIO_SCHEMA, SHAPE_GROUP, SHAPE_SCHEMA, TEXT_GROUP, TEXT_SCHEMA,
        TRANSFORM_GROUP, TRANSFORM_SCHEMA, resolve_range,
    },
};
use crate::{CatalogRow, EffectAddDialog, EffectRow, ParamRow, PropertiesWindow};
use neoutl_easing_api::{EditResultC, EditResultCode, KeyframeC};
use neoutl_shared_abi::ParamKind;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

/// 直近に追加したエフェクトIDの履歴（新しい順、最大8件）。プロセス生存中のみ保持し
/// ディスク永続化はしない（最近使用ソートは同一セッション内の利便性のためのもの）。
static RECENT_EFFECT_IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// エフェクト右クリックメニューの「コピー」で保持する1件分クリップボード。
/// プロセス生存中のみ保持、ディスク永続化はしない。オブジェクト間貼り付けを許容する
/// （EffectInstanceはeffect_idと自パラメータのみを保持し、対象オブジェクトに依存しないため）。
static EFFECT_CLIPBOARD: Mutex<Option<crate::ecs::types::EffectInstance>> = Mutex::new(None);

/// 折り畳み見出し(kind==Group)の開閉状態スコープ。プロジェクトファイルへは非保存の
/// ホストUIローカル状態（プロセス生存中のみ有効）。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum GroupScope {
    Object(i32),
    Effect(i32, i32),
}

static GROUP_OPEN_STATE: Mutex<Option<std::collections::HashMap<(GroupScope, String), bool>>> =
    Mutex::new(None);

fn toggle_group_open(scope: GroupScope, label: &str) {
    let mut guard = GROUP_OPEN_STATE.lock().unwrap();
    let map = guard.get_or_insert_with(std::collections::HashMap::new);
    let entry = map.entry((scope, label.to_owned())).or_insert(true);
    *entry = !*entry;
}

/// Group行の現在の開閉状態を取得する。未記録の場合はinitial_openをそのまま用いる
/// （object_schema::group_field/C ABI側default_floatの初期値）。
fn is_group_open(scope: GroupScope, label: &str, initial_open: bool) -> bool {
    let mut guard = GROUP_OPEN_STATE.lock().unwrap();
    let map = guard.get_or_insert_with(std::collections::HashMap::new);
    *map.entry((scope, label.to_owned())).or_insert(initial_open)
}

fn mark_effect_used(id: &str) {
    let mut recent = RECENT_EFFECT_IDS.lock().unwrap();
    recent.retain(|x| x != id);
    recent.insert(0, id.to_owned());
    recent.truncate(8);
}

/// エフェクトカタログの全件と、カテゴリ一覧（重複除去・昇順）を起動時に一度構築する。
/// フィルタ・ソートは`filtered()`が都度算出し、EffectAddDialog表示のたびに反映する。
struct EffectCatalogState {
    all: Vec<CatalogRow>,
    categories: Vec<SharedString>,
}

impl EffectCatalogState {
    fn build() -> Self {
        let mut all: Vec<CatalogRow> = crate::effects::loader::registry()
            .iter()
            .map(|p| CatalogRow {
                id: p.id.clone().into(),
                name: p.name.clone().into(),
                category: p.category.clone().into(),
            })
            .collect();
        all.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));

        let mut categories: Vec<SharedString> = all.iter().map(|r| r.category.clone()).collect();
        categories.sort();
        categories.dedup();

        Self { all, categories }
    }

    /// sort_mode: 0=カテゴリ順, 1=名前順, 2=最近使用順
    fn filtered(&self, query: &str, sort_mode: i32, category: &str) -> Vec<CatalogRow> {
        let q = query.to_lowercase();
        let mut rows: Vec<CatalogRow> = self
            .all
            .iter()
            .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q))
            .filter(|r| category.is_empty() || r.category.as_str() == category)
            .cloned()
            .collect();

        match sort_mode {
            1 => rows.sort_by(|a, b| a.name.cmp(&b.name)),
            2 => {
                let recent = RECENT_EFFECT_IDS.lock().unwrap();
                rows.sort_by_key(|r| {
                    recent
                        .iter()
                        .position(|id| id.as_str() == r.id.as_str())
                        .unwrap_or(usize::MAX)
                });
            }
            _ => rows.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name))),
        }
        rows
    }
}

/// EffectAddDialogの検索・ソート・カテゴリ操作をカタログ再算出へ配線する。
/// confirm/cancelもここで確定し、setup()側は生成・表示要求のみを担う。
fn wire_effect_add_dialog(
    dialog: &EffectAddDialog,
    catalog_state: &Rc<EffectCatalogState>,
    props_weak: &slint::Weak<PropertiesWindow>,
) {
    dialog.set_categories(ModelRc::new(VecModel::from(
        catalog_state.categories.clone(),
    )));

    let refresh = {
        let dialog_weak = dialog.as_weak();
        let catalog_state = catalog_state.clone();
        move || {
            let Some(d) = dialog_weak.upgrade() else {
                return;
            };
            let rows = catalog_state.filtered(
                d.get_query().as_str(),
                d.get_sort_mode(),
                d.get_category_filter().as_str(),
            );
            d.set_catalog(ModelRc::new(VecModel::from(rows)));
        }
    };
    refresh();

    {
        let refresh = refresh.clone();
        dialog.on_query_changed(move |_| refresh());
    }
    {
        let refresh = refresh.clone();
        dialog.on_sort_changed(move |_| refresh());
    }
    {
        let refresh = refresh.clone();
        dialog.on_category_changed(move |_| refresh());
    }

    {
        let props_weak = props_weak.clone();
        let dialog_weak = dialog.as_weak();
        dialog.on_confirm(move |id| {
            if let Some(p) = props_weak.upgrade() {
                p.invoke_add_effect(id.clone());
            }
            mark_effect_used(id.as_str());
            if let Some(d) = dialog_weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    {
        let dialog_weak = dialog.as_weak();
        dialog.on_cancel(move || {
            if let Some(d) = dialog_weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
}

unsafe extern "C" fn keyframe_edit_trampoline(
    user_data: *mut std::ffi::c_void,
    result: EditResultC,
) {
    let boxed: Box<Box<dyn FnOnce(EditResultC)>> =
        unsafe { Box::from_raw(user_data as *mut Box<dyn FnOnce(EditResultC)>) };
    (*boxed)(result);
}

pub fn setup(
    props: &PropertiesWindow,
    _preview_weak: slint::Weak<crate::PreviewWindow>,
    _timeline_weak: slint::Weak<crate::TimelineWindow>,
    state: SharedAppState,
) {
    {
        let (state, pw) = (state.clone(), props.as_weak());
        props.on_open_keyframe_editor(move |_group, key, effect_index, frame| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let oid = id as usize;

            let world_holder = app_state::active_world(&state);
            let world = world_holder.lock().unwrap();

            let keyframes = if effect_index < 0 {
                world.get_keyframes(oid, key.as_str())
            } else {
                world.get_effect_keyframes(oid, effect_index as usize, key.as_str())
            };
            let Some(target) = keyframes.iter().find(|k| k.frame == frame) else {
                return;
            };
            let engine_id = target.engine_id.clone();
            let Some(eng) = crate::easings::loader::by_id(&engine_id) else {
                return;
            };

            let payloads: Vec<Vec<u8>> =
                keyframes.iter().map(|k| k.engine_payload.clone()).collect();
            let keyframes_c: Vec<KeyframeC> = keyframes
                .iter()
                .zip(payloads.iter())
                .map(|(k, payload)| KeyframeC {
                    frame: k.frame,
                    value: k.value,
                    payload_ptr: if payload.is_empty() {
                        std::ptr::null()
                    } else {
                        payload.as_ptr()
                    },
                    payload_len: payload.len(),
                })
                .collect();
            drop(world);

            let key_owned = key.to_string();
            let (state_cb, pw_cb) = (state.clone(), pw.clone());
            let on_result: Box<dyn FnOnce(EditResultC)> = Box::new(move |result: EditResultC| {
                if result.code != EditResultCode::Success || result.keyframes_ptr.is_null() {
                    return;
                }
                let Some(p) = pw_cb.upgrade() else {
                    unsafe { (eng.vtable.free_keyframes)(result.keyframes_ptr, result.count) };
                    return;
                };
                let world_holder = app_state::active_world(&state_cb);
                let mut world = world_holder.lock().unwrap();
                let edited =
                    unsafe { std::slice::from_raw_parts(result.keyframes_ptr, result.count) };
                for kc in edited {
                    let payload = if kc.payload_ptr.is_null() || kc.payload_len == 0 {
                        Vec::new()
                    } else {
                        unsafe { std::slice::from_raw_parts(kc.payload_ptr, kc.payload_len) }
                            .to_vec()
                    };
                    if effect_index < 0 {
                        world.set_keyframe(
                            oid,
                            &key_owned,
                            kc.frame,
                            kc.value,
                            engine_id.clone(),
                            payload,
                        );
                    } else {
                        world.set_effect_keyframe(
                            oid,
                            effect_index as usize,
                            &key_owned,
                            kc.frame,
                            kc.value,
                            engine_id.clone(),
                            payload,
                        );
                    }
                }
                unsafe { (eng.vtable.free_keyframes)(result.keyframes_ptr, result.count) };
                refresh(&p, &world);
            });
            let user_data = Box::into_raw(Box::new(on_result)) as *mut std::ffi::c_void;

            unsafe {
                (eng.vtable.open_editor_window)(
                    std::ptr::null(),
                    keyframes_c.as_ptr(),
                    keyframes_c.len(),
                    keyframe_edit_trampoline,
                    user_data,
                );
            }
        });
    }

    {
        let catalog_state = Rc::new(EffectCatalogState::build());
        let dialog_slot: Rc<RefCell<Option<EffectAddDialog>>> = Rc::new(RefCell::new(None));
        let pw = props.as_weak();
        props.on_open_effect_add_dialog(move || {
            let mut slot = dialog_slot.borrow_mut();
            if slot.is_none() {
                let Ok(dialog) = EffectAddDialog::new() else {
                    return;
                };
                wire_effect_add_dialog(&dialog, &catalog_state, &pw);
                *slot = Some(dialog);
            }
            if let Some(d) = slot.as_ref() {
                let _ = d.show();
            }
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_object_param_segment(move |group, key, frame, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let oid = id as usize;
            let (clip_start, clip_end) = world.get_time_range(oid);
            let base = current_object_param_value(&world, oid, group.as_str(), key.as_str());
            write_segment_value(
                |w| w.get_keyframes(oid, key.as_str()),
                |w, f, v, eng_id, payload| w.set_keyframe(oid, key.as_str(), f, v, eng_id, payload),
                &mut world,
                clip_start,
                clip_end,
                base,
                frame,
                value,
            );
            let track = world.get_keyframes(oid, key.as_str());
            let current_frame = world.current_frame();
            drop(world);
            let seg = resolve_segment(&track, clip_start, clip_end, current_frame, base);
            update_object_param_segment(&p, group.as_str(), key.as_str(), &seg);
        });
    }

    {
        let state = state.clone();
        props.on_commit_object_param(move |_group, _key| {
            app_state::snapshot_before_edit(&state);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_object_param_bool(move |group, key, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            apply_object_param(
                &mut world,
                id as usize,
                group.as_str(),
                key.as_str(),
                if value { 1.0 } else { 0.0 },
            );
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_object_param_text(move |group, key, text| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            apply_object_param_text(
                &mut world,
                id as usize,
                group.as_str(),
                key.as_str(),
                text.as_str(),
            );
            drop(world);
            update_object_param_text(&p, group.as_str(), key.as_str(), text.as_str());
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_pick_object_folder(move |group, key| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let Some(path) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            let path_str = path.to_string_lossy().into_owned();
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            apply_object_param_text(
                &mut world,
                id as usize,
                group.as_str(),
                key.as_str(),
                path_str.as_str(),
            );
            refresh(&p, &world);
        });
    }

    {
        let pw = props.as_weak();
        let state = state.clone();
        props.on_toggle_object_settings_group(move |_group, label| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            toggle_group_open(GroupScope::Object(id), &label);
            let world_holder = app_state::active_world(&state);
            let world = world_holder.lock().unwrap();
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_effect_enabled(move |index, enabled| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_effect_enabled(id as usize, index as usize, enabled);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_remove_effect(move |index| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.remove_effect(id as usize, index as usize);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_param_segment(move |index, key, frame, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let oid = id as usize;
            let eidx = index as usize;
            let (clip_start, clip_end) = world.get_time_range(oid);
            let base = current_effect_param_value(&world, oid, eidx, key.as_str());
            write_segment_value(
                |w| w.get_effect_keyframes(oid, eidx, key.as_str()),
                |w, f, v, eng_id, payload| {
                    w.set_effect_keyframe(oid, eidx, key.as_str(), f, v, eng_id, payload)
                },
                &mut world,
                clip_start,
                clip_end,
                base,
                frame,
                value,
            );
            let track = world.get_effect_keyframes(oid, eidx, key.as_str());
            let current_frame = world.current_frame();
            drop(world);
            let seg = resolve_segment(&track, clip_start, clip_end, current_frame, base);
            update_effect_param_segment(&p, index, key.as_str(), &seg);
        });
    }

    {
        let state = state.clone();
        props.on_commit_param(move |_index, _key| {
            app_state::snapshot_before_edit(&state);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_param_bool(move |index, key, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_effect_param_bool(id as usize, index as usize, key.as_str(), value);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_param_enum(move |index, key, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_effect_param_enum(
                id as usize,
                index as usize,
                key.as_str(),
                value.max(0) as u32,
            );
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_param_text(move |index, key, text| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_effect_param_path(
                id as usize,
                index as usize,
                key.as_str(),
                text.to_string(),
            );
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_pick_param_file(move |index, key| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let Some(path) = rfd::FileDialog::new().pick_file() else {
                return;
            };
            let path_str = path.to_string_lossy().into_owned();
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_effect_param_path(id as usize, index as usize, key.as_str(), path_str);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_pick_param_folder(move |index, key| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let Some(path) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            let path_str = path.to_string_lossy().into_owned();
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_effect_param_path(id as usize, index as usize, key.as_str(), path_str);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_toggle_param_settings_group(move |index, label| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            toggle_group_open(GroupScope::Effect(id, index), label.as_str());
            let world_holder = app_state::active_world(&state);
            let world = world_holder.lock().unwrap();
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_param_track(move |index, key, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_effect_param_track_ref(id as usize, index as usize, key.as_str(), value);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_add_effect(move |effect_id| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.add_effect(id as usize, effect_id.as_str());
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_move_effect(move |from, to| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 || from < 0 || to < 0 {
                return;
            }
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.reorder_effect(id as usize, from as usize, to as usize);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_copy_effect(move |index| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 || index < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let world = world_holder.lock().unwrap();
            if let Some(instance) = world.get_effect_instance(id as usize, index as usize) {
                *EFFECT_CLIPBOARD.lock().unwrap() = Some(instance);
            }
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_paste_effect(move |index| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let Some(instance) = EFFECT_CLIPBOARD.lock().unwrap().clone() else {
                return;
            };
            app_state::snapshot_before_edit(&state);
            let insert_at = if index < 0 { 0 } else { index as usize + 1 };
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.insert_effect(id as usize, insert_at, instance);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_duplicate_effect(move |index| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 || index < 0 {
                return;
            }
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.duplicate_effect(id as usize, index as usize);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_add_midpoint(move |group, key, effect_index, frame| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let oid = id as usize;
            let (clip_start, clip_end) = world.get_time_range(oid);
            let frame = frame.clamp(clip_start, clip_end);
            if effect_index < 0 {
                let base = current_object_param_value(&world, oid, group.as_str(), key.as_str());
                let existing = world.get_keyframes(oid, key.as_str());
                let value = resolve_display_value(base, &existing, frame);
                write_segment_value(
                    |w| w.get_keyframes(oid, key.as_str()),
                    |w, f, v, eng_id, payload| {
                        w.set_keyframe(oid, key.as_str(), f, v, eng_id, payload)
                    },
                    &mut world,
                    clip_start,
                    clip_end,
                    base,
                    frame,
                    value,
                );
                let track = world.get_keyframes(oid, key.as_str());
                let current_frame = world.current_frame();
                drop(world);
                let seg = resolve_segment(&track, clip_start, clip_end, current_frame, base);
                update_object_param_segment(&p, group.as_str(), key.as_str(), &seg);
            } else {
                let eidx = effect_index as usize;
                let base = current_effect_param_value(&world, oid, eidx, key.as_str());
                let existing = world.get_effect_keyframes(oid, eidx, key.as_str());
                let value = resolve_display_value(base, &existing, frame);
                write_segment_value(
                    |w| w.get_effect_keyframes(oid, eidx, key.as_str()),
                    |w, f, v, eng_id, payload| {
                        w.set_effect_keyframe(oid, eidx, key.as_str(), f, v, eng_id, payload)
                    },
                    &mut world,
                    clip_start,
                    clip_end,
                    base,
                    frame,
                    value,
                );
                let track = world.get_effect_keyframes(oid, eidx, key.as_str());
                let current_frame = world.current_frame();
                drop(world);
                let seg = resolve_segment(&track, clip_start, clip_end, current_frame, base);
                update_effect_param_segment(&p, effect_index, key.as_str(), &seg);
            }
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_remove_midpoint(move |group, key, effect_index, frame| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let oid = id as usize;
            let world_holder = app_state::active_world(&state);
            let (clip_start, clip_end) = {
                let world = world_holder.lock().unwrap();
                world.get_time_range(oid)
            };
            if frame == clip_start || frame == clip_end {
                return;
            }
            app_state::snapshot_before_edit(&state);
            let mut world = world_holder.lock().unwrap();
            if effect_index < 0 {
                let base = current_object_param_value(&world, oid, group.as_str(), key.as_str());
                world.remove_keyframe(oid, key.as_str(), frame);
                let track = world.get_keyframes(oid, key.as_str());
                let current_frame = world.current_frame();
                drop(world);
                let seg = resolve_segment(&track, clip_start, clip_end, current_frame, base);
                update_object_param_segment(&p, group.as_str(), key.as_str(), &seg);
            } else {
                let eidx = effect_index as usize;
                let base = current_effect_param_value(&world, oid, eidx, key.as_str());
                world.remove_effect_keyframe(oid, eidx, key.as_str(), frame);
                let track = world.get_effect_keyframes(oid, eidx, key.as_str());
                let current_frame = world.current_frame();
                drop(world);
                let seg = resolve_segment(&track, clip_start, clip_end, current_frame, base);
                update_effect_param_segment(&p, effect_index, key.as_str(), &seg);
            }
        });
    }

    {
        let state = state.clone();
        props.on_commit_midpoint_drag(move |_group, _key, _effect_index| {
            app_state::snapshot_before_edit(&state);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_move_midpoint(move |group, key, effect_index, old_frame, new_frame| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let oid = id as usize;
            let (clip_start, clip_end) = world.get_time_range(oid);
            if old_frame == clip_start || old_frame == clip_end {
                return;
            }
            if effect_index < 0 {
                let existing = world.get_keyframes(oid, key.as_str());
                let (lo, hi) = clamp_to_neighbors(&existing, clip_start, clip_end, old_frame);
                if lo > hi {
                    return;
                }
                let new_frame = new_frame.clamp(lo, hi);
                world.move_keyframe(oid, key.as_str(), old_frame, new_frame);
                let base = current_object_param_value(&world, oid, group.as_str(), key.as_str());
                let track = world.get_keyframes(oid, key.as_str());
                let current_frame = world.current_frame();
                drop(world);
                let seg = resolve_segment(&track, clip_start, clip_end, current_frame, base);
                update_object_param_segment(&p, group.as_str(), key.as_str(), &seg);
            } else {
                let eidx = effect_index as usize;
                let existing = world.get_effect_keyframes(oid, eidx, key.as_str());
                let (lo, hi) = clamp_to_neighbors(&existing, clip_start, clip_end, old_frame);
                if lo > hi {
                    return;
                }
                let new_frame = new_frame.clamp(lo, hi);
                world.move_effect_keyframe(oid, eidx, key.as_str(), old_frame, new_frame);
                let base = current_effect_param_value(&world, oid, eidx, key.as_str());
                let track = world.get_effect_keyframes(oid, eidx, key.as_str());
                let current_frame = world.current_frame();
                drop(world);
                let seg = resolve_segment(&track, clip_start, clip_end, current_frame, base);
                update_effect_param_segment(&p, effect_index, key.as_str(), &seg);
            }
        });
    }
}

pub fn select_object(props: &PropertiesWindow, world: &EcsWorld, object_id: i32) {
    props.set_object_id(object_id);
    refresh(props, world);
}

/// プロパティパネル表示・スライダー描画は必ずこの関数を経由し、静的値の直接表示を禁止する
/// （静的値表示は中間点追加後もスライダーへ反映されず「常に左右同期」して見える不具合の原因だった）。
fn resolve_display_value(base: f32, keyframes: &[Keyframe], frame: i32) -> f32 {
    if keyframes.is_empty() {
        base
    } else {
        let first_engine = &keyframes[0].engine_id;
        let eng = crate::easings::loader::by_id(first_engine);
        let raw: Vec<(i32, f32, Vec<u8>)> = keyframes
            .iter()
            .map(|k| (k.frame, k.value, k.engine_payload.clone()))
            .collect();
        if let Some(e) = eng {
            e.evaluate(&raw, frame, base)
        } else {
            base
        }
    }
}

fn keyframe_payload_for_write(keyframes: &[Keyframe], frame: i32) -> (String, Vec<u8>) {
    keyframes
        .iter()
        .find(|k| k.frame == frame)
        .map(|k| (k.engine_id.clone(), k.engine_payload.clone()))
        .unwrap_or_else(|| ("neoutl-easing-standard".to_string(), Vec::new()))
}

/// old_frameの左右に隣接する境界点を探し、new_frameの許容範囲(lo, hi)を返す。
/// 中間点ドラッグが隣接点を追い越して並び順を破壊しないよう、機械的に範囲制限する。
fn clamp_to_neighbors(
    keyframes: &[Keyframe],
    clip_start: i32,
    clip_end: i32,
    old_frame: i32,
) -> (i32, i32) {
    let mut boundary: Vec<i32> = std::iter::once(clip_start)
        .chain(keyframes.iter().map(|k| k.frame))
        .chain(std::iter::once(clip_end))
        .collect();
    boundary.sort_unstable();
    boundary.dedup();
    let pos = boundary.iter().position(|&f| f == old_frame).unwrap_or(0);
    let lo = boundary
        .get(pos.wrapping_sub(1))
        .copied()
        .unwrap_or(clip_start)
        + 1;
    let hi = boundary.get(pos + 1).copied().unwrap_or(clip_end) - 1;
    (lo, hi)
}

/// エフェクトパラメータ(key)の基準値(static_value)を読み出す。区間書き込み時の
/// シード値・中間点追加時の補間基準に使う。on_set_param_segmentと同一ロジック。
fn current_effect_param_value(world: &EcsWorld, oid: usize, eidx: usize, key: &str) -> f32 {
    world
        .get_effect_instance(oid, eidx)
        .and_then(|inst| {
            inst.params.get(key).map(|p| match &p.static_value {
                crate::ecs::types::Value::Number(n) => *n,
                _ => 0.0,
            })
        })
        .unwrap_or(0.0)
}

/// 中間点区間の解決結果。boundary_framesは常に2要素以上（clip_start, clip_end含む）。
struct SegmentInfo {
    boundary_frames: Vec<i32>,
    start_frame: i32,
    end_frame: i32,
    start_value: f32,
    end_value: f32,
    /// クランプ前の現在フレームにおける実効値。ParamRow.valueの表示に使う
    /// （区間の両端値とは別に、現在の再生位置そのものの値を保持する）。
    current_value: f32,
}

/// clip範囲・既存中間点・現在フレームから、現在フレームを内包する区間を確定する。
/// frameがclip範囲外の場合はclip_start/clip_endへクランプしてから探索する。
/// 中間点0件時はboundary_framesが[clip_start, clip_end]の2点のみとなり、
/// 区間全体が単一区間として扱われる（両端点は常に実在する境界として提示する）。
fn resolve_segment(
    keyframes: &[Keyframe],
    clip_start: i32,
    clip_end: i32,
    frame: i32,
    base: f32,
) -> SegmentInfo {
    let frame = frame.clamp(clip_start, clip_end);
    let mut boundary_frames: Vec<i32> = std::iter::once(clip_start)
        .chain(keyframes.iter().map(|k| k.frame))
        .chain(std::iter::once(clip_end))
        .collect();
    boundary_frames.sort_unstable();
    boundary_frames.dedup();

    let start_frame = boundary_frames
        .iter()
        .rev()
        .find(|&&f| f <= frame)
        .copied()
        .unwrap_or(clip_start);
    let end_frame = boundary_frames
        .iter()
        .find(|&&f| f > start_frame)
        .copied()
        .unwrap_or(clip_end);

    SegmentInfo {
        start_value: resolve_display_value(base, keyframes, start_frame),
        end_value: resolve_display_value(base, keyframes, end_frame),
        current_value: resolve_display_value(base, keyframes, frame),
        boundary_frames,
        start_frame,
        end_frame,
    }
}

/// 中間点0件時に片側だけ編集された場合、両端点(clip_start, clip_end)をbase値で
/// 先に実点化してから、対象frameのみ新しい値へ上書きする。この関数を経由しない
/// 直接set_keyframe呼び出しは、区間モデルの「両端が常に実在する」前提を破壊するため禁止する。
/// clip_start == clip_endの縮退区間（幅0のクリップ）ではシードを行わずframeのみ書き込む。
fn write_segment_value(
    keyframes_of: impl Fn(&EcsWorld) -> Vec<Keyframe>,
    mut set_kf: impl FnMut(&mut EcsWorld, i32, f32, String, Vec<u8>),
    world: &mut EcsWorld,
    clip_start: i32,
    clip_end: i32,
    base: f32,
    frame: i32,
    value: f32,
) {
    let existing = keyframes_of(world);
    if existing.is_empty() && clip_start != clip_end {
        set_kf(
            world,
            clip_start,
            base,
            "neoutl-easing-standard".to_string(),
            Vec::new(),
        );
        set_kf(
            world,
            clip_end,
            base,
            "neoutl-easing-standard".to_string(),
            Vec::new(),
        );
    }
    let existing = keyframes_of(world);
    let (eng_id, payload) = keyframe_payload_for_write(&existing, frame);
    set_kf(world, frame, value, eng_id, payload);
}

/// object-params一行分の書き込みを、スキーマのgroup/keyから該当コンポーネントへ振り分ける。
/// key単位のフィールド選択はParamAccess::set_param（各コンポーネント定義側）に委譲する。
/// ここではgroup名から対象コンポーネントを選び、読み出し→trait経由の書き込み→保存のみを行う。
fn apply_object_param(world: &mut EcsWorld, oid: usize, group: &str, key: &str, value: f32) {
    match group {
        TRANSFORM_GROUP => {
            let mut t = world.get_transform(oid).unwrap_or_default();
            if t.set_param(key, value) {
                world.set_transform(oid, t);
            }
        }
        TEXT_GROUP => {
            let mut t = world.get_text(oid).unwrap_or_default();
            if t.set_param(key, value) {
                world.set_text(oid, t.text, t.font_size);
            }
        }
        SHAPE_GROUP => {
            let mut s: ShapeParams = world.get_shape(oid).unwrap_or_default();
            if s.set_param(key, value) {
                world.set_shape(oid, s);
            }
        }
        AUDIO_GROUP => {
            let mut a = world.get_audio_params(oid).unwrap_or_default();
            if a.set_param(key, value) {
                world.set_audio_params(oid, a.volume, a.pan, a.mute);
            }
        }
        _ => {
            world.set_plugin_param(oid, key, value);
        }
    }
}

/// 中間点編集ウィンドウが「新規作成」時にvalue初期値として使う現在値を取得する。
/// apply_object_paramと同一のgroup分岐だが、書き込みではなく読み出しのみを行う。
pub(crate) fn current_object_param_value(
    world: &EcsWorld,
    oid: usize,
    group: &str,
    key: &str,
) -> f32 {
    match group {
        TRANSFORM_GROUP => world
            .get_transform(oid)
            .and_then(|t| t.get_param(key))
            .unwrap_or(0.0),
        TEXT_GROUP => world
            .get_text(oid)
            .and_then(|t| t.get_param(key))
            .unwrap_or(0.0),
        SHAPE_GROUP => world
            .get_shape(oid)
            .and_then(|s: ShapeParams| s.get_param(key))
            .unwrap_or(0.0),
        AUDIO_GROUP => world
            .get_audio_params(oid)
            .and_then(|a| a.get_param(key))
            .unwrap_or(0.0),
        _ => world
            .get_plugin_params(oid)
            .and_then(|p| p.get(key).copied())
            .unwrap_or(0.0),
    }
}

/// ParamKind::Text専用の書き込み経路。現状ホスト内蔵ではTEXT_GROUPの"text"キーのみが対象。
fn apply_object_param_text(world: &mut EcsWorld, oid: usize, group: &str, key: &str, text: &str) {
    if group == TEXT_GROUP && key == "text" {
        let cur = world.get_text(oid).unwrap_or_default();
        world.set_text(oid, text.to_owned(), cur.font_size);
    }
}

/// スキーマ配列を現在値で解決し、ParamRow列へ展開する。
/// stage-relativeレンジ（X/Y/Z）はここでピクセル値へ確定する。
/// get_text: kind==Textの行にのみ使用。対象外keyにはNoneを返せばよい。
/// kind==Track行の選択候補。自分自身(exclude_oid)を除くアクティブシーン内の全オブジェクトを
/// 列挙する（ドキュメント記載の最小スコープ「他オブジェクト参照」に基づく）。
/// kind==Track行の選択候補一式。TrackOption列と、ComboBox.model用に事前展開した
/// 表示名平坦配列(names)を対で保持する（Slint側は式内で[struct]への.map()を書けないため）。
struct TrackOptions {
    options: ModelRc<crate::TrackOption>,
    names: ModelRc<SharedString>,
}

/// kind==Track行の選択候補。自分自身(exclude_oid)を除くアクティブシーン内の全オブジェクトを
/// 列挙する（ドキュメント記載の最小スコープ「他オブジェクト参照」に基づく）。
fn build_track_options(world: &EcsWorld, exclude_oid: usize) -> TrackOptions {
    let options: Vec<crate::TrackOption> = world
        .get_timeline_objects()
        .into_iter()
        .filter(|o| o.id as usize != exclude_oid)
        .map(|o| crate::TrackOption {
            id: o.id,
            name: SharedString::from(format!("#{} (layer{})", o.id, o.layer)),
        })
        .collect();
    let names: Vec<SharedString> = options.iter().map(|o| o.name.clone()).collect();
    TrackOptions {
        options: ModelRc::new(VecModel::from(options)),
        names: ModelRc::new(VecModel::from(names)),
    }
}

fn empty_track_options() -> TrackOptions {
    TrackOptions {
        options: ModelRc::new(VecModel::from(Vec::<crate::TrackOption>::new())),
        names: ModelRc::new(VecModel::from(Vec::<SharedString>::new())),
    }
}

fn push_schema_rows(
    out: &mut Vec<ParamRow>,
    schema: &'static [crate::ecs::object_schema::ParamSchema],
    scope: GroupScope,
    stage_w: f32,
    stage_h: f32,
    clip_start: i32,
    clip_end: i32,
    current_frame: i32,
    get: impl Fn(&str) -> f32,
    get_text: impl Fn(&str) -> Option<String>,
    keyframes: impl Fn(&str) -> Vec<Keyframe>,
    track_options: &TrackOptions,
) {
    let mut collapsed = false;
    for s in schema {
        if s.kind == ParamKind::Group {
            let initial_open = match s.range {
                crate::ecs::object_schema::Range::Fixed(_, max) => max > 0.5,
                _ => true,
            };
            collapsed = !is_group_open(scope, s.label, initial_open);
        } else if collapsed {
            continue;
        }
        if !crate::ecs::object_schema::is_visible(s, &get) {
            continue;
        }
        let (min, max) = resolve_range(s.range, stage_w, stage_h);
        let track = keyframes(s.key);
        let non_interpolable = matches!(
            s.kind,
            ParamKind::Text
                | ParamKind::Enum
                | ParamKind::FilePath
                | ParamKind::Track
                | ParamKind::Separator
                | ParamKind::Group
                | ParamKind::Folder
        );
        let base = if non_interpolable { 0.0 } else { get(s.key) };
        let frames: Vec<i32> = track.iter().map(|k| k.frame).collect();
        let seg = resolve_segment(&track, clip_start, clip_end, current_frame, base);
        let enum_index = if s.kind == ParamKind::Enum {
            get(s.key) as i32
        } else if s.kind == ParamKind::Track {
            get(s.key) as i32
        } else if s.kind == ParamKind::Group {
            if collapsed { 0 } else { 1 }
        } else {
            0
        };
        out.push(ParamRow {
            effect_index: -1,
            key: SharedString::from(s.key),
            label: SharedString::from(s.label),
            group: SharedString::from(s.group),
            value: seg.current_value,
            kind: match s.kind {
                ParamKind::Float => 0,
                ParamKind::Bool => 1,
                ParamKind::Color => 2,
                ParamKind::Enum => 3,
                ParamKind::Text => 4,
                ParamKind::FilePath => 5,
                ParamKind::Track => 6,
                ParamKind::Separator => 7,
                ParamKind::Group => 8,
                ParamKind::Folder => 9,
            },
            min,
            max,
            text: SharedString::from(get_text(s.key).unwrap_or_default()),
            enum_options: ModelRc::new(VecModel::from(
                s.enum_options
                    .iter()
                    .map(|o| SharedString::from(*o))
                    .collect::<Vec<_>>(),
            )),
            enum_index,
            track_options: track_options.options.clone(),
            track_names: track_options.names.clone(),
            has_keyframes: !frames.is_empty(),
            keyframe_frames: ModelRc::new(VecModel::from(frames)),
            boundary_frames: ModelRc::new(VecModel::from(seg.boundary_frames)),
            segment_start_frame: seg.start_frame,
            segment_end_frame: seg.end_frame,
            segment_start_value: seg.start_value,
            segment_end_value: seg.end_value,
        });
    }
}

/// C ABI越しのParamSchema配列（オブジェクトプラグイン・エフェクトプラグイン共通形式）を
/// 現在値で解決しParamRow列へ展開する。両プラグイン種別はneoutl-shared-abi::ParamSchemaを
/// 共有するため、この一関数で処理できる（Phase6: push_plugin_rowsとエフェクトパラメータ
/// 生成ループの重複を解消）。
/// current: Float/Bool/Color/Enum/Trackのf32表現（Enum/Trackはインデックス/object_idをf32化）。
/// text: Text/FilePathの文字列値。対象外kindの行にはNoneを返せばよい。
fn push_c_abi_param_rows(
    out: &mut Vec<ParamRow>,
    schema: &[neoutl_shared_abi::ParamSchema],
    scope: GroupScope,
    group: &str,
    effect_index: i32,
    clip_start: i32,
    clip_end: i32,
    current_frame: i32,
    current: impl Fn(&str) -> f32,
    text: impl Fn(&str) -> Option<String>,
    keyframes: impl Fn(&str) -> Vec<Keyframe>,
    track_options: &TrackOptions,
) {
    let mut collapsed = false;
    for s in schema {
        let key = unsafe { s.key.as_str() };
        let label = unsafe { s.label.as_str() };
        if s.kind == ParamKind::Group {
            let initial_open = s.default_float != 0.0;
            collapsed = !is_group_open(scope, label, initial_open);
        } else if collapsed {
            continue;
        }
        let base = current(key);
        let track = keyframes(key);
        let frames: Vec<i32> = track.iter().map(|k| k.frame).collect();
        let seg = resolve_segment(&track, clip_start, clip_end, current_frame, base);
        let enum_options: Vec<SharedString> =
            neoutl_shared_abi::split_enum_options(unsafe { s.enum_options.as_str() })
                .into_iter()
                .map(SharedString::from)
                .collect();
        let enum_index = if s.kind == ParamKind::Group {
            if collapsed { 0 } else { 1 }
        } else {
            base as i32
        };
        out.push(ParamRow {
            effect_index,
            key: SharedString::from(key),
            label: SharedString::from(label),
            group: SharedString::from(group),
            value: seg.current_value,
            kind: match s.kind {
                ParamKind::Float => 0,
                ParamKind::Bool => 1,
                ParamKind::Color => 2,
                ParamKind::Enum => 3,
                ParamKind::Text => 4,
                ParamKind::FilePath => 5,
                ParamKind::Track => 6,
                ParamKind::Separator => 7,
                ParamKind::Group => 8,
                ParamKind::Folder => 9,
            },
            min: s.min,
            max: s.max,
            text: SharedString::from(text(key).unwrap_or_default()),
            enum_options: ModelRc::new(VecModel::from(enum_options)),
            enum_index,
            track_options: track_options.options.clone(),
            track_names: track_options.names.clone(),
            has_keyframes: !frames.is_empty(),
            keyframe_frames: ModelRc::new(VecModel::from(frames)),
            boundary_frames: ModelRc::new(VecModel::from(seg.boundary_frames)),
            segment_start_frame: seg.start_frame,
            segment_end_frame: seg.end_frame,
            segment_start_value: seg.start_value,
            segment_end_value: seg.end_value,
        });
    }
}

/// プラグイン提供オブジェクトのObjectMeta.property_schemaをParamRow列へ展開する。
/// 現在値はPluginParams（未設定ならスキーマのdefault_float）から取得する。
///
/// 注意: レンダリング側（renderer/pipeline.rs::write_standard_uniform）はShape系の
/// パラメータをネイティブのShapeParamsコンポーネント（object_schema::SHAPE_SCHEMA、
/// group="図形"）からのみ読み出し、PluginParamsは一切参照しない。Text系も同様に
/// TextContentコンポーネント（object_schema::TEXT_SCHEMA、group="テキスト"）からのみ
/// 読み出す。そのためこの関数が生成するplugin.name群の行を編集しても描画には反映されない
/// （SHAPE_SCHEMA・TEXT_SCHEMA側の行を編集すること）。has_shape/has_textがtrue、
/// すなわちネイティブスキーマの行が既に同じ内容をカバーしている場合はここでの
/// 重複行生成をスキップし、「操作してもガン無視される」編集不能な行をUI上に
/// 出さないようにする。
fn push_plugin_rows(
    out: &mut Vec<ParamRow>,
    world: &EcsWorld,
    oid: usize,
    clip_start: i32,
    clip_end: i32,
    current_frame: i32,
) {
    if world.get_shape(oid).is_some() {
        return;
    }
    if world.get_text(oid).is_some() {
        return;
    }
    let Some(kind_id) = world.get_kind_id(oid) else {
        return;
    };
    let Some(plugin) = crate::objects::loader::by_kind_id(kind_id) else {
        return;
    };
    let meta = unsafe { &*((plugin.vtable.meta)()) };
    if meta.property_schema_ptr.is_null() || meta.property_schema_len == 0 {
        return;
    }
    let schema =
        unsafe { std::slice::from_raw_parts(meta.property_schema_ptr, meta.property_schema_len) };
    let current = world.get_plugin_params(oid).unwrap_or_default();
    let track_options = empty_track_options();
    push_c_abi_param_rows(
        out,
        schema,
        GroupScope::Object(oid as i32),
        &plugin.name,
        -1,
        clip_start,
        clip_end,
        current_frame,
        |key| {
            current.get(key).copied().unwrap_or_else(|| {
                schema
                    .iter()
                    .find(|s| unsafe { s.key.as_str() } == key)
                    .map_or(0.0, |s| s.default_float)
            })
        },
        |_| None,
        |_| Vec::new(),
        &track_options,
    );
}

/// object_paramsモデルの該当行(group/key一致)のみ区間フィールドを書き換える。
/// ModelRcの同一性を保つため、Slint側のコンポーネント再構築(=ドラッグ状態/
/// テキスト選択状態の喪失)を発生させない。構造変化を伴わない値更新はこの経路を使う。
fn update_object_param_segment(
    props: &PropertiesWindow,
    group: &str,
    key: &str,
    seg: &SegmentInfo,
) {
    let model = props.get_object_params();
    for i in 0..model.row_count() {
        let Some(mut row) = model.row_data(i) else {
            continue;
        };
        if row.group.as_str() == group && row.key.as_str() == key {
            apply_segment_to_row(&mut row, seg);
            model.set_row_data(i, row);
            return;
        }
    }
}

/// ParamRow一行へSegmentInfoの内容を書き込む。row.valueは区間開始値をそのまま用いる
/// （タイムライン先頭からの通し値表示としては区間開始側が現在フレームの実効値に一致する）。
fn apply_segment_to_row(row: &mut ParamRow, seg: &SegmentInfo) {
    row.value = seg.current_value;
    row.has_keyframes = seg.boundary_frames.len() > 2;
    row.segment_start_frame = seg.start_frame;
    row.segment_end_frame = seg.end_frame;
    row.segment_start_value = seg.start_value;
    row.segment_end_value = seg.end_value;
    if row.boundary_frames.row_count() == seg.boundary_frames.len() {
        for (i, &f) in seg.boundary_frames.iter().enumerate() {
            row.boundary_frames.set_row_data(i, f);
        }
    } else {
        row.boundary_frames = ModelRc::new(VecModel::from(seg.boundary_frames.clone()));
    }
}

/// object_paramsモデルの該当行(group/key一致)のtextフィールドのみ書き換える。
/// kind==4(Text)行専用。update_object_param_segmentと同一方針。
fn update_object_param_text(props: &PropertiesWindow, group: &str, key: &str, text: &str) {
    let model = props.get_object_params();
    for i in 0..model.row_count() {
        let Some(mut row) = model.row_data(i) else {
            continue;
        };
        if row.group.as_str() == group && row.key.as_str() == key {
            row.text = SharedString::from(text);
            model.set_row_data(i, row);
            return;
        }
    }
}

/// paramsモデル(エフェクトパラメータ)の該当行(effect_index/key一致)のみ区間フィールドを
/// 書き換える。update_object_param_segmentと同一方針。
fn update_effect_param_segment(
    props: &PropertiesWindow,
    effect_index: i32,
    key: &str,
    seg: &SegmentInfo,
) {
    let model = props.get_params();
    for i in 0..model.row_count() {
        let Some(mut row) = model.row_data(i) else {
            continue;
        };
        if row.effect_index == effect_index && row.key.as_str() == key {
            apply_segment_to_row(&mut row, seg);
            model.set_row_data(i, row);
            return;
        }
    }
}

/// 再生・シークでcurrent_frameのみ変化した際に呼ぶ軽量パス。区間ハイライト
/// （segment-start/end-frame）と表示値のみを全行再計算する。boundary-frames自体は
/// 変化しないためモデル差し替えは行わない（フルrefresh()よりはるかに軽い）。
/// これを呼ばないと、中間点を直接操作しない限りハイライト区間が再生位置に追従しない。
pub fn refresh_current_frame(props: &PropertiesWindow, world: &EcsWorld) {
    let id = props.get_object_id();
    let current_frame = world.current_frame();
    props.set_current_frame(current_frame);
    if id < 0 {
        return;
    }
    let oid = id as usize;
    let (clip_start, clip_end) = world.get_time_range(oid);

    let object_model = props.get_object_params();
    for i in 0..object_model.row_count() {
        let Some(mut row) = object_model.row_data(i) else {
            continue;
        };
        if row.kind != 0 && row.kind != 2 {
            continue;
        }
        let keyframes = world.get_keyframes(oid, row.key.as_str());
        let base = current_object_param_value(world, oid, row.group.as_str(), row.key.as_str());
        let seg = resolve_segment(&keyframes, clip_start, clip_end, current_frame, base);
        apply_segment_to_row(&mut row, &seg);
        object_model.set_row_data(i, row);
    }

    let params_model = props.get_params();
    for i in 0..params_model.row_count() {
        let Some(mut row) = params_model.row_data(i) else {
            continue;
        };
        if row.effect_index < 0 || (row.kind != 0 && row.kind != 2) {
            continue;
        }
        let eidx = row.effect_index as usize;
        let keyframes = world.get_effect_keyframes(oid, eidx, row.key.as_str());
        let base = current_effect_param_value(world, oid, eidx, row.key.as_str());
        let seg = resolve_segment(&keyframes, clip_start, clip_end, current_frame, base);
        apply_segment_to_row(&mut row, &seg);
        params_model.set_row_data(i, row);
    }
}

fn refresh(props: &PropertiesWindow, world: &EcsWorld) {
    let id = props.get_object_id();
    if id < 0 {
        return;
    }
    let oid = id as usize;

    let project = world.get_project();
    let stage_w = project.width as f32;
    let stage_h = project.height as f32;
    props.set_stage_width(stage_w);
    props.set_stage_height(stage_h);
    props.set_total_frames(world.total_frames().max(1));
    let current_frame = world.current_frame();
    props.set_current_frame(current_frame);
    let (clip_start, clip_end) = world.get_time_range(oid);

    let mut object_params: Vec<ParamRow> = Vec::new();
    let track_options = build_track_options(world, oid);

    if let Some(t) = world.get_transform(oid) {
        props.set_has_transform(true);
        push_schema_rows(
            &mut object_params,
            TRANSFORM_SCHEMA,
            GroupScope::Object(oid as i32),
            stage_w,
            stage_h,
            clip_start,
            clip_end,
            current_frame,
            |k| t.get_param(k).unwrap_or(0.0),
            |_| None,
            |k| world.get_keyframes(oid, k),
            &track_options,
        );
    } else {
        props.set_has_transform(false);
    }

    if let Some(text) = world.get_text(oid) {
        props.set_has_text(true);
        let body = text.text.clone();
        push_schema_rows(
            &mut object_params,
            TEXT_SCHEMA,
            GroupScope::Object(oid as i32),
            stage_w,
            stage_h,
            clip_start,
            clip_end,
            current_frame,
            |k| text.get_param(k).unwrap_or(0.0),
            |k| (k == "text").then(|| body.clone()),
            |k| world.get_keyframes(oid, k),
            &track_options,
        );
    } else {
        props.set_has_text(false);
    }

    if let Some(shape) = world.get_shape(oid) {
        props.set_has_shape(true);
        push_schema_rows(
            &mut object_params,
            SHAPE_SCHEMA,
            GroupScope::Object(oid as i32),
            stage_w,
            stage_h,
            clip_start,
            clip_end,
            current_frame,
            |k| shape.get_param(k).unwrap_or(0.0),
            |_| None,
            |k| world.get_keyframes(oid, k),
            &track_options,
        );
    } else {
        props.set_has_shape(false);
    }

    if let Some(audio) = world.get_audio_params(oid) {
        props.set_has_audio(true);
        push_schema_rows(
            &mut object_params,
            AUDIO_SCHEMA,
            GroupScope::Object(oid as i32),
            stage_w,
            stage_h,
            clip_start,
            clip_end,
            current_frame,
            |k| audio.get_param(k).unwrap_or(0.0),
            |_| None,
            |k| world.get_keyframes(oid, k),
            &track_options,
        );
    } else {
        props.set_has_audio(false);
    }

    push_plugin_rows(
        &mut object_params,
        world,
        oid,
        clip_start,
        clip_end,
        current_frame,
    );

    props.set_object_params(ModelRc::new(VecModel::from(object_params)));

    let instances = world.get_effects(oid);
    let rows: Vec<EffectRow> = instances
        .iter()
        .enumerate()
        .map(|(i, e)| EffectRow {
            index: i as i32,
            name: find_effect(&e.effect_id)
                .map_or(e.effect_id.as_str(), |m| m.name)
                .into(),
            enabled: e.enabled,
            dragging: false,
        })
        .collect();
    props.set_effects(ModelRc::new(VecModel::from(rows)));

    let mut params = Vec::new();
    for (i, e) in instances.iter().enumerate() {
        let Some(meta) = find_effect(&e.effect_id) else {
            continue;
        };
        let schema = param_schema(meta);
        push_c_abi_param_rows(
            &mut params,
            schema,
            GroupScope::Effect(id, i as i32),
            meta.name,
            i as i32,
            clip_start,
            clip_end,
            current_frame,
            |key| {
                e.params
                    .get(key)
                    .map(|p| match &p.static_value {
                        crate::ecs::types::Value::Number(n) => *n,
                        crate::ecs::types::Value::Bool(b) => {
                            if *b {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        crate::ecs::types::Value::Enum(idx) => *idx as f32,
                        crate::ecs::types::Value::TrackRef(oid) => *oid as f32,
                        crate::ecs::types::Value::Text(_)
                        | crate::ecs::types::Value::FilePath(_) => 0.0,
                    })
                    .unwrap_or_else(|| {
                        schema
                            .iter()
                            .find(|s| unsafe { s.key.as_str() } == key)
                            .map_or(0.0, |s| s.default_float)
                    })
            },
            |key| {
                e.params.get(key).and_then(|p| match &p.static_value {
                    crate::ecs::types::Value::Text(s) | crate::ecs::types::Value::FilePath(s) => {
                        Some(s.clone())
                    }
                    _ => None,
                })
            },
            |key| {
                e.params
                    .get(key)
                    .map(|p| p.keyframes.clone())
                    .unwrap_or_default()
            },
            &track_options,
        );
    }
    props.set_params(ModelRc::new(VecModel::from(params)));
}
