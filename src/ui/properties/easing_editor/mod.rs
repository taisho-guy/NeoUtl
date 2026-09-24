mod curve_view;
mod handles;
mod params;
mod presets;
mod script_pane;
mod strip;
mod track;

use crate::ecs::EcsWorld;
use crate::ecs::types::{Keyframe, next_edit_seq};
use crate::infra::localization::effect_param_label;
use curve_view::{CurveView, fmt};
use egui::{Color32, Key, Modifiers, PointerButton, Sense, Stroke, Vec2, pos2, vec2};
use egui_material_icons::icons;
use neoutl_easing_standard::{
    ApplyMode, CurveKind, EasingPayload, ease, encode_payload, parse_payload,
};
use params::Edit;
use std::sync::Mutex;
pub use track::TrackTarget;

const CATEGORY_LABELS: [&str; 5] = ["標準", "振動", "バウンス", "スクリプト", "頂点"];
const HIT_RADIUS: f32 = 12.0;
const PREVIEW_SECONDS: f64 = 1.5;
const SCRIPT_DEBOUNCE: f64 = 0.4;
const CONFIRM_SECONDS: f64 = 4.0;
const NOTICE_SECONDS: f64 = 3.0;
const MODE_HELP: [(&str, &str); 3] = [
    (
        "通常",
        "このキーフレームから次のキーフレームまでを補間します",
    ),
    (
        "中間点を無視",
        "このキーフレームを補間計算から除外し、前後の点をつなぎます",
    ),
    (
        "中間点を補間",
        "このキーフレームの値を前後の補間結果に自動設定します",
    ),
];
const MODES: [ApplyMode; 3] = [
    ApplyMode::Normal,
    ApplyMode::IgnoreMidPoint,
    ApplyMode::Interpolate,
];
const HINT: &str = "ドラッグ: 移動 / Shift: 0.25刻み / Ctrl: 点対称ミラー / 矢印: 微調整 (Shift: 大) / Tab: 次のハンドル / Del・右クリック: リセット / ホイール: ズーム / 背景ドラッグ: パン / ダブルクリック: 頂点追加 (頂点)";

fn category_index(kind: &CurveKind) -> usize {
    match kind {
        CurveKind::Elastic { .. } => 1,
        CurveKind::Bounce { .. } => 2,
        CurveKind::Script { .. } => 3,
        CurveKind::Normal { .. } => 4,
        _ => 0,
    }
}

fn category_default(index: usize) -> CurveKind {
    match index {
        1 => CurveKind::default_elastic(),
        2 => CurveKind::default_bounce(),
        3 => CurveKind::default_script(),
        4 => CurveKind::default_normal(),
        _ => CurveKind::default_bezier(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ApplyScope {
    All,
    FromSelected,
}

struct Reference {
    label: String,
    samples: Vec<[f32; 2]>,
}

struct EditorState {
    target: TrackTarget,
    label: String,
    selected_frame: Option<i32>,
    selected_point: Option<usize>,
    dragging: Option<usize>,
    panning: bool,
    zoom: f32,
    pan: Vec2,
    follow_playhead: bool,
    last_playhead: i32,
    scope: ApplyScope,
    confirm_apply: Option<f64>,
    notice: Option<(String, f64)>,
    hover_preview: Option<CurveKind>,
    reference: Option<Reference>,
    last_curve: Vec<[f32; 2]>,
    preview_start: Option<f64>,
    script_buf: Option<(i32, String)>,
    script_hold: f64,
    script_error: Option<String>,
    script_cache: script_pane::Cache,
    pending_before: Option<Vec<Keyframe>>,
    menu_data: Option<(f32, f32)>,
    menu_point: Option<usize>,
    presets: presets::PresetUi,
}

impl EditorState {
    fn new(target: TrackTarget, label: &str, reference: Option<Reference>) -> Self {
        Self {
            target,
            label: label.to_owned(),
            selected_frame: None,
            selected_point: None,
            dragging: None,
            panning: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            follow_playhead: true,
            last_playhead: i32::MIN,
            scope: ApplyScope::All,
            confirm_apply: None,
            notice: None,
            hover_preview: None,
            reference,
            last_curve: Vec::new(),
            preview_start: None,
            script_buf: None,
            script_hold: 0.0,
            script_error: None,
            script_cache: None,
            pending_before: None,
            menu_data: None,
            menu_point: None,
            presets: presets::PresetUi::focused(),
        }
    }

    fn notify(&mut self, now: f64, text: impl Into<String>) {
        self.notice = Some((text.into(), now));
    }
}

static ACTIVE: Mutex<Option<EditorState>> = Mutex::new(None);

pub fn toggle(target: TrackTarget, label: &str) {
    let mut guard = ACTIVE.lock().unwrap();
    let previous = guard.take();
    if previous.as_ref().is_some_and(|s| s.target == target) {
        return;
    }
    let reference = previous.and_then(|s| {
        if s.last_curve.is_empty() {
            s.reference
        } else {
            Some(Reference {
                label: effect_param_label(&s.label).to_string(),
                samples: s.last_curve,
            })
        }
    });
    *guard = Some(EditorState::new(target, label, reference));
}

pub fn is_open() -> bool {
    ACTIVE.lock().unwrap().is_some()
}

pub fn close() {
    *ACTIVE.lock().unwrap() = None;
}

enum Action {
    Close,
    Undo,
    Redo,
    Select(usize),
    AddKeyframe,
    DeleteEnd,
    Apply(usize, usize),
    Reset,
    Pin,
    InitStart,
    AddEnd,
}

struct Frame<'a> {
    st: &'a mut EditorState,
    ctx: egui::Context,
    now: f64,
    track: Vec<Keyframe>,
    sel: usize,
    payload: EasingPayload,
    playhead: i32,
    can_undo: bool,
    can_redo: bool,
    edit: Edit,
    actions: Vec<Action>,
    out: presets::Out,
}

impl Frame<'_> {
    fn set_kind(&mut self, kind: CurveKind) {
        self.payload.kind = kind;
        self.payload.modifiers.clear();
        self.st.script_buf = None;
        self.st.selected_point = None;
        self.edit.changed = true;
    }
}

pub(super) fn finite_width(ui: &egui::Ui) -> f32 {
    let w = ui.available_width();
    if w.is_finite() {
        w
    } else {
        ui.ctx().content_rect().width()
    }
}

fn button(
    ui: &mut egui::Ui,
    icon: impl Into<&'static str>,
    text: &str,
    hint: &str,
) -> egui::Response {
    let icon: &str = icon.into();
    let label = if text.is_empty() {
        icon.to_owned()
    } else {
        format!("{icon} {text}")
    };
    ui.add(egui::Button::new(label).min_size(vec2(24.0, 24.0)))
        .on_hover_text(hint)
}

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, world: &mut EcsWorld) -> bool {
    let Some(mut st) = ACTIVE.lock().unwrap().take() else {
        return false;
    };
    let keep = run(ctx, ui, world, &mut st);
    if keep {
        *ACTIVE.lock().unwrap() = Some(st);
    }
    true
}

