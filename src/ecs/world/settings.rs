use crate::ecs::EcsWorld;
use crate::ecs::resources::{ProjectResource, SystemSettingsResource};
use crate::ecs::transform::Camera;
use shipyard::{UniqueView, UniqueViewMut};

impl EcsWorld {
    pub fn set_fps(&mut self, fps: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.fps = fps;
            });
        self.touch();
    }

    pub fn set_resolution(&mut self, width: u32, height: u32) {
        let fps = self.get_project().fps;
        self.apply_scene_resolution(width, height, fps);
    }

    pub fn get_project(&self) -> ProjectResource {
        self.world
            .run(|project: UniqueView<ProjectResource>| project.clone())
    }

    pub fn set_project_meta(&mut self, name: String, dir: std::path::PathBuf) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.name = name;
                project.dir = Some(dir);
            });
        self.touch();
    }

    pub fn set_audio_format(&mut self, sample_rate: u32, channels: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.audio_sample_rate = sample_rate;
                project.audio_channels = channels;
            });
        self.touch();
    }

    pub(crate) fn apply_scene_resolution(&mut self, width: u32, height: u32, fps: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.width = width;
                project.height = height;
                project.fps = fps;
            });
        self.set_camera(Camera::for_resolution(width as f32, height as f32));
        self.touch();
    }

    pub fn get_system_settings(&self) -> SystemSettingsResource {
        self.world
            .run(|s: UniqueView<SystemSettingsResource>| s.clone())
    }

    pub fn set_system_settings(&mut self, s: SystemSettingsResource) {
        self.world
            .run(|mut slot: UniqueViewMut<SystemSettingsResource>| *slot = s);
    }
}
