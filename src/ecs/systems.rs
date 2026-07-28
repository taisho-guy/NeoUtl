use super::EcsWorld;
use crate::ecs::components::{
    AudioParams, KeyframeTracks, KindId, MediaSource, ObjectId, SceneId, ShapeParams, TextContent,
    TimeRange,
};
use crate::ecs::effects::{EffectStack, compute_effect_params_at};
use crate::ecs::resources::{ProjectResource, SceneResource, TimelineResource};
use crate::ecs::transform::{
    Camera, DEFAULT_FOV_DEG, Projection, Transform, compute_global_matrix, compute_mvp,
    rescale_for_source,
};
use crate::ecs::types::Value;
use crate::media::MediaKind;
use shipyard::{Get, IntoIter, UniqueView, View};
use std::collections::HashMap;

pub struct ActiveObject {
    pub kind_id: u32,
    pub start_frame: i32,
    pub source_frame: i64,
    /// ObjectId由来のクリップ識別子。MediaCache::frame_atのinstance_keyとして渡し、
    /// 同一ソースファイルを複数クリップが同時参照する際のデコードセッション分離に用いる。
    pub clip_instance: u64,
    pub text_content: Option<TextContent>,
    pub shape_params: Option<ShapeParams>,
    pub media_source: Option<MediaSource>,
    pub global_matrix: [f32; 16],
    pub mvp: [f32; 16],
    pub opacity: f32,
    pub audio: AudioParams,
    pub effects: Vec<(String, HashMap<String, Value>)>,
}

/// シーン内の全オブジェクトへ適用する射影方式。常にPerspectiveを返す。
///
/// 過去の実装はkind_idのDimensionality（2D/3D/Both）によりOrtho/Perspectiveを
/// オブジェクトごとに切り替えていたが、これは誤りだった。奥行き0の平面（Text/Image/
/// Video等のTwoDオブジェクト）をOrtho射影下でX軸・Y軸回転させると、遠近法的な
/// 奥行き手がかりが一切生じないまま幅または高さがcos(角度)倍で線形に0へ収束するだけになり、
/// 「回転しているように見えず、ただ描画範囲が狭まって消える」という誤動作を引き起こす
/// （Z軸回転はOrtho/Perspective双方で平面内回転として同一に見えるため、この問題は
/// X軸・Y軸回転でのみ顕在化する）。Dimensionality::Both指定のShapeのみPerspectiveと
/// なるため、Shapeだけが正しく回転して見える非対称な挙動が生じていた。
///
/// Camera::for_resolutionは非回転時にPerspectiveでもOrtho同等（z=0平面がproject_height
/// 一杯に一致）となるよう設計されているため、全オブジェクトをPerspectiveへ統一しても
/// 静止表示（回転なし）への影響はない。
fn projection_for(_kind_id: u32) -> Projection {
    Projection::Perspective {
        fov_deg: DEFAULT_FOV_DEG,
    }
}

/// get_active_objects_systemの引数タプル型定義。
/// clippy::type_complexityの指摘に基づき、関数シグネチャ直書きから分離する。
type UniqueGroupViews<'v> = (
    UniqueView<'v, TimelineResource>,
    UniqueView<'v, SceneResource>,
    UniqueView<'v, ProjectResource>,
    UniqueView<'v, Camera>,
);
type SelectorGroupViews<'v> = (
    View<'v, TimeRange>,
    View<'v, KindId>,
    View<'v, SceneId>,
    View<'v, TextContent>,
    View<'v, ShapeParams>,
    View<'v, MediaSource>,
    View<'v, ObjectId>,
);
type PayloadGroupViews<'v> = (
    View<'v, Transform>,
    View<'v, KeyframeTracks>,
    View<'v, AudioParams>,
    View<'v, EffectStack>,
);

