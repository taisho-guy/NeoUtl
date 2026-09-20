use crate::app_state::{self, SharedAppState};
use crate::ui::{self, UiState};
use std::sync::MutexGuard;

#[cxx::bridge(namespace = "neoqtl")]
mod ffi {
    #[derive(Clone, Debug, Default)]
    struct ClipData {
        id: i32,
        start_frame: i32,
        duration_frames: i32,
        layer: i32,
        color_index: i32,
        kind_known: bool,
        selected: bool,
        locked: bool,
        label: String,
        keyframe_frames: Vec<i32>,
    }

    #[derive(Clone, Debug, Default)]
    struct SceneTabData {
        id: i32,
        name: String,
        active: bool,
    }

    #[derive(Clone, Debug, Default)]
    struct GridSettingsData {
        mode: String,
        bpm: f32,
        offset: f32,
        interval: i32,
        subdivision: i32,
    }

    extern "Rust" {
        fn timeline_clips() -> Vec<ClipData>;
        fn timeline_scene_tabs() -> Vec<SceneTabData>;
        fn timeline_grid_settings() -> GridSettingsData;

        fn timeline_current_frame() -> i32;
        fn timeline_total_frames() -> i32;
        fn timeline_layer_count() -> i32;
        fn timeline_layer_hidden(layer: i32) -> bool;
        fn timeline_layer_locked(layer: i32) -> bool;
        fn timeline_scale() -> f32;
        fn timeline_project_fps() -> i32;
        fn timeline_enable_snap() -> bool;
        fn timeline_magnetic_snap_range() -> i32;

        fn timeline_set_viewport(content_x: f32, content_y: f32, width: f32, height: f32);
        fn timeline_set_zoom(value: f32);
        fn timeline_seek(frame: i32);

        fn timeline_select_clip(id: i32, additive: bool);
        fn timeline_clear_selection();
        fn timeline_select_in_rect(x0: f32, y0: f32, x1: f32, y1: f32);

        fn timeline_apply_clip_batch_move(clip_id: i32, delta_layer: i32, delta_start: i32);
        fn timeline_apply_clip_resize(clip_id: i32, delta_start: i32, delta_duration: i32);
        fn timeline_split_clip(id: i32, frame: i32);
        fn timeline_delete_selected();
        fn timeline_move_keyframe(id: i32, from_frame: i32, to_frame: i32);

        fn timeline_switch_scene(scene_id: i32) -> bool;
        fn timeline_add_scene(name: &str) -> i32;
        fn timeline_remove_scene(scene_id: i32) -> bool;

        fn timeline_query(name: &str, args_json: &str) -> String;
        fn timeline_invoke(name: &str, args_json: &str) -> String;

        fn ui_settings_all() -> String;
        fn ui_settings_value(key: &str) -> String;
        fn ui_settings_set(key: &str, value_json: &str);
        fn ui_settings_save() -> bool;
    }
}

use ffi::{ClipData, GridSettingsData, SceneTabData};

fn ui() -> MutexGuard<'static, UiState> {
    ui::ui_state().lock().unwrap()
}

macro_rules! with_state {
    ($guard:ident, $state:ident, $default:expr) => {
        let $guard = ui();
        let Some($state) = $guard.app_state.as_ref() else {
            return $default;
        };
    };
}

macro_rules! with_timeline_mut {
    ($guard:ident, $state:ident, $timeline:ident) => {
        let mut $guard = ui();
        let Some($state) = $guard.app_state.clone() else {
            return;
        };
        let Some($timeline) = $guard.timeline.as_mut() else {
            return;
        };
    };
}

pub fn timeline_clips() -> Vec<ClipData> {
    let guard = ui();
    let (Some(state), Some(timeline)) = (guard.app_state.as_ref(), guard.timeline.as_ref()) else {
        return Vec::new();
    };
    let layer_states = {
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        world.layer_states()
    };
    timeline
        .get_timeline_objects(state)
        .iter()
        .map(|o| ClipData {
            id: o.id,
            start_frame: o.start_frame,
            duration_frames: (o.end_frame - o.start_frame).max(1),
            layer: o.layer,
            color_index: o.kind.max(0),
            kind_known: o.kind_known,
            selected: o.selected,
            locked: layer_states.get(o.layer as usize).is_some_and(|s| s.1),
            label: o.label.clone(),
            keyframe_frames: o.keyframe_frames.clone(),
        })
        .collect()
}

pub fn timeline_scene_tabs() -> Vec<SceneTabData> {
    with_state!(guard, state, Vec::new());
    crate::ui::timeline::scene_tabs::list_scenes(state)
        .into_iter()
        .map(|s| SceneTabData {
            id: s.id,
            name: s.name,
            active: s.active,
        })
        .collect()
}

pub fn timeline_grid_settings() -> GridSettingsData {
    with_state!(guard, state, GridSettingsData::default());
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let active = world.active_scene();
    let Some(scene) = world.scenes().into_iter().find(|s| s.id == active) else {
        return GridSettingsData::default();
    };
    GridSettingsData {
        mode: match scene.grid_mode {
            1 => "BPM".to_string(),
            2 => "Frame".to_string(),
            _ => "Auto".to_string(),
        },
        bpm: scene.grid_bpm,
        offset: scene.grid_offset,
        interval: scene.grid_interval,
        subdivision: scene.grid_subdivision,
    }
}

pub fn timeline_current_frame() -> i32 {
    with_state!(guard, state, 0);
    let world_holder = app_state::active_world(state);
    let frame = world_holder.lock().unwrap().current_frame();
    frame
}

