use serde::{Deserialize, Serialize};
use shipyard::Component;
use std::collections::HashMap;

/// キー文字列によるf32フィールドの汎用read/write窓口。
/// UI層(properties.rs)はgroup名で対象コンポーネントを選ぶだけとなり、
/// key単位の分岐は各コンポーネント定義の直下(このtraitのimpl)に一本化される。
/// object_schema.rsのkeyと1:1で対応する。
pub trait ParamAccess {
    fn get_param(&self, key: &str) -> Option<f32>;
    /// keyが未知の場合false（呼び出し側はplugin_param等へのフォールバックに使う）。
    fn set_param(&mut self, key: &str, value: f32) -> bool;
}

#[derive(Clone, Copy, Debug, Component)]
pub struct TimeRange {
    pub start_frame: i32,
    pub end_frame: i32,
}

#[derive(Clone, Copy, Debug, Component)]
pub struct ObjectId(pub usize);

#[derive(Clone, Copy, Debug, Component)]
pub struct KindId(pub u32);

#[derive(Clone, Copy, Debug, Component)]
pub struct Layer(pub i32);

#[derive(Clone, Copy, Debug, Component)]
pub struct SceneId(pub i32);

/// シーン参照オブジェクト。SCENE_STABLE_IDのクリップにのみ付与する。
/// target_sceneはSceneResource.scenes[].id。ネスト評価・循環検出は
/// SceneResource::would_cycle/systems::get_active_objects_systemが担う。
#[derive(Clone, Copy, Debug, Component, Serialize, Deserialize)]
pub struct SceneObject {
    pub target_scene: i32,
}

#[derive(Clone, Copy, Debug, Component, Serialize, Deserialize)]
pub struct AudioParams {
    pub volume: f32,
    pub pan: f32,
    pub mute: bool,
}

impl Default for AudioParams {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pan: 0.0,
            mute: false,
        }
    }
}

impl ParamAccess for AudioParams {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "volume" => self.volume,
            "pan" => self.pan,
            "mute" => {
                if self.mute {
                    1.0
                } else {
                    0.0
                }
            }
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        match key {
            "volume" => self.volume = value,
            "pan" => self.pan = value,
            "mute" => self.mute = value > 0.5,
            _ => return false,
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Component, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub font_family: String,
    pub bold: bool,
    pub italic: bool,
    pub align: TextAlign,
    pub line_height: f32,
    pub outline_width: f32,
    pub outline_color: [f32; 4],
}

impl Default for TextContent {
    fn default() -> Self {
        Self {
            text: "New Text".to_owned(),
            font_size: 48.0,
            color: [1.0, 1.0, 1.0, 1.0],
            font_family: String::new(),
            bold: false,
            italic: false,
            align: TextAlign::Left,
            line_height: 1.2,
            outline_width: 0.0,
            outline_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// 位置(X/Y/Z)はTransform（object_schema::TRANSFORM_SCHEMA）へ一本化する。
/// TextContentはテキスト固有パラメータ(font_size等)のみを保持する。
impl ParamAccess for TextContent {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "font_size" => self.font_size,
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        match key {
            "font_size" => self.font_size = value,
            _ => return false,
        }
        true
    }
}

/// 図形種別。sides==4はRect、sides>=8はEllipse近似として扱う（現行UI上のプリセット分岐）。
#[derive(Clone, Copy, Debug, Component, Serialize, Deserialize)]
pub struct ShapeParams {
    pub sides: u32,
    pub fill_color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub extrude_depth: f32,
}

impl Default for ShapeParams {
    fn default() -> Self {
        Self {
            sides: 4,
            fill_color: [1.0, 1.0, 1.0, 1.0],
            stroke_color: [0.0, 0.0, 0.0, 0.0],
            stroke_width: 0.0,
            extrude_depth: 0.0,
        }
    }
}

impl ParamAccess for ShapeParams {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "sides" => self.sides as f32,
            "extrude_depth" => self.extrude_depth,
            "stroke_width" => self.stroke_width,
            "fill_r" => self.fill_color[0],
            "fill_g" => self.fill_color[1],
            "fill_b" => self.fill_color[2],
            "fill_a" => self.fill_color[3],
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        match key {
            "sides" => self.sides = value.max(3.0) as u32,
            "extrude_depth" => self.extrude_depth = value.max(0.0),
            "stroke_width" => self.stroke_width = value.max(0.0),
            "fill_r" => self.fill_color[0] = value,
            "fill_g" => self.fill_color[1] = value,
            "fill_b" => self.fill_color[2] = value,
            "fill_a" => self.fill_color[3] = value,
            _ => return false,
        }
        true
    }
}

#[derive(Clone, Debug, Default, Component, Serialize, Deserialize)]
pub struct PluginParams(pub HashMap<String, f32>);

/// Transform/TextContent/ShapeParams/AudioParams等、ParamAccessを実装する
/// ネイティブコンポーネント向けの中間点集合。keyはParamAccessのkeyと1:1対応する。
/// エフェクトパラメータの中間点はEffectInstance::params側のEffectParam::keyframesが
/// 個別に保持するため、ここには含めない（所有者・ライフタイムが異なる別データを
/// 単一箇所へ無理に統合しない）。
///
/// エンティティに未付与＝中間点なし（静的値のみ）を意味する。1件でも中間点を打った
/// 時点でShipyard側へadd_componentされる（EcsWorld::set_keyframe参照）。
#[derive(Clone, Debug, Default, Component, Serialize, Deserialize)]
pub struct KeyframeTracks(pub HashMap<String, Vec<crate::ecs::types::Keyframe>>);

impl KeyframeTracks {
    pub fn set_keyframe(
        &mut self,
        key: &str,
        frame: i32,
        value: f32,
        engine_id: String,
        engine_payload: Vec<u8>,
    ) {
        let track = self.0.entry(key.to_owned()).or_default();
        let edit_seq = crate::ecs::types::next_edit_seq();
        match track.iter_mut().find(|k| k.frame == frame) {
            Some(existing) => {
                existing.value = value;
                existing.engine_id = engine_id;
                existing.engine_payload = engine_payload;
                existing.edit_seq = edit_seq;
            }
            None => {
                track.push(crate::ecs::types::Keyframe {
                    frame,
                    value,
                    engine_id,
                    engine_payload,
                    edit_seq,
                });
                track.sort_by_key(|k| k.frame);
            }
        }
    }

