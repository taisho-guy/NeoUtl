use crate::ecs::audio_plugins::PluginChain;
use crate::ecs::components::{
    AudioParams, ClipTarget, GroupControl, KeyframeTracks, KindId, Layer, MediaSource, ObjectId,
    PluginParams, SceneId, SceneObject, ShapeParams, TextContent, TimeRange,
};
use crate::ecs::effects::EffectStack;
use crate::ecs::transform::Transform;
use shipyard::{Borrow, BorrowInfo, View};

#[derive(Borrow, BorrowInfo)]
pub(crate) struct ObjectQueryViews<'v> {
    pub(crate) object_ids: View<'v, ObjectId>,
    pub(crate) time_ranges: View<'v, TimeRange>,
    pub(crate) kind_ids: View<'v, KindId>,
    pub(crate) layers: View<'v, Layer>,
    pub(crate) scene_ids: View<'v, SceneId>,
    pub(crate) transforms: View<'v, Transform>,
    pub(crate) audio: View<'v, AudioParams>,
    pub(crate) stacks: View<'v, EffectStack>,
    pub(crate) texts: View<'v, TextContent>,
    pub(crate) shapes: View<'v, ShapeParams>,
    pub(crate) plugins: View<'v, PluginParams>,
    pub(crate) media: View<'v, MediaSource>,
    pub(crate) keyframes: View<'v, KeyframeTracks>,
    pub(crate) plugin_chains: View<'v, PluginChain>,
    pub(crate) scene_objects: View<'v, SceneObject>,
    pub(crate) group_controls: View<'v, GroupControl>,
    pub(crate) clip_targets: View<'v, ClipTarget>,
}
