use crate::ecs::audio_plugins::PluginInstanceRef;
use crate::ecs::components::{
    AudioParams, ClipTarget, GroupControl, MediaSource, ShapeParams, TextContent,
};
use crate::ecs::resources::SceneMeta;
use crate::ecs::transform::Transform;
use crate::ecs::types::{EffectInstance, Keyframe};
use crate::media::MediaKind;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ObjectPayload {
    pub text: Option<TextContent>,
    pub shape: Option<ShapeParams>,
    pub plugin_params: Option<HashMap<String, f32>>,
    pub media: Option<MediaSourceDoc>,
    pub plugin_chain: Option<Vec<PluginInstanceRef>>,
    #[serde(default)]
    pub scene: Option<i32>,
    #[serde(default)]
    pub group_control: Option<GroupControl>,
    #[serde(default)]
    pub clip_target: Option<ClipTarget>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    #[serde(default)]
    pub keyframes: HashMap<String, Vec<Keyframe>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentModel {
    pub project_name: String,
    pub audio_sample_rate: u32,
    pub audio_channels: u32,
    pub active_scene: i32,
    pub next_object_id: usize,
    pub scenes: Vec<SceneMeta>,
    pub objects: Vec<ObjectDoc>,
}
