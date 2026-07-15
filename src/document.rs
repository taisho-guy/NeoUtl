// src/document.rs
use crate::ecs::components::{AudioParams, MediaSource, ShapeParams, TextContent};
use crate::ecs::resources::SceneMeta;
use crate::ecs::transform::Transform;
use crate::ecs::types::EffectInstance;
use crate::media::MediaKind;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// MediaSourceの永続化形。フィールド構成はMediaSourceと1:1対応する。
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

/// kind_id固有の追加パラメータ。ECS側の任意コンポーネント付与に対応する。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ObjectPayload {
    pub text: Option<TextContent>,
    pub shape: Option<ShapeParams>,
    pub plugin_params: Option<HashMap<String, f32>>,
    pub media: Option<MediaSourceDoc>,
}

/// 1オブジェクトの正本データ（AviQtl::Core::Clip相当）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectDoc {
    pub id: usize,
    pub scene_id: i32,
    pub kind_id: u32,
    pub layer: i32,
    pub start_frame: i32,
    pub end_frame: i32,
    pub transform: Transform,
    pub audio: AudioParams,
    pub effects: Vec<EffectInstance>,
    pub payload: ObjectPayload,
}

/// プロジェクト全体の正本データ（AviQtl::Core::DocumentModel相当）。
/// ECS(EcsWorld)はこの構造から焼き込まれる描画専用ランタイム状態を持つのみとし、
/// Undo/Redo・ファイル保存はこの構造のみを対象とする。
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
