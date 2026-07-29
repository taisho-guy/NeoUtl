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
pub struct KeyframeTracks(pub HashMap<String, Vec<neoutl_interp::Keyframe>>);

impl KeyframeTracks {
    pub fn set_keyframe(
        &mut self,
        key: &str,
        frame: i32,
        value: f32,
        easing: neoutl_interp::Easing,
    ) {
        let track = self.0.entry(key.to_owned()).or_default();
        match track.iter_mut().find(|k| k.frame == frame) {
            Some(existing) => {
                existing.value = value;
                existing.easing = easing;
            }
            None => {
                track.push(neoutl_interp::Keyframe {
                    frame,
                    value,
                    easing,
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

    /// split_frame（絶対フレーム）でクリップを分割する。呼び出し元自身は前半
    /// （frame < split_frame）のみを残し、返り値のタプルが (後半用KeyframeTracks,
    /// 分割点での評価値マップ) となる。評価値マップは、後半エンティティに複製する
    /// ネイティブコンポーネント（Transform等）のフィールドへ`ParamAccess::set_param`
    /// で書き戻すためのもの（分割点をまたいで値が飛ばないようにする、AviQtl
    /// splitTracksの「後半start値を分割点評価値にする」と同じ役割）。
    /// `fallback_for`は各keyの基準値（分割対象コンポーネントのParamAccess::get_param）
    /// を返すクロージャ。取得できないkeyは0.0を基準値とみなす。
    pub fn split_at(
        &mut self,
        split_frame: i32,
        fallback_for: impl Fn(&str) -> Option<f32>,
    ) -> (KeyframeTracks, HashMap<String, f32>) {
        let mut second = HashMap::new();
        let mut evaluated = HashMap::new();

        for (key, track) in self.0.iter_mut() {
            let fallback = fallback_for(key).unwrap_or(0.0);
            evaluated.insert(
                key.clone(),
                neoutl_interp::evaluate(track, split_frame, fallback),
            );

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
            let value = neoutl_interp::evaluate(track, frame, fallback);
            target.set_param(key, value);
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
