use crate::app::state::{self as app_state, SharedAppState};
use crate::ecs::components::{MediaSource, TextAlign, TextContent};
use crate::ecs::object_query_views::ObjectQueryViews;
use neoutl_media_runtime::MediaKind;
use neoutl_sdk::extension::{EditSession, EffectInfo, ObjectInfo, ProjectInfo};
use shipyard::{Get, IntoIter};
use std::path::PathBuf;

pub struct AppEditSession<'a> {
    pub state: &'a SharedAppState,
}

impl<'a> AppEditSession<'a> {
    pub fn new(state: &'a SharedAppState) -> Self {
        Self { state }
    }
}

impl<'a> EditSession for AppEditSession<'a> {
    fn project_info(&self) -> ProjectInfo {
        let s = self.state.lock().unwrap();
        let active = s.active;
        let active_scene_id = {
            let world_holder = s.sessions[active].world.clone();
            drop(s);
            world_holder.lock().unwrap().active_scene()
        };
        let s = self.state.lock().unwrap();
        let m = &s.sessions[active].meta;
        ProjectInfo {
            name: m.name.clone(),
            dir: m.dir.to_string_lossy().to_string(),
            fps: m.fps,
            width: m.width,
            height: m.height,
            audio_sample_rate: m.audio_sample_rate,
            audio_channels: m.audio_channels,
            active_scene_id,
        }
    }

    fn cursor_frame(&self) -> i32 {
        let world_holder = app_state::active_world(self.state);
        world_holder.lock().unwrap().current_frame()
    }

    fn total_frames(&self) -> i32 {
        let world_holder = app_state::active_world(self.state);
        world_holder.lock().unwrap().total_frames()
    }

    fn selected_object_ids(&self) -> Vec<usize> {
        let world_holder = app_state::active_world(self.state);
        let world = world_holder.lock().unwrap();
        world.selected_ids().iter().copied().collect()
    }

    fn find_objects_at(&self, layer: i32, frame: i32) -> Vec<ObjectInfo> {
        let world_holder = app_state::active_world(self.state);
        let world = world_holder.lock().unwrap();
        let mut results = Vec::new();

        world.world.run(|v: ObjectQueryViews| {
            for (id, range, l, scene, kind) in (
                &v.object_ids,
                &v.time_ranges,
                &v.layers,
                &v.scene_ids,
                &v.kind_ids,
            )
                .iter()
            {
                if l.0 == layer && range.start_frame <= frame && frame < range.end_frame {
                    let stable_id = crate::ecs::resolve_stable_id(kind.0, id.0);
                    results.push(ObjectInfo {
                        id: id.0,
                        scene_id: scene.0,
                        kind_stable_id: stable_id.clone(),
                        name: stable_id,
                        layer: l.0,
                        start_frame: range.start_frame,
                        duration: range.end_frame - range.start_frame,
                        end_frame: range.end_frame,
                    });
                }
            }
        });

        results
    }

    fn get_object(&self, id: usize) -> Option<ObjectInfo> {
        let world_holder = app_state::active_world(self.state);
        let world = world_holder.lock().unwrap();
        let entity = world.find_entity(id)?;

        world.world.run(|v: ObjectQueryViews| {
            let range = (&v.time_ranges).get(entity).ok()?;
            let layer = (&v.layers).get(entity).ok()?;
            let scene = (&v.scene_ids).get(entity).ok()?;
            let kind = (&v.kind_ids).get(entity).ok()?;
            let stable_id = crate::ecs::resolve_stable_id(kind.0, id);
            Some(ObjectInfo {
                id,
                scene_id: scene.0,
                kind_stable_id: stable_id.clone(),
                name: stable_id,
                layer: layer.0,
                start_frame: range.start_frame,
                duration: range.end_frame - range.start_frame,
                end_frame: range.end_frame,
            })
        })
    }

    fn get_object_effects(&self, object_id: usize) -> Vec<EffectInfo> {
        let world_holder = app_state::active_world(self.state);
        let world = world_holder.lock().unwrap();
        let Some(entity) = world.find_entity(object_id) else {
            return Vec::new();
        };

        world
            .world
            .run(|v: ObjectQueryViews| {
                let stack = (&v.stacks).get(entity).ok()?;
                let mut effects = Vec::new();
                for (index, eff) in stack.0.iter().enumerate() {
                    let effect_id = eff.effect_id.clone();
                    let name = crate::effects::loader::by_id(&effect_id)
                        .map(|s| s.name().to_owned())
                        .unwrap_or_else(|| effect_id.clone());
                    effects.push(EffectInfo {
                        index,
                        effect_id,
                        name,
                        enabled: eff.enabled,
                    });
                }
                Some(effects)
            })
            .unwrap_or_default()
    }

