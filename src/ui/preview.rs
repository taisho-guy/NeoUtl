use crate::app_state::{self, SharedAppState};
use crate::ecs::resources::ProjectResource;
use crate::ecs::systems::get_active_objects_system;
use crate::renderer::RenderEngine;
use crate::{
    PreviewWindow, ProjectSettingsWindow, ProjectTabItem, PropertiesWindow, SystemSettingsWindow,
    TimelineWindow,
};
use slint::{ComponentHandle, ModelRc, VecModel, Weak};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

pub type GpuSlot = Rc<RefCell<Option<(slint::wgpu_29::wgpu::Device, slint::wgpu_29::wgpu::Queue)>>>;

/// 再生開始時刻と開始フレームの記録。current_frameはこの2値と経過実時間から
/// 決定論的に算出する（on_request_render呼び出し頻度＝redraw頻度に非依存）。
type PlaybackAnchor = Rc<RefCell<Option<(Instant, i32)>>>;

const PLAYBACK_TICK_MS: u64 = 16;

/// WGPUデバイス/キューは全プロジェクト共通のため一度だけ取得し、以後のRenderEngine生成に使い回す。
pub fn install_rendering_notifier(preview: &PreviewWindow, gpu_slot: GpuSlot) {
    preview
        .window()
        .set_rendering_notifier(move |state, graphics_api| {
            if let (
                slint::RenderingState::RenderingSetup,
                slint::GraphicsAPI::WGPU29 { device, queue, .. },
            ) = (state, graphics_api)
            {
                let mut slot = gpu_slot.borrow_mut();
                if slot.is_none() {
                    *slot = Some((device.clone(), queue.clone()));
                }
            }
        })
        .expect("rendering notifier登録失敗");
}

/// プレビューウィンドウの解像度・FPS表示をProjectResourceへ確実に同期する唯一の窓口。
/// 初期化・プロジェクト切替・毎フレーム描画のいずれもここを経由し、反映漏れ・重複を防ぐ。
fn sync_resolution_fps(preview: &PreviewWindow, proj: &ProjectResource) {
    preview.set_fps(proj.fps as i32);
    preview.set_res_width(proj.width as i32);
    preview.set_res_height(proj.height as i32);
}

/// apply_frame呼び出し元のうちAudioMixer駆動を必要とする経路（playback_timer/on_seek/
/// on_step_frame）が共有する速度指定。100=等速。
const SPEED_NORMAL_PERCENT: i32 = 100;

fn apply_frame(
    frame: i32,
    state: &SharedAppState,
    preview_weak: &Weak<PreviewWindow>,
    timeline_weak: &Weak<TimelineWindow>,
    props_weak: &Weak<PropertiesWindow>,
) {
    apply_frame_with_speed(
        frame,
        SPEED_NORMAL_PERCENT,
        state,
        preview_weak,
        timeline_weak,
        props_weak,
    );
}

/// フレーム反映の唯一の窓口。任意方向シーク検出時（フレーム差分の絶対値>1）は
/// AudioMixer::resetを呼び、通常時はprocess_frameでtick分の音声を合成する。
fn apply_frame_with_speed(
    frame: i32,
    speed_percent: i32,
    state: &SharedAppState,
    preview_weak: &Weak<PreviewWindow>,
    timeline_weak: &Weak<TimelineWindow>,
    props_weak: &Weak<PropertiesWindow>,
) {
    let world_holder = app_state::active_world(state);
    let mixer_holder = app_state::active_audio_mixer(state);
    let mut world = world_holder.lock().unwrap();
    let previous = world.current_frame();
    let clamped = frame.clamp(0, world.total_frames());
    world.set_current_frame(clamped);

    let mut mixer = mixer_holder.lock().unwrap();
    if (clamped - previous).abs() > 1 {
        mixer.reset();
    }
    mixer.process_frame(&world, clamped, f64::from(speed_percent) / 100.0);
    drop(mixer);

    if let Some(p) = preview_weak.upgrade() {
        p.set_current_frame(clamped);
    }
    if let Some(t) = timeline_weak.upgrade() {
        t.set_current_frame(clamped);
    }
    if let Some(props) = props_weak.upgrade() {
        crate::ui::properties::refresh_current_frame(&props, &world);
    }
}