pub fn timeline_total_frames() -> i32 {
    with_state!(guard, state, 0);
    let world_holder = app_state::active_world(state);
    let frames = world_holder.lock().unwrap().total_frames();
    frames
}

pub fn timeline_layer_count() -> i32 {
    with_state!(guard, state, 0);
    let world_holder = app_state::active_world(state);
    let count = world_holder.lock().unwrap().layer_states().len() as i32;
    count
}

pub fn timeline_layer_hidden(layer: i32) -> bool {
    with_state!(guard, state, false);
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    !world
        .layer_states()
        .get(layer.max(0) as usize)
        .is_some_and(|s| s.0)
}

pub fn timeline_layer_locked(layer: i32) -> bool {
    with_state!(guard, state, false);
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    world
        .layer_states()
        .get(layer.max(0) as usize)
        .is_some_and(|s| s.1)
}

pub fn timeline_scale() -> f32 {
    let guard = ui();
    guard.timeline.as_ref().map_or(1.0, |t| t.zoom_scale)
}

pub fn timeline_project_fps() -> i32 {
    with_state!(guard, state, 30);
    let world_holder = app_state::active_world(state);
    let fps = world_holder.lock().unwrap().get_project().fps as i32;
    fps
}

pub fn timeline_enable_snap() -> bool {
    with_state!(guard, state, true);
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let active = world.active_scene();
    world
        .scenes()
        .into_iter()
        .find(|s| s.id == active)
        .is_none_or(|s| s.enable_snap)
}

pub fn timeline_magnetic_snap_range() -> i32 {
    with_state!(guard, state, 5);
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let active = world.active_scene();
    world
        .scenes()
        .into_iter()
        .find(|s| s.id == active)
        .map_or(5, |s| s.magnetic_snap_range)
}

pub fn timeline_set_viewport(content_x: f32, content_y: f32, width: f32, height: f32) {
    let mut guard = ui();
    let Some(timeline) = guard.timeline.as_mut() else {
        return;
    };
    timeline.scroll_x = content_x.max(0.0);
    timeline.scroll_y = content_y.max(0.0);
    timeline.viewport_width = width;
    timeline.viewport_height = height;
}

pub fn timeline_set_zoom(value: f32) {
    let mut guard = ui();
    if let Some(timeline) = guard.timeline.as_mut() {
        timeline.zoom_scale = value.clamp(0.05, 100.0);
    }
}

pub fn timeline_seek(frame: i32) {
    with_state!(guard, state, ());
    let world_holder = app_state::active_world(state);
    world_holder.lock().unwrap().set_current_frame(frame.max(0));
}

pub fn timeline_select_clip(id: i32, additive: bool) {
    with_timeline_mut!(guard, state, timeline);
    timeline.select_clip(&state, id, additive);
}

pub fn timeline_clear_selection() {
    with_timeline_mut!(guard, state, timeline);
    timeline.clear_selection(&state);
}

pub fn timeline_select_in_rect(x0: f32, y0: f32, x1: f32, y1: f32) {
    with_timeline_mut!(guard, state, timeline);
    timeline.select_in_rect(&state, x0, y0, x1, y1);
}

pub fn timeline_apply_clip_batch_move(clip_id: i32, delta_layer: i32, delta_start: i32) {
    with_timeline_mut!(guard, state, timeline);
    timeline.apply_clip_batch_move(&state, clip_id, delta_layer, delta_start);
}

pub fn timeline_apply_clip_resize(clip_id: i32, delta_start: i32, delta_duration: i32) {
    with_timeline_mut!(guard, state, timeline);
    timeline.apply_clip_resize(&state, clip_id, delta_start, delta_duration);
}

pub fn timeline_split_clip(id: i32, frame: i32) {
    with_timeline_mut!(guard, state, timeline);
    timeline.split_clip_at(&state, id, frame);
}

pub fn timeline_delete_selected() {
    with_timeline_mut!(guard, state, timeline);
    timeline.delete_selected(&state);
}

pub fn timeline_move_keyframe(id: i32, from_frame: i32, to_frame: i32) {
    with_timeline_mut!(guard, state, timeline);
    timeline.keyframe_moved(&state, id, from_frame, to_frame);
}

pub fn timeline_switch_scene(scene_id: i32) -> bool {
    with_state!(guard, state, false);
    crate::ui::timeline::scene_tabs::switch_scene(state, scene_id)
}

pub fn timeline_add_scene(name: &str) -> i32 {
    with_state!(guard, state, -1);
    crate::ui::timeline::scene_tabs::add_scene(state, name)
}

pub fn timeline_remove_scene(scene_id: i32) -> bool {
    with_state!(guard, state, false);
    crate::ui::timeline::scene_tabs::remove_scene(state, scene_id)
}

use crate::ecs::SceneSettings;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static PREVIEW_IDS: OnceLock<Mutex<Vec<i32>>> = OnceLock::new();
static SCRUBBING: AtomicBool = AtomicBool::new(false);
static COMPOSITE_VIEW: AtomicBool = AtomicBool::new(false);

fn preview_ids() -> &'static Mutex<Vec<i32>> {
    PREVIEW_IDS.get_or_init(|| Mutex::new(Vec::new()))
}

fn state() -> Option<SharedAppState> {
    ui().app_state.clone()
}

fn arg_list(raw: &str) -> Vec<Value> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn ai(a: &[Value], n: usize) -> i32 {
    a.get(n).and_then(Value::as_i64).unwrap_or(0) as i32
}

fn au(a: &[Value], n: usize) -> usize {
    ai(a, n).max(0) as usize
}

