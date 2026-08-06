pub mod curve;
pub mod legacy;
pub mod script;

pub use curve::{
    CurveKind, CurveSegment, Modifier, add_segment, evaluate_kind_with_modifiers, remove_segment,
    replace_segment_kind,
};

use neoutl_easing_api::{
    EasingEngineMeta, EasingEngineVTable, EditResultC, EditResultCode, KeyframeC,
};
use serde::{Deserialize, Serialize};
use std::ffi::{CString, c_void};
use std::sync::OnceLock;

/// 1区間分のカーブ本体とその直上に掛かるモディファイアスタック。
/// AviUtl `GraphCurve`が自身の`modifiers`を保持する構造に対応する。
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

/// FFI境界を越えないインプロセス呼び出し（`neoutl-easing-standard`をrlibとして
/// 直接依存するegui側エディタ）でも同一の符号化規則を使うための公開版。
/// 新形式デコードに失敗した場合のみ旧`StandardEasing`形式を試み、
/// それも失敗すればLinearへフォールバックする（決定事項3）。
pub fn parse_payload(slice: &[u8]) -> EasingPayload {
    if slice.is_empty() {
        return EasingPayload::linear();
    }
    if let Ok(payload) = serde_json::from_slice::<EasingPayload>(slice) {
        return payload;
    }
    if let Ok(legacy_kind) = serde_json::from_slice::<legacy::StandardEasing>(slice) {
        return EasingPayload {
            kind: curve::from_legacy(&legacy_kind),
            modifiers: Vec::new(),
        };
    }
    eprintln!("[neoutl-easing-standard] 旧形式ペイロード解読失敗、Linearへフォールバック");
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

static META: OnceLock<(CString, CString, EasingEngineMeta)> = OnceLock::new();

unsafe extern "C" fn meta() -> *const EasingEngineMeta {
    let (_, _, m) = META.get_or_init(|| {
        let id_c = CString::new("neoutl-easing-standard").unwrap();
        let name_c = CString::new("Standard Easing Engine").unwrap();
        let m = EasingEngineMeta {
            id: id_c.as_ptr(),
            name: name_c.as_ptr(),
        };
        (id_c, name_c, m)
    });
    m as *const EasingEngineMeta
}

unsafe extern "C" fn evaluate_c(
    keyframes_ptr: *const KeyframeC,
    count: usize,
    frame: i32,
    fallback: f32,
) -> f32 {
    let points = unsafe { decode_keyframes(keyframes_ptr, count) };
    match points.as_slice() {
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
            let idx = points.partition_point(|k| k.frame <= frame);
            let (a, b) = (&points[idx - 1], &points[idx]);
            if a.easing.is_step() {
                return a.value;
            }
            let span = (b.frame - a.frame).max(1) as f32;
            let t = (frame - a.frame) as f32 / span;
            a.value + (b.value - a.value) * ease(&a.easing, t)
        }
    }
}

/// サードパーティ.dll/.so向けFFI経路。ホスト(NeoUtl本体)は本クレートをrlibとして
/// 直接リンクしており、標準エンジンのカーブ編集UIは`src/ui/properties/easing_editor.rs`
/// が`EasingPayload`/`parse_payload`/`encode_payload`をインプロセス直接呼び出しする
/// ため、FFI別ウィンドウは開かず即Cancelを返す（サードパーティエンジンのみこの経路を使う）。
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
