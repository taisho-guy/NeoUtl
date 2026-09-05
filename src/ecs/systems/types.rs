use crate::ecs::components::{ClipMode, MediaSource, ShapeParams, TextContent};
use crate::ecs::types::Value;
use shipyard::EntityId;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameBufferKind {
    Group,
}

#[derive(Clone, Copy, Debug)]
pub enum ComposeSource {
    NestedScene {
        target_scene: i32,
        local_frame: i32,
    },
    FrameBuffer {
        controller: EntityId,
        kind: FrameBufferKind,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct ClipTargetInfo {
    pub controller: EntityId,
    pub mode: ClipMode,
    pub chroma_hue: f32,
    pub chroma_tolerance: f32,
    pub blend_edge: bool,
}

#[derive(Clone)]
pub struct ActiveObject {
    pub kind_id: u32,
    pub source_frame: i64,
    pub clip_instance: u64,
    pub text_content: Option<TextContent>,
    pub shape_params: Option<ShapeParams>,
    pub media_source: Option<MediaSource>,
    pub mvp: [f32; 16],
    pub opacity: f32,
    pub effects: Vec<(String, HashMap<String, Value>)>,
    pub compose_source: Option<ComposeSource>,
    pub layer: i32,
    pub clip_target: Option<ClipTargetInfo>,
    pub zbuffer_depth: Option<f32>,
}

pub type CapturedObjects = HashMap<EntityId, Vec<ActiveObject>>;
