use super::EcsWorld;

mod active_query;
mod audio;
mod camera;
mod curtain;
pub mod expression;
mod types;

pub use active_query::{get_active_objects_system, get_active_objects_system_at};
pub use audio::get_active_audio_system;
#[allow(unused_imports)]
pub use expression::evaluate_expressions_for_world;
pub use types::{ActiveObject, CapturedObjects, ComposeSource, FrameBufferKind};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::EcsWorld;
    use crate::ecs::components::{
        ClipMode, ClipTarget, GroupControl, MediaSource, ShapeParams, TextContent,
    };
    use crate::ecs::effects::EffectStack;
    use crate::ecs::types::{EffectInstance, EffectParam, Value};
    use neoutl_media_runtime::MediaKind;
    use shipyard::{Get, ViewMut};
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
        world.set_layer(gc_id, 0);
        let captured_child = world.add_object(0, 30, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(captured_child, 1);
        let out_of_span = world.add_object(0, 30, KIND_TEXT, 2, Some(TextContent::default()));
        world.set_layer(out_of_span, 2);
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
        world.set_layer(gc_id, 0);
        let captured_child = world.add_object(0, 30, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(captured_child, 1);
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
        world.set_layer(gc_id, 0);
        let child = world.add_object(0, 30, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(child, 1);
        world.set_current_frame(0);
        let (active, captured) = get_active_objects_system(&world);

        assert!(captured.is_empty(), "非FBOグループは捕捉対象を生成しない");
        assert!(active.iter().any(|a| a.clip_instance == child as u64));
    }

    #[test]
    fn clip_layer_span_excludes_out_of_range_layer() {
        let mut world = EcsWorld::new();
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
        world.set_layer(cc_id, 0);
        let in_range = world.add_object(0, 30, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(in_range, 1);
        let out_of_range = world.add_object(0, 30, KIND_TEXT, 2, Some(TextContent::default()));
        world.set_layer(out_of_range, 2);
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
        let cc_id = world.add_object(0, 30, KIND_SHAPE, 0, None);
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
        world.set_layer(cc_id, 0);
        let child = world.add_object(0, 30, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(child, 1);
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
            layer_count_down: 2,
            layer_count_up: 0,
            generate_framebuffer: true,
            hide_captured: false,
            camera: None,
        };
        let gc_id = world.add_group_control_object(0, 30, KIND_GROUP_CONTROL, 0, gc);
        world.set_layer(gc_id, 0);
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
        let leaf = world.add_object(0, 30, KIND_TEXT, 2, Some(TextContent::default()));
        world.set_layer(leaf, 2);
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

    static DUMMY_CAMERA_META: neoutl_object_api::ObjectMeta = neoutl_object_api::ObjectMeta {
        stable_id: neoutl_object_api::CAMERA_STABLE_ID,
        name: "Camera",
        dimensionality: neoutl_object_api::Dimensionality::ThreeD,
        property_groups: neoutl_object_api::FfiSlice::from_static(&[]),
    };

    static DUMMY_LIGHT_META: neoutl_object_api::ObjectMeta = neoutl_object_api::ObjectMeta {
        stable_id: neoutl_object_api::LIGHT_STABLE_ID,
        name: "Light",
        dimensionality: neoutl_object_api::Dimensionality::ThreeD,
        property_groups: neoutl_object_api::FfiSlice::from_static(&[]),
    };

    unsafe extern "C" fn dummy_camera_meta() -> *const neoutl_object_api::ObjectMeta {
        &raw const DUMMY_CAMERA_META
    }

    unsafe extern "C" fn dummy_light_meta() -> *const neoutl_object_api::ObjectMeta {
        &raw const DUMMY_LIGHT_META
    }

    unsafe extern "C" fn dummy_vertex_count() -> u32 {
        0
    }

    unsafe extern "C" fn dummy_wgsl() -> neoutl_object_api::WgslSource {
        neoutl_object_api::WgslSource {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    unsafe extern "C" fn dummy_render(_ctx: *const neoutl_object_api::RenderContext) {}

    static DUMMY_CAMERA_VTABLE: neoutl_object_api::ObjectVTable = neoutl_object_api::ObjectVTable {
        meta: dummy_camera_meta,
        vertex_count: dummy_vertex_count,
        wgsl: dummy_wgsl,
        render: dummy_render,
        read_ref_layer: None,
        setup_accelerator: None,
    };

    static DUMMY_LIGHT_VTABLE: neoutl_object_api::ObjectVTable = neoutl_object_api::ObjectVTable {
        meta: dummy_light_meta,
        vertex_count: dummy_vertex_count,
        wgsl: dummy_wgsl,
        render: dummy_render,
        read_ref_layer: None,
        setup_accelerator: None,
    };

    fn setup_camera_kind() -> u32 {
        crate::objects::loader::register_static(
            neoutl_object_api::CAMERA_STABLE_ID,
            "Camera",
            &DUMMY_CAMERA_VTABLE,
        )
    }

    fn setup_light_kind() -> u32 {
        crate::objects::loader::register_static(
            neoutl_object_api::LIGHT_STABLE_ID,
            "Light",
            &DUMMY_LIGHT_VTABLE,
        )
    }

    #[test]
    fn camera_independent_object_resolves_view_and_is_not_rendered() {
        let mut world = EcsWorld::new();
        let camera_kind = setup_camera_kind();

        let mut custom_camera = crate::ecs::transform::Camera::for_resolution(1920.0, 1080.0);
        custom_camera.pos_z = 2000.0;
        custom_camera.fov_deg = 30.0;
        let cam_id = world.add_camera_object(0, 60, camera_kind, 0, custom_camera);
        world.set_layer(cam_id, 0);

        let child_id = world.add_object(0, 60, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(child_id, 1);

        world.set_current_frame(0);
        let (active, _captured) = get_active_objects_system(&world);

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].clip_instance, child_id as u64);

        let default_cam = crate::ecs::transform::Camera::for_resolution(1920.0, 1080.0);
        let default_mvp = crate::ecs::transform::compute_mvp(
            &crate::ecs::transform::compute_global_matrix(
                &crate::ecs::transform::Transform::default(),
            ),
            &default_cam,
            1920.0,
            1080.0,
            crate::ecs::transform::Projection::Perspective {
                fov_deg: default_cam.fov_deg,
            },
        );
        assert_ne!(active[0].mvp, default_mvp);
    }

    #[test]
    fn multi_camera_scene_switching_across_timeline() {
        let mut world = EcsWorld::new();
        let camera_kind = setup_camera_kind();

        let mut cam1 = crate::ecs::transform::Camera::for_resolution(1920.0, 1080.0);
        cam1.pos_z = 500.0;
        let cam1_id = world.add_camera_object(0, 30, camera_kind, 0, cam1);
        world.set_layer(cam1_id, 0);

        let mut cam2 = crate::ecs::transform::Camera::for_resolution(1920.0, 1080.0);
        cam2.pos_z = 2500.0;
        let cam2_id = world.add_camera_object(30, 30, camera_kind, 0, cam2);
        world.set_layer(cam2_id, 0);

        let child_id = world.add_object(0, 60, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(child_id, 1);

        world.set_current_frame(10);
        let (active_at_10, _) = get_active_objects_system(&world);
        assert_eq!(active_at_10.len(), 1);
        let mvp_10 = active_at_10[0].mvp;

        world.set_current_frame(45);
        let (active_at_45, _) = get_active_objects_system(&world);
        assert_eq!(active_at_45.len(), 1);
        let mvp_45 = active_at_45[0].mvp;

        assert_ne!(mvp_10, mvp_45);
    }

    #[test]
    fn multi_camera_layer_priority_selects_top_layer() {
        let mut world = EcsWorld::new();
        let camera_kind = setup_camera_kind();

        let mut cam_top = crate::ecs::transform::Camera::for_resolution(1920.0, 1080.0);
        cam_top.fov_deg = 30.0;
        let cam_top_id = world.add_camera_object(0, 60, camera_kind, 0, cam_top);
        world.set_layer(cam_top_id, 0);

        let mut cam_bottom = crate::ecs::transform::Camera::for_resolution(1920.0, 1080.0);
        cam_bottom.fov_deg = 90.0;
        let cam_bottom_id = world.add_camera_object(0, 60, camera_kind, 1, cam_bottom);
        world.set_layer(cam_bottom_id, 1);

        let child_id = world.add_object(0, 60, KIND_TEXT, 2, Some(TextContent::default()));
        world.set_layer(child_id, 2);

        world.set_current_frame(0);
        let (active, _) = get_active_objects_system(&world);
        assert_eq!(active.len(), 1);

        let top_mvp = crate::ecs::transform::compute_mvp(
            &crate::ecs::transform::compute_global_matrix(
                &crate::ecs::transform::Transform::default(),
            ),
            &cam_top,
            1920.0,
            1080.0,
            crate::ecs::transform::Projection::Perspective {
                fov_deg: cam_top.fov_deg,
            },
        );
        assert_eq!(active[0].mvp, top_mvp);
    }

    #[test]
    fn light_independent_object_is_excluded_from_active() {
        let mut world = EcsWorld::new();
        let light_kind = setup_light_kind();

        let light_id = world.add_light_object(0, 60, light_kind, 0);
        world.set_layer(light_id, 0);

        let child_id = world.add_object(0, 60, KIND_TEXT, 1, Some(TextContent::default()));
        world.set_layer(child_id, 1);

        world.set_current_frame(0);
        let (active, _) = get_active_objects_system(&world);

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].clip_instance, child_id as u64);
    }

    #[test]
    fn camera_params_get_set() {
        let mut world = EcsWorld::new();
        let camera_kind =
            crate::objects::loader::ensure_kind_id(neoutl_object_api::CAMERA_STABLE_ID);
        let cam = crate::ecs::transform::Camera::for_resolution(1920.0, 1080.0);
        let cam_id = world.add_camera_object(0, 60, camera_kind, 0, cam);

        let retrieved = world.get_camera_params(cam_id);
        assert_eq!(retrieved, Some(cam));

        let mut updated = cam;
        updated.fov_deg = 75.0;
        world.set_camera_params(cam_id, updated);

        let retrieved_updated = world.get_camera_params(cam_id);
        assert_eq!(retrieved_updated, Some(updated));
    }
}