fn finish(world: &mut EcsWorld, st: &mut EditorState) {
    if let Some(before) = st.pending_before.take() {
        let after = track::read(world, &st.target);
        track::push_history(world, &st.target, before, after);
    }
}

fn commit(world: &mut EcsWorld, st: &mut EditorState, edit: impl FnOnce(&mut Vec<Keyframe>)) {
    finish(world, st);
    let before = track::read(world, &st.target);
    let mut after = before.clone();
    edit(&mut after);
    track::write(world, &st.target, &after);
    track::push_history(world, &st.target, before, after);
}

fn run(ctx: &egui::Context, ui: &mut egui::Ui, world: &mut EcsWorld, st: &mut EditorState) -> bool {
    ui.style_mut().interaction.tooltip_delay = 0.0;
    let now = ui.input(|i| i.time);
    let track = track::read(world, &st.target);
    let playhead = world.current_frame();
    let mut actions = Vec::new();

    if track.len() < 2 {
        let (clip_start, clip_end) = track::clip_bounds(world, &st.target);
        match track.first() {
            None => {
                ui.label("始点キーフレームがありません。始点は必須です。");
                if ui
                    .button(format!(
                        "{} 始点を追加 (f{clip_start})",
                        <&str>::from(icons::ICON_ADD)
                    ))
                    .clicked()
                {
                    actions.push(Action::InitStart);
                }
            }
            Some(first) => {
                ui.label(format!(
                    "始点 f{} のみです。終点は任意で、追加すると区間が生まれます。",
                    first.frame
                ));
                let at = if playhead > first.frame && playhead <= clip_end {
                    playhead
                } else {
                    clip_end
                };
                if ui
                    .add_enabled(
                        at > first.frame,
                        egui::Button::new(format!(
                            "{} 終点を追加 (f{at})",
                            <&str>::from(icons::ICON_ADD)
                        )),
                    )
                    .on_hover_text("再生位置 (範囲外はクリップ終端) に終点を追加")
                    .on_disabled_hover_text("始点がクリップ終端以降のため追加できません")
                    .clicked()
                {
                    actions.push(Action::AddEnd);
                }
            }
        }
        if ui.button("閉じる").clicked() {
            actions.push(Action::Close);
        }
        return apply_actions(
            world,
            st,
            &track,
            0,
            &EasingPayload::linear(),
            now,
            actions,
            ctx,
        );
    }

    if st.follow_playhead && playhead != st.last_playhead {
        st.last_playhead = playhead;
        let last = track.len() - 1;
        if (track[0].frame..=track[last].frame).contains(&playhead) {
            let i = track
                .iter()
                .rposition(|k| k.frame <= playhead)
                .unwrap_or(0)
                .min(last - 1);
            st.selected_frame = Some(track[i].frame);
        }
    }
    let sel = st
        .selected_frame
        .and_then(|fr| track.iter().position(|k| k.frame <= fr && fr == k.frame))
        .filter(|i| i + 1 < track.len())
        .unwrap_or(0);
    if st.selected_frame != Some(track[sel].frame) {
        st.selected_frame = Some(track[sel].frame);
        st.selected_point = None;
    }

    let mut payload = parse_payload(&track[sel].engine_payload);
    if matches!(payload.kind, CurveKind::Linear) {
        payload.kind = CurveKind::Bezier {
            handle_left: [1.0 / 3.0, 1.0 / 3.0],
            handle_right: [2.0 / 3.0, 2.0 / 3.0],
        };
    }
    if let (Some((frame, text)), CurveKind::Script { source }) = (&st.script_buf, &mut payload.kind)
        && *frame == track[sel].frame
    {
        source.clone_from(text);
    }
    let stored_script = match parse_payload(&track[sel].engine_payload).kind {
        CurveKind::Script { source } => Some(source),
        _ => None,
    };

    let mut f = Frame {
        st,
        ctx: ctx.clone(),
        now,
        track,
        sel,
        payload,
        playhead,
        can_undo: world.can_undo(),
        can_redo: world.can_redo(),
        edit: Edit::default(),
        actions,
        out: presets::Out::default(),
    };

    if ui.ui_contains_pointer() && !ctx.egui_wants_keyboard_input() {
        let pasted = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Paste(s) => Some(s.clone()),
                _ => None,
            })
        });
        if let Some(text) = pasted {
            match serde_json::from_str::<EasingPayload>(&text) {
                Ok(p) => {
                    f.payload = p;
                    f.st.script_buf = None;
                    f.edit.changed = true;
                    f.st.notify(now, "貼り付けました");
                }
                Err(_) => {
                    f.st.notify(now, "貼り付け失敗: イージングのJSONではありません")
                }
            }
        }
    }

    let width = finite_width(ui);
    egui::ScrollArea::vertical()
        .id_salt("easing_editor_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(width);
            toolbar(&mut f, ui);
            ui.separator();
            if width >= 640.0 {
                ui.columns(2, |cols| {
                    left(&mut f, &mut cols[0]);
                    right(&mut f, &mut cols[1]);
                });
            } else {
                left(&mut f, ui);
                ui.separator();
                right(&mut f, ui);
            }
        });

    if let Some(kind) = f.out.apply.take() {
        f.set_kind(kind);
    }
    f.st.hover_preview = f.out.hover.take();
    if let Some(text) = f.out.notice.take() {
        f.st.notify(now, text);
    }
    if f.st.hover_preview.is_some() || f.st.preview_start.is_some() {
        ctx.request_repaint();
    }

    let script_dirty = match (&f.payload.kind, &stored_script) {
        (CurveKind::Script { source }, Some(stored)) => source != stored,
        _ => false,
    };
    let mut script_pending = false;
    if script_dirty && f.st.script_error.is_none() {
        if now >= f.st.script_hold {
            f.edit.changed = true;
        } else {
            script_pending = true;
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    if script_dirty && f.st.script_error.is_some() {
        f.edit.changed = f.edit.changed && !matches!(f.payload.kind, CurveKind::Script { .. });
    }

    let esc_close = ui.ui_contains_pointer()
        && !ctx.egui_wants_keyboard_input()
        && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    if esc_close {
        f.actions.push(Action::Close);
    }

    let Frame {
        st,
        track,
        sel,
        payload,
        edit,
        actions,
        ..
    } = f;
    if edit.changed {
        if st.pending_before.is_none() {
            st.pending_before = Some(track.clone());
        }
        let mut keys = track.clone();
        keys[sel].engine_payload = encode_payload(&payload);
        keys[sel].edit_seq = next_edit_seq();
        track::write(world, &st.target, &keys);
    }
    if !edit.live && st.dragging.is_none() && !script_pending {
        finish(world, st);
    }
    apply_actions(world, st, &track, sel, &payload, now, actions, ctx)
}

#[allow(clippy::too_many_arguments)]
fn apply_actions(
    world: &mut EcsWorld,
    st: &mut EditorState,
    track: &[Keyframe],
    sel: usize,
    payload: &EasingPayload,
    now: f64,
    actions: Vec<Action>,
    ctx: &egui::Context,
) -> bool {
    let mut keep = true;
    for action in actions {
        match action {
            Action::Close => keep = false,
            Action::Undo => {
                finish(world, st);
                st.script_buf = None;
                world.undo();
            }
            Action::Redo => {
                finish(world, st);
                st.script_buf = None;
                world.redo();
            }
            Action::Select(i) => {
                if let Some(k) = track.get(i) {
                    st.selected_frame = Some(k.frame);
                    st.selected_point = None;
                    st.script_buf = None;
                }
            }
            Action::Pin => {
                st.reference = Some(Reference {
                    label: effect_param_label(&st.label).to_string(),
                    samples: st.last_curve.clone(),
                });
                st.notify(now, "現在の曲線を参照に固定しました");
            }
            Action::InitStart => {
                let keys = track::start_only(world, &st.target);
                commit(world, st, |k| *k = keys);
            }
            Action::AddEnd => {
                match track::with_end(world, &st.target, track, world.current_frame()) {
                    Some(keys) => {
                        commit(world, st, |k| *k = keys);
                        st.notify(now, "終点を追加しました");
                    }
                    None => st.notify(now, "追加できません: 始点がクリップ終端以降です"),
                }
            }
            Action::Reset => {
                st.script_buf = None;
                commit(world, st, |k| {
                    k[sel].engine_payload = encode_payload(&EasingPayload::linear());
                    k[sel].edit_seq = next_edit_seq();
                });
                st.notify(now, "直線に初期化しました (Undo可)");
            }
            Action::Apply(from, to) => {
                let encoded = encode_payload(payload);
                commit(world, st, |k| {
                    for key in k.iter_mut().take(to).skip(from) {
                        key.engine_payload = encoded.clone();
                        key.edit_seq = next_edit_seq();
                    }
                });
                st.notify(now, format!("{}区間に適用しました (Undo可)", to - from));
            }
            Action::DeleteEnd => {
                if let Some(end) = track.get(sel + 1) {
                    let removed = end.frame;
                    commit(world, st, |k| {
                        k.remove(sel + 1);
                    });
                    st.notify(
                        now,
                        format!("終点 f{removed} を削除しました (始点は残ります)"),
                    );
                }
            }
            Action::AddKeyframe => add_keyframe(world, st, track, sel, payload, now),
        }
    }
    let _ = ctx;
    keep
}

fn add_keyframe(
    world: &mut EcsWorld,
    st: &mut EditorState,
    track: &[Keyframe],
    sel: usize,
    payload: &EasingPayload,
    now: f64,
) {
    let playhead = world.current_frame();
    let last = &track[track.len() - 1];
    let (_, clip_end) = track::clip_bounds(world, &st.target);
    if playhead > last.frame && playhead <= clip_end {
        let mut key = last.clone();
        key.frame = playhead;
        key.edit_seq = next_edit_seq();
        commit(world, st, |k| k.push(key));
        st.selected_frame = Some(last.frame);
        st.notify(now, format!("終点 f{playhead} を追加しました"));
        return;
    }
    let j = track
        .windows(2)
        .position(|w| w[0].frame < playhead && playhead < w[1].frame)
        .unwrap_or(sel);
    let (a, b) = (&track[j], &track[j + 1]);
    let frame = if a.frame < playhead && playhead < b.frame {
        playhead
    } else {
        (a.frame + b.frame) / 2
    };
    if frame <= a.frame || frame >= b.frame {
        st.notify(now, "挿入できません: 区間が1フレーム以下です");
        return;
    }
    let seg = if j == sel {
        payload.clone()
    } else {
        parse_payload(&a.engine_payload)
    };
    let t = (frame - a.frame) as f32 / (b.frame - a.frame) as f32;
    let value = a.value + (b.value - a.value) * ease(&seg, t);
    let mut key = a.clone();
    key.frame = frame;
    key.value = value;
    key.edit_seq = next_edit_seq();
    commit(world, st, |k| k.insert(j + 1, key));
    st.selected_frame = Some(frame);
    st.selected_point = None;
    st.notify(now, format!("f{frame} にキーフレームを追加しました"));
}

fn toolbar(f: &mut Frame, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(effect_param_label(&f.st.label)).strong());
        ui.separator();
        if ui
            .add_enabled(
                f.can_undo,
                egui::Button::new(icons::ICON_UNDO).min_size(vec2(24.0, 24.0)),
            )
            .on_hover_text("元に戻す")
            .clicked()
        {
            f.actions.push(Action::Undo);
        }
        if ui
            .add_enabled(
                f.can_redo,
                egui::Button::new(icons::ICON_REDO).min_size(vec2(24.0, 24.0)),
            )
            .on_hover_text("やり直す")
            .clicked()
        {
            f.actions.push(Action::Redo);
        }
        if button(
            ui,
            icons::ICON_CONTENT_COPY,
            "コピー",
            "曲線をJSONでコピー (Ctrl+Vで貼り付け)",
        )
        .clicked()
        {
            f.ctx
                .copy_text(serde_json::to_string_pretty(&f.payload).unwrap_or_default());
            f.st.notify(f.now, "コピーしました。Ctrl+V で貼り付けできます");
        }
        if button(
            ui,
            icons::ICON_LIBRARY_ADD,
            "保存",
            "現在の曲線をプリセットとして保存 (再起動後も残ります)",
        )
        .clicked()
        {
            presets::save(f.payload.kind.clone(), &mut f.st.presets);
            f.st.notify(f.now, "保存しました。名前は保存済み一覧で変更できます");
        }
        if button(
            ui,
            icons::ICON_REFRESH,
            "直線に初期化",
            "この区間を直線へ戻す",
        )
        .clicked()
        {
            f.actions.push(Action::Reset);
        }
        if button(
            ui,
            icons::ICON_PUSH_PIN,
            "参照に固定",
            "現在の曲線を点線で残し、別の項目と比較",
        )
        .clicked()
        {
            f.actions.push(Action::Pin);
        }
        if button(ui, icons::ICON_CLOSE, "閉じる", "エディタを閉じる (Esc)").clicked() {
            f.actions.push(Action::Close);
        }
    });
    ui.horizontal_wrapped(|ui| {
        let current = category_index(&f.payload.kind);
        for (i, label) in CATEGORY_LABELS.iter().enumerate() {
            if ui.selectable_label(current == i, *label).clicked() && current != i {
                f.set_kind(category_default(i));
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        let segments = f.track.len() - 1;
        if button(ui, icons::ICON_CHEVRON_LEFT, "", "前の区間").clicked() && f.sel > 0 {
            f.actions.push(Action::Select(f.sel - 1));
        }
        ui.label(format!("{} / {}", f.sel + 1, segments));
        if button(ui, icons::ICON_CHEVRON_RIGHT, "", "次の区間").clicked() && f.sel + 1 < segments
        {
            f.actions.push(Action::Select(f.sel + 1));
        }
        if button(
            ui,
            icons::ICON_ADD,
            "再生位置に追加",
            "再生位置にキーフレームを挿入 (最終キーフレームより後なら終点として追加)",
        )
        .clicked()
        {
            f.actions.push(Action::AddKeyframe);
        }
        if ui
            .add(
                egui::Button::new(format!("{} 終点を削除", <&str>::from(icons::ICON_REMOVE)))
                    .min_size(vec2(24.0, 24.0)),
            )
            .on_hover_text("区間の終点キーフレームを削除 (末尾なら区間ごと消え、次の区間があれば統合。始点は削除不可)")
            .clicked()
        {
            f.actions.push(Action::DeleteEnd);
        }
        ui.checkbox(&mut f.st.follow_playhead, "再生位置に追従")
            .on_hover_text("再生ヘッドが動くと、その区間を選択");
        if button(
            ui,
            icons::ICON_PLAY_ARROW,
            "プレビュー",
            "曲線に沿った動きを再生",
        )
        .clicked()
        {
            f.st.preview_start = Some(f.now);
        }
        if button(
            ui,
            icons::ICON_FIT_SCREEN,
            "表示リセット",
            "ズームとパンを初期化",
        )
        .clicked()
        {
            f.st.zoom = 1.0;
            f.st.pan = Vec2::ZERO;
        }
    });
    ui.horizontal_wrapped(|ui| {
        let reversible = matches!(
            f.payload.kind,
            CurveKind::Bounce { .. } | CurveKind::Elastic { .. }
        );
        if ui
            .add_enabled(
                reversible,
                egui::Button::new(format!("{} 反転", <&str>::from(icons::ICON_SWAP_HORIZ)))
                    .min_size(vec2(24.0, 24.0)),
            )
            .on_disabled_hover_text("反転は「振動」「バウンス」でのみ使えます")
            .clicked()
        {
            if let CurveKind::Bounce { reversed, .. } | CurveKind::Elastic { reversed, .. } =
                &mut f.payload.kind
            {
                *reversed = !*reversed;
            }
            f.edit.changed = true;
        }
        ui.separator();
        ui.label("補間モード");
        for (mode, (label, help)) in MODES.iter().zip(MODE_HELP) {
            if ui
                .selectable_label(f.payload.apply_mode == *mode, label)
                .on_hover_text(help)
                .clicked()
            {
                f.payload.apply_mode = *mode;
                f.edit.changed = true;
            }
        }
    });
    ui.label(egui::RichText::new(HINT).weak().small());
    if let Some((text, at)) = &f.st.notice
        && f.now - at < NOTICE_SECONDS
    {
        ui.colored_label(ui.visuals().warn_fg_color, text);
        f.ctx
            .request_repaint_after(std::time::Duration::from_millis(500));
    }
}

fn left(f: &mut Frame, ui: &mut egui::Ui) {
    let (k0, k1) = (f.track[f.sel].clone(), f.track[f.sel + 1].clone());
    let pts = handles::control_points(&f.payload.kind);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(CATEGORY_LABELS[category_index(&f.payload.kind)]).strong());
        ui.weak(format!(
            "区間 {}/{}　f{} → f{} ({}f)　値 {} → {}",
            f.sel + 1,
            f.track.len() - 1,
            k0.frame,
            k1.frame,
            k1.frame - k0.frame,
            fmt(k0.value),
            fmt(k1.value)
        ));
        match f.st.selected_point.and_then(|i| pts.get(i)) {
            Some(p) => ui.weak(format!("選択: {}", p.label)),
            None => ui.weak("頂点未選択"),
        };
        if let Some(r) = &f.st.reference {
            ui.weak(format!("参照: {}", r.label));
            if ui
                .small_button(icons::ICON_CLOSE)
                .on_hover_text("参照を消去")
                .clicked()
            {
                f.st.reference = None;
            }
        }
    });
    graph(f, ui, &pts, &k0, &k1);
    if let Some(i) = strip::show(ui, &f.track, f.sel, f.playhead) {
        f.actions.push(Action::Select(i));
    }
    ui.group(|ui| {
        ui.set_min_width(ui.available_width());
        params::show(ui, &mut f.payload.kind, &mut f.edit);
    });
    apply_row(f, ui);
}

