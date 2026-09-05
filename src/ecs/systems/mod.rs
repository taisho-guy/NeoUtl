use super::EcsWorld;

mod active_query;
mod audio;
mod camera;
mod curtain;
mod types;

pub use active_query::{get_active_objects_system, get_active_objects_system_at};
pub use audio::get_active_audio_system;
pub use types::{ActiveObject, CapturedObjects, ComposeSource, FrameBufferKind};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::EcsWorld;
    use crate::ecs::components::MediaSource;
    use crate::ecs::effects::EffectStack;
    use crate::ecs::types::{EffectInstance, EffectParam, Value};
    use neoutl_media_runtime::MediaKind;
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
            camera: None,
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
            camera: None,
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
            camera: None,
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
            camera: None,
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
            camera: None,
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
            camera: None,
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