fn af(a: &[Value], n: usize) -> f32 {
    a.get(n).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn ab(a: &[Value], n: usize) -> bool {
    a.get(n).and_then(Value::as_bool).unwrap_or(false)
}

fn at(a: &[Value], n: usize) -> String {
    a.get(n)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn aiv(a: &[Value], n: usize) -> Vec<i32> {
    a.get(n)
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(Value::as_i64)
                .map(|x| x as i32)
                .collect()
        })
        .unwrap_or_default()
}

macro_rules! world_ro {
    ($world:ident, $default:expr) => {
        let Some(session) = state() else {
            return $default;
        };
        let holder = app_state::active_world(&session);
        let $world = holder.lock().unwrap();
    };
}

macro_rules! world_rw {
    ($world:ident, $default:expr) => {
        let Some(session) = state() else {
            return $default;
        };
        app_state::snapshot_before_edit(&session);
        let holder = app_state::active_world(&session);
        let mut $world = holder.lock().unwrap();
    };
}

fn selected_ids() -> Vec<i32> {
    let guard = ui();
    let Some(timeline) = guard.timeline.as_ref() else {
        return Vec::new();
    };
    let mut ids: Vec<i32> = timeline.selected_ids.iter().copied().collect();
    ids.sort_unstable();
    ids
}

fn first_selected() -> i32 {
    selected_ids().first().copied().unwrap_or(-1)
}

fn clip_of(world: &crate::ecs::EcsWorld, id: i32) -> Option<crate::ecs::TimelineData> {
    world
        .get_timeline_objects()
        .into_iter()
        .find(|o| o.id == id)
}

pub fn timeline_query(name: &str, args_json: &str) -> String {
    json!([query(name, &arg_list(args_json))]).to_string()
}

pub fn timeline_invoke(name: &str, args_json: &str) -> String {
    json!([invoke(name, &arg_list(args_json))]).to_string()
}

fn query(name: &str, a: &[Value]) -> Value {
    match name {
        "project" => q_project(),
        "transport" => q_transport(),
        "selection" => {
            json!({"selectedClipId": first_selected(), "selectedClipIds": selected_ids()})
        }
        "scenes" => q_scenes(),
        "cursorFrame" => json!(timeline_current_frame()),
        "timelineDuration" => json!(timeline_total_frames()),
        "selectedLayer" => json!(ui().timeline.as_ref().map_or(0, |t| t.selected_layer)),
        "currentSceneId" => json!(q_current_scene_id()),
        "previewSelectionIds" => json!(preview_ids().lock().unwrap().clone()),
        "currentProjectUrl" => q_project_url(),
        "hasUnsavedChanges" => json!(q_dirty()),
        "isExporting" => json!(q_exporting()),
        "clipStartFrame" => json!(q_clip_field(first_selected(), 0)),
        "clipDurationFrames" => json!(q_clip_field(first_selected(), 1)),
        "isAudioClip" => q_is_audio(ai(a, 0)),
        "isLayerHidden" => json!(timeline_layer_hidden(ai(a, 0))),
        "isLayerLocked" => json!(timeline_layer_locked(ai(a, 0))),
        "clipByUpperObject" => json!(q_clip_field(ai(a, 0), 2) > 0),
        "getClipTypeColor" => json!(crate::ui::timeline::util::clip_color_hex(
            ai(a, 0),
            true,
            false
        )),
        "getAvailableEffects" => q_available_effects(),
        "getAvailableObjects" => q_available_objects(),
        "getClipEffectStack" | "getClipEffectsModel" => q_effect_stack(au(a, 0)),
        "getClipEffectIndex" => q_effect_index(au(a, 0), &at(a, 1)),
        "getEffectParameters" => q_effect_parameters(au(a, 0), au(a, 1)),
        "getPluginCategories" => q_plugin_categories(),
        "getPluginsByCategory" => q_plugins_by_category(&at(a, 0)),
        "getSceneClips" => q_scene_clips(ai(a, 0)),
        "getSceneDuration" => json!(q_scene_duration(ai(a, 0))),
        "getSceneInfo" => q_scene_info(ai(a, 0)),
        "getWaveformPeaks" => q_waveform_peaks(au(a, 0), au(a, 1)),
        "resolveDragDelta" => q_resolve_drag_delta(a),
        _ => Value::Null,
    }
}

fn q_project() -> Value {
    world_ro!(world, Value::Null);
    let p = world.get_project();
    json!({"fps": p.fps, "width": p.width, "height": p.height, "sampleRate": p.audio_sample_rate})
}

fn q_transport() -> Value {
    let (playing, speed_percent) = {
        let guard = ui();
        guard
            .preview
            .as_ref()
            .map_or((false, 100), |p| (p.is_playing, p.speed_percent))
    };
    json!({
        "isPlaying": playing,
        "playbackSpeed": f64::from(speed_percent) / 100.0,
        "isScrubbing": SCRUBBING.load(Ordering::Relaxed),
        "currentFrame": timeline_current_frame(),
        "totalFrames": timeline_total_frames(),
    })
}

fn q_scenes() -> Value {
    Value::Array(
        timeline_scene_tabs()
            .into_iter()
            .map(|s| json!({"id": s.id, "name": s.name, "active": s.active}))
            .collect(),
    )
}

fn q_current_scene_id() -> i32 {
    world_ro!(world, -1);
    world.active_scene()
}

fn q_project_url() -> Value {
    world_ro!(world, json!(""));
    json!(
        world
            .get_project()
            .dir
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default()
    )
}

fn q_dirty() -> bool {
    let Some(session) = state() else {
        return false;
    };
    let s = session.lock().unwrap();
    s.sessions.get(s.active).is_some_and(|x| x.dirty)
}

fn q_exporting() -> bool {
    ui().dialogs
        .as_ref()
        .is_some_and(|d| d.export_dialog.active_queue.is_some())
}

