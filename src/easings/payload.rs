use crate::easings::curve::{CurveKind, Modifier, evaluate_kind_with_modifiers};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EasingPayload {
    pub kind: CurveKind,
    pub modifiers: Vec<Modifier>,
}

impl EasingPayload {
    pub fn linear() -> Self {
        Self {
            kind: CurveKind::Linear,
            modifiers: Vec::new(),
        }
    }

    pub fn is_step(&self) -> bool {
        matches!(
            self.kind,
            CurveKind::Bounce { cor, .. } if cor == 0.0
        )
    }
}

pub fn ease(payload: &EasingPayload, t: f32) -> f32 {
    evaluate_kind_with_modifiers(&payload.kind, &payload.modifiers, t)
}

pub fn parse_payload(bytes: &[u8]) -> EasingPayload {
    if bytes.is_empty() {
        return EasingPayload::linear();
    }
    if let Ok(payload) = serde_json::from_slice::<EasingPayload>(bytes) {
        return payload;
    }
    eprintln!("[NeoUtl][easings] ペイロード解読失敗、Linearへフォールバック");
    EasingPayload::linear()
}

pub fn encode_payload(payload: &EasingPayload) -> Vec<u8> {
    serde_json::to_vec(payload).unwrap_or_default()
}