fn apply_row(f: &mut Frame, ui: &mut egui::Ui) {
    let total = f.track.len() - 1;
    let from = if f.st.scope == ApplyScope::All {
        0
    } else {
        f.sel
    };
    let encoded = encode_payload(&f.payload);
    let differs = |i: usize| f.track[i].engine_payload != encoded;
    let others = (from..total).filter(|&i| i != f.sel && differs(i)).count();
    let confirming =
        f.st.confirm_apply
            .is_some_and(|t| f.now - t < CONFIRM_SECONDS);
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut f.st.scope, ApplyScope::All, "全区間");
        ui.selectable_value(&mut f.st.scope, ApplyScope::FromSelected, "この区間以降");
        let label = if confirming {
            format!("上書きを確定 ({others}区間を変更)")
        } else {
            format!("この曲線を適用 ({}区間)", total - from)
        };
        let clicked = ui
            .add_enabled(
                others > 0,
                egui::Button::new(label).min_size(vec2(0.0, 28.0)),
            )
            .on_hover_text("この区間の曲線を範囲内の各区間へコピー (Undo可)")
            .on_disabled_hover_text("範囲内はすべて同じ曲線です")
            .clicked();
        if clicked {
            if confirming {
                f.actions.push(Action::Apply(from, total));
                f.st.confirm_apply = None;
            } else {
                f.st.confirm_apply = Some(f.now);
                f.ctx
                    .request_repaint_after(std::time::Duration::from_secs_f64(CONFIRM_SECONDS));
            }
        }
    });
}