    fn get_param_float(
        &self,
        object_id: usize,
        effect_idx: Option<usize>,
        key: &str,
    ) -> Option<f32> {
        let world_holder = app_state::active_world(self.state);
        let world = world_holder.lock().unwrap();
        match effect_idx {
            Some(idx) => {
                let entity = world.find_entity(object_id)?;
                world.world.run(|v: ObjectQueryViews| {
                    let stack = (&v.stacks).get(entity).ok()?;
                    let eff = stack.0.get(idx)?;
                    let param = eff.params.get(key)?;
                    match param.evaluate(0) {
                        crate::ecs::types::Value::Number(n) => Some(n),
                        _ => None,
                    }
                })
            }
            None => {
                let entity = world.find_entity(object_id)?;
                world.world.run(|v: ObjectQueryViews| {
                    if let Ok(transform) = (&v.transforms).get(entity) {
                        match key {
                            "x" => return Some(transform.x),
                            "y" => return Some(transform.y),
                            "z" => return Some(transform.z),
                            "scale" | "scale_x" => return Some(transform.scale_x),
                            "scale_y" => return Some(transform.scale_y),
                            "opacity" => return Some(transform.opacity),
                            "rotation" | "rot_z" => return Some(transform.rot_z),
                            "rot_x" => return Some(transform.rot_x),
                            "rot_y" => return Some(transform.rot_y),
                            _ => {}
                        }
                    }
                    if let Ok(plugins) = (&v.plugins).get(entity) {
                        return plugins.0.get(key).copied();
                    }
                    None
                })
            }
        }
    }

    fn get_param_string(
        &self,
        object_id: usize,
        _effect_idx: Option<usize>,
        key: &str,
    ) -> Option<String> {
        let world_holder = app_state::active_world(self.state);
        let world = world_holder.lock().unwrap();
        let entity = world.find_entity(object_id)?;
        world.world.run(|v: ObjectQueryViews| {
            if key == "text" {
                if let Ok(text) = (&v.texts).get(entity) {
                    return Some(text.text.clone());
                }
            }
            if key == "path" {
                if let Ok(media) = (&v.media).get(entity) {
                    return Some(media.path.to_string_lossy().to_string());
                }
            }
            None
        })
    }

    fn set_cursor_frame(&mut self, frame: i32) {
        let world_holder = app_state::active_world(self.state);
        world_holder.lock().unwrap().set_current_frame(frame);
    }

    fn select_object(&mut self, id: usize, clear_others: bool) {
        let world_holder = app_state::active_world(self.state);
        world_holder.lock().unwrap().select_id(id, clear_others);
    }

    fn clear_selection(&mut self) {
        let world_holder = app_state::active_world(self.state);
        world_holder.lock().unwrap().clear_selection();
    }

    fn create_object(
        &mut self,
        stable_id: &str,
        layer: i32,
        start_frame: i32,
        duration: i32,
    ) -> Result<usize, String> {
        app_state::snapshot_before_edit(self.state);
        let kind_id = crate::objects::loader::ensure_kind_id(stable_id);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        let id = world.add_object(start_frame, duration, kind_id, layer, None);
        Ok(id)
    }

    fn create_text_object(
        &mut self,
        text: &str,
        layer: i32,
        start_frame: i32,
        duration: i32,
    ) -> Result<usize, String> {
        app_state::snapshot_before_edit(self.state);
        let kind_id = crate::objects::loader::ensure_kind_id(neoutl_object_api::TEXT_STABLE_ID);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        let text_content = TextContent {
            text: text.to_owned(),
            font_size: 48.0,
            color: [1.0, 1.0, 1.0, 1.0],
            font_family_stack: Vec::new(),
            bold: false,
            italic: false,
            align: TextAlign::Center,
            line_height: 1.2,
            outline_width: 0.0,
            outline_color: [0.0, 0.0, 0.0, 1.0],
        };
        let id = world.add_object(start_frame, duration, kind_id, layer, Some(text_content));
        Ok(id)
    }

    fn create_media_object(
        &mut self,
        file_path: &str,
        layer: i32,
        start_frame: i32,
        duration: Option<i32>,
    ) -> Result<usize, String> {
        app_state::snapshot_before_edit(self.state);
        let path = PathBuf::from(file_path);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let (kind, stable_id) = match ext.as_str() {
            "png" | "jpg" | "jpeg" | "webp" | "bmp" => {
                (MediaKind::Image, neoutl_object_api::IMAGE_STABLE_ID)
            }
            "wav" | "mp3" | "flac" | "aac" | "ogg" => {
                (MediaKind::Audio, neoutl_object_api::AUDIO_STABLE_ID)
            }
            _ => (MediaKind::Video, neoutl_object_api::VIDEO_STABLE_ID),
        };

        let kind_id = crate::objects::loader::ensure_kind_id(stable_id);
        let duration = duration.unwrap_or(150);
        let media = MediaSource {
            path,
            kind,
            trim_in_frame: 0,
        };

        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        let id = world.add_media_object(start_frame, duration, kind_id, layer, media);
        Ok(id)
    }

    fn move_object(
        &mut self,
        id: usize,
        new_layer: i32,
        new_start_frame: i32,
    ) -> Result<(), String> {
        app_state::snapshot_before_edit(self.state);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        world.move_object(id, new_layer, new_start_frame);
        Ok(())
    }

