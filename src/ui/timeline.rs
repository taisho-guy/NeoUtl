use crate::app_state::{self, SharedAppState};
use crate::ecs::{
    EcsWorld,
    components::{MediaSource, ShapeParams, TextContent},
};
use crate::objects::registry;
use crate::{
    ContextMenuItem, LayerState, ObjectKindItem, PreviewWindow, PropertiesWindow,
    SceneSettingsWindow, SceneTabItem, TimelineObject, TimelineWindow,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel, Weak};

pub fn setup(
    timeline: &TimelineWindow,
    preview_weak: Weak<PreviewWindow>,
    props_weak: Weak<PropertiesWindow>,
    scene_settings_weak: Weak<SceneSettingsWindow>,
    state: SharedAppState,
) {
    let kinds: Vec<ObjectKindItem> = registry()
        .iter()
        .enumerate()
        .map(|(kind_id, plugin)| ObjectKindItem {
            kind: kind_id as i32,
            name: plugin.name.clone().into(),
        })
        .collect();
    timeline.set_available_kinds(ModelRc::new(VecModel::from(kinds)));

    {
        let (state, tw, preview_w, props_w) = (
            state.clone(),
            timeline.as_weak(),
            preview_weak.clone(),
            props_weak.clone(),
        );
        timeline.on_seek_timeline(move |frame| {
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let clamped = frame.clamp(0, world.total_frames());
            world.set_current_frame(clamped);
            if let Some(t) = tw.upgrade() {
                t.set_current_frame(clamped);
            }
            if let Some(p) = preview_w.upgrade() {
                p.set_current_frame(clamped);
            }
            if let Some(props) = props_w.upgrade() {
                crate::ui::properties::refresh_current_frame(&props, &world);
            }
        });
    }

    {
        let (state, pw) = (state.clone(), props_weak.clone());
        timeline.on_keyframe_clicked(move |id, _frame| {
            if let Some(p) = pw.upgrade() {
                let world_holder = app_state::active_world(&state);
                let world = world_holder.lock().unwrap();
                crate::ui::properties::select_object(&p, &world, id);
            }
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_add_object_at(move |frame, layer, kind_idx| {
            let Some(t) = tw.upgrade() else { return };
            let Some(plugin) = registry().get(kind_idx as usize) else {
                return;
            };
            let start = frame.max(0);
            let layer = layer.max(0);
            let kind_id = kind_idx as u32;

            match plugin.name.as_str() {
                "Video" | "Image" | "Audio" => {
                    let Some(path) = rfd::FileDialog::new().pick_file() else {
                        return;
                    };
                    let Some(kind) = crate::media::detect_kind(&path) else {
                        return;
                    };
                    app_state::snapshot_before_edit(&state);
                    let world_holder = app_state::active_world(&state);
                    let mut world = world_holder.lock().unwrap();
                    let media = MediaSource {
                        path,
                        kind,
                        trim_in_frame: 0,
                    };
                    world.add_media_object(start, 90, kind_id, layer, media);
                    sync(&t, pw.upgrade().as_ref(), &world);
                }
                "Text" => {
                    app_state::snapshot_before_edit(&state);
                    let world_holder = app_state::active_world(&state);
                    let mut world = world_holder.lock().unwrap();
                    world.add_object(start, 90, kind_id, layer, Some(TextContent::default()));
                    sync(&t, pw.upgrade().as_ref(), &world);
                }
                "Shape" => {
                    app_state::snapshot_before_edit(&state);
                    let world_holder = app_state::active_world(&state);
                    let mut world = world_holder.lock().unwrap();
                    world.add_shape_object(start, 90, kind_id, layer, ShapeParams::default());
                    sync(&t, pw.upgrade().as_ref(), &world);
                }
                _ => {
                    app_state::snapshot_before_edit(&state);
                    let world_holder = app_state::active_world(&state);
                    let mut world = world_holder.lock().unwrap();
                    world.add_object(start, 90, kind_id, layer, None);
                    sync(&t, pw.upgrade().as_ref(), &world);
                }
            }
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_delete_object(move |id| {
            if id < 0 {
                return;
            }
            if let Some(t) = tw.upgrade() {
                app_state::snapshot_before_edit(&state);
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                world.delete_object(id as usize);
                sync(&t, pw.upgrade().as_ref(), &world);
            }
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_split_object_at(move |id, frame| {
            if id < 0 {
                return;
            }
            let Some(t) = tw.upgrade() else { return };
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.split_object(id as usize, frame);
            sync(&t, pw.upgrade().as_ref(), &world);
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), props_weak.clone());
        timeline.on_keyframe_moved(move |id, old_frame, new_frame| {
            let Some(t) = tw.upgrade() else { return };
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let moved = world.move_keyframe(id as usize, "", old_frame, new_frame);
            if moved {
                crate::ui::timeline::refresh_keyframe_markers(&t, &world);
            }
            if let Some(p) = pw.upgrade() {
                crate::ui::properties::select_object(&p, &world, id);
            }
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), props_weak.clone());
        timeline.on_select_object(move |id| {
            if let Some(t) = tw.upgrade() {
                let objs = t.get_objects();
                for i in 0..objs.row_count() {
                    let Some(mut o) = objs.row_data(i) else {
                        continue;
                    };
                    let selected = o.id == id;
                    if o.selected != selected {
                        o.selected = selected;
                        objs.set_row_data(i, o);
                    }
                }
            }
            if let Some(p) = pw.upgrade() {
                let world_holder = app_state::active_world(&state);
                let world = world_holder.lock().unwrap();
                crate::ui::properties::select_object(&p, &world, id);
            }
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_move_object(move |id, start, layer, ripple| {
            let state = state.clone();
            let tw = tw.clone();
            let pw = pw.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(t) = tw.upgrade() else { return };
                let world_holder = app_state::active_world(&state);
                let exists = world_holder.lock().unwrap().object_exists(id as usize);
                if !exists {
                    let world = world_holder.lock().unwrap();
                    sync(&t, pw.upgrade().as_ref(), &world);
                    return;
                }
                app_state::snapshot_before_edit(&state);
                let mut world = world_holder.lock().unwrap();
                if ripple {
                    world.ripple_move_object(id as usize, start);
                } else {
                    world.move_object(id as usize, start, layer);
                }
                sync(&t, pw.upgrade().as_ref(), &world);
            });
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_resize_object(move |id, start, end, ripple| {
            let state = state.clone();
            let tw = tw.clone();
            let pw = pw.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(t) = tw.upgrade() else { return };
                let world_holder = app_state::active_world(&state);
                let exists = world_holder.lock().unwrap().object_exists(id as usize);
                if !exists {
                    let world = world_holder.lock().unwrap();
                    sync(&t, pw.upgrade().as_ref(), &world);
                    return;
                }
                app_state::snapshot_before_edit(&state);
                let mut world = world_holder.lock().unwrap();
                if ripple {
                    world.ripple_resize_object(id as usize, end);
                } else {
                    world.resize_object(id as usize, start, end);
                }
                sync(&t, pw.upgrade().as_ref(), &world);
            });
        });
    }

    {
        let tw = timeline.as_weak();
        timeline.on_range_select(move |start_frame, end_frame, start_layer, end_layer| {
            let Some(t) = tw.upgrade() else { return };
            let objs = t.get_objects();
            let updated: Vec<TimelineObject> = objs
                .iter()
                .map(|mut o| {
                    o.selected = o.start_frame < end_frame
                        && o.end_frame > start_frame
                        && o.layer >= start_layer
                        && o.layer <= end_layer;
                    o
                })
                .collect();
            t.set_objects(ModelRc::new(VecModel::from(updated)));
        });
    }

    {
        let (state, tw, pw, prw) = (
            state.clone(),
            timeline.as_weak(),
            preview_weak.clone(),
            props_weak.clone(),
        );
        timeline.on_undo_requested(move || {
            if app_state::undo_active(&state) {
                crate::ui::preview::sync_active_session(&state, &pw, &tw, &prw);
            }
        });
    }

    {
        let (state, tw, pw, prw) = (
            state.clone(),
            timeline.as_weak(),
            preview_weak.clone(),
            props_weak.clone(),
        );
        timeline.on_redo_requested(move || {
            if app_state::redo_active(&state) {
                crate::ui::preview::sync_active_session(&state, &pw, &tw, &prw);
            }
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_set_zoom(move |scale| {
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_zoom(scale);
            if let Some(t) = tw.upgrade() {
                t.set_zoom_scale(world.zoom());
            }
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_toggle_layer_visible(move |layer| {
            if let Some(t) = tw.upgrade() {
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                let current = world.layer_states();
                let visible = current.get(layer as usize).map_or(true, |s| s.0);
                world.set_layer_visible(layer as usize, !visible);
                sync(&t, pw.upgrade().as_ref(), &world);
            }
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_toggle_layer_locked(move |layer| {
            if let Some(t) = tw.upgrade() {
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                let current = world.layer_states();
                let locked = current.get(layer as usize).map_or(false, |s| s.1);
                world.set_layer_locked(layer as usize, !locked);
                sync(&t, pw.upgrade().as_ref(), &world);
            }
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_switch_scene_tab(move |id| {
            if let Some(t) = tw.upgrade() {
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                if world.switch_scene(id) {
                    sync(&t, pw.upgrade().as_ref(), &world);
                    sync_scene_tabs(&t, &world);
                }
            }
        });
    }

    {
        let (state, sw) = (state.clone(), scene_settings_weak.clone());
        timeline.on_open_scene_settings_create(move || {
            if let Some(w) = sw.upgrade() {
                crate::ui::scene_settings::open_for_create(&w, &state);
            }
        });
    }

    {
        let (state, sw) = (state.clone(), scene_settings_weak.clone());
        timeline.on_open_scene_settings_edit(move |scene_id| {
            if let Some(w) = sw.upgrade() {
                crate::ui::scene_settings::open_for_edit(&w, &state, scene_id);
            }
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_close_scene_tab(move |id| {
            if let Some(t) = tw.upgrade() {
                app_state::snapshot_before_edit(&state);
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                if world.scenes().len() > 1 {
                    world.remove_scene(id);
                    sync(&t, pw.upgrade().as_ref(), &world);
                    sync_scene_tabs(&t, &world);
                }
            }
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_context_menu_requested(move |hit_id, _frame, _layer| {
            let Some(t) = tw.upgrade() else { return };
            let ripple_mode = t.get_ripple_mode();
            let clipboard_empty = app_state::clipboard(&state).is_empty();
            let kinds = t.get_available_kinds();
            let items = build_context_menu(hit_id, ripple_mode, clipboard_empty, &kinds);
            t.set_menu_items(ModelRc::new(VecModel::from(items)));
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_duplicate_requested(move |hit_id| {
            let Some(t) = tw.upgrade() else { return };
            if hit_id < 0 {
                return;
            }
            let ids = selection_target_ids(&t, hit_id);
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let frame = world.current_frame();
            let layer = t.get_selected_layer();
            world.duplicate_objects(&ids, frame, layer);
            sync(&t, pw.upgrade().as_ref(), &world);
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_cut_requested(move |hit_id| {
            let Some(t) = tw.upgrade() else { return };
            if hit_id < 0 {
                return;
            }
            let ids = selection_target_ids(&t, hit_id);
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let docs = world.cut_objects(&ids);
            app_state::set_clipboard(&state, docs);
            sync(&t, pw.upgrade().as_ref(), &world);
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_copy_requested(move |hit_id| {
            let Some(t) = tw.upgrade() else { return };
            if hit_id < 0 {
                return;
            }
            let ids = selection_target_ids(&t, hit_id);
            let world_holder = app_state::active_world(&state);
            let world = world_holder.lock().unwrap();
            let docs = world.copy_objects(&ids);
            app_state::set_clipboard(&state, docs);
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_paste_requested(move || {
            let Some(t) = tw.upgrade() else { return };
            let docs = app_state::clipboard(&state);
            if docs.is_empty() {
                return;
            }
            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let frame = world.current_frame();
            let layer = t.get_selected_layer();
            world.paste_objects(&docs, frame, layer);
            sync(&t, pw.upgrade().as_ref(), &world);
        });
    }

    sync_active_session(&state, &timeline.as_weak());
}

/// タイムライン右クリックメニューの項目集合を構築する唯一の経路。
/// AviQtl(ui/qml/timeline/TimelineView.qml::rebuildMenu)の項目順序を踏襲する。
/// hit-id>=0（クリップ上）: 削除→分割→複製→区切り→切り取り→コピー→区切り→リップルモード切替。
///   複数選択時（呼び出し側selection_ids参照）は削除/複製/切り取り/コピーとも選択全体へ適用する。
///   AviQtl側のクリッピング/エフェクト追加サブメニューはエフェクトカタログ連携が
///   未実装のため本関数では対象外（次段対応）。
/// hit-id<0（背景上）: 登録済みオブジェクト種別ごとのAdd項目→区切り→元に戻す→やり直す→貼り付け。
///   貼り付けはclipboard_empty時disabledとする。AviQtl側のシーン設定/プロジェクト設定/
///   環境設定は該当ウィンドウのWeak参照がtimeline::setup()に配線されていないため対象外（次段対応）。
fn build_context_menu(
    hit_id: i32,
    ripple_mode: bool,
    clipboard_empty: bool,
    kinds: &ModelRc<ObjectKindItem>,
) -> Vec<ContextMenuItem> {
    let sep = || ContextMenuItem {
        label: String::new().into(),
        action: 4,
        kind: -1,
        enabled: false,
    };
    if hit_id >= 0 {
        return vec![
            ContextMenuItem {
                label: "🗑  Delete".into(),
                action: 1,
                kind: -1,
                enabled: true,
            },
            ContextMenuItem {
                label: "✂  Split at Playhead".into(),
                action: 0,
                kind: -1,
                enabled: true,
            },
            ContextMenuItem {
                label: "⧉  Duplicate".into(),
                action: 7,
                kind: -1,
                enabled: true,
            },
            sep(),
            ContextMenuItem {
                label: "✂  Cut".into(),
                action: 8,
                kind: -1,
                enabled: true,
            },
            ContextMenuItem {
                label: "📋  Copy".into(),
                action: 9,
                kind: -1,
                enabled: true,
            },
            sep(),
            ContextMenuItem {
                label: if ripple_mode {
                    "🔗  Ripple Mode: On".into()
                } else {
                    "🔗  Ripple Mode: Off".into()
                },
                action: 3,
                kind: -1,
                enabled: true,
            },
        ];
    }
    let mut items: Vec<ContextMenuItem> = kinds
        .iter()
        .map(|k| ContextMenuItem {
            label: format!("＋  Add {}", k.name).into(),
            action: 2,
            kind: k.kind,
            enabled: true,
        })
        .collect();
    items.push(sep());
    items.push(ContextMenuItem {
        label: "↩  元に戻す".into(),
        action: 5,
        kind: -1,
        enabled: true,
    });
    items.push(ContextMenuItem {
        label: "↪  やり直す".into(),
        action: 6,
        kind: -1,
        enabled: true,
    });
    items.push(ContextMenuItem {
        label: "📌  貼り付け".into(),
        action: 10,
        kind: -1,
        enabled: !clipboard_empty,
    });
    items
}

/// 右クリック対象(hit-id)に対する操作適用先id集合を決定する。
/// AviQtl::TimelineView::shouldApplyToSelection相当: 現在選択が複数件かつ
/// hit-idがその選択に含まれる場合のみ選択全体を対象とし、それ以外はhit-id単体を対象とする。
fn selection_target_ids(t: &TimelineWindow, hit_id: i32) -> Vec<usize> {
    let objs = t.get_objects();
    let selected: Vec<usize> = objs
        .iter()
        .filter(|o| o.selected)
        .map(|o| o.id as usize)
        .collect();
    if selected.len() > 1 && selected.contains(&(hit_id as usize)) {
        selected
    } else {
        vec![hit_id as usize]
    }
}

/// アクティブプロジェクト切替時、タイムライン全体（オブジェクト・レイヤー・シーンタブ）を再同期する。
/// プレビュー側のtotal-framesは呼び出し側（preview::sync_active_session）が別途担う。
pub fn sync_active_session(state: &SharedAppState, timeline_weak: &Weak<TimelineWindow>) {
    let Some(t) = timeline_weak.upgrade() else {
        return;
    };
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    sync(&t, None, &world);
    sync_scene_tabs(&t, &world);
    t.set_zoom_scale(world.zoom());
    t.set_layer_count(world.layer_count());
}

fn to_slint(data: &crate::ecs::TimelineData) -> TimelineObject {
    let plugin = registry().get(data.kind as usize);
    TimelineObject {
        id: data.id,
        start_frame: data.start_frame,
        end_frame: data.end_frame,
        kind: data.kind,
        kind_known: plugin.is_some(),
        layer: data.layer,
        label: plugin.map_or("Unknown", |p| p.name.as_str()).into(),
        selected: false,
        keyframe_frames: ModelRc::new(VecModel::from(Vec::<i32>::new())),
    }
}

/// タイムライン内部モデルをECSと同期する。`preview`が渡された場合は本体ウィンドウの
/// total-framesも同時に更新し、タイムライン編集による総フレーム数変化を伝播させる。
fn sync(timeline: &TimelineWindow, preview: Option<&PreviewWindow>, world: &EcsWorld) {
    let total = world.total_frames();
    timeline.set_total_frames(total);
    if let Some(p) = preview {
        p.set_total_frames(total);
    }

    let selected_id = timeline
        .get_objects()
        .iter()
        .find(|o| o.selected)
        .map(|o| o.id);

    let objs: Vec<TimelineObject> = world
        .get_timeline_objects()
        .iter()
        .map(to_slint)
        .map(|mut o| {
            o.selected = Some(o.id) == selected_id;
            o
        })
        .collect();
    timeline.set_objects(ModelRc::new(VecModel::from(objs)));

    let states: Vec<LayerState> = world
        .layer_states()
        .iter()
        .map(|&(visible, locked)| LayerState { visible, locked })
        .collect();
    timeline.set_layer_states(ModelRc::new(VecModel::from(states)));
}

fn sync_scene_tabs(timeline: &TimelineWindow, world: &EcsWorld) {
    let active = world.active_scene();
    let tabs: Vec<SceneTabItem> = world
        .scenes()
        .iter()
        .map(|s| SceneTabItem {
            id: s.id,
            name: s.name.clone().into(),
            active: s.id == active,
        })
        .collect();
    timeline.set_scene_tabs(ModelRc::new(VecModel::from(tabs)));
}

/// プロパティパネルで選択中のパラメータ（中間点編集ボタンを押した対象）の
/// 中間点フレーム位置を、対応するクリップの行にのみ反映する。他クリップは空へ戻す
/// （選択切替時に前選択のマーカーが残留しないようにするため）。
/// 補間計算・値評価は一切行わない（純粋にフレーム位置の表示用）。
pub fn refresh_keyframe_markers(_timeline: &TimelineWindow, _world: &EcsWorld) {}
