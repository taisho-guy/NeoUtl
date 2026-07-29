use crate::app_state::{self, SharedAppState};
use crate::ecs::EcsWorld;
use crate::ecs::types::Easing;
use crate::{KeyframeEditorWindow, PropertiesWindow, TimelineWindow};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::rc::Rc;

/// プロパティパネル・タイムライン双方が「今どのパラメータを編集対象としているか」を
/// 共有するための状態。中間点の追加・削除でクリップ側マーカーを即時再描画するために使う。
#[derive(Clone)]
pub struct ActiveParam {
    pub object_id: i32,
    pub effect_index: i32,
    pub group: String,
    pub key: String,
}

pub type ActiveParamSlot = Rc<RefCell<Option<ActiveParam>>>;

pub fn new_active_param_slot() -> ActiveParamSlot {
    Rc::new(RefCell::new(None))
}

/// easing-kindコンボボックスのインデックスとEasingヴァリアントの対応表。
/// KeyframeEditorWindow.slint側のeasing-kind==23/24分岐と数値を一致させること。
const EASING_NAMES: [&str; 25] = [
    "リニア",
    "ステップ（無補間）",
    "イーズインサイン",
    "イーズアウトサイン",
    "イーズインアウトサイン",
    "イーズインクアッド",
    "イーズアウトクアッド",
    "イーズインアウトクアッド",
    "イーズインキュービック",
    "イーズアウトキュービック",
    "イーズインアウトキュービック",
    "イーズインクアート",
    "イーズアウトクアート",
    "イーズインアウトクアート",
    "イーズインエクスポ",
    "イーズアウトエクスポ",
    "イーズインアウトエクスポ",
    "イーズインバック",
    "イーズアウトバック",
    "イーズインアウトバック",
    "イーズインバウンス",
    "イーズアウトバウンス",
    "イーズインアウトバウンス",
    "ベジェ",
    "ランダム（seed固定）",
];

fn easing_index(e: &Easing) -> i32 {
    match e {
        Easing::Linear => 0,
        Easing::Step => 1,
        Easing::EaseInSine => 2,
        Easing::EaseOutSine => 3,
        Easing::EaseInOutSine => 4,
        Easing::EaseInQuad => 5,
        Easing::EaseOutQuad => 6,
        Easing::EaseInOutQuad => 7,
        Easing::EaseInCubic => 8,
        Easing::EaseOutCubic => 9,
        Easing::EaseInOutCubic => 10,
        Easing::EaseInQuart => 11,
        Easing::EaseOutQuart => 12,
        Easing::EaseInOutQuart => 13,
        Easing::EaseInExpo => 14,
        Easing::EaseOutExpo => 15,
        Easing::EaseInOutExpo => 16,
        Easing::EaseInBack => 17,
        Easing::EaseOutBack => 18,
        Easing::EaseInOutBack => 19,
        Easing::EaseInBounce => 20,
        Easing::EaseOutBounce => 21,
        Easing::EaseInOutBounce => 22,
        Easing::Bezier { .. } => 23,
        Easing::Random { .. } => 24,
    }
}

fn easing_from_ui(win: &KeyframeEditorWindow) -> Easing {
    match win.get_easing_kind() {
        0 => Easing::Linear,
        1 => Easing::Step,
        2 => Easing::EaseInSine,
        3 => Easing::EaseOutSine,
        4 => Easing::EaseInOutSine,
        5 => Easing::EaseInQuad,
        6 => Easing::EaseOutQuad,
        7 => Easing::EaseInOutQuad,
        8 => Easing::EaseInCubic,
        9 => Easing::EaseOutCubic,
        10 => Easing::EaseInOutCubic,
        11 => Easing::EaseInQuart,
        12 => Easing::EaseOutQuart,
        13 => Easing::EaseInOutQuart,
        14 => Easing::EaseInExpo,
        15 => Easing::EaseOutExpo,
        16 => Easing::EaseInOutExpo,
        17 => Easing::EaseInBack,
        18 => Easing::EaseOutBack,
        19 => Easing::EaseInOutBack,
        20 => Easing::EaseInBounce,
        21 => Easing::EaseOutBounce,
        22 => Easing::EaseInOutBounce,
        24 => Easing::Random {
            seed: win.get_random_seed().max(0) as u32,
            step: win.get_random_step().max(1),
        },
        _ => Easing::Bezier {
            cp1: (win.get_bezier_cp1_x(), win.get_bezier_cp1_y()),
            cp2: (win.get_bezier_cp2_x(), win.get_bezier_cp2_y()),
        },
    }
}

