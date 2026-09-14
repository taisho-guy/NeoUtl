use crate::objects::registry;
use crate::ui::types::TimelineObject;

pub fn to_timeline_object(data: &crate::ecs::TimelineData, fps: f64) -> TimelineObject {
    let registry_snapshot = registry();
    let plugin = registry_snapshot.get(data.kind as usize);
    let is_audio = plugin.is_some_and(|p| p.name == "Audio");
    let waveform_duration_frames = data
        .media_path
        .as_deref()
        .filter(|_| is_audio)
        .and_then(|path| {
            neoutl_media_runtime::cache::global()
                .load_audio(path)
                .ok()
                .map(|audio| {
                    (audio.frame_count() as f64 / audio.sample_rate as f64 * fps).ceil() as i32
                })
        })
        .unwrap_or(0);
    TimelineObject {
        id: data.id,
        start_frame: data.start_frame,
        end_frame: data.end_frame,
        kind: data.kind,
        kind_known: plugin.is_some(),
        layer: data.layer,
        label: plugin.map_or_else(
            || crate::localization::tr("Unknown"),
            |p| crate::localization::object_name(&p.name),
        ),
        selected: false,
        keyframe_frames: Vec::new(),
        waveform: None,
        has_waveform: waveform_duration_frames > 0,
        waveform_origin_frame: -data.media_trim_in_frame as i32,
        waveform_duration_frames,
        group_layer_count_down: data.group_layer_count_down,
        group_layer_count_up: data.group_layer_count_up,
        clip_layer_count_down: data.clip_layer_count_down,
        clip_layer_count_up: data.clip_layer_count_up,
    }
}
