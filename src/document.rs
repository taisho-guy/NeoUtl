use crate::ecs::audio_plugins::PluginInstanceRef;
use crate::ecs::components::{
    AudioParams, ClipTarget, GroupControl, MediaSource, ShapeParams, TextContent,
};
use crate::ecs::resources::SceneMeta;
use crate::ecs::transform::Transform;
use crate::ecs::types::{EffectInstance, Keyframe};
use crate::media::MediaKind;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct MediaSourceDoc {
    pub path: PathBuf,
    pub kind: MediaKind,
    pub trim_in_frame: i64,
}

impl From<&MediaSource> for MediaSourceDoc {
    fn from(m: &MediaSource) -> Self {
        Self {
            path: m.path.clone(),
            kind: m.kind,
            trim_in_frame: m.trim_in_frame,
        }
    }
}

impl From<&MediaSourceDoc> for MediaSource {
    fn from(m: &MediaSourceDoc) -> Self {
        Self {
            path: m.path.clone(),
            kind: m.kind,
            trim_in_frame: m.trim_in_frame,
        }
    }
}

impl From<&MediaSourceDoc> for neoutl_schema::MediaSourceDoc {
    fn from(m: &MediaSourceDoc) -> Self {
        Self {
            path: m.path.to_string_lossy().to_string(),
            kind: match m.kind {
                MediaKind::Video => neoutl_schema::MediaKind::Video as i32,
                MediaKind::Image => neoutl_schema::MediaKind::Image as i32,
                MediaKind::Audio => neoutl_schema::MediaKind::Audio as i32,
            },
            trim_in_frame: m.trim_in_frame,
        }
    }
}

impl TryFrom<&neoutl_schema::MediaSourceDoc> for MediaSourceDoc {
    type Error = String;