/// anchorと経過実時間から現在再生すべきフレームを算出する。
/// fps・speed_percentが0以下の異常値の場合は開始フレームのまま停滞させる（進行フレーム跳躍防止）。
fn frame_from_anchor(
    anchor_instant: Instant,
    anchor_frame: i32,
    fps: i32,
    speed_percent: i32,
) -> i32 {
    if fps <= 0 || speed_percent <= 0 {
        return anchor_frame;
    }
    let elapsed_secs = anchor_instant.elapsed().as_secs_f64();
    let advanced_frames = elapsed_secs * f64::from(fps) * (f64::from(speed_percent) / 100.0);
    anchor_frame + advanced_frames.floor() as i32
}

pub(crate) fn sync_project_tabs(state: &SharedAppState, preview: &PreviewWindow) {
    let s = state.lock().unwrap();
    let tabs: Vec<ProjectTabItem> = s
        .sessions
        .iter()
        .enumerate()
        .map(|(i, sess)| ProjectTabItem {
            index: i as i32,
            name: sess.meta.name.clone().into(),
            active: i == s.active,
        })
        .collect();
    drop(s);
    preview.set_project_tabs(ModelRc::new(VecModel::from(tabs)));
}

/// アクティブプロジェクト切替時、プレビュー・タイムライン・プロパティ各ウィンドウを再同期する。
pub fn sync_active_session(
    state: &SharedAppState,
    preview_weak: &Weak<PreviewWindow>,
    timeline_weak: &Weak<TimelineWindow>,
    props_weak: &Weak<PropertiesWindow>,
) {
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let proj = world.get_project();
    let total = world.total_frames();
    drop(world);

    app_state::active_audio_mixer(state).lock().unwrap().pause();

    if let Some(p) = preview_weak.upgrade() {
        sync_resolution_fps(&p, &proj);
        p.set_total_frames(total);
        p.set_current_frame(0);
        p.set_is_playing(false);
        sync_project_tabs(state, &p);
    }
    crate::ui::timeline::sync_active_session(state, timeline_weak);
    if let Some(pr) = props_weak.upgrade() {
        pr.set_object_id(-1);
    }
}