fn q_clip_field(id: i32, field: usize) -> i32 {
    world_ro!(world, 0);
    let Some(c) = clip_of(&world, id) else {
        return 0;
    };
    match field {
        0 => c.start_frame,
        1 => (c.end_frame - c.start_frame).max(1),
        _ => c.clip_layer_count_up,
    }
}

fn q_is_audio(id: i32) -> Value {
    world_ro!(world, json!(false));
    json!(world.is_audio_object(id.max(0) as usize))
}

fn q_available_effects() -> Value {
    Value::Array(
        crate::effects::loader::registry()
            .iter()
            .map(|e| json!({"id": e.id(), "name": e.name(), "category": e.category()}))
            .collect(),
    )
}

fn q_available_objects() -> Value {
    Value::Array(
        crate::objects::loader::registry()
            .iter()
            .map(|o| json!({"id": o.stable_id, "name": o.name, "kind": o.kind_id}))
            .collect(),
    )
}

fn q_effect_stack(object_id: usize) -> Value {
    world_ro!(world, Value::Array(Vec::new()));
    Value::Array(
        world
            .effect_stack_of(object_id)
            .iter()
            .enumerate()
            .map(|(index, e)| {
                let name = crate::effects::loader::by_id(&e.effect_id)
                    .map_or_else(|| e.effect_id.clone(), |p| p.name().to_string());
                json!({"index": index, "id": e.effect_id, "name": name, "enabled": e.enabled})
            })
            .collect(),
    )
}

fn q_effect_index(object_id: usize, effect_id: &str) -> Value {
    world_ro!(world, json!(-1));
    json!(
        world
            .effect_stack_of(object_id)
            .iter()
            .position(|e| e.effect_id == effect_id)
            .map_or(-1, |i| i as i32)
    )
}

fn q_effect_parameters(object_id: usize, index: usize) -> Value {
    world_ro!(world, Value::Array(Vec::new()));
    let stack = world.effect_stack_of(object_id);
    let Some(instance) = stack.get(index) else {
        return Value::Array(Vec::new());
    };
    let Some(plugin) = crate::effects::loader::by_id(&instance.effect_id) else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        plugin
            .param_schema()
            .into_iter()
            .enumerate()
            .map(|(p_idx, row)| {
                let value = world
                    .effect_param_f32(object_id, index, &row.key)
                    .unwrap_or(row.default_float);
                json!({
                    "pIdx": p_idx,
                    "key": row.key,
                    "label": row.label,
                    "kind": format!("{:?}", row.kind),
                    "min": row.min,
                    "max": row.max,
                    "step": row.step,
                    "value": value,
                    "options": row.enum_options,
                })
            })
            .collect(),
    )
}

fn plugin_format_name(format: maolan_host_adapter::PluginFormat) -> String {
    format!("{format:?}")
}

fn q_plugin_categories() -> Value {
    let mut categories: Vec<String> = crate::audio::plugin_registry::get_all()
        .into_iter()
        .map(|e| plugin_format_name(e.format))
        .collect();
    categories.sort_unstable();
    categories.dedup();
    json!(categories)
}

fn q_plugins_by_category(category: &str) -> Value {
    Value::Array(
        crate::audio::plugin_registry::get_all()
            .into_iter()
            .filter(|e| plugin_format_name(e.format) == category)
            .map(|e| json!({"id": e.plugin_id, "name": e.name, "vendor": e.vendor}))
            .collect(),
    )
}

fn q_scene_clips(scene_id: i32) -> Value {
    world_ro!(world, Value::Array(Vec::new()));
    Value::Array(
        world
            .get_scene_objects(scene_id)
            .into_iter()
            .map(|o| {
                json!({
                    "id": o.id,
                    "startFrame": o.start_frame,
                    "durationFrames": (o.end_frame - o.start_frame).max(1),
                    "layer": o.layer,
                    "type": o.kind,
                })
            })
            .collect(),
    )
}

fn q_scene_duration(scene_id: i32) -> i32 {
    world_ro!(world, 0);
    world
        .get_scene_objects(scene_id)
        .iter()
        .map(|o| o.end_frame)
        .max()
        .unwrap_or(0)
        .max(
            world
                .scenes()
                .into_iter()
                .find(|s| s.id == scene_id)
                .map_or(0, |s| s.total_frames),
        )
}

fn q_scene_meta(scene_id: i32) -> Option<crate::ecs::resources::SceneMeta> {
    let session = state()?;
    let holder = app_state::active_world(&session);
    let world = holder.lock().unwrap();
    world.scenes().into_iter().find(|s| s.id == scene_id)
}

fn q_scene_info(scene_id: i32) -> Value {
    let Some(s) = q_scene_meta(scene_id) else {
        return Value::Null;
    };
    json!({
        "id": s.id,
        "name": s.name,
        "width": s.width,
        "height": s.height,
        "fps": s.fps,
        "totalFrames": s.total_frames,
        "gridMode": s.grid_mode,
        "gridBpm": s.grid_bpm,
        "gridOffset": s.grid_offset,
        "gridInterval": s.grid_interval,
        "gridSubdivision": s.grid_subdivision,
        "enableSnap": s.enable_snap,
        "magneticSnapRange": s.magnetic_snap_range,
    })
}

fn q_waveform_peaks(object_id: usize, width: usize) -> Value {
    world_ro!(world, Value::Array(Vec::new()));
    if !world.is_audio_object(object_id) {
        return Value::Array(Vec::new());
    }
    json!(vec![0.0_f32; width])
}

