use super::EcsWorld;
use super::active_query::is_active_at;
use crate::ecs::components::{
    AudioParams, KeyframeTracks, MediaSource, ObjectId, SceneId, TimeRange,
};
use crate::ecs::resources::{ProjectResource, SceneResource};
use neoutl_media_runtime::MediaKind;
use shipyard::{Get, IntoIter, UniqueView, View};

type AudioSelectorViews<'v> = (
    View<'v, TimeRange>,
    View<'v, SceneId>,
    View<'v, MediaSource>,
    View<'v, ObjectId>,
);
type AudioPayloadViews<'v> = (
    View<'v, AudioParams>,
    View<'v, KeyframeTracks>,
    View<'v, crate::ecs::audio_plugins::PluginChain>,
);

pub fn get_active_audio_system(
    world: &EcsWorld,
    frame: i32,
) -> Vec<crate::audio::mixer::ActiveAudioEntity> {
    world.world.run(
        |(scenes, project): (UniqueView<SceneResource>, UniqueView<ProjectResource>),
         (time_ranges, scene_ids, media_sources, object_ids): AudioSelectorViews,
         (audio_params, keyframe_tracks, plugin_chains): AudioPayloadViews| {
            let active_scene = scenes.active_scene;
            let fps = f64::from(project.fps.max(1));
            let mut active = Vec::new();

            for (id, (range, scene, media_source)) in
                (&time_ranges, &scene_ids, &media_sources).iter().with_id()
            {
                if !matches!(media_source.kind, MediaKind::Audio) {
                    continue;
                }
                if !is_active_at(range, scene, active_scene, frame) {
                    continue;
                }
                let keyframes = keyframe_tracks.get(id).ok();
                let mut audio = audio_params.get(id).copied().unwrap_or_default();
                if let Some(kt) = keyframes {
                    kt.apply(&mut audio, frame);
                }
                let source_frame =
                    media_source.trim_in_frame + i64::from(frame - range.start_frame);

                active.push(crate::audio::mixer::ActiveAudioEntity {
                    id: object_ids.get(id).map_or(0, |o| o.0 as usize),
                    audio,
                    media_source: Some(media_source.clone()),
                    source_frame,
                    fps,
                    plugin_chain: plugin_chains
                        .get(id)
                        .map(|c| c.0.clone())
                        .unwrap_or_default(),
                });
            }
            active
        },
    )
}