pub fn setup(
    preview: &PreviewWindow,
    timeline_weak: Weak<TimelineWindow>,
    props_weak: Weak<PropertiesWindow>,
    settings_weak: Weak<SystemSettingsWindow>,
    project_settings_weak: Weak<ProjectSettingsWindow>,
    state: SharedAppState,
    gpu_slot: GpuSlot,
) {
    let preview_weak = preview.as_weak();
    let playback_anchor: PlaybackAnchor = Rc::new(RefCell::new(None));

    crate::media::cache::global().set_redraw_callback({
        let redraw_weak = preview_weak.clone();
        std::sync::Arc::new(move || {
            let redraw_weak = redraw_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(p) = redraw_weak.upgrade() {
                    p.window().request_redraw();
                }
            });
        })
    });

    sync_project_tabs(&state, preview);
    {
        let world_holder = app_state::active_world(&state);
        let world = world_holder.lock().unwrap();
        let proj = world.get_project();
        sync_resolution_fps(preview, &proj);
        preview.set_total_frames(world.total_frames());
    }

    let playback_timer = slint::Timer::default();
    playback_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(PLAYBACK_TICK_MS),
        {
            let preview_weak = preview_weak.clone();
            let timeline_weak = timeline_weak.clone();
            let props_weak = props_weak.clone();
            let state = state.clone();
            let playback_anchor = playback_anchor.clone();
            move || {
                let Some(p) = preview_weak.upgrade() else {
                    return;
                };
                if !p.get_is_playing() {
                    return;
                }
                let Some((anchor_instant, anchor_frame)) = *playback_anchor.borrow() else {
                    return;
                };
                let fps = p.get_fps();
                let speed_percent = p
                    .get_speed_percent()
                    .max(crate::config::PLAYBACK_SPEED_MIN_PERCENT);
                let total = p.get_total_frames();
                let next = frame_from_anchor(anchor_instant, anchor_frame, fps, speed_percent);

                {
                    let world_holder = app_state::active_world(&state);
                    let world = world_holder.lock().unwrap();
                    app_state::active_audio_mixer(&state)
                        .lock()
                        .unwrap()
                        .process_frame(
                            &world,
                            p.get_current_frame(),
                            f64::from(speed_percent) / 100.0,
                        );
                }

                if next >= total {
                    p.set_is_playing(false);
                    *playback_anchor.borrow_mut() = None;
                    apply_frame_with_speed(
                        total,
                        speed_percent,
                        &state,
                        &preview_weak,
                        &timeline_weak,
                        &props_weak,
                    );
                    app_state::active_audio_mixer(&state)
                        .lock()
                        .unwrap()
                        .pause();
                } else if next != p.get_current_frame() {
                    apply_frame_with_speed(
                        next,
                        speed_percent,
                        &state,
                        &preview_weak,
                        &timeline_weak,
                        &props_weak,
                    );
                }
                p.window().request_redraw();
            }
        },
    );
    std::mem::forget(playback_timer);

    preview.on_request_render({
        let preview_weak = preview_weak.clone();
        let state = state.clone();
        let gpu_slot = gpu_slot.clone();
        move || {
            let world_holder = app_state::active_world(&state);
            let engine_holder = app_state::active_engine(&state);

            let world = world_holder.lock().unwrap();
            let proj = world.get_project();
            let active = get_active_objects_system(&world);
            drop(world);

            let mut engine_lock = engine_holder.lock().unwrap();
            if engine_lock.is_none()
                && let Some((device, queue)) = gpu_slot.borrow().clone()
            {
                *engine_lock = Some(RenderEngine::new(device, queue, proj.width, proj.height));
            }
            if let Some(ref mut engine) = *engine_lock {
                if engine.render_width != proj.width || engine.render_height != proj.height {
                    engine.resize_render_target(proj.width, proj.height);
                }
                engine.render(&active, &proj);
                let img = slint::Image::try_from(engine.texture.clone()).unwrap();
                if let Some(p) = preview_weak.upgrade() {
                    p.set_video_frame(img);
                    sync_resolution_fps(&p, &proj);
                }
            }
        }
    });

    preview.on_toggle_play({
        let preview_weak = preview_weak.clone();
        let playback_anchor = playback_anchor.clone();
        let state = state.clone();
        move || {
            if let Some(p) = preview_weak.upgrade() {
                let playing = !p.get_is_playing();
                p.set_is_playing(playing);
                let mixer = app_state::active_audio_mixer(&state);
                if playing {
                    *playback_anchor.borrow_mut() = Some((Instant::now(), p.get_current_frame()));
                    mixer.lock().unwrap().play();
                } else {
                    *playback_anchor.borrow_mut() = None;
                    mixer.lock().unwrap().pause();
                }
            }
        }
    });

    preview.on_seek({
        let preview_weak = preview_weak.clone();
        let timeline_weak = timeline_weak.clone();
        let props_weak = props_weak.clone();
        let state = state.clone();
        let playback_anchor = playback_anchor.clone();
        move |frame| {
            apply_frame(frame, &state, &preview_weak, &timeline_weak, &props_weak);
            if let Some(p) = preview_weak.upgrade()
                && p.get_is_playing()
            {
                *playback_anchor.borrow_mut() = Some((Instant::now(), p.get_current_frame()));
            }
        }
    });

    preview.on_step_frame({
        let preview_weak = preview_weak.clone();
        let timeline_weak = timeline_weak.clone();
        let props_weak = props_weak.clone();
        let state = state.clone();
        let playback_anchor = playback_anchor.clone();
        move |delta| {
            if let Some(p) = preview_weak.upgrade() {
                let next = p.get_current_frame() + delta;
                apply_frame(next, &state, &preview_weak, &timeline_weak, &props_weak);
                if p.get_is_playing() {
                    *playback_anchor.borrow_mut() = Some((Instant::now(), p.get_current_frame()));
                }
            }
        }
    });

    preview.on_set_speed({
        let preview_weak = preview_weak.clone();
        let playback_anchor = playback_anchor.clone();
        move |percent| {
            if let Some(p) = preview_weak.upgrade() {
                p.set_speed_percent(percent.clamp(
                    crate::config::PLAYBACK_SPEED_MIN_PERCENT,
                    crate::config::PLAYBACK_SPEED_MAX_PERCENT,
                ));
                if p.get_is_playing() {
                    *playback_anchor.borrow_mut() = Some((Instant::now(), p.get_current_frame()));
                }
            }
        }
    });

    preview.on_switch_project_tab({
        let state = state.clone();
        let preview_weak = preview_weak.clone();
        let timeline_weak = timeline_weak.clone();
        let props_weak = props_weak.clone();
        let playback_anchor = playback_anchor.clone();
        move |index| {
            {
                let mut s = state.lock().unwrap();
                if (index as usize) < s.sessions.len() {
                    s.active = index as usize;
                }
            }
            *playback_anchor.borrow_mut() = None;
            sync_active_session(&state, &preview_weak, &timeline_weak, &props_weak);
        }
    });

    preview.on_close_project_tab({
        let state = state.clone();
        let preview_weak = preview_weak.clone();
        let timeline_weak = timeline_weak.clone();
        let props_weak = props_weak.clone();
        let playback_anchor = playback_anchor.clone();
        move |index| {
            let should_quit = {
                let mut s = state.lock().unwrap();
                if s.sessions.len() <= 1 {
                    true
                } else {
                    let idx = index as usize;
                    if idx < s.sessions.len() {
                        s.sessions.remove(idx);
                        if s.active >= s.sessions.len() {
                            s.active = s.sessions.len() - 1;
                        } else if idx < s.active {
                            s.active -= 1;
                        }
                    }
                    false
                }
            };
            if should_quit {
                let _ = slint::quit_event_loop();
            } else {
                *playback_anchor.borrow_mut() = None;
                sync_active_session(&state, &preview_weak, &timeline_weak, &props_weak);
            }
        }
    });

    preview.on_save_project({
        let state = state.clone();
        move || {
            let world_holder = app_state::active_world(&state);
            let world = world_holder.lock().unwrap();
            let _ = crate::project::save_from_world(&world);
        }
    });
    preview.on_save_project_as(|| {});
    preview.on_export_media(|| {});
    preview.on_undo({
        let state = state.clone();
        let preview_weak = preview_weak.clone();
        let timeline_weak = timeline_weak.clone();
        let props_weak = props_weak.clone();
        move || {
            if app_state::undo_active(&state) {
                sync_active_session(&state, &preview_weak, &timeline_weak, &props_weak);
            }
        }
    });
    preview.on_redo({
        let state = state.clone();
        let preview_weak = preview_weak.clone();
        let timeline_weak = timeline_weak.clone();
        let props_weak = props_weak.clone();
        move || {
            if app_state::redo_active(&state) {
                sync_active_session(&state, &preview_weak, &timeline_weak, &props_weak);
            }
        }
    });

    preview.on_show_timeline({
        let timeline_weak = timeline_weak.clone();
        move || {
            if let Some(t) = timeline_weak.upgrade() {
                let _ = t.show();
            }
        }
    });

    preview.on_show_properties({
        let props_weak = props_weak.clone();
        move || {
            if let Some(p) = props_weak.upgrade() {
                let _ = p.show();
            }
        }
    });

    preview.on_show_system_settings({
        let settings_weak = settings_weak.clone();
        move || {
            if let Some(w) = settings_weak.upgrade() {
                let _ = w.show();
                w.window().request_redraw();
            }
        }
    });

    preview.on_show_project_settings({
        let project_settings_weak = project_settings_weak.clone();
        let state = state.clone();
        move || {
            if let Some(w) = project_settings_weak.upgrade() {
                crate::ui::project_settings::open(&w, &state);
            }
        }
    });

    preview.on_quit(|| {
        let _ = slint::quit_event_loop();
    });

    preview.on_raw_key_event({
        let preview_weak = preview.as_weak();
        move |ctrl, shift, alt, key| {
            let Some(preview) = preview_weak.upgrade() else {
                return;
            };
            dispatch_global_command(&preview, ctrl, shift, alt, &key);
        }
    });
}

