mod audio;
mod clip_target;
mod expression;
mod group_control;
mod hierarchy;
mod param_access;
mod shape_misc;
mod text;
mod time_remap;

pub use audio::AudioParams;
pub use clip_target::{ClipMode, ClipTarget};
#[allow(unused_imports)]
pub use expression::{ExpressionBinding, PropertyExpressions};
pub use group_control::{GroupControl, SceneObject};
pub use hierarchy::ParentRef;
pub use param_access::{KindId, Layer, ObjectId, ParamAccess, SceneId, TimeRange};
pub use shape_misc::{KeyframeTracks, MediaSource, PluginParams, ShapeParams};
pub use text::{TextAlign, TextContent};
pub use time_remap::TimeRemap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, shipyard::Component)]
pub enum BlendMode {
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    Exclusion,
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
}

impl BlendMode {
    pub fn pipeline_index(self) -> u32 {
        match self {
            BlendMode::Normal => 0,
            BlendMode::Add => 1,
            BlendMode::Multiply => 2,
            BlendMode::Screen => 3,
            BlendMode::Overlay => 4,
            BlendMode::Darken => 5,
            BlendMode::Lighten => 6,
            BlendMode::Difference => 7,
            BlendMode::Exclusion => 8,
        }
    }

    pub fn from_pipeline_index(index: u32) -> Self {
        match index {
            1 => BlendMode::Add,
            2 => BlendMode::Multiply,
            3 => BlendMode::Screen,
            4 => BlendMode::Overlay,
            5 => BlendMode::Darken,
            6 => BlendMode::Lighten,
            7 => BlendMode::Difference,
            8 => BlendMode::Exclusion,
            _ => BlendMode::Normal,
        }
    }
}