/// 区間内進捗を32分割サンプリングし、プレビュー曲線のSVGパスコマンド文字列を作る。
/// この関数以外に補間演算を行う箇所を作らない（neoutl_interp::easeへ完全委譲）。
/// viewbox 0..1空間の"M x y L x y ..."系列。yはPathのviewbox上端が0のため1-vへ反転する。
fn build_preview_path_commands(easing: Easing) -> String {
    const SAMPLES: i32 = 32;
    let mut commands = String::from("M 0 1");
    for i in 0..=SAMPLES {
        let t = i as f32 / SAMPLES as f32;
        let v = neoutl_interp::ease(easing, t);
        commands.push_str(&format!(" L {t} {}", 1.0 - v));
    }
    commands
}

fn refresh_preview(win: &KeyframeEditorWindow) {
    let commands = build_preview_path_commands(easing_from_ui(win));
    win.set_preview_path_commands(SharedString::from(commands));
}

/// object_id・effect_index・group/keyから対象トラックを取得する。
/// effect_index<0はネイティブパラメータ（KeyframeTracks経由）、
/// 0以上はエフェクトパラメータ（EffectStack経由）を意味する。
fn keyframes_for(
    world: &EcsWorld,
    object_id: i32,
    effect_index: i32,
    key: &str,
) -> Vec<crate::ecs::types::Keyframe> {
    if effect_index < 0 {
        world.get_keyframes(object_id as usize, key)
    } else {
        world.get_effect_keyframes(object_id as usize, effect_index as usize, key)
    }
}

/// 指定frameの中間点をウィンドウへ反映する。既存点があればその値・補間種別を、
/// 無ければ現在の基準値をvalueの初期値として表示する（新規作成モード）。
fn load_point(
    win: &KeyframeEditorWindow,
    world: &EcsWorld,
    object_id: i32,
    effect_index: i32,
    key: &str,
    frame: i32,
    fallback_value: f32,
) {
    let track = keyframes_for(world, object_id, effect_index, key);
    match track.iter().find(|k| k.frame == frame) {
        Some(k) => {
            win.set_point_exists(true);
            win.set_value(k.value);
            win.set_easing_kind(easing_index(&k.easing));
            if let Easing::Bezier { cp1, cp2 } = k.easing {
                win.set_bezier_cp1_x(cp1.0);
                win.set_bezier_cp1_y(cp1.1);
                win.set_bezier_cp2_x(cp2.0);
                win.set_bezier_cp2_y(cp2.1);
            }
            if let Easing::Random { seed, step } = k.easing {
                win.set_random_seed(seed as i32);
                win.set_random_step(step);
            }
        }
        None => {
            win.set_point_exists(false);
            win.set_value(fallback_value);
            win.set_easing_kind(0);
        }
    }
    refresh_preview(win);
}

/// 現在値の取得。ネイティブパラメータはParamAccess経由、エフェクトパラメータは
/// EffectParam::static_valueをNumber前提で読む（Bool/Textは中間点非対応のため
/// このウィンドウ自体を開くボタンがproperties側でkind==0/2のみに限定されている）。
fn current_value(
    world: &EcsWorld,
    object_id: i32,
    effect_index: i32,
    group: &str,
    key: &str,
) -> f32 {
    if effect_index < 0 {
        crate::ui::properties::current_object_param_value(world, object_id as usize, group, key)
    } else {
        world
            .get_effect_instance(object_id as usize, effect_index as usize)
            .and_then(|e| e.params.get(key).cloned())
            .map(|p| match p.static_value {
                crate::ecs::types::Value::Number(n) => n,
                _ => 0.0,
            })
            .unwrap_or(0.0)
    }
}

/// プロパティパネル側のボタンクリックから呼ばれる。ウィンドウ内容を対象パラメータ・
/// 現在フレームで初期化する。
pub fn open_for(
    win: &KeyframeEditorWindow,
    world: &EcsWorld,
    object_id: i32,
    effect_index: i32,
    group: String,
    key: String,
    frame_hint: i32,
) {
    let frame = if frame_hint >= 0 {
        frame_hint
    } else {
        world.current_frame()
    };
    let label = format!("{group} / {key}");
    win.set_object_id(object_id);
    win.set_effect_index(effect_index);
    win.set_group(SharedString::from(group.clone()));
    win.set_key(SharedString::from(key.clone()));
    win.set_label(SharedString::from(label));
    win.set_frame(frame);
    let fallback = current_value(world, object_id, effect_index, &group, &key);
    load_point(win, world, object_id, effect_index, &key, frame, fallback);
}

