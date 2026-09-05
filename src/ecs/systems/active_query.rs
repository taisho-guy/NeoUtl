use super::EcsWorld;
use super::camera::{projection_for, resolve_camera, zbuffer_sort_key};
use super::curtain::{ControllerKind, CurtainInfo, group_only, resolve_group_chain};
use super::types::{ActiveObject, CapturedObjects, ClipTargetInfo, ComposeSource, FrameBufferKind};
use crate::ecs::components::{
    AudioParams, ClipTarget, GroupControl, KeyframeTracks, KindId, Layer, MediaSource, ObjectId,
    SceneId, SceneObject, ShapeParams, TextContent, TimeRange,
};
use crate::ecs::effects::{EffectStack, compute_effect_params_at};
use crate::ecs::resources::{
    ProjectResource, SceneResource, SystemSettingsResource, TimelineResource,
};
use crate::ecs::transform::{
    Camera, GlobalMatrix, Transform, compute_chained_matrix, compute_global_matrix, compute_mvp,
    compute_relative_matrix, rescale_for_source, scale_to_pixels,
};
use neoutl_media_runtime::MediaKind;
use neoutl_object_api::UNIT_SIZE_PX;
use shipyard::{Get, IntoIter, UniqueView, View};
use std::collections::HashMap;

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

pub(crate) fn is_active_at(
    range: &TimeRange,
    scene: &SceneId,
    active_scene: i32,
    frame: i32,
) -> bool {
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
                        camera: gc.camera,
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

                                    let mut layer_positions: HashMap<i32, (f32, f32, f32)> = HashMap::new();
            for (id, (range, scene, layer)) in (&time_ranges, &scene_ids, &layers).iter().with_id()
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
                layer_positions.insert(layer.0, (transform.x, transform.y, transform.z));
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
                        let src_fps = neoutl_media_runtime::cache::global()
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
                        match neoutl_media_runtime::cache::global().dimensions(&src.path) {
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

                let active_camera = resolve_camera(&chain_idx, &controllers, &layer_positions);
                let effective_camera = active_camera.map_or(*camera, |(_, c)| c);
                let mvp = compute_mvp(
                    &matrix,
                    &effective_camera,
                    project_width,
                    project_height,
                    projection_for(kind.0, effective_camera.fov_deg),
                );
                let zbuffer_depth = match active_camera {
                    Some((camera_layer, cam)) if cam.zbuffer_enabled => {
                        Some(zbuffer_sort_key(camera_layer, &matrix, &cam))
                    }
                    _ => None,
                };
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
                    zbuffer_depth,
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
                    let inner_camera = resolve_camera(inner_chain, &controllers, &layer_positions);
                    let inner_effective_camera = inner_camera.map_or(*camera, |(_, c)| c);
                    let inner_mvp = compute_mvp(
                        &inner_matrix,
                        &inner_effective_camera,
                        project_width,
                        project_height,
                        projection_for(kind.0, inner_effective_camera.fov_deg),
                    );
                    let inner_zbuffer_depth = match inner_camera {
                        Some((camera_layer, cam)) if cam.zbuffer_enabled => {
                            Some(zbuffer_sort_key(camera_layer, &inner_matrix, &cam))
                        }
                        _ => None,
                    };
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
                        zbuffer_depth: inner_zbuffer_depth,
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
                        let stationary_camera =
                            resolve_camera(&stationary_chain, &controllers, &layer_positions);
                        let stationary_effective_camera =
                            stationary_camera.map_or(*camera, |(_, c)| c);
                        let stationary_mvp = compute_mvp(
                            &stationary_matrix,
                            &stationary_effective_camera,
                            project_width,
                            project_height,
                            projection_for(kind.0, stationary_effective_camera.fov_deg),
                        );
                        let stationary_zbuffer_depth = match stationary_camera {
                            Some((camera_layer, cam)) if cam.zbuffer_enabled => Some(
                                zbuffer_sort_key(camera_layer, &stationary_matrix, &cam),
                            ),
                            _ => None,
                        };
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
                            zbuffer_depth: stationary_zbuffer_depth,
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
                let self_camera = resolve_camera(&chain_idx, &controllers, &layer_positions);
                let self_effective_camera = self_camera.map_or(*camera, |(_, cam)| cam);
                let mvp = compute_mvp(
                    &matrix,
                    &self_effective_camera,
                    project_width,
                    project_height,
                    projection_for(kind.0, self_effective_camera.fov_deg),
                );
                let self_zbuffer_depth = match self_camera {
                    Some((camera_layer, cam)) if cam.zbuffer_enabled => {
                        Some(zbuffer_sort_key(camera_layer, &matrix, &cam))
                    }
                    _ => None,
                };
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
                            zbuffer_depth: self_zbuffer_depth,
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
                                neoutl_media_runtime::cache::global()
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
                            zbuffer_depth: self_zbuffer_depth,
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

            active.sort_by(|a, b| {
                let ka = a.zbuffer_depth.unwrap_or(a.layer as f32);
                let kb = b.zbuffer_depth.unwrap_or(b.layer as f32);
                ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
            });

            (active, captured)
        },
    )
}
