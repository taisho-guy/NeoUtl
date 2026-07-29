use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 補間の実体（イージング計算・区間評価）はneoutl-interpクレートへ外部化する。
/// ここでは再エクスポートのみを行い、ECS層は評価アルゴリズムを一切保持しない。
pub use neoutl_interp::{Easing, Keyframe};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Number(f32),
    Bool(bool),
    Text(String),
}

/// エフェクトの1パラメータ。`static_value`はframe=0相当の基準値、
/// `keyframes`は基準値に追従する中間点集合（frame昇順）。
/// Bool/Textは中間点非対応（数値のみ補間対象）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectParam {
    pub static_value: Value,
    pub keyframes: Vec<Keyframe>,
}

impl EffectParam {
    pub fn new(static_value: Value) -> Self {
        Self {
            static_value,
            keyframes: Vec::new(),
        }
    }

    /// 指定フレームでの実効値。評価はneoutl_interp::evaluateへ完全委譲する
    /// （UI・ドキュメント層側での補間計算は一切行わない）。
    pub fn evaluate(&self, frame: i32) -> Value {
        match &self.static_value {
            Value::Number(base) if !self.keyframes.is_empty() => {
                Value::Number(neoutl_interp::evaluate(&self.keyframes, frame, *base))
            }
            other => other.clone(),
        }
    }

    /// 基準値のみを書き換える。既存の中間点は保持する
    /// （旧実装は編集のたびに中間点を消していた欠陥を修正）。
    pub fn set_static(&mut self, value: Value) {
        self.static_value = value;
    }

    /// frame位置へ中間点を1件設定する。同一frameが既にあれば上書きする。
    pub fn set_keyframe(&mut self, frame: i32, value: f32, easing: Easing) {
        match self.keyframes.iter_mut().find(|k| k.frame == frame) {
            Some(existing) => {
                existing.value = value;
                existing.easing = easing;
            }
            None => {
                self.keyframes.push(Keyframe {
                    frame,
                    value,
                    easing,
                });
                self.keyframes.sort_by_key(|k| k.frame);
            }
        }
    }

    pub fn remove_keyframe(&mut self, frame: i32) {
        self.keyframes.retain(|k| k.frame != frame);
    }

    /// 中間点をold_frameからnew_frameへ移動する。new_frameに既存点がある場合は失敗する
    /// （上書き移動を許すと編集操作を伴わない値消失が起きるため）。old_frame==new_frameは
    /// 何もせず成功扱い。対象点が存在しない場合は失敗する。
    pub fn move_keyframe(&mut self, old_frame: i32, new_frame: i32) -> bool {
        if old_frame == new_frame {
            return true;
        }
        if self.keyframes.iter().any(|k| k.frame == new_frame) {
            return false;
        }
        let Some(k) = self.keyframes.iter_mut().find(|k| k.frame == old_frame) else {
            return false;
        };
        k.frame = new_frame;
        self.keyframes.sort_by_key(|k| k.frame);
        true
    }

    /// split_frame（絶対フレーム）でクリップを分割する際、この呼び出し元自身は
    /// 前半（frame < split_frame の中間点のみ）として残り、返り値が後半用の
    /// （KeyframeTracks::apply / EffectStack評価がともに絶対フレームで参照するため）。
    /// 後半のstatic_valueは分割点での評価値を継承する（AviQtl splitTracksの
    /// 「後半トラックのstartに分割点評価値を複製する」方針と同一）。
    /// Bool/Textはそもそも中間点非対応のため、static_valueをそのまま複製するのみ。
    pub fn split_at(&mut self, split_frame: i32) -> EffectParam {
        let second_value = match &self.static_value {
            Value::Number(base) => {
                let v = if self.keyframes.is_empty() {
                    *base
                } else {
                    neoutl_interp::evaluate(&self.keyframes, split_frame, *base)
                };
                Value::Number(v)
            }
            other => other.clone(),
        };
        let second_keyframes: Vec<Keyframe> = self
            .keyframes
            .iter()
            .filter(|k| k.frame > split_frame)
            .cloned()
            .collect();
        self.keyframes.retain(|k| k.frame < split_frame);
        EffectParam {
            static_value: second_value,
            keyframes: second_keyframes,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectInstance {
    pub effect_id: String,
    pub enabled: bool,
    pub params: HashMap<String, EffectParam>,
}

impl EffectInstance {
    pub fn new(effect_id: impl Into<String>) -> Self {
        Self {
            effect_id: effect_id.into(),
            enabled: true,
            params: HashMap::new(),
        }
    }
}
