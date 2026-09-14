use crate::app_state::{self, SharedAppState};
use crate::ecs::resources::ProjectResource;
use std::sync::Arc;
use std::time::Instant;

pub struct LegacyWindows {}

type PlaybackAnchor = Option<(Instant, i32)>;

const SPEED_NORMAL_PERCENT: i32 = 100;

pub struct PreviewPanel {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub is_playing: bool,
    pub playback_anchor: PlaybackAnchor,
    pub speed_percent: i32,
    pub current_frame: i32,
    pub total_frames: i32,
    pub fps: i32,
    pub session_generation: u64,
    pub open_system_settings: bool,
    pub open_project_settings: bool,
    pub open_keybindings: bool,
    pub open_timeline: bool,
    pub open_export: bool,
    pub open_properties: bool,
}

impl PreviewPanel {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>, _legacy: LegacyWindows) -> Self {
        Self {
            device,
            queue,
            is_playing: false,
            playback_anchor: None,
            speed_percent: SPEED_NORMAL_PERCENT,
            current_frame: 0,
            total_frames: 0,
            fps: 30,
            session_generation: 0,
            open_system_settings: false,
            open_project_settings: false,
            open_keybindings: false,
            open_timeline: true,
            open_export: false,
            open_properties: true,
        }
    }

    pub fn session_generation(&self) -> u64 {
        self.session_generation
    }

    pub fn sync_active_session(&mut self, state: &SharedAppState) {
        self.refresh_total_frames(state);
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let proj = world.get_project();
        self.sync_resolution_fps(&proj);
        self.current_frame = world.current_frame();
    }

    pub fn refresh_total_frames(&mut self, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        self.total_frames = world_holder.lock().unwrap().total_frames();
    }

    pub fn seek(&mut self, frame: i32, state: &SharedAppState) {
        self.apply_frame(frame, state);
        if self.is_playing {
            self.playback_anchor = Some((Instant::now(), self.current_frame));
        }
    }

    fn sync_resolution_fps(&mut self, proj: &ProjectResource) {
        self.fps = proj.fps as i32;
    }

    pub fn apply_frame_with_speed(
        &mut self,
        frame: i32,
        speed_percent: i32,
        state: &SharedAppState,
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
        drop(world);

        self.current_frame = clamped;
    }

    pub fn apply_frame(&mut self, frame: i32, state: &SharedAppState) {
        self.apply_frame_with_speed(frame, SPEED_NORMAL_PERCENT, state);
    }

    pub fn toggle_play(&mut self, state: &SharedAppState) {
        self.is_playing = !self.is_playing;
        let mixer = app_state::active_audio_mixer(state);
        if self.is_playing {
            self.playback_anchor = Some((Instant::now(), self.current_frame));
            mixer.lock().unwrap().play();
        } else {
            self.playback_anchor = None;
            mixer.lock().unwrap().pause();
        }
    }

    pub fn step_next_frame(&mut self, state: &SharedAppState) {
        self.seek(self.current_frame + 1, state);
    }

    pub fn step_prev_frame(&mut self, state: &SharedAppState) {
        self.seek(self.current_frame - 1, state);
    }
}