    fn try_from(m: &neoutl_schema::MediaSourceDoc) -> Result<Self, Self::Error> {
        Ok(Self {
            path: PathBuf::from(&m.path),
            kind: match m.kind() {
                neoutl_schema::MediaKind::Video => MediaKind::Video,
                neoutl_schema::MediaKind::Image => MediaKind::Image,
                neoutl_schema::MediaKind::Audio => MediaKind::Audio,
                _ => MediaKind::Video,
            },
            trim_in_frame: m.trim_in_frame,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ObjectPayload {
    pub text: Option<TextContent>,
    pub shape: Option<ShapeParams>,
    pub plugin_params: Option<HashMap<String, f32>>,
    pub media: Option<MediaSourceDoc>,
    pub plugin_chain: Option<Vec<PluginInstanceRef>>,
    pub scene: Option<i32>,
    pub group_control: Option<GroupControl>,
    pub clip_target: Option<ClipTarget>,
}

impl From<&ObjectPayload> for neoutl_schema::ObjectPayload {
    fn from(p: &ObjectPayload) -> Self {
        Self {
            text: p.text.as_ref().map(|v| neoutl_schema::TextContent::from(v)),
            shape: p
                .shape
                .as_ref()
                .map(|v| neoutl_schema::ShapeParams::from(v)),
            plugin_params: p.plugin_params.clone().unwrap_or_default(),
            media: p
                .media
                .as_ref()
                .map(|v| neoutl_schema::MediaSourceDoc::from(v)),
            plugin_chain: p
                .plugin_chain
                .clone()
                .unwrap_or_default()
                .iter()
                .map(neoutl_schema::PluginInstanceRef::from)
                .collect(),
            scene: p.scene,
            group_control: p
                .group_control
                .as_ref()
                .map(neoutl_schema::GroupControl::from),
            clip_target: p.clip_target.as_ref().map(neoutl_schema::ClipTarget::from),
        }
    }
}

impl TryFrom<&neoutl_schema::ObjectPayload> for ObjectPayload {
    type Error = String;

    fn try_from(p: &neoutl_schema::ObjectPayload) -> Result<Self, Self::Error> {
        Ok(Self {
            text: p.text.as_ref().map(TextContent::try_from).transpose()?,
            shape: p.shape.as_ref().map(ShapeParams::try_from).transpose()?,
            plugin_params: (!p.plugin_params.is_empty()).then(|| {
                p.plugin_params
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect()
            }),
            media: p.media.as_ref().map(MediaSourceDoc::try_from).transpose()?,
            plugin_chain: (!p.plugin_chain.is_empty()).then(|| {
                p.plugin_chain
                    .iter()
                    .map(PluginInstanceRef::try_from)
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap_or_default()
            }),
            scene: p.scene,
            group_control: p
                .group_control
                .as_ref()
                .map(GroupControl::try_from)
                .transpose()?,
            clip_target: p
                .clip_target
                .as_ref()
                .map(ClipTarget::try_from)
                .transpose()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ObjectDoc {
    pub id: usize,
    pub scene_id: i32,
    pub kind_stable_id: String,
    pub layer: i32,
    pub start_frame: i32,
    pub end_frame: i32,
    pub transform: Transform,
    pub audio: AudioParams,
    pub effects: Vec<EffectInstance>,
    pub payload: ObjectPayload,
    pub keyframes: HashMap<String, Vec<Keyframe>>,
}

impl From<&ObjectDoc> for neoutl_schema::ObjectDoc {
    fn from(value: &ObjectDoc) -> Self {
        let mut keyframes = std::collections::HashMap::new();
        for (track, frames) in &value.keyframes {
            keyframes.insert(
                track.clone(),
                neoutl_schema::KeyframeTrack {
                    keyframes: frames.iter().map(neoutl_schema::Keyframe::from).collect(),
                },
            );
        }
        Self {
            id: value.id as u64,
            scene_id: value.scene_id,
            kind_stable_id: value.kind_stable_id.clone(),
            layer: value.layer,
            start_frame: value.start_frame,
            end_frame: value.end_frame,
            transform: Some(neoutl_schema::Transform::from(&value.transform)),
            audio: Some(neoutl_schema::AudioParams::from(&value.audio)),
            effects: value
                .effects
                .iter()
                .map(neoutl_schema::EffectInstance::from)
                .collect(),
            payload: Some(neoutl_schema::ObjectPayload::from(&value.payload)),
            keyframes,
        }
    }
}

impl TryFrom<&neoutl_schema::ObjectDoc> for ObjectDoc {
    type Error = String;

    fn try_from(value: &neoutl_schema::ObjectDoc) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id as usize,
            scene_id: value.scene_id,
            kind_stable_id: value.kind_stable_id.clone(),
            layer: value.layer,
            start_frame: value.start_frame,
            end_frame: value.end_frame,
            transform: Transform::try_from(
                value
                    .transform
                    .as_ref()
                    .unwrap_or(&neoutl_schema::Transform::default()),
            )?,
            audio: AudioParams::try_from(
                value
                    .audio
                    .as_ref()
                    .unwrap_or(&neoutl_schema::AudioParams::default()),
            )?,
            effects: value
                .effects
                .iter()
                .map(EffectInstance::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            payload: ObjectPayload::try_from(
                value
                    .payload
                    .as_ref()
                    .unwrap_or(&neoutl_schema::ObjectPayload::default()),
            )?,
            keyframes: value
                .keyframes
                .iter()
                .map(|(k, v)| {
                    Ok((
                        k.clone(),
                        v.keyframes
                            .iter()
                            .map(Keyframe::try_from)
                            .collect::<Result<Vec<_>, _>>()?,
                    ))
                })
                .collect::<Result<HashMap<_, _>, String>>()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct DocumentModel {
    pub project_name: String,
    pub audio_sample_rate: u32,
    pub audio_channels: u32,
    pub active_scene: i32,
    pub next_object_id: usize,
    pub scenes: Vec<SceneMeta>,
    pub objects: Vec<ObjectDoc>,
}

impl From<&DocumentModel> for neoutl_schema::DocumentModel {
    fn from(value: &DocumentModel) -> Self {
        Self {
            schema_version: 1,
            project_name: value.project_name.clone(),
            audio_sample_rate: value.audio_sample_rate,
            audio_channels: value.audio_channels,
            active_scene: value.active_scene,
            next_object_id: value.next_object_id as u64,
            scenes: value
                .scenes
                .iter()
                .map(neoutl_schema::SceneMeta::from)
                .collect(),
            objects: value
                .objects
                .iter()
                .map(neoutl_schema::ObjectDoc::from)
                .collect(),
        }
    }
}

impl TryFrom<&neoutl_schema::DocumentModel> for DocumentModel {
    type Error = String;

    fn try_from(value: &neoutl_schema::DocumentModel) -> Result<Self, Self::Error> {
        Ok(Self {
            project_name: value.project_name.clone(),
            audio_sample_rate: value.audio_sample_rate,
            audio_channels: value.audio_channels,
            active_scene: value.active_scene,
            next_object_id: value.next_object_id as usize,
            scenes: value
                .scenes
                .iter()
                .map(SceneMeta::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            objects: value
                .objects
                .iter()
                .map(ObjectDoc::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl TryFrom<neoutl_schema::DocumentModel> for DocumentModel {
    type Error = String;

    fn try_from(value: neoutl_schema::DocumentModel) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}