fn q_resolve_drag_delta(a: &[Value]) -> Value {
    let delta_frame = ai(a, 1);
    let delta_layer = ai(a, 2);
    let selection_min_frame = ai(a, 4);
    let selection_min_layer = ai(a, 5);
    let selection_max_layer = ai(a, 6);
    let layer_count = ai(a, 7).max(1);
    json!({
        "deltaFrame": delta_frame.max(-selection_min_frame),
        "deltaLayer": delta_layer
            .max(-selection_min_layer)
            .min(layer_count - 1 - selection_max_layer),
    })
}

fn invoke(name: &str, a: &[Value]) -> Value {
    match name {
        "togglePlay" => i_toggle_play(),
        "syncPlaybackSpeed" => i_sync_playback_speed(),
        "seek" | "setCurrentFrame_seek" | "scrubTo" => {
            timeline_seek(ai(a, 0));
            Value::Null
        }
        "pause" => i_pause(),
        "beginScrub" | "endScrub" => {
            SCRUBBING.store(name == "beginScrub", Ordering::Relaxed);
            Value::Null
        }
        "updateViewport" => {
            let guard_sizes = {
                let guard = ui();
                guard
                    .timeline
                    .as_ref()
                    .map_or((0.0, 0.0), |t| (t.viewport_width, t.viewport_height))
            };
            timeline_set_viewport(af(a, 0), af(a, 1), guard_sizes.0, guard_sizes.1);
            Value::Null
        }
        "setCompositeView" => {
            COMPOSITE_VIEW.store(ab(a, 0), Ordering::Relaxed);
            Value::Null
        }
        "setSelectedLayer" => {
            let mut guard = ui();
            if let Some(timeline) = guard.timeline.as_mut() {
                timeline.selected_layer = ai(a, 0).max(0);
            }
            Value::Null
        }
        "applySelectionIds" => i_apply_selection_ids(&aiv(a, 0)),
        "handleClipClick" => {
            timeline_select_clip(ai(a, 0), ai(a, 1) != 0);
            Value::Null
        }
        "updateSelectionPreview" => i_update_selection_preview(a),
        "clearSelectionPreview" => {
            preview_ids().lock().unwrap().clear();
            Value::Null
        }
        "finalizeSelectionPreview" => {
            let ids = preview_ids().lock().unwrap().clone();
            i_apply_selection_ids(&ids)
        }
        "updateClip" => i_update_clip(au(a, 0), ai(a, 1), ai(a, 2), ai(a, 3)),
        "deleteClip" => i_delete_ids(&[ai(a, 0)]),
        "deleteSelectedClips" => i_delete_ids(&selected_ids()),
        "copyClip" => i_copy(&[ai(a, 0)], false),
        "copySelectedClips" => i_copy(&selected_ids(), false),
        "cutClip" => i_copy(&[ai(a, 0)], true),
        "cutSelectedClips" => i_copy(&selected_ids(), true),
        "pasteClip" => i_paste(ai(a, 0), ai(a, 1)),
        "splitClip" => i_split(&[ai(a, 0)], ai(a, 1)),
        "splitSelectedClips" => i_split(&selected_ids(), ai(a, 0)),
        "moveSelectedClips" => i_move_selected(ai(a, 0), ai(a, 1)),
        "resizeSelectedClips" => i_resize_selected(ai(a, 0), ai(a, 1)),
        "applyClipBatchMove" => i_apply_clip_batch_move(a),
        "setClipByUpperObject" => i_set_clip_by_upper_object(au(a, 0), ab(a, 1)),
        "setKeyframe" => i_set_keyframe(a),
        "removeKeyframe" => i_remove_keyframe(a),
        "moveKeyframe" => i_move_keyframe(a),
        "addEffect" => i_effect_mut(a, EffectOp::Add),
        "removeEffect" => i_effect_mut(a, EffectOp::Remove),
        "removeMultipleEffects" => i_remove_multiple_effects(a),
        "reorderEffects" => i_effect_mut(a, EffectOp::Reorder),
        "reorderMultipleEffects" => i_reorder_multiple_effects(a),
        "setEffectEnabled" => i_effect_mut(a, EffectOp::SetEnabled),
        "setEffectParameter" | "updateClipEffectParam" => i_set_effect_parameter(a),
        "addAudioPlugin" => i_add_audio_plugin(au(a, 0), &at(a, 1)),
        "removeAudioPlugin" => i_audio_plugin_mut(a, AudioOp::Remove),
        "setAudioPluginEnabled" => i_audio_plugin_mut(a, AudioOp::SetEnabled),
        "reorderAudioPlugins" => i_audio_plugin_mut(a, AudioOp::Reorder),
        "setLayerState" => i_set_layer_state(au(a, 0), ab(a, 1), ai(a, 2)),
        "insertLayers" => i_shift_layers(ai(a, 0) + i32::from(!ab(a, 2)), i32::MAX, ai(a, 1)),
        "shiftLayers" => i_shift_layers(ai(a, 0), ai(a, 1), ai(a, 2)),
        "createScene" => json!(timeline_add_scene(&at(a, 0))),
        "removeScene" => json!(timeline_remove_scene(ai(a, 0))),
        "switchScene" => json!(timeline_switch_scene(ai(a, 0))),
        "updateSceneSettings" => i_update_scene_settings(a),
        "createObject" => i_create_object(&at(a, 0), ai(a, 1), ai(a, 2)),
        "updateAudioSampleRate" => i_update_audio_sample_rate(),
        "saveProject" => i_save_project(),
        "undo" => i_history(true),
        "redo" => i_history(false),
        "exportVideoAsync" => i_export_video(a),
        "cancelExport" => i_cancel_export(),
        _ => Value::Null,
    }
}