pub fn setup(
    win: &KeyframeEditorWindow,
    state: SharedAppState,
    props_weak: Weak<PropertiesWindow>,
    timeline_weak: Weak<TimelineWindow>,
    active_param: ActiveParamSlot,
) {
    win.set_easing_names(ModelRc::new(VecModel::from(
        EASING_NAMES
            .iter()
            .map(|s| SharedString::from(*s))
            .collect::<Vec<_>>(),
    )));

    {
        let (state, ww) = (state.clone(), win.as_weak());
        win.on_frame_changed(move |frame| {
            let Some(w) = ww.upgrade() else { return };
            let world_holder = app_state::active_world(&state);
            let world = world_holder.lock().unwrap();
            let object_id = w.get_object_id();
            let effect_index = w.get_effect_index();
            let key = w.get_key().to_string();
            let group = w.get_group().to_string();
            let fallback = current_value(&world, object_id, effect_index, &group, &key);
            load_point(&w, &world, object_id, effect_index, &key, frame, fallback);
        });
    }

    {
        let ww = win.as_weak();
        win.on_easing_changed(move |_| {
            if let Some(w) = ww.upgrade() {
                refresh_preview(&w);
            }
        });
    }
    {
        let ww = win.as_weak();
        win.on_bezier_changed(move || {
            if let Some(w) = ww.upgrade() {
                refresh_preview(&w);
            }
        });
    }
    {
        let ww = win.as_weak();
        win.on_random_changed(move || {
            if let Some(w) = ww.upgrade() {
                refresh_preview(&w);
            }
        });
    }

    {
        let (state, ww, pw, tw, active) = (
            state.clone(),
            win.as_weak(),
            props_weak.clone(),
            timeline_weak.clone(),
            active_param.clone(),
        );
        win.on_apply(move || {
            let Some(w) = ww.upgrade() else { return };
            let object_id = w.get_object_id();
            if object_id < 0 {
                return;
            }
            let effect_index = w.get_effect_index();
            let group = w.get_group().to_string();
            let key = w.get_key().to_string();
            let frame = w.get_frame();
            let value = w.get_value();
            let easing = easing_from_ui(&w);

            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            if effect_index < 0 {
                world.set_keyframe(object_id as usize, &key, frame, value, easing);
            } else {
                world.set_effect_keyframe(
                    object_id as usize,
                    effect_index as usize,
                    &key,
                    frame,
                    value,
                    easing,
                );
            }
            *active.borrow_mut() = Some(ActiveParam {
                object_id,
                effect_index,
                group: group.clone(),
                key: key.clone(),
            });
            w.set_point_exists(true);
            if let Some(p) = pw.upgrade() {
                crate::ui::properties::select_object(&p, &world, object_id);
            }
            if let Some(t) = tw.upgrade() {
                crate::ui::timeline::refresh_keyframe_markers(&t, &world, &active);
            }
        });
    }

    {
        let (state, ww, pw, tw, active) = (
            state.clone(),
            win.as_weak(),
            props_weak.clone(),
            timeline_weak.clone(),
            active_param.clone(),
        );
        win.on_delete(move || {
            let Some(w) = ww.upgrade() else { return };
            let object_id = w.get_object_id();
            if object_id < 0 {
                return;
            }
            let effect_index = w.get_effect_index();
            let key = w.get_key().to_string();
            let frame = w.get_frame();

            app_state::snapshot_before_edit(&state);
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            if effect_index < 0 {
                world.remove_keyframe(object_id as usize, &key, frame);
            } else {
                world.remove_effect_keyframe(
                    object_id as usize,
                    effect_index as usize,
                    &key,
                    frame,
                );
            }
            w.set_point_exists(false);
            if let Some(p) = pw.upgrade() {
                crate::ui::properties::select_object(&p, &world, object_id);
            }
            if let Some(t) = tw.upgrade() {
                crate::ui::timeline::refresh_keyframe_markers(&t, &world, &active);
            }
            let _ = w.hide();
        });
    }

    {
        let ww = win.as_weak();
        win.on_cancel(move || {
            if let Some(w) = ww.upgrade() {
                let _ = w.hide();
            }
        });
    }
}