pub fn get_active_objects_system(world: &EcsWorld) -> Vec<ActiveObject> {
    world.world.run(
        |(timeline, scenes, project, camera): UniqueGroupViews,
         (
            time_ranges,
            kind_ids,
            scene_ids,
            text_contents,
            shape_params,
            media_sources,
            object_ids,
        ): SelectorGroupViews,
         (transforms, keyframe_tracks, audio_params, effect_stacks): PayloadGroupViews| {
            let current = timeline.current_frame;
            let active_scene = scenes.active_scene;
            let project_width = project.width.max(1) as f32;
            let project_height = project.height.max(1) as f32;
            let mut active = Vec::new();

            for (id, (range, kind, scene)) in (&time_ranges, &kind_ids, &scene_ids).iter().with_id()
            {
                if scene.0 != active_scene {
                    continue;
                }
                if current < range.start_frame || current >= range.end_frame {
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

                let mut audio = audio_params.get(id).copied().unwrap_or_default();
                if let Some(kt) = keyframes {
                    kt.apply(&mut audio, current);
                }

                let media_source = media_sources.get(id).ok().cloned();
                let source_frame = media_source.as_ref().map_or(0, |m| {
                    let base = f64::from(current - range.start_frame);
                    let ratio = if matches!(m.kind, MediaKind::Video) {
                        crate::media::cache::global()
                            .source_fps(&m.path)
                            .map_or(1.0, |src_fps| src_fps / f64::from(project.fps.max(1)))
                    } else {
                        1.0
                    };
                    m.trim_in_frame + (base * ratio).round() as i64
                });
                let matrix = compute_global_matrix(&transform);
                let matrix = match &media_source {
                    Some(src) if matches!(src.kind, MediaKind::Video | MediaKind::Image) => {
                        match crate::media::cache::global().dimensions(&src.path) {
                            Ok((w, h)) => rescale_for_source(&matrix, w as f32, h as f32),
                            Err(_) => matrix,
                        }
                    }
                    _ => matrix,
                };
                let mvp = compute_mvp(
                    &matrix,
                    &camera,
                    project_width,
                    project_height,
                    projection_for(kind.0),
                );
                let opacity = transform.opacity;
                let effects = effect_stacks
                    .get(id)
                    .map(|stack| compute_effect_params_at(stack, current))
                    .unwrap_or_default();

                active.push(ActiveObject {
                    kind_id: kind.0,
                    start_frame: range.start_frame,
                    clip_instance: object_ids.get(id).map_or(0, |o| o.0 as u64),
                    source_frame,
                    text_content,
                    shape_params: shape,
                    media_source,
                    global_matrix: matrix.0,
                    mvp,
                    opacity,
                    audio,
                    effects,
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
        assert_eq!(get_active_objects_system(&world).len(), 0);

        world.set_current_frame(10);
        assert_eq!(get_active_objects_system(&world).len(), 1);

        world.set_current_frame(19);
        assert_eq!(get_active_objects_system(&world).len(), 1);

        world.set_current_frame(20);
        assert_eq!(get_active_objects_system(&world).len(), 0);
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
        let active_a = get_active_objects_system(&world);
        assert_eq!(active_a.len(), 1);
        assert_eq!(active_a[0].clip_instance, id_a as u64);

        world.switch_scene(scene_b);
        world.set_current_frame(0);
        let active_b = get_active_objects_system(&world);
        assert_eq!(active_b.len(), 1);
        assert_eq!(active_b[0].clip_instance, id_b as u64);
    }

    #[test]
    fn unregistered_kind_falls_back_to_ortho_projection() {
        let (mut world, _id) = world_with_object(0, 30);
        world.set_current_frame(0);
        let active = get_active_objects_system(&world);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].mvp[15], 1.0);
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
        let active = get_active_objects_system(&world);
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
        let active = get_active_objects_system(&world);
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
        let active = get_active_objects_system(&world);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].effects.len(), 1);
        assert_eq!(active[0].effects[0].0, "test_effect");
        assert_eq!(
            active[0].effects[0].1.get("amount"),
            Some(&Value::Number(0.5))
        );
    }
}