fn i_toggle_play() -> Value {
    let Some(session) = state() else {
        return Value::Null;
    };
    let mut guard = ui();
    if let Some(preview) = guard.preview.as_mut() {
        preview.toggle_play(&session);
    }
    Value::Null
}

fn i_pause() -> Value {
    let Some(session) = state() else {
        return Value::Null;
    };
    let mut guard = ui();
    if let Some(preview) = guard.preview.as_mut()
        && preview.is_playing
    {
        preview.toggle_play(&session);
    }
    Value::Null
}

fn i_sync_playback_speed() -> Value {
    let Some(session) = state() else {
        return Value::Null;
    };
    let mut guard = ui();
    if let Some(preview) = guard.preview.as_mut() {
        preview.refresh_total_frames(&session);
    }
    Value::Null
}

fn i_apply_selection_ids(ids: &[i32]) -> Value {
    let Some(session) = state() else {
        return Value::Null;
    };
    {
        let mut guard = ui();
        if let Some(timeline) = guard.timeline.as_mut() {
            timeline.selected_ids = ids.iter().copied().collect();
        }
    }
    let holder = app_state::active_world(&session);
    holder
        .lock()
        .unwrap()
        .set_selected_ids(ids.iter().map(|id| *id as usize).collect());
    preview_ids().lock().unwrap().clear();
    Value::Null
}

fn i_update_selection_preview(a: &[Value]) -> Value {
    let (f0, f1) = (ai(a, 0).min(ai(a, 1)), ai(a, 0).max(ai(a, 1)));
    let (l0, l1) = (ai(a, 2).min(ai(a, 3)), ai(a, 2).max(ai(a, 3)));
    let additive = ab(a, 4);
    world_ro!(world, Value::Null);
    let mut ids: Vec<i32> = world
        .get_timeline_objects()
        .into_iter()
        .filter(|o| o.start_frame <= f1 && o.end_frame >= f0 && o.layer >= l0 && o.layer <= l1)
        .map(|o| o.id)
        .collect();
    if additive {
        ids.extend(selected_ids());
        ids.sort_unstable();
        ids.dedup();
    }
    *preview_ids().lock().unwrap() = ids;
    Value::Null
}

fn i_update_clip(object_id: usize, layer: i32, start_frame: i32, duration: i32) -> Value {
    world_rw!(world, Value::Null);
    world.move_object(object_id, start_frame, layer);
    world.resize_object(object_id, start_frame, start_frame + duration.max(1));
    world.update_total_frames();
    Value::Null
}

fn i_delete_ids(ids: &[i32]) -> Value {
    world_rw!(world, Value::Null);
    world.delete_objects(&ids.iter().map(|id| *id as usize).collect::<Vec<_>>());
    world.update_total_frames();
    Value::Null
}

fn i_copy(ids: &[i32], cut: bool) -> Value {
    let Some(session) = state() else {
        return Value::Null;
    };
    let targets: Vec<usize> = ids.iter().map(|id| *id as usize).collect();
    let holder = app_state::active_world(&session);
    let docs = if cut {
        app_state::snapshot_before_edit(&session);
        let mut world = holder.lock().unwrap();
        let docs = world.cut_objects(&targets);
        world.update_total_frames();
        docs
    } else {
        holder.lock().unwrap().copy_objects(&targets)
    };
    app_state::set_clipboard(&session, docs);
    Value::Null
}

fn i_paste(frame: i32, layer: i32) -> Value {
    let Some(session) = state() else {
        return Value::Null;
    };
    let docs = app_state::clipboard(&session);
    app_state::snapshot_before_edit(&session);
    let holder = app_state::active_world(&session);
    let mut world = holder.lock().unwrap();
    let ids = world.paste_objects(&docs, frame, layer);
    world.update_total_frames();
    json!(ids)
}

fn i_split(ids: &[i32], frame: i32) -> Value {
    world_rw!(world, Value::Null);
    for id in ids {
        world.split_object(*id as usize, frame);
    }
    world.update_total_frames();
    Value::Null
}

fn i_move_selected(delta_frame: i32, delta_layer: i32) -> Value {
    let ids = selected_ids();
    world_rw!(world, Value::Null);
    for id in ids {
        let Some(c) = clip_of(&world, id) else {
            continue;
        };
        world.move_object(
            id as usize,
            (c.start_frame + delta_frame).max(0),
            (c.layer + delta_layer).max(0),
        );
    }
    world.update_total_frames();
    Value::Null
}

fn i_resize_selected(delta_start: i32, delta_duration: i32) -> Value {
    let ids = selected_ids();
    world_rw!(world, Value::Null);
    for id in ids {
        let Some(c) = clip_of(&world, id) else {
            continue;
        };
        let start = (c.start_frame + delta_start).max(0);
        world.resize_object(
            id as usize,
            start,
            (c.end_frame + delta_duration).max(start + 1),
        );
    }
    world.update_total_frames();
    Value::Null
}

fn i_apply_clip_batch_move(a: &[Value]) -> Value {
    let moves = a
        .first()
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    world_rw!(world, Value::Null);
    for m in moves {
        let id = m.get("id").and_then(Value::as_i64).unwrap_or(-1);
        if id < 0 {
            continue;
        }
        let layer = m.get("layer").and_then(Value::as_i64).unwrap_or(0) as i32;
        let start = m.get("startFrame").and_then(Value::as_i64).unwrap_or(0) as i32;
        world.move_object(id as usize, start.max(0), layer.max(0));
    }
    world.update_total_frames();
    Value::Null
}

fn i_set_clip_by_upper_object(object_id: usize, enabled: bool) -> Value {
    world_rw!(world, Value::Null);
    let mut target = world.get_clip_target(object_id);
    target.enabled = enabled;
    target.layer_count_up = u32::from(enabled);
    world.set_clip_target(object_id, target);
    Value::Null
}