    fn resize_object(&mut self, id: usize, new_duration: i32) -> Result<(), String> {
        app_state::snapshot_before_edit(self.state);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        let (start, _) = {
            let entity = world
                .find_entity(id)
                .ok_or_else(|| format!("オブジェクト {id} が見つかりません"))?;
            world.world.run(|v: ObjectQueryViews| {
                let range = (&v.time_ranges).get(entity).map_err(|e| e.to_string())?;
                Ok::<_, String>((range.start_frame, range.end_frame))
            })?
        };
        world.resize_object(id, start, start + new_duration);
        Ok(())
    }

    fn delete_object(&mut self, id: usize) -> Result<(), String> {
        app_state::snapshot_before_edit(self.state);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        world.delete_object(id);
        Ok(())
    }

    fn duplicate_object(&mut self, id: usize) -> Result<usize, String> {
        app_state::snapshot_before_edit(self.state);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        let (start, layer) = {
            let entity = world
                .find_entity(id)
                .ok_or_else(|| format!("オブジェクト {id} が見つかりません"))?;
            world.world.run(|v: ObjectQueryViews| {
                let range = (&v.time_ranges).get(entity).map_err(|e| e.to_string())?;
                let l = (&v.layers).get(entity).map_err(|e| e.to_string())?;
                Ok::<_, String>((range.start_frame, l.0))
            })?
        };
        let new_ids = world.duplicate_objects(&[id], start, layer);
        new_ids
            .first()
            .copied()
            .ok_or_else(|| format!("オブジェクトID {id} の複製に失敗しました"))
    }

    fn add_effect(&mut self, object_id: usize, effect_id: &str) -> Result<usize, String> {
        app_state::snapshot_before_edit(self.state);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        world.add_effect(object_id, effect_id);
        let count = world
            .find_entity(object_id)
            .and_then(|entity| {
                world
                    .world
                    .run(|v: ObjectQueryViews| (&v.stacks).get(entity).ok().map(|s| s.0.len()))
            })
            .unwrap_or(1);
        Ok(count.saturating_sub(1))
    }

    fn remove_effect(&mut self, object_id: usize, effect_idx: usize) -> Result<(), String> {
        app_state::snapshot_before_edit(self.state);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        world.remove_effect(object_id, effect_idx);
        Ok(())
    }

    fn set_effect_enabled(
        &mut self,
        object_id: usize,
        effect_idx: usize,
        enabled: bool,
    ) -> Result<(), String> {
        app_state::snapshot_before_edit(self.state);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        world.set_effect_enabled(object_id, effect_idx, enabled);
        Ok(())
    }

    fn set_param_float(
        &mut self,
        object_id: usize,
        effect_idx: Option<usize>,
        key: &str,
        value: f32,
    ) -> Result<(), String> {
        app_state::snapshot_before_edit(self.state);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        match effect_idx {
            Some(idx) => {
                world.set_effect_param(object_id, idx, key, value);
            }
            None => {
                let Some(entity) = world.find_entity(object_id) else {
                    return Err(format!("オブジェクト {object_id} が見つかりません"));
                };
                world.world.run(
                    |mut transforms: shipyard::ViewMut<crate::ecs::transform::Transform>| {
                        if let Ok(mut t) = (&mut transforms).get(entity) {
                            match key {
                                "x" => t.x = value,
                                "y" => t.y = value,
                                "z" => t.z = value,
                                "scale" => {
                                    t.scale_x = value;
                                    t.scale_y = value;
                                }
                                "scale_x" => t.scale_x = value,
                                "scale_y" => t.scale_y = value,
                                "opacity" => t.opacity = value,
                                "rotation" | "rot_z" => t.rot_z = value,
                                "rot_x" => t.rot_x = value,
                                "rot_y" => t.rot_y = value,
                                _ => {}
                            }
                        }
                    },
                );
                world.touch();
            }
        }
        Ok(())
    }

    fn set_param_string(
        &mut self,
        object_id: usize,
        _effect_idx: Option<usize>,
        key: &str,
        value: &str,
    ) -> Result<(), String> {
        app_state::snapshot_before_edit(self.state);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        let Some(entity) = world.find_entity(object_id) else {
            return Err(format!("オブジェクト {object_id} が見つかりません"));
        };
        if key == "text" {
            world
                .world
                .run(|mut texts: shipyard::ViewMut<TextContent>| {
                    if let Ok(mut t) = (&mut texts).get(entity) {
                        t.text = value.to_owned();
                    }
                });
            world.touch();
        }
        Ok(())
    }

    fn create_scene(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Result<i32, String> {
        app_state::snapshot_before_edit(self.state);
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        let scene_id = world.add_scene(name);
        world.update_scene_settings(
            scene_id,
            crate::ecs::SceneSettings {
                width,
                height,
                fps,
                ..Default::default()
            },
        );
        Ok(scene_id)
    }

    fn switch_scene(&mut self, scene_id: i32) -> Result<(), String> {
        let world_holder = app_state::active_world(self.state);
        let mut world = world_holder.lock().unwrap();
        if world.switch_scene(scene_id) {
            Ok(())
        } else {
            Err(format!("シーン {scene_id} の切り替えに失敗しました"))
        }
    }
}