    /// 空になったトラックはキーごと削除し、以後の走査対象から外す。
    pub fn remove_keyframe(&mut self, key: &str, frame: i32) {
        if let Some(track) = self.0.get_mut(key) {
            track.retain(|k| k.frame != frame);
            if track.is_empty() {
                self.0.remove(key);
            }
        }
    }

    /// 指定keyの中間点をold_frameからnew_frameへ移動する。new_frameに既存点があれば失敗する。
    pub fn move_keyframe(&mut self, key: &str, old_frame: i32, new_frame: i32) -> bool {
        let Some(track) = self.0.get_mut(key) else {
            return false;
        };
        if old_frame == new_frame {
            return true;
        }
        if track.iter().any(|k| k.frame == new_frame) {
            return false;
        }
        let Some(k) = track.iter_mut().find(|k| k.frame == old_frame) else {
            return false;
        };
        k.frame = new_frame;
        track.sort_by_key(|k| k.frame);
        true
    }

    pub fn clamp_to_range(
        &mut self,
        _old_start: i32,
        _old_end: i32,
        _new_start: i32,
        _new_end: i32,
    ) {
    }

    pub fn shift(&mut self, delta: i32) {
        for track in self.0.values_mut() {
            for k in track.iter_mut() {
                k.frame += delta;
            }
        }
    }

    pub fn split_at(
        &mut self,
        split_frame: i32,
        fallback_for: impl Fn(&str) -> Option<f32>,
    ) -> (KeyframeTracks, HashMap<String, f32>) {
        let mut second = HashMap::new();
        let mut evaluated = HashMap::new();

        for (key, track) in self.0.iter_mut() {
            let fallback = fallback_for(key).unwrap_or(0.0);
            let eval_val = if track.is_empty() {
                fallback
            } else {
                let first_engine = &track[0].engine_id;
                let eng = crate::easings::loader::by_id(first_engine);
                let raw: Vec<(i32, f32, Vec<u8>)> = track
                    .iter()
                    .map(|k| (k.frame, k.value, k.engine_payload.clone()))
                    .collect();
                if let Some(e) = eng {
                    e.evaluate(&raw, split_frame, fallback)
                } else {
                    fallback
                }
            };
            evaluated.insert(key.clone(), eval_val);

            let second_track: Vec<_> = track
                .iter()
                .filter(|k| k.frame > split_frame)
                .cloned()
                .collect();
            track.retain(|k| k.frame < split_frame);
            if !second_track.is_empty() {
                second.insert(key.clone(), second_track);
            }
        }
        self.0.retain(|_, track| !track.is_empty());

        (KeyframeTracks(second), evaluated)
    }

    /// 対象コンポーネントへ、指定フレームでの評価値を書き込む。
    pub fn apply(&self, target: &mut impl ParamAccess, frame: i32) {
        for (key, track) in &self.0 {
            let Some(fallback) = target.get_param(key) else {
                continue;
            };
            let val = if track.is_empty() {
                fallback
            } else {
                let first_engine = &track[0].engine_id;
                let eng = crate::easings::loader::by_id(first_engine);
                let raw: Vec<(i32, f32, Vec<u8>)> = track
                    .iter()
                    .map(|k| (k.frame, k.value, k.engine_payload.clone()))
                    .collect();
                if let Some(e) = eng {
                    e.evaluate(&raw, frame, fallback)
                } else {
                    fallback
                }
            };
            target.set_param(key, val);
        }
    }
}

/// 動画・画像・音声オブジェクトが参照する外部メディアファイル。
/// デコード自体はMediaCache（src/media/cache.rs）が担い、このコンポーネントは
/// パス・種別・素材内トリム開始位置のみを保持する。
#[derive(Clone, Debug, Component, Serialize, Deserialize)]
pub struct MediaSource {
    pub path: std::path::PathBuf,
    pub kind: crate::media::MediaKind,
    pub trim_in_frame: i64,
}