fn i_set_keyframe(a: &[Value]) -> Value {
    let (object_id, index, key, frame, value) = (au(a, 0), ai(a, 1), at(a, 2), ai(a, 3), af(a, 4));
    let engine_id = a
        .get(5)
        .and_then(|o| o.get("engineId"))
        .and_then(Value::as_str)
        .unwrap_or("linear")
        .to_string();
    world_rw!(world, Value::Null);
    if index < 0 {
        world.set_keyframe(object_id, &key, frame, value, engine_id, Vec::new());
    } else {
        world.set_effect_keyframe(
            object_id,
            index as usize,
            &key,
            frame,
            value,
            engine_id,
            Vec::new(),
        );
    }
    Value::Null
}

fn i_remove_keyframe(a: &[Value]) -> Value {
    let (object_id, index, key, frame) = (au(a, 0), ai(a, 1), at(a, 2), ai(a, 3));
    world_rw!(world, Value::Null);
    if index < 0 {
        world.remove_keyframe(object_id, &key, frame);
    } else {
        world.remove_effect_keyframe(object_id, index as usize, &key, frame);
    }
    Value::Null
}

fn i_move_keyframe(a: &[Value]) -> Value {
    let (object_id, index, key, from, to) = (au(a, 0), ai(a, 1), at(a, 2), ai(a, 3), ai(a, 4));
    world_rw!(world, Value::Null);
    if index < 0 {
        world.move_keyframe(object_id, &key, from, to);
        return Value::Null;
    }
    let value = world
        .effect_param_f32(object_id, index as usize, &key)
        .unwrap_or(0.0);
    world.remove_effect_keyframe(object_id, index as usize, &key, from);
    world.set_effect_keyframe(
        object_id,
        index as usize,
        &key,
        to,
        value,
        "linear".to_string(),
        Vec::new(),
    );
    Value::Null
}

enum EffectOp {
    Add,
    Remove,
    Reorder,
    SetEnabled,
}

fn i_effect_mut(a: &[Value], op: EffectOp) -> Value {
    let object_id = au(a, 0);
    world_rw!(world, Value::Null);
    match op {
        EffectOp::Add => world.add_effect(object_id, &at(a, 1)),
        EffectOp::Remove => world.remove_effect(object_id, au(a, 1)),
        EffectOp::Reorder => world.reorder_effect(object_id, au(a, 1), au(a, 2)),
        EffectOp::SetEnabled => world.set_effect_enabled(object_id, au(a, 1), ab(a, 2)),
    }
    Value::Null
}

fn i_remove_multiple_effects(a: &[Value]) -> Value {
    let object_id = au(a, 0);
    let mut indices = aiv(a, 1);
    indices.sort_unstable_by(|x, y| y.cmp(x));
    world_rw!(world, Value::Null);
    for index in indices {
        world.remove_effect(object_id, index.max(0) as usize);
    }
    Value::Null
}

fn i_reorder_multiple_effects(a: &[Value]) -> Value {
    let object_id = au(a, 0);
    let mut indices = aiv(a, 1);
    indices.sort_unstable();
    let target = au(a, 2);
    world_rw!(world, Value::Null);
    for (offset, index) in indices.into_iter().enumerate() {
        world.reorder_effect(object_id, index.max(0) as usize, target + offset);
    }
    Value::Null
}

fn i_set_effect_parameter(a: &[Value]) -> Value {
    let object_id = au(a, 0);
    let index = au(a, 1);
    let (key, value) = (at(a, 2), af(a, 3));
    world_rw!(world, Value::Null);
    world.set_effect_param(object_id, index, &key, value);
    Value::Null
}

enum AudioOp {
    Remove,
    SetEnabled,
    Reorder,
}

fn i_audio_plugin_mut(a: &[Value], op: AudioOp) -> Value {
    let object_id = au(a, 0);
    world_rw!(world, Value::Null);
    match op {
        AudioOp::Remove => world.remove_audio_plugin(object_id, au(a, 1)),
        AudioOp::SetEnabled => world.set_audio_plugin_bypass(object_id, au(a, 1), !ab(a, 2)),
        AudioOp::Reorder => world.reorder_audio_plugin(object_id, au(a, 1), au(a, 2)),
    }
    Value::Null
}

fn i_add_audio_plugin(object_id: usize, plugin_id: &str) -> Value {
    let Some(entry) = crate::audio::plugin_registry::find_by_id_or_path(plugin_id) else {
        return json!(false);
    };
    world_rw!(world, json!(false));
    world.add_audio_plugin(object_id, &entry, Vec::new());
    json!(true)
}

fn i_set_layer_state(layer: usize, value: bool, field: i32) -> Value {
    world_rw!(world, Value::Null);
    if field == 0 {
        world.set_layer_locked(layer, value);
    } else {
        world.set_layer_visible(layer, value);
    }
    Value::Null
}

fn i_shift_layers(from_layer: i32, to_layer: i32, delta: i32) -> Value {
    world_rw!(world, Value::Null);
    let targets: Vec<(usize, i32)> = world
        .get_timeline_objects()
        .into_iter()
        .filter(|o| o.layer >= from_layer && o.layer <= to_layer)
        .map(|o| (o.id as usize, (o.layer + delta).max(0)))
        .collect();
    for (id, layer) in targets {
        world.set_layer(id, layer);
    }
    Value::Null
}