fn right(f: &mut Frame, ui: &mut egui::Ui) {
    if let CurveKind::Script { source } = &mut f.payload.kind {
        let mut buffer = source.clone();
        if script_pane::show(ui, &mut buffer, f.st.script_error.as_deref()) {
            f.st.script_error = script_pane::validate(&buffer).err();
            f.st.script_hold = f.now + SCRIPT_DEBOUNCE;
            f.st.script_buf = Some((f.track[f.sel].frame, buffer.clone()));
            *source = buffer;
        }
        ui.separator();
    }
    presets::show(ui, &mut f.st.presets, &f.payload.kind, &mut f.out);
}

fn graph(
    f: &mut Frame,
    ui: &mut egui::Ui,
    pts: &[handles::ControlPoint],
    k0: &Keyframe,
    k1: &Keyframe,
) {
    let v = ui.visuals().clone();
    let (accent, weak, guide, grid_col, ink, outline) = (
        v.selection.stroke.color,
        v.weak_text_color(),
        v.widgets.inactive.fg_stroke.color,
        v.widgets.noninteractive.bg_stroke.color,
        v.text_color(),
        v.extreme_bg_color,
    );
    let is_script = matches!(f.payload.kind, CurveKind::Script { .. });
    let samples: Vec<[f32; 2]> = match &f.payload.kind {
        CurveKind::Script { source }
            if f.st.script_error.is_some() && f.st.script_cache.is_some() =>
        {
            let _ = source;
            f.st.script_cache
                .as_ref()
                .map(|(_, p)| p.clone())
                .unwrap_or_default()
        }
        CurveKind::Script { source } => {
            script_pane::samples(&mut f.st.script_cache, source).to_vec()
        }
        _ => curve_view::sample(&f.payload, 160),
    };
    let (y_lo, y_hi) = curve_view::y_range(&samples, pts.iter().map(|p| p.pos[1]));
    let width = finite_width(ui).clamp(200.0, 1600.0);
    let height = (width * (y_hi - y_lo)).clamp(260.0, 480.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let mut view = CurveView::fit(rect, y_lo, y_hi, f.st.zoom, f.st.pan);

    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if let (true, Some(p)) = (scroll != 0.0, response.hover_pos()) {
            ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            let anchor = view.to_data(p);
            f.st.zoom = (f.st.zoom * (scroll * 0.004).exp()).clamp(0.25, 16.0);
            view = CurveView::fit(rect, y_lo, y_hi, f.st.zoom, f.st.pan);
            f.st.pan += p - view.to_screen(anchor.0, anchor.1);
            view = CurveView::fit(rect, y_lo, y_hi, f.st.zoom, f.st.pan);
        }
    }
    let to_screen = |p: [f32; 2]| view.to_screen(p[0], p[1]);
    let hover_hit = response
        .hover_pos()
        .and_then(|p| handles::nearest(pts, to_screen, p, HIT_RADIUS));
    let pointer = response.interact_pointer_pos();

    if response.drag_started_by(PointerButton::Primary) {
        let hit = pointer.and_then(|p| handles::nearest(pts, to_screen, p, HIT_RADIUS));
        f.st.dragging = hit;
        f.st.panning = hit.is_none();
        if hit.is_some() {
            f.st.selected_point = hit;
        }
    }
    if response.dragged_by(PointerButton::Middle) {
        f.st.pan += response.drag_delta();
    }
    if response.dragged_by(PointerButton::Primary) {
        match (f.st.dragging, pointer) {
            (Some(i), Some(p)) => {
                if let Some(cp) = pts.get(i) {
                    let mods = ui.input(|i| i.modifiers);
                    handles::apply_drag(&mut f.payload.kind, cp.point, view.to_data(p), mods);
                    f.edit.changed = true;
                    f.edit.live = true;
                }
            }
            _ if f.st.panning => f.st.pan += response.drag_delta(),
            _ => {}
        }
    }
    if response.drag_stopped() {
        f.st.dragging = None;
        f.st.panning = false;
    }
    if response.clicked() {
        f.st.selected_point = pointer.and_then(|p| handles::nearest(pts, to_screen, p, HIT_RADIUS));
    }
    if response.double_clicked()
        && hover_hit.is_none()
        && let (CurveKind::Normal { segments }, Some(p)) = (&mut f.payload.kind, pointer)
    {
        neoutl_easing_standard::add_segment(segments, view.to_data(p).0.clamp(0.05, 0.95));
        f.edit.changed = true;
    }
    if response.secondary_clicked() {
        f.st.menu_point = pointer.and_then(|p| handles::nearest(pts, to_screen, p, HIT_RADIUS));
        f.st.menu_data = pointer.map(|p| view.to_data(p));
    }
    response.context_menu(|ui| {
        if let Some(cp) = f.st.menu_point.and_then(|i| pts.get(i)) {
            if ui.button(handles::reset_label(&f.payload.kind)).clicked() {
                if handles::reset_point(&mut f.payload.kind, cp.point) {
                    f.edit.changed = true;
                    f.st.selected_point = None;
                }
                ui.close();
            }
        }
        if let (CurveKind::Normal { segments }, Some((x, _))) =
            (&mut f.payload.kind, f.st.menu_data)
            && ui.button("ここに頂点を追加").clicked()
        {
            neoutl_easing_standard::add_segment(segments, x.clamp(0.05, 0.95));
            f.edit.changed = true;
            ui.close();
        }
        if ui.button("表示をリセット").clicked() {
            f.st.zoom = 1.0;
            f.st.pan = Vec2::ZERO;
            ui.close();
        }
    });

    if (response.has_focus() || response.hovered()) && !ui.ctx().egui_wants_keyboard_input() {
        let (delta, del, tab, esc) = ui.input_mut(|i| {
            let mut d = Vec2::ZERO;
            for (mods, step) in [(Modifiers::NONE, 0.01), (Modifiers::SHIFT, 0.05)] {
                for (key, dir) in [
                    (Key::ArrowLeft, vec2(-1.0, 0.0)),
                    (Key::ArrowRight, vec2(1.0, 0.0)),
                    (Key::ArrowUp, vec2(0.0, 1.0)),
                    (Key::ArrowDown, vec2(0.0, -1.0)),
                ] {
                    if i.consume_key(mods, key) {
                        d += dir * step;
                    }
                }
            }
            (
                d,
                i.consume_key(Modifiers::NONE, Key::Delete)
                    || i.consume_key(Modifiers::NONE, Key::Backspace),
                response.has_focus() && i.consume_key(Modifiers::NONE, Key::Tab),
                f.st.selected_point.is_some() && i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if tab && !pts.is_empty() {
            f.st.selected_point = Some(f.st.selected_point.map_or(0, |i| (i + 1) % pts.len()));
        }
        if esc {
            f.st.selected_point = None;
        }
        if let Some(cp) = f.st.selected_point.and_then(|i| pts.get(i)) {
            if delta != Vec2::ZERO {
                handles::apply_drag(
                    &mut f.payload.kind,
                    cp.point,
                    (cp.pos[0] + delta.x, cp.pos[1] + delta.y),
                    Modifiers::NONE,
                );
                f.edit.changed = true;
            }
            if del && handles::reset_point(&mut f.payload.kind, cp.point) {
                f.edit.changed = true;
                f.st.selected_point = None;
            }
        }
    }
    if f.st.dragging.is_some() || f.st.panning {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if hover_hit.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    let samples = if f.edit.changed && !is_script {
        curve_view::sample(&f.payload, 160)
    } else {
        samples
    };
    let pts = &handles::control_points(&f.payload.kind);
    let shown = f.payload.clone();
    let value_at = |t: f32| {
        if is_script {
            curve_view::interpolate(&samples, t)
        } else {
            ease(&shown, t)
        }
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, outline);
    if !(view.scale().is_finite() && view.scale() > 1.0) {
        f.st.last_curve = samples;
        return;
    }
    let step = curve_view::grid_step(view.scale());
    let (x_min, y_top) = view.to_data(rect.left_top());
    let (x_max, y_bottom) = view.to_data(rect.right_bottom());
    let font = egui::FontId::proportional(10.0);
    let mut gx = (x_min / step).floor() * step;
    let mut guard = 0;
    while gx <= x_max && guard < 512 {
        guard += 1;
        let sx = view.to_screen(gx, 0.0).x;
        painter.vline(sx, rect.y_range(), Stroke::new(1.0, grid_col));
        painter.text(
            pos2(sx + 2.0, rect.bottom() - 2.0),
            egui::Align2::LEFT_BOTTOM,
            fmt(gx),
            font.clone(),
            weak,
        );
        gx += step;
    }
    let mut gy = (y_bottom / step).floor() * step;
    guard = 0;
    while gy <= y_top && guard < 512 {
        guard += 1;
        let sy = view.to_screen(0.0, gy).y;
        painter.hline(rect.x_range(), sy, Stroke::new(1.0, grid_col));
        painter.text(
            pos2(rect.left() + 3.0, sy - 1.0),
            egui::Align2::LEFT_BOTTOM,
            fmt(gy),
            font.clone(),
            weak,
        );
        gy += step;
    }
    painter.rect_stroke(
        egui::Rect::from_two_pos(view.to_screen(0.0, 1.0), view.to_screen(1.0, 0.0)),
        0.0,
        Stroke::new(1.5, guide),
        egui::StrokeKind::Middle,
    );
    painter.text(
        pos2(rect.right() - 4.0, view.to_screen(1.0, 0.0).y - 2.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("始 {}", fmt(k0.value)),
        font.clone(),
        ink,
    );
    painter.text(
        pos2(rect.right() - 4.0, view.to_screen(1.0, 1.0).y + 2.0),
        egui::Align2::RIGHT_TOP,
        format!("終 {}", fmt(k1.value)),
        font.clone(),
        ink,
    );

    let polyline = |points: &[[f32; 2]]| -> Vec<egui::Pos2> {
        points.iter().map(|p| view.to_screen(p[0], p[1])).collect()
    };
    if let Some(r) = &f.st.reference {
        painter.extend(egui::Shape::dashed_line(
            &polyline(&r.samples),
            Stroke::new(1.5, weak),
            6.0,
            4.0,
        ));
    }
    if let Some(kind) = &f.st.hover_preview
        && !matches!(kind, CurveKind::Script { .. })
    {
        let ghost = EasingPayload {
            kind: kind.clone(),
            ..Default::default()
        };
        painter.add(egui::Shape::line(
            polyline(&curve_view::sample(&ghost, 96)),
            Stroke::new(2.0, accent.linear_multiply(0.5)),
        ));
    }
    match &f.payload.kind {
        CurveKind::Bezier {
            handle_left: l,
            handle_right: r,
        } => {
            painter.line_segment(
                [view.to_screen(0.0, 0.0), view.to_screen(l[0], l[1])],
                Stroke::new(1.0, guide),
            );
            painter.line_segment(
                [view.to_screen(1.0, 1.0), view.to_screen(r[0], r[1])],
                Stroke::new(1.0, guide),
            );
        }
        CurveKind::Elastic { reversed, .. } => {
            if let Some(cp) = pts.first() {
                let base_y = if *reversed { 0.0 } else { 1.0 };
                painter.line_segment(
                    [view.to_screen(cp.pos[0], base_y), to_screen(cp.pos)],
                    Stroke::new(1.0, guide),
                );
            }
        }
        CurveKind::Normal { segments } => {
            for s in segments {
                painter.vline(
                    view.to_screen(s.anchor_start[0], 0.0).x,
                    rect.y_range(),
                    Stroke::new(1.0, guide),
                );
            }
        }
        _ => {}
    }
    painter.add(egui::Shape::line(
        polyline(&samples),
        Stroke::new(3.0, accent),
    ));

    let t_play = (f.playhead - k0.frame) as f32 / (k1.frame - k0.frame).max(1) as f32;
    if (0.0..=1.0).contains(&t_play) {
        let x = view.to_screen(t_play, 0.0).x;
        painter.extend(egui::Shape::dashed_line(
            &[pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, v.warn_fg_color),
            4.0,
            4.0,
        ));
        let y = value_at(t_play);
        painter.circle_filled(view.to_screen(t_play, y), 4.0, v.warn_fg_color);
        painter.text(
            view.to_screen(t_play, y) + vec2(6.0, -6.0),
            egui::Align2::LEFT_BOTTOM,
            format!(
                "f{}: {}",
                f.playhead,
                fmt(k0.value + (k1.value - k0.value) * y)
            ),
            font.clone(),
            v.warn_fg_color,
        );
    }
    if let Some(start) = f.st.preview_start {
        let t = ((f.now - start) / PREVIEW_SECONDS) as f32;
        if t >= 1.0 {
            f.st.preview_start = None;
        } else {
            let y = value_at(t);
            let dot = view.to_screen(t, y);
            painter.hline(
                dot.x..=rect.right(),
                dot.y,
                Stroke::new(1.0, accent.linear_multiply(0.6)),
            );
            painter.circle_filled(dot, 6.0, Color32::from_rgb(0xff, 0x9e, 0x3d));
            painter.text(
                pos2(rect.right() - 4.0, dot.y - 3.0),
                egui::Align2::RIGHT_BOTTOM,
                fmt(k0.value + (k1.value - k0.value) * y),
                font.clone(),
                ink,
            );
        }
    }

    let start = view.to_screen(0.0, 0.0);
    painter.circle_stroke(start, 6.0, Stroke::new(2.0, ink));
    painter.text(
        start + vec2(9.0, 4.0),
        egui::Align2::LEFT_TOP,
        "始点 (固定)",
        font.clone(),
        weak,
    );
    let end = view.to_screen(1.0, 1.0);
    painter.circle_filled(end, 5.0, ink);
    painter.text(
        end + vec2(-9.0, -4.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("終点 f{}", k1.frame),
        font.clone(),
        weak,
    );
    for (i, cp) in pts.iter().enumerate() {
        let center = to_screen(cp.pos);
        let selected = f.st.selected_point == Some(i);
        let hot = selected || hover_hit == Some(i) || f.st.dragging == Some(i);
        let radius = 6.0 + if hot { 2.0 } else { 0.0 };
        painter.circle(
            center,
            radius,
            if selected { accent } else { ink },
            Stroke::new(2.0, outline),
        );
        if selected {
            painter.circle_stroke(center, radius + 4.0, Stroke::new(1.5, accent));
        }
        let galley = painter.layout_no_wrap(cp.label.clone(), font.clone(), ink);
        let label_rect =
            egui::Rect::from_min_size(center + vec2(10.0, -22.0), galley.size()).expand(3.0);
        painter.rect_filled(label_rect, 3.0, outline.gamma_multiply(0.85));
        painter.galley(label_rect.shrink(3.0).min, galley, ink);
    }
    f.st.last_curve = samples;
}
