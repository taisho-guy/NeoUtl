pub mod curve;
pub mod script;

pub use curve::{
    ApplyMode, CurveKind, CurveSegment, Modifier, SegmentCurveKind, add_segment, bounce_handle,
    bounce_set_handle, drag_anchor_x, elastic_amp_handle_y, elastic_freq_decay_handle,
    elastic_set_amp, elastic_set_freq_decay, evaluate_kind_with_modifiers, remove_segment,
    replace_segment_kind,
};

use neoutl_easing_api::{
    EasingEngineMeta, EasingEngineVTable, EditResultC, EditResultCode, KeyframeC,
};
use neoutl_shared_abi::StrRef;
use serde::{Deserialize, Serialize};
use std::ffi::c_void;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EasingPayload {
    pub kind: CurveKind,
    pub modifiers: Vec<Modifier>,
    #[serde(default)]
    pub apply_mode: ApplyMode,
}

impl EasingPayload {
    pub fn linear() -> Self {
        Self {
            kind: CurveKind::Linear,
            modifiers: Vec::new(),
            apply_mode: ApplyMode::Normal,
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

pub fn parse_payload(slice: &[u8]) -> EasingPayload {
    if slice.is_empty() {
        return EasingPayload::linear();
    }
    if let Ok(payload) = serde_json::from_slice::<EasingPayload>(slice) {
        return payload;
    }
    eprintln!("[neoutl-easing-standard] ペイロード解読失敗、Linearへフォールバック");
    EasingPayload::linear()
}

pub fn encode_payload(payload: &EasingPayload) -> Vec<u8> {
    serde_json::to_vec(payload).unwrap_or_default()
}

pub struct DecodedKeyframe {
    pub frame: i32,
    pub value: f32,
    pub easing: EasingPayload,
}

unsafe fn decode_keyframes(keyframes_ptr: *const KeyframeC, count: usize) -> Vec<DecodedKeyframe> {
    if keyframes_ptr.is_null() || count == 0 {
        return Vec::new();
    }
    let slice = unsafe { std::slice::from_raw_parts(keyframes_ptr, count) };
    slice
        .iter()
        .map(|k| {
            let payload = if !k.payload_ptr.is_null() && k.payload_len > 0 {
                unsafe { std::slice::from_raw_parts(k.payload_ptr, k.payload_len) }
            } else {
                &[]
            };
            DecodedKeyframe {
                frame: k.frame,
                value: k.value,
                easing: parse_payload(payload),
            }
        })
        .collect()
}

fn resolve_effective_values(points: &[DecodedKeyframe]) -> Vec<f32> {
    let mut values: Vec<f32> = points.iter().map(|p| p.value).collect();
    for i in 0..points.len() {
        if points[i].easing.apply_mode != ApplyMode::Interpolate {
            continue;
        }
        let prev = (0..i)
            .rev()
            .find(|&j| points[j].easing.apply_mode != ApplyMode::Interpolate);
        let next =
            (i + 1..points.len()).find(|&j| points[j].easing.apply_mode != ApplyMode::Interpolate);
        if let (Some(p), Some(n)) = (prev, next) {
            let span = (points[n].frame - points[p].frame).max(1) as f32;
            let t = (points[i].frame - points[p].frame) as f32 / span;
            values[i] = values[p] + (values[n] - values[p]) * t;
        }
    }
    values
}

fn effective_indices(points: &[DecodedKeyframe]) -> Vec<usize> {
    let last = points.len() - 1;
    (0..points.len())
        .filter(|&i| {
            i == 0 || i == last || points[i].easing.apply_mode != ApplyMode::IgnoreMidPoint
        })
        .collect()
}

fn evaluate_track(points: &[DecodedKeyframe], frame: i32, fallback: f32) -> f32 {
    match points {
        [] => fallback,
        [only] => only.value,
        _ => {
            let first = &points[0];
            let last = &points[points.len() - 1];
            if frame <= first.frame {
                return first.value;
            }
            if frame >= last.frame {
                return last.value;
            }
            let values = resolve_effective_values(points);
            let effective = effective_indices(points);
            let pos = effective.partition_point(|&i| points[i].frame <= frame);
            let pos = pos.clamp(1, effective.len() - 1);
            let (ia, ib) = (effective[pos - 1], effective[pos]);
            if points[ia].easing.is_step() {
                return values[ia];
            }
            let span = (points[ib].frame - points[ia].frame).max(1) as f32;
            let t = (frame - points[ia].frame) as f32 / span;
            values[ia] + (values[ib] - values[ia]) * ease(&points[ia].easing, t)
        }
    }
}

static META: EasingEngineMeta = EasingEngineMeta {
    id: StrRef::from_str("neoutl-easing-standard"),
    name: StrRef::from_str("Standard Easing Engine"),
};

unsafe extern "C" fn meta() -> *const EasingEngineMeta {
    &META
}

unsafe extern "C" fn evaluate_c(
    keyframes_ptr: *const KeyframeC,
    count: usize,
    frame: i32,
    fallback: f32,
) -> f32 {
    let points = unsafe { decode_keyframes(keyframes_ptr, count) };
    evaluate_track(&points, frame, fallback)
}

unsafe extern "C" fn open_editor_window_c(
    _host_handle: *const c_void,
    _keyframes_ptr: *const KeyframeC,
    _count: usize,
    on_complete: unsafe extern "C" fn(*mut c_void, EditResultC),
    user_data: *mut c_void,
) {
    unsafe {
        on_complete(
            user_data,
            EditResultC {
                code: EditResultCode::Cancel,
                keyframes_ptr: std::ptr::null_mut(),
                count: 0,
            },
        )
    };
}

unsafe extern "C" fn serialize_c(
    keyframes_ptr: *const KeyframeC,
    count: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) {
    let points = unsafe { decode_keyframes(keyframes_ptr, count) };
    let json_bytes = serde_json::to_vec(
        &points
            .iter()
            .map(|p| (p.frame, p.value, p.easing.clone()))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default();
    let mut boxed = json_bytes.into_boxed_slice();
    if !out_len.is_null() {
        unsafe { *out_len = boxed.len() };
    }
    if !out_ptr.is_null() {
        unsafe { *out_ptr = boxed.as_mut_ptr() };
    }
    std::mem::forget(boxed);
}

unsafe extern "C" fn deserialize_c(
    bytes_ptr: *const u8,
    len: usize,
    out_keyframes: *mut *mut KeyframeC,
    out_count: *mut usize,
) {
    if bytes_ptr.is_null() || len == 0 {
        if !out_count.is_null() {
            unsafe { *out_count = 0 };
        }
        if !out_keyframes.is_null() {
            unsafe { *out_keyframes = std::ptr::null_mut() };
        }
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts(bytes_ptr, len) };
    let raw: Vec<(i32, f32, EasingPayload)> = serde_json::from_slice(slice).unwrap_or_default();

    let mut out_vec = Vec::with_capacity(raw.len());
    for (frame, value, easing) in raw {
        let payload = encode_payload(&easing);
        let mut boxed_p = payload.into_boxed_slice();
        let p_len = boxed_p.len();
        let p_ptr = boxed_p.as_mut_ptr();
        std::mem::forget(boxed_p);
        out_vec.push(KeyframeC {
            frame,
            value,
            payload_ptr: p_ptr,
            payload_len: p_len,
        });
    }

    let count = out_vec.len();
    let mut boxed = out_vec.into_boxed_slice();
    let ptr = boxed.as_mut_ptr();
    std::mem::forget(boxed);

    if !out_count.is_null() {
        unsafe { *out_count = count };
    }
    if !out_keyframes.is_null() {
        unsafe { *out_keyframes = ptr };
    }
}

unsafe extern "C" fn free_bytes_c(ptr: *mut u8, len: usize) {
    if !ptr.is_null() && len > 0 {
        unsafe {
            let _ = Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
        }
    }
}

unsafe extern "C" fn free_keyframes_c(ptr: *mut KeyframeC, count: usize) {
    if !ptr.is_null() && count > 0 {
        unsafe {
            let slice = std::slice::from_raw_parts_mut(ptr, count);
            for item in &mut *slice {
                if !item.payload_ptr.is_null() && item.payload_len > 0 {
                    let _ = Box::from_raw(std::slice::from_raw_parts_mut(
                        item.payload_ptr as *mut u8,
                        item.payload_len,
                    ));
                }
            }
            let _ = Box::from_raw(slice);
        }
    }
}

static VTABLE: EasingEngineVTable = EasingEngineVTable {
    meta,
    evaluate: evaluate_c,
    open_editor_window: open_editor_window_c,
    serialize: serialize_c,
    deserialize: deserialize_c,
    free_bytes: free_bytes_c,
    free_keyframes: free_keyframes_c,
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn neoutl_easing_engine_entry() -> *const EasingEngineVTable {
    &VTABLE
}