fn i_update_scene_settings(a: &[Value]) -> Value {
    let scene_id = ai(a, 0);
    let Some(meta) = q_scene_meta(scene_id) else {
        return json!(false);
    };
    let mut settings = SceneSettings::from(&meta);
    settings.name = at(a, 1);
    settings.width = au(a, 2) as u32;
    settings.height = au(a, 3) as u32;
    settings.fps = af(a, 4).round().max(1.0) as u32;
    settings.grid_mode = ai(a, 6);
    settings.grid_bpm = af(a, 7);
    world_rw!(world, json!(false));
    let applied = world.update_scene_settings(scene_id, settings);
    world.update_total_frames();
    json!(applied)
}

fn i_create_object(stable_id: &str, frame: i32, layer: i32) -> Value {
    let Some(plugin) = crate::objects::loader::by_stable_id(stable_id) else {
        return json!(-1);
    };
    world_rw!(world, json!(-1));
    let fps = world.get_project().fps.max(1) as i32;
    let id = world.add_object(frame.max(0), fps * 5, plugin.kind_id, layer.max(0), None);
    world.update_total_frames();
    json!(id)
}

fn i_update_audio_sample_rate() -> Value {
    let Some(session) = state() else {
        return Value::Null;
    };
    let holder = app_state::active_world(&session);
    let project = holder.lock().unwrap().get_project();
    let mixer = app_state::active_audio_mixer(&session);
    mixer
        .lock()
        .unwrap()
        .set_sample_rate(project.audio_sample_rate);
    Value::Null
}

fn i_save_project() -> Value {
    let Some(session) = state() else {
        return json!(false);
    };
    let index = session.lock().unwrap().active;
    json!(app_state::save_session(&session, index))
}

fn i_history(undo: bool) -> Value {
    let Some(session) = state() else {
        return json!(false);
    };
    let holder = app_state::active_world(&session);
    let mut world = holder.lock().unwrap();
    json!(if undo { world.undo() } else { world.redo() })
}

fn i_export_video(a: &[Value]) -> Value {
    let Some(session) = state() else {
        return json!(false);
    };
    let options = a.first().cloned().unwrap_or(Value::Null);
    let mut guard = ui();
    let Some(dialogs) = guard.dialogs.as_mut() else {
        return json!(false);
    };
    let dialog = &mut dialogs.export_dialog;
    if let Some(path) = options.get("outputPath").and_then(Value::as_str) {
        dialog.output_path = path.to_string();
    }
    if let Some(start) = options.get("startFrame").and_then(Value::as_i64) {
        dialog.start_frame = start as i32;
    }
    if let Some(end) = options.get("endFrame").and_then(Value::as_i64) {
        dialog.end_frame = end as i32;
    }
    dialog.start_export(&session);
    json!(dialog.active_queue.is_some())
}

fn i_cancel_export() -> Value {
    let mut guard = ui();
    let Some(dialogs) = guard.dialogs.as_mut() else {
        return json!(false);
    };
    dialogs.export_dialog.active_queue = None;
    json!(true)
}

static UI_SETTINGS: OnceLock<Mutex<Value>> = OnceLock::new();

fn ui_settings_defaults() -> Value {
    json!({
        "timelineMaxLayers": 128,
        "timelineTrackHeight": 30,
        "timelineRulerHeight": 32,
        "timelineLayerHeaderWidth": 60,
        "timelineClipResizeHandleWidth": 10,
        "timelineHeaderHeight": 28,
        "timelineZoomMin": 10,
        "timelineZoomMax": 400,
        "timelineZoomStep": 10,
        "minClipDurationFrames": 5,
        "magneticSnapRange": 10,
        "enableSnap": true,
        "shortcuts": {},
        "defaultProjectFps": 60,
        "defaultProjectFrames": 300,
        "defaultProjectWidth": 1920,
        "defaultProjectHeight": 1080,
        "defaultProjectSampleRate": 48000,
        "exportDefaultCodec": "h264_vaapi",
        "exportDefaultCrf": 20,
        "exportDefaultBitrateMbps": 15,
        "exportDefaultAudioCodec": "aac",
        "exportDefaultAudioBitrateKbps": 192,
        "recentProjects": [],
        "recentProjectMaxCount": 10,
        "sceneFramesMin": 100,
        "sceneFramesMax": 24000,
        "sceneFramesStep": 100,
        "sceneWidthMax": 8000,
        "settingDialogSidebarRight": false
    })
}

fn ui_settings_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent()
                .map(|d| d.join("settings").join("ui-settings.json"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("settings/ui-settings.json"))
}

fn ui_settings() -> &'static Mutex<Value> {
    UI_SETTINGS.get_or_init(|| {
        let mut merged = ui_settings_defaults();
        let stored = std::fs::read_to_string(ui_settings_path())
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if let (Some(target), Some(source)) = (
            merged.as_object_mut(),
            stored.as_ref().and_then(Value::as_object),
        ) {
            for (key, value) in source {
                target.insert(key.clone(), value.clone());
            }
        }
        Mutex::new(merged)
    })
}

pub fn ui_settings_all() -> String {
    ui_settings().lock().unwrap().to_string()
}

pub fn ui_settings_value(key: &str) -> String {
    json!([ui_settings().lock().unwrap().get(key).cloned()]).to_string()
}

pub fn ui_settings_set(key: &str, value_json: &str) {
    let Ok(parsed) = serde_json::from_str::<Vec<Value>>(value_json) else {
        return;
    };
    let Some(value) = parsed.into_iter().next() else {
        return;
    };
    if let Some(map) = ui_settings().lock().unwrap().as_object_mut() {
        map.insert(key.to_string(), value);
    }
}

pub fn ui_settings_save() -> bool {
    let path = ui_settings_path();
    let Some(dir) = path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    std::fs::write(path, ui_settings().lock().unwrap().to_string()).is_ok()
}