pub fn dispatch_global_command(
    preview: &PreviewWindow,
    ctrl: bool,
    shift: bool,
    alt: bool,
    key: &str,
) {
    use crate::shortcuts::{CommandId, Scope, resolve};
    let Some(cmd) = resolve(Scope::Global, ctrl, shift, alt, key) else {
        return;
    };
    match cmd {
        CommandId::NewProject => preview.invoke_new_project(),
        CommandId::OpenProject => preview.invoke_open_project(),
        CommandId::SaveProject => preview.invoke_save_project(),
        CommandId::SaveProjectAs => preview.invoke_save_project_as(),
        CommandId::ExportMedia => preview.invoke_export_media(),
        CommandId::Quit => preview.invoke_quit(),
        CommandId::Undo => preview.invoke_undo(),
        CommandId::Redo => preview.invoke_redo(),
        CommandId::TogglePlay => preview.invoke_toggle_play(),
        CommandId::StepFrameFwd => preview.invoke_step_frame(1),
        CommandId::StepFrameBack => preview.invoke_step_frame(-1),
        CommandId::ShowTimeline => preview.invoke_show_timeline(),
        CommandId::ShowProperties => preview.invoke_show_properties(),
        CommandId::ShowSystemSettings => preview.invoke_show_system_settings(),
        CommandId::ShowProjectSettings => preview.invoke_show_project_settings(),
        CommandId::NextProjectTab | CommandId::PrevProjectTab | CommandId::CloseProjectTab => {}
        _ => {}
    }
}
