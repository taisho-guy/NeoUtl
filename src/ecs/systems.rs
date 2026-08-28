use super::EcsWorld;
use crate::ecs::components::{
    AudioParams, ClipMode, ClipTarget, GroupControl, KeyframeTracks, KindId, Layer, MediaSource,
    ObjectId, SceneId, SceneObject, ShapeParams, TextContent, TimeRange,
};
use crate::ecs::effects::{EffectStack, compute_effect_params_at};
use crate::ecs::resources::{
    ProjectResource, SceneResource, SystemSettingsResource, TimelineResource,
};
use crate::ecs::transform::{
    Camera, DEFAULT_FOV_DEG, GlobalMatrix, Projection, Transform, compute_chained_matrix,
    compute_global_matrix, compute_mvp, compute_relative_matrix, rescale_for_source,
    scale_to_pixels,
};
use crate::ecs::types::Value;
use crate::media::MediaKind;
use neoutl_object_api::UNIT_SIZE_PX;
use shipyard::{EntityId, Get, IntoIter, UniqueView, View};
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
}

pub type CapturedObjects = HashMap<EntityId, Vec<ActiveObject>>;

fn projection_for(_kind_id: u32) -> Projection {
    Projection::Perspective {
        fov_deg: DEFAULT_FOV_DEG,
    }
}

#[derive(Clone, Copy)]
enum ControllerKind {
    Group {
        generate_framebuffer: bool,
        hide_captured: bool,
    },
    Clip {
        mode: ClipMode,
        chroma_hue: f32,
        chroma_tolerance: f32,
        blend_edge: bool,
    },
}

struct CurtainInfo {
    entity: EntityId,
    layer: i32,
    span: (u32, u32),
    matrix: GlobalMatrix,
    effects: Vec<(String, HashMap<String, Value>)>,
    opacity: f32,
    kind: ControllerKind,
    render_self: bool,
}

impl CurtainInfo {
    fn requires_fb(&self) -> bool {
        match self.kind {
            ControllerKind::Group {
                generate_framebuffer,
                ..
            } => generate_framebuffer,
            ControllerKind::Clip { .. } => true,
        }
    }

    fn hide_captured(&self) -> bool {
        match self.kind {
            ControllerKind::Group { hide_captured, .. } => hide_captured,
            ControllerKind::Clip { .. } => false,
        }
    }
}

fn curtain_covers_layer(curtain_layer: i32, span: (u32, u32), target_layer: i32) -> bool {
    let (down, up) = span;
    if target_layer > curtain_layer {
        target_layer <= curtain_layer + down as i32
    } else if target_layer < curtain_layer {
        target_layer >= curtain_layer - up as i32
    } else {
        false
    }
}

fn group_only(chain: &[usize], controllers: &[CurtainInfo]) -> Vec<usize> {
    chain
        .iter()
        .copied()
        .filter(|&i| matches!(controllers[i].kind, ControllerKind::Group { .. }))
        .collect()
}

fn resolve_group_chain(obj_layer: i32, controllers: &[CurtainInfo], max_depth: i32) -> Vec<usize> {
    let mut chain = Vec::new();
    let mut cursor_layer = obj_layer;
    loop {
        if chain.len() as i32 >= max_depth.max(0) {
            break;
        }
        let mut nearest: Option<(usize, i32)> = None;
        for (idx, c) in controllers.iter().enumerate() {
            if chain.contains(&idx) {
                continue;
            }
            if !curtain_covers_layer(c.layer, c.span, cursor_layer) {
                continue;
            }
            let dist = (cursor_layer - c.layer).abs();
            if nearest.is_none_or(|(_, d)| dist < d) {
                nearest = Some((idx, dist));
            }
        }
        let Some((idx, _)) = nearest else {
            break;
        };
        chain.push(idx);
        cursor_layer = controllers[idx].layer;
    }
    chain
}

type UniqueGroupViews<'v> = (
    UniqueView<'v, TimelineResource>,
    UniqueView<'v, SceneResource>,
    UniqueView<'v, ProjectResource>,
    UniqueView<'v, Camera>,
    UniqueView<'v, SystemSettingsResource>,
);
type SelectorGroupViews<'v> = (
    View<'v, TimeRange>,
    View<'v, KindId>,
    View<'v, SceneId>,
    View<'v, Layer>,
    View<'v, TextContent>,
    View<'v, ShapeParams>,
    View<'v, MediaSource>,
    View<'v, ObjectId>,
    View<'v, SceneObject>,
    View<'v, ClipTarget>,
);
type PayloadGroupViews<'v> = (
    View<'v, Transform>,
    View<'v, KeyframeTracks>,
    View<'v, AudioParams>,
    View<'v, EffectStack>,
    View<'v, GroupControl>,
);

