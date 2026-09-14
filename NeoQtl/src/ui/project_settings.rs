use crate::app_state::{self, SharedAppState};
use crate::project;

#[derive(Clone, Debug)]
pub struct ProjectSettingsWindow {
    pub open: bool,
    pub project_name: String,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub audio_sample_rate: u32,
    pub audio_channels: u32,
}

impl ProjectSettingsWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            project_name: "Project".into(),
            fps: 30,
            width: 1920,
            height: 1080,
            audio_sample_rate: 48000,
            audio_channels: 2,
        }
    }

    pub fn open(&mut self, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let project = world.get_project();
        drop(world);

        self.project_name = project.name;
        self.fps = project.fps;
        self.width = project.width;
        self.height = project.height;
        self.audio_sample_rate = project.audio_sample_rate;
        self.audio_channels = project.audio_channels;
        self.open = true;
    }

    pub fn confirm(&mut self, state: &SharedAppState) {
        let name = self.project_name.clone();
        let sample_rate = self.audio_sample_rate.max(1);
        let channels = self.audio_channels.clamp(1, 8);

        let world_holder = app_state::active_world(state);
        app_state::snapshot_before_edit(state);
        let mut world = world_holder.lock().unwrap();
        let dir = world
            .get_project()
            .dir
            .unwrap_or_else(project::projects_dir);
        world.set_project_meta(name.clone(), dir);
        world.set_fps(self.fps);
        world.set_resolution(self.width, self.height);
        world.set_audio_format(sample_rate, channels);
        let _ = project::save_from_world(&world);
        drop(world);
        app_state::active_audio_mixer(state)
            .lock()
            .unwrap()
            .set_sample_rate(sample_rate);

        {
            let mut s = state.lock().unwrap();
            let active = s.active;
            if active < s.sessions.len() {
                s.sessions[active].meta.name = name;
            }
        }

        self.open = false;
    }

    pub fn cancel(&mut self) {
        self.open = false;
    }
}