fn is_active_at(range: &TimeRange, scene: &SceneId, active_scene: i32, frame: i32) -> bool {
    scene.0 == active_scene && frame >= range.start_frame && frame < range.end_frame
}

pub fn get_active_objects_system(world: &EcsWorld) -> (Vec<ActiveObject>, CapturedObjects) {
    let active_scene = world.active_scene();
    let current = world.current_frame();
    get_active_objects_system_at(world, active_scene, current)
}

pub fn get_active_objects_system_at(
    world: &EcsWorld,
    active_scene: i32,
    current: i32,
) -> (Vec<ActiveObject>, CapturedObjects) {
    world.world.run(
        |(_timeline, scenes, project, camera, system_settings): UniqueGroupViews,
         (
            time_ranges,
            kind_ids,
            scene_ids,
            layers,
            text_contents,
            shape_params,
            media_sources,
            object_ids,
            scene_objects,
            clip_targets,
        ): SelectorGroupViews,
         (
            transforms,
            keyframe_tracks,
            _audio_params,
            effect_stacks,
            group_controls,
        ): PayloadGroupViews| {
            let project_width = project.width.max(1) as f32;
            let project_height = project.height.max(1) as f32;
            let max_depth = system_settings.max_group_chain_depth;

            let mut controllers: Vec<CurtainInfo> = Vec::new();
            for (id, (range, scene, layer, gc)) in
                (&time_ranges, &scene_ids, &layers, &group_controls)
                    .iter()
                    .with_id()
            {
                if scene.0 != active_scene
                    || current < range.start_frame
                    || current >= range.end_frame
                {
                    continue;
                }
                let mut transform = transforms.get(id).copied().unwrap_or_default();
                if let Ok(kt) = keyframe_tracks.get(id) {
                    kt.apply(&mut transform, current);
                }
                let effects = effect_stacks
                    .get(id)
                    .map(|stack| compute_effect_params_at(stack, current, world))
                    .unwrap_or_default();
                controllers.push(CurtainInfo {
                    entity: id,
                    layer: layer.0,
                    span: (gc.layer_count_down, gc.layer_count_up),
                    matrix: compute_relative_matrix(&transform),
                    effects,
                    opacity: transform.opacity,
                    kind: ControllerKind::Group {
                        generate_framebuffer: gc.generate_framebuffer,
                        hide_captured: gc.hide_captured,
                    },
                    render_self: true,
                });
            }
            for (id, (range, scene, layer, ct)) in
                (&time_ranges, &scene_ids, &layers, &clip_targets)
                    .iter()
                    .with_id()
            {
                if !ct.enabled
                    || scene.0 != active_scene
                    || current < range.start_frame
                    || current >= range.end_frame
                {
                    continue;
                }
                let mut transform = transforms.get(id).copied().unwrap_or_default();
                if let Ok(kt) = keyframe_tracks.get(id) {
                    kt.apply(&mut transform, current);
                }
                let effects = effect_stacks
                    .get(id)
                    .map(|stack| compute_effect_params_at(stack, current, world))
                    .unwrap_or_default();
                controllers.push(CurtainInfo {
                    entity: id,
                    layer: layer.0,
                    span: (ct.layer_count_down, ct.layer_count_up),
                    matrix: compute_relative_matrix(&transform),
                    effects,
                    opacity: transform.opacity,
                    kind: ControllerKind::Clip {
                        mode: ct.mode,
                        chroma_hue: ct.chroma_hue,
                        chroma_tolerance: ct.chroma_tolerance,
                        blend_edge: ct.blend_edge,
                    },
                    render_self: ct.render_self,
                });
            }

            let mut active = Vec::new();
            let mut captured: CapturedObjects = HashMap::new();

            for (id, (range, kind, scene)) in (&time_ranges, &kind_ids, &scene_ids).iter().with_id()
            {
                if !is_active_at(range, scene, active_scene, current) {
                    continue;
                }
                if group_controls.get(id).is_ok() {
                    continue;
                }
                if clip_targets.get(id).is_ok_and(|t| t.enabled) {
                    continue;
                }
                let keyframes = keyframe_tracks.get(id).ok();

                let mut transform = transforms.get(id).copied().unwrap_or_default();
                if let Some(kt) = keyframes {
                    kt.apply(&mut transform, current);
                }

                let mut text_content = text_contents.get(id).ok().cloned();
                if let (Some(tc), Some(kt)) = (text_content.as_mut(), keyframes) {
                    kt.apply(tc, current);
                }

                let mut shape = shape_params.get(id).ok().copied();
                if let (Some(sp), Some(kt)) = (shape.as_mut(), keyframes) {
                    kt.apply(sp, current);
                }

                let media_source = media_sources.get(id).ok().cloned();
                let source_frame = media_source.as_ref().map_or(0, |m| {
                    let base = f64::from(current - range.start_frame);
                    let ratio = if matches!(m.kind, MediaKind::Video) {
                        let src_fps = crate::media::cache::global()
                            .source_fps(&m.path)
                            .unwrap_or(f64::from(project.fps.max(1)));
                        src_fps / f64::from(project.fps.max(1))
                    } else {
                        1.0
                    };
                    m.trim_in_frame + (base * ratio).round() as i64
                });
                let compose_source =
                    scene_objects
                        .get(id)
                        .ok()
                        .map(|s| ComposeSource::NestedScene {
                            target_scene: s.target_scene,
                            local_frame: current - range.start_frame,
                        });

                let matrix = compute_global_matrix(&transform);
                let local_matrix = match &media_source {
                    Some(src) if matches!(src.kind, MediaKind::Video | MediaKind::Image) => {
                        match crate::media::cache::global().dimensions(&src.path) {
                            Ok((w, h)) => rescale_for_source(&matrix, w as f32, h as f32),
                            Err(_) => matrix,
                        }
                    }
                    _ => match compose_source {
                        Some(ComposeSource::NestedScene { target_scene, .. }) => {
                            match scenes.find(target_scene) {
                                Some(scene) => rescale_for_source(
                                    &matrix,
                                    scene.width as f32,
                                    scene.height as f32,
                                ),
                                None => matrix,
                            }
                        }
                        _ => matrix,
                    },
                };

                let obj_layer = layers.get(id).map_or(0, |l| l.0);
                let chain_idx = resolve_group_chain(obj_layer, &controllers, max_depth);
                let group_idx = group_only(&chain_idx, &controllers);
                let chain_matrices: Vec<GlobalMatrix> =
                    group_idx.iter().map(|&i| controllers[i].matrix).collect();
                let matrix = compute_chained_matrix(&chain_matrices, &local_matrix);

                let mvp = compute_mvp(
                    &matrix,
                    &camera,
                    project_width,
                    project_height,
                    projection_for(kind.0),
                );
                let mut opacity = transform.opacity;
                for &i in &group_idx {
                    opacity *= controllers[i].opacity;
                }
                let mut effects = effect_stacks
                    .get(id)
                    .map(|stack| compute_effect_params_at(stack, current, world))
                    .unwrap_or_default();
                for &i in group_idx.iter().rev() {
                    let mut prefixed = controllers[i].effects.clone();
                    prefixed.append(&mut effects);
                    effects = prefixed;
                }

                                                let clip_target = chain_idx.iter().find_map(|&i| match controllers[i].kind {
                    ControllerKind::Clip {
                        mode,
                        chroma_hue,
                        chroma_tolerance,
                        blend_edge,
                    } => Some(ClipTargetInfo {
                        controller: controllers[i].entity,
                        mode,
                        chroma_hue,
                        chroma_tolerance,
                        blend_edge,
                    }),
                    ControllerKind::Group { .. } => None,
                });

                let active_object = ActiveObject {
                    kind_id: kind.0,
                    clip_instance: object_ids.get(id).map_or(0, |o| o.0 as u64),
                    source_frame,
                    text_content,
                    shape_params: shape,
                    media_source,
                    mvp,
                    opacity,
                    effects,
                    compose_source,
                    layer: obj_layer,
                    clip_target,
                };

                let fb_pos = chain_idx.iter().position(|&i| {
                    matches!(
                        controllers[i].kind,
                        ControllerKind::Group {
                            generate_framebuffer: true,
                            ..
                        }
                    )
                });

                if let Some(pos) = fb_pos {
                    let controller = controllers[chain_idx[pos]].entity;
                    let hide_captured = controllers[chain_idx[pos]].hide_captured();
                    let inner_chain = &chain_idx[..pos];
                    let inner_group_idx = group_only(inner_chain, &controllers);
                    let inner_matrices: Vec<GlobalMatrix> = inner_group_idx
                        .iter()
                        .map(|&i| controllers[i].matrix)
                        .collect();
                    let inner_matrix = compute_chained_matrix(&inner_matrices, &local_matrix);
                    let inner_mvp = compute_mvp(
                        &inner_matrix,
                        &camera,
                        project_width,
                        project_height,
                        projection_for(kind.0),
                    );
                    let mut inner_opacity = transform.opacity;
                    for &i in &inner_group_idx {
                        inner_opacity *= controllers[i].opacity;
                    }
                    let mut inner_effects = effect_stacks
                        .get(id)
                        .map(|stack| compute_effect_params_at(stack, current, world))
                        .unwrap_or_default();
                    for &i in inner_group_idx.iter().rev() {
                        let mut prefixed = controllers[i].effects.clone();
                        prefixed.append(&mut inner_effects);
                        inner_effects = prefixed;
                    }
                    let inner_clip_target = inner_chain.iter().find_map(|&i| match controllers[i].kind {
                        ControllerKind::Clip {
                            mode,
                            chroma_hue,
                            chroma_tolerance,
                            blend_edge,
                        } => Some(ClipTargetInfo {
                            controller: controllers[i].entity,
                            mode,
                            chroma_hue,
                            chroma_tolerance,
                            blend_edge,
                        }),
                        ControllerKind::Group { .. } => None,
                    });
                    let captured_object = ActiveObject {
                        mvp: inner_mvp,
                        opacity: inner_opacity,
                        effects: inner_effects,
                        clip_target: inner_clip_target,
                        ..active_object.clone()
                    };
                    captured
                        .entry(controller)
                        .or_default()
                        .push(captured_object);
                    if !hide_captured {
                        let stationary_chain: Vec<usize> = chain_idx
                            .iter()
                            .copied()
                            .enumerate()
                            .filter(|&(i, _)| i != pos)
                            .map(|(_, v)| v)
                            .collect();
                        let stationary_group_idx = group_only(&stationary_chain, &controllers);
                        let stationary_matrices: Vec<GlobalMatrix> = stationary_group_idx
                            .iter()
                            .map(|&i| controllers[i].matrix)
                            .collect();
                        let stationary_matrix =
                            compute_chained_matrix(&stationary_matrices, &local_matrix);
                        let stationary_mvp = compute_mvp(
                            &stationary_matrix,
                            &camera,
                            project_width,
                            project_height,
                            projection_for(kind.0),
                        );
                        let mut stationary_opacity = transform.opacity;
                        for &i in &stationary_group_idx {
                            stationary_opacity *= controllers[i].opacity;
                        }
                        let mut stationary_effects = effect_stacks
                            .get(id)
                            .map(|stack| compute_effect_params_at(stack, current, world))
                            .unwrap_or_default();
                        for &i in stationary_group_idx.iter().rev() {
                            let mut prefixed = controllers[i].effects.clone();
                            prefixed.append(&mut stationary_effects);
                            stationary_effects = prefixed;
                        }
                        active.push(ActiveObject {
                            mvp: stationary_mvp,
                            opacity: stationary_opacity,
                            effects: stationary_effects,
                            ..active_object
                        });
                    }
                } else {
                    active.push(active_object);
                }
            }

            for c in controllers.iter() {
                if !c.requires_fb() {
                    continue;
                }
                let Ok(kind) = kind_ids.get(c.entity) else {
                    continue;
                };
                let chain_idx = resolve_group_chain(c.layer, &controllers, max_depth);
                let group_idx = group_only(&chain_idx, &controllers);
                let chain_matrices: Vec<GlobalMatrix> =
                    group_idx.iter().map(|&i| controllers[i].matrix).collect();
                let own_matrix = match c.kind {
                    ControllerKind::Group { .. } => {
                        scale_to_pixels(&c.matrix, project_width, project_height)
                    }
                    ControllerKind::Clip { .. } => {
                        scale_to_pixels(&c.matrix, UNIT_SIZE_PX, UNIT_SIZE_PX)
                    }
                };
                let matrix = compute_chained_matrix(&chain_matrices, &own_matrix);
                let mvp = compute_mvp(
                    &matrix,
                    &camera,
                    project_width,
                    project_height,
                    projection_for(kind.0),
                );
                let mut opacity = c.opacity;
                for &i in &group_idx {
                    opacity *= controllers[i].opacity;
                }
                let mut effects = c.effects.clone();
                for &i in group_idx.iter().rev() {
                    let mut prefixed = controllers[i].effects.clone();
                    prefixed.append(&mut effects);
                    effects = prefixed;
                }

                match c.kind {
                    ControllerKind::Group { .. } => {
                        if !c.render_self {
                            continue;
                        }
                        active.push(ActiveObject {
                            kind_id: kind.0,
                            clip_instance: object_ids.get(c.entity).map_or(0, |o| o.0 as u64),
                            source_frame: 0,
                            text_content: None,
                            shape_params: None,
                            media_source: None,
                            mvp,
                            opacity,
                            effects,
                            compose_source: Some(ComposeSource::FrameBuffer {
                                controller: c.entity,
                                kind: FrameBufferKind::Group,
                            }),
                            layer: c.layer,
                            clip_target: None,
                        });
                    }
                    ControllerKind::Clip { .. } => {
                        let keyframes = keyframe_tracks.get(c.entity).ok();

                        let mut text_content = text_contents.get(c.entity).ok().cloned();
                        if let (Some(tc), Some(kt)) = (text_content.as_mut(), keyframes) {
                            kt.apply(tc, current);
                        }
                        let mut shape = shape_params.get(c.entity).ok().copied();
                        if let (Some(sp), Some(kt)) = (shape.as_mut(), keyframes) {
                            kt.apply(sp, current);
                        }
                        let media_source = media_sources.get(c.entity).ok().cloned();
                        let source_frame = media_source.as_ref().map_or(0, |m| {
                            let base = time_ranges
                                .get(c.entity)
                                .map_or(0.0, |r| f64::from(current - r.start_frame));
                            let ratio = if matches!(m.kind, MediaKind::Video) {
                                crate::media::cache::global()
                                    .source_fps(&m.path)
                                    .map_or(1.0, |src_fps| src_fps / f64::from(project.fps.max(1)))
                            } else {
                                1.0
                            };
                            m.trim_in_frame + (base * ratio).round() as i64
                        });

                        let mold_object = ActiveObject {
                            kind_id: kind.0,
                            clip_instance: object_ids.get(c.entity).map_or(0, |o| o.0 as u64),
                            source_frame,
                            text_content,
                            shape_params: shape,
                            media_source,
                            mvp,
                            opacity,
                            effects,
                            compose_source: None,
                            layer: c.layer,
                            clip_target: None,
                        };
                        captured
                            .entry(c.entity)
                            .or_default()
                            .push(mold_object.clone());
                        if c.render_self {
                            active.push(mold_object);
                        }
                    }
                }
            }

            active.sort_by_key(|o| o.layer);

            (active, captured)
        },
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::EcsWorld;
    use crate::ecs::components::MediaSource;
    use crate::ecs::effects::EffectStack;
    use crate::ecs::types::{EffectInstance, EffectParam, Value};
    use crate::media::MediaKind;
    use shipyard::ViewMut;
    use std::path::PathBuf;

    const KIND_TEXT: u32 = 100;
    const KIND_SHAPE: u32 = 200;
    const KIND_GROUP_CONTROL: u32 = 900;

    fn world_with_object(start: i32, end: i32) -> (EcsWorld, usize) {
        let mut world = EcsWorld::new();
        let id = world.add_object(
            start,
            end - start,
            KIND_TEXT,
            0,
            Some(TextContent::default()),
        );
        (world, id)
    }

    #[test]
    fn frame_range_boundary() {
        let (mut world, _id) = world_with_object(10, 20);

        world.set_current_frame(9);
        assert_eq!(get_active_objects_system(&world).0.len(), 0);

        world.set_current_frame(10);
        assert_eq!(get_active_objects_system(&world).0.len(), 1);

        world.set_current_frame(19);
        assert_eq!(get_active_objects_system(&world).0.len(), 1);

        world.set_current_frame(20);
        assert_eq!(get_active_objects_system(&world).0.len(), 0);
    }

    #[test]
    fn scene_filter() {
        let mut world = EcsWorld::new();
        let scene_a = world.active_scene();
        let id_a = world.add_object(0, 30, KIND_TEXT, 0, Some(TextContent::default()));
        let scene_b = world.add_scene("Scene B");
        world.switch_scene(scene_b);
        let id_b = world.add_object(0, 30, KIND_TEXT, 0, Some(TextContent::default()));

        world.switch_scene(scene_a);
        world.set_current_frame(0);
        let (active_a, _captured) = get_active_objects_system(&world);
        assert_eq!(active_a.len(), 1);
        assert_eq!(active_a[0].clip_instance, id_a as u64);

        world.switch_scene(scene_b);
        world.set_current_frame(0);
        let (active_b, _captured) = get_active_objects_system(&world);
        assert_eq!(active_b.len(), 1);
        assert_eq!(active_b[0].clip_instance, id_b as u64);
    }

    #[test]
    fn all_kinds_use_perspective_projection() {
        let (mut world, _id) = world_with_object(0, 30);
        world.set_current_frame(0);
        let (active, _captured) = get_active_objects_system(&world);
        assert_eq!(active.len(), 1);
        assert_ne!(active[0].mvp[15], 0.0);
    }

    #[test]
    fn shape_object_carries_shape_params() {
        let mut world = EcsWorld::new();
        let shape = ShapeParams {
            sides: 6,
            ..ShapeParams::default()
        };
        let id = world.add_shape_object(0, 30, KIND_SHAPE, 0, shape);
        world.set_current_frame(0);
        let (active, _captured) = get_active_objects_system(&world);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].clip_instance, id as u64);
        assert_eq!(active[0].shape_params.map(|s| s.sides), Some(6));
        assert!(active[0].text_content.is_none());
    }

    #[test]
    fn clip_instance_uniqueness_across_same_source() {
        let mut world = EcsWorld::new();
        let media = MediaSource {
            path: PathBuf::from("nonexistent.png"),
            kind: MediaKind::Image,
            trim_in_frame: 0,
        };
        let id1 = world.add_media_object(0, 30, KIND_SHAPE, 0, media.clone());
        let id2 = world.add_media_object(0, 30, KIND_SHAPE, 1, media);
        world.set_current_frame(0);
        let (active, _captured) = get_active_objects_system(&world);
        assert_eq!(active.len(), 2);
        let instances: Vec<u64> = active.iter().map(|a| a.clip_instance).collect();
        assert_ne!(instances[0], instances[1]);
        assert!(instances.contains(&(id1 as u64)));
        assert!(instances.contains(&(id2 as u64)));
    }

    #[test]
    fn effect_stack_propagation() {
        let (mut world, id) = world_with_object(0, 30);
        let entity = world.find_entity(id).expect("entity存在前提");
        world.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                let mut instance = EffectInstance::new("test_effect");
                instance
                    .params
                    .insert("amount".to_string(), EffectParam::new(Value::Number(0.5)));
                stack.0.push(instance);
            }
        });
        world.set_current_frame(0);
        let (active, _captured) = get_active_objects_system(&world);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].effects.len(), 1);
        assert_eq!(active[0].effects[0].0, "test_effect");
        assert_eq!(
            active[0].effects[0].1.get("amount"),
            Some(&Value::Number(0.5))
        );
    }

    #[test]
    fn group_control_chain_moves_child_down() {
        let mut world = EcsWorld::new();
        let gc_id =
            world.add_group_control_object(0, 30, KIND_GROUP_CONTROL, 0, GroupControl::default());
        world.set_layer(gc_id, 0);
        world.set_transform_param(gc_id, "x", 100.0);
        let child_id = world.add_object(0, 30, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(child_id, 1);
        world.set_current_frame(0);
        let (active, _captured) = get_active_objects_system(&world);
        let child = active
            .iter()
            .find(|a| a.clip_instance == child_id as u64)
            .unwrap();
        assert_ne!(child.mvp[12], 0.0);
    }

    #[test]
    fn group_control_layer_count_excludes_out_of_range_layer() {
        let mut world = EcsWorld::new();
        let gc = GroupControl {
            layer_count_down: 1,
            layer_count_up: 0,
            generate_framebuffer: false,
            hide_captured: false,
        };
        let gc_id = world.add_group_control_object(0, 30, KIND_GROUP_CONTROL, 0, gc);
        world.set_layer(gc_id, 0);
        world.set_transform_param(gc_id, "x", 100.0);
        let in_range = world.add_object(0, 30, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(in_range, 1);
        let out_of_range = world.add_object(0, 30, KIND_TEXT, 2, Some(TextContent::default()));
        world.set_layer(out_of_range, 2);
        world.set_current_frame(0);
        let (active, _captured) = get_active_objects_system(&world);
        let in_obj = active
            .iter()
            .find(|a| a.clip_instance == in_range as u64)
            .unwrap();
        let out_obj = active
            .iter()
            .find(|a| a.clip_instance == out_of_range as u64)
            .unwrap();
        assert_ne!(in_obj.mvp[12], 0.0);
        assert_eq!(out_obj.mvp[12], 0.0);
    }

    #[test]
    fn group_control_upward_range_affects_layer_above() {
        let mut world = EcsWorld::new();
        let gc = GroupControl {
            layer_count_down: 0,
            layer_count_up: 1,
            generate_framebuffer: false,
            hide_captured: false,
        };
        let gc_id = world.add_group_control_object(0, 30, KIND_GROUP_CONTROL, 0, gc);
        world.set_layer(gc_id, 5);
        world.set_transform_param(gc_id, "x", 100.0);
        let above = world.add_object(0, 30, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(above, 4);
        let out_of_range = world.add_object(0, 30, KIND_TEXT, 2, Some(TextContent::default()));
        world.set_layer(out_of_range, 3);
        world.set_current_frame(0);
        let (active, _captured) = get_active_objects_system(&world);
        let above_obj = active
            .iter()
            .find(|a| a.clip_instance == above as u64)
            .unwrap();
        let out_obj = active
            .iter()
            .find(|a| a.clip_instance == out_of_range as u64)
            .unwrap();
        assert_ne!(above_obj.mvp[12], 0.0);
        assert_eq!(out_obj.mvp[12], 0.0);
    }

    #[test]
    fn framebuffer_capture_respects_span_and_keeps_visible_by_default() {
        let mut world = EcsWorld::new();
        let gc = GroupControl {
            layer_count_down: 1,
            layer_count_up: 0,
            generate_framebuffer: true,
            hide_captured: false,
        };
        let gc_id = world.add_group_control_object(0, 30, KIND_GROUP_CONTROL, 0, gc);
        world.set_layer(gc_id, 1);
        let captured_child = world.add_object(0, 30, KIND_TEXT, 0, Some(TextContent::default()));
        world.set_layer(captured_child, 0);
        let out_of_span = world.add_object(0, 30, KIND_TEXT, -1, Some(TextContent::default()));
        world.set_layer(out_of_span, -1);
        world.set_current_frame(0);
        let (active, captured) = get_active_objects_system(&world);

        let entity = world.find_entity(gc_id).expect("entity存在前提");
        let captured_list = captured.get(&entity).expect("捕捉対象存在前提");
        assert_eq!(captured_list.len(), 1);
        assert_eq!(captured_list[0].clip_instance, captured_child as u64);

        assert!(
            active
                .iter()
                .any(|a| a.clip_instance == captured_child as u64),
            "hide_captured=false時は通常経路にも残存"
        );
        assert!(
            active.iter().any(|a| a.clip_instance == out_of_span as u64),
            "span範囲外オブジェクトは非捕捉かつ通常描画継続"
        );
    }

    #[test]
    fn framebuffer_hide_captured_removes_from_active() {
        let mut world = EcsWorld::new();
        let gc = GroupControl {
            layer_count_down: 1,
            layer_count_up: 0,
            generate_framebuffer: true,
            hide_captured: true,
        };
        let gc_id = world.add_group_control_object(0, 30, KIND_GROUP_CONTROL, 0, gc);
        world.set_layer(gc_id, 1);
        let captured_child = world.add_object(0, 30, KIND_TEXT, 0, Some(TextContent::default()));
        world.set_layer(captured_child, 0);
        world.set_current_frame(0);
        let (active, captured) = get_active_objects_system(&world);

        let entity = world.find_entity(gc_id).expect("entity存在前提");
        assert_eq!(captured.get(&entity).map(Vec::len), Some(1));
        assert!(
            !active
                .iter()
                .any(|a| a.clip_instance == captured_child as u64),
            "hide_captured=true時は通常経路から除外"
        );
    }

    #[test]
    fn plain_group_control_never_captures() {
        let mut world = EcsWorld::new();
        let gc = GroupControl {
            layer_count_down: 1,
            layer_count_up: 0,
            generate_framebuffer: false,
            hide_captured: false,
        };
        let gc_id = world.add_group_control_object(0, 30, KIND_GROUP_CONTROL, 0, gc);
        world.set_layer(gc_id, 1);
        let child = world.add_object(0, 30, KIND_TEXT, 0, Some(TextContent::default()));
        world.set_layer(child, 0);
        world.set_current_frame(0);
        let (active, captured) = get_active_objects_system(&world);

        assert!(captured.is_empty(), "非FBOグループは捕捉対象を生成しない");
        assert!(active.iter().any(|a| a.clip_instance == child as u64));
    }

    #[test]
    fn clip_layer_span_excludes_out_of_range_layer() {
        let mut world = EcsWorld::new();
        let cc_id = world.add_object(0, 30, KIND_SHAPE, 1, None);
        world.set_clip_target(
            cc_id,
            ClipTarget {
                enabled: true,
                layer_count_down: 1,
                layer_count_up: 0,
                ..ClipTarget::default()
            },
        );
        world.set_layer(cc_id, 1);
        let in_range = world.add_object(0, 30, KIND_TEXT, 0, Some(TextContent::default()));
        world.set_layer(in_range, 0);
        let out_of_range = world.add_object(0, 30, KIND_TEXT, -1, Some(TextContent::default()));
        world.set_layer(out_of_range, -1);
        world.set_current_frame(0);
        let (active, captured) = get_active_objects_system(&world);

        let entity = world.find_entity(cc_id).expect("entity存在前提");
        let captured_list = captured.get(&entity).map(Vec::len).unwrap_or(0);
        assert_eq!(captured_list, 1, "span範囲内のみ捕捉されmoldを構成");

        let in_obj = active
            .iter()
            .find(|a| a.clip_instance == in_range as u64)
            .unwrap();
        assert!(
            in_obj.clip_target.is_some(),
            "span範囲内オブジェクトは自動的にcontentとして識別"
        );
        let out_obj = active
            .iter()
            .find(|a| a.clip_instance == out_of_range as u64)
            .unwrap();
        assert!(
            out_obj.clip_target.is_none(),
            "span範囲外はクリップ対象化されない"
        );
    }

    #[test]
    fn clip_mode_luminance_invert_is_stored_in_active_object() {
        let mut world = EcsWorld::new();
        let cc_id = world.add_object(0, 30, KIND_SHAPE, 1, None);
        world.set_clip_target(
            cc_id,
            ClipTarget {
                enabled: true,
                layer_count_down: 1,
                layer_count_up: 0,
                mode: ClipMode::LuminanceInvert,
                ..ClipTarget::default()
            },
        );
        world.set_layer(cc_id, 1);
        let child = world.add_object(0, 30, KIND_TEXT, 0, Some(TextContent::default()));
        world.set_layer(child, 0);
        world.set_current_frame(0);
        let (active, _captured) = get_active_objects_system(&world);
        let child_obj = active
            .iter()
            .find(|a| a.clip_instance == child as u64)
            .unwrap();
        assert_eq!(
            child_obj.clip_target.map(|t| t.mode),
            Some(ClipMode::LuminanceInvert)
        );
    }

    #[test]
    fn clip_and_group_curtains_resolve_independently() {
        let mut world = EcsWorld::new();
        let gc = GroupControl {
            layer_count_down: 1,
            layer_count_up: 0,
            generate_framebuffer: true,
            hide_captured: false,
        };
        let gc_id = world.add_group_control_object(0, 30, KIND_GROUP_CONTROL, 0, gc);
        world.set_layer(gc_id, 2);
        let cc_id = world.add_object(0, 30, KIND_SHAPE, 0, None);
        world.set_clip_target(
            cc_id,
            ClipTarget {
                enabled: true,
                layer_count_down: 1,
                layer_count_up: 0,
                ..ClipTarget::default()
            },
        );
        world.set_layer(cc_id, 1);
        let leaf = world.add_object(0, 30, KIND_TEXT, 0, Some(TextContent::default()));
        world.set_layer(leaf, 0);
        world.set_current_frame(0);
        let (active, captured) = get_active_objects_system(&world);

        let gc_entity = world.find_entity(gc_id).expect("entity存在前提");
        let cc_entity = world.find_entity(cc_id).expect("entity存在前提");
        assert_eq!(
            captured.get(&gc_entity).map(Vec::len),
            Some(1),
            "Groupチェーンはleafを1回のみ捕捉"
        );
        assert_eq!(
            captured.get(&cc_entity).map(Vec::len),
            Some(1),
            "Clipチェーンはleafを1回のみ捕捉"
        );
        let leaf_instances = active
            .iter()
            .filter(|a| a.clip_instance == leaf as u64)
            .count();
        assert_eq!(
            leaf_instances, 1,
            "統一controllers解決によりleafは1回のみ描画対象化"
        );
    }
}
