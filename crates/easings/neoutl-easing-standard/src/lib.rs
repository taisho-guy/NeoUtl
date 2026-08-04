use neoutl_easing_api::{
    EasingEngineMeta, EasingEngineVTable, EditResultC, EditResultCode, KeyframeC,
};
use serde::{Deserialize, Serialize};
use std::ffi::{CString, c_void};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum StandardEasing {
    Linear,
    Step,
    EaseInSine,
    EaseOutSine,
    EaseInOutSine,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    EaseInBack,
    EaseOutBack,
    EaseInOutBack,
    EaseInBounce,
    EaseOutBounce,
    EaseInOutBounce,
    Bezier { cp1: (f32, f32), cp2: (f32, f32) },
    Random { seed: u32, step: i32 },
}

impl Default for StandardEasing {
    fn default() -> Self {
        StandardEasing::Linear
    }
}

fn bounce_out(x: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if x < 1.0 / D1 {
        N1 * x * x
    } else if x < 2.0 / D1 {
        let x = x - 1.5 / D1;
        N1 * x * x + 0.75
    } else if x < 2.5 / D1 {
        let x = x - 2.25 / D1;
        N1 * x * x + 0.9375
    } else {
        let x = x - 2.625 / D1;
        N1 * x * x + 0.984375
    }
}

fn bezier_ease(t: f32, cp1: (f32, f32), cp2: (f32, f32)) -> f32 {
    let sample_x = |u: f32| {
        let mu = 1.0 - u;
        3.0 * mu * mu * u * cp1.0 + 3.0 * mu * u * u * cp2.0 + u * u * u
    };
    let sample_y = |u: f32| {
        let mu = 1.0 - u;
        3.0 * mu * mu * u * cp1.1 + 3.0 * mu * u * u * cp2.1 + u * u * u
    };
    let mut u = t;
    for _ in 0..8 {
        let mu = 1.0 - u;
        let x = sample_x(u);
        let err = x - t;
        if err.abs() < 1e-5 {
            break;
        }
        let dx =
            3.0 * mu * mu * cp1.0 + 6.0 * mu * u * (cp2.0 - cp1.0) + 3.0 * u * u * (1.0 - cp2.0);
        if dx.abs() < 1e-6 {
            break;
        }
        u -= err / dx;
    }
    sample_y(u.clamp(0.0, 1.0))
}

fn splitmix32(seed: u32) -> u32 {
    let mut z = seed.wrapping_add(0x9E3779B9);
    z = (z ^ (z >> 16)).wrapping_mul(0x85EBCA6B);
    z = (z ^ (z >> 13)).wrapping_mul(0xC2B2AE35);
    z ^ (z >> 16)
}

fn random_unit(seed: u32, idx: i64) -> f32 {
    let idx_bits = (idx as i64 as u64 as u32).wrapping_mul(0x27D4_EB2F);
    let combined = seed ^ idx_bits;
    (splitmix32(combined) as f64 / (u32::MAX as f64 + 1.0)) as f32
}

pub fn ease(kind: StandardEasing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        StandardEasing::Linear => t,
        StandardEasing::Step => 0.0,
        StandardEasing::EaseInSine => 1.0 - (t * std::f32::consts::FRAC_PI_2).cos(),
        StandardEasing::EaseOutSine => (t * std::f32::consts::FRAC_PI_2).sin(),
        StandardEasing::EaseInOutSine => -((std::f32::consts::PI * t).cos() - 1.0) / 2.0,
        StandardEasing::EaseInQuad => t * t,
        StandardEasing::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
        StandardEasing::EaseInOutQuad => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        }
        StandardEasing::EaseInCubic => t * t * t,
        StandardEasing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
        StandardEasing::EaseInOutCubic => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        StandardEasing::EaseInQuart => t.powi(4),
        StandardEasing::EaseOutQuart => 1.0 - (1.0 - t).powi(4),
        StandardEasing::EaseInOutQuart => {
            if t < 0.5 {
                8.0 * t.powi(4)
            } else {
                1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
            }
        }
        StandardEasing::EaseInExpo => {
            if t == 0.0 {
                0.0
            } else {
                2f32.powf(10.0 * t - 10.0)
            }
        }
        StandardEasing::EaseOutExpo => {
            if t == 1.0 {
                1.0
            } else {
                1.0 - 2f32.powf(-10.0 * t)
            }
        }
        StandardEasing::EaseInOutExpo => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else if t < 0.5 {
                2f32.powf(20.0 * t - 10.0) / 2.0
            } else {
                (2.0 - 2f32.powf(-20.0 * t + 10.0)) / 2.0
            }
        }
        StandardEasing::EaseInBack => {
            const C1: f32 = 1.70158;
            const C3: f32 = C1 + 1.0;
            C3 * t * t * t - C1 * t * t
        }
        StandardEasing::EaseOutBack => {
            const C1: f32 = 1.70158;
            const C3: f32 = C1 + 1.0;
            let u = t - 1.0;
            1.0 + C3 * u * u * u + C1 * u * u
        }
        StandardEasing::EaseInOutBack => {
            const C2: f32 = 1.70158 * 1.525;
            if t < 0.5 {
                (2.0 * t).powi(2) * ((C2 + 1.0) * 2.0 * t - C2) / 2.0
            } else {
                let u = 2.0 * t - 2.0;
                (u * u * ((C2 + 1.0) * u + C2) + 2.0) / 2.0
            }
        }
        StandardEasing::EaseInBounce => 1.0 - bounce_out(1.0 - t),
        StandardEasing::EaseOutBounce => bounce_out(t),
        StandardEasing::EaseInOutBounce => {
            if t < 0.5 {
                (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
            } else {
                (1.0 + bounce_out(2.0 * t - 1.0)) / 2.0
            }
        }
        StandardEasing::Bezier { cp1, cp2 } => bezier_ease(t, cp1, cp2),
        StandardEasing::Random { seed, step } => {
            let step = step.max(1) as f32;
            let idx = (t * 16.0 / step).floor() as i64;
            random_unit(seed, idx)
        }
    }
}

pub struct DecodedKeyframe {
    pub frame: i32,
    pub value: f32,
    pub easing: StandardEasing,
}

/// FFI境界を越えないインプロセス呼び出し（`neoutl-easing-standard`をrlibとして
/// 直接依存するegui側エディタ）でも同一の符号化規則を使うための公開版。
pub fn parse_payload(slice: &[u8]) -> StandardEasing {
    if slice.is_empty() {
        return StandardEasing::Linear;
    }
    serde_json::from_slice(slice).unwrap_or(StandardEasing::Linear)
}

pub fn encode_payload(easing: StandardEasing) -> Vec<u8> {
    serde_json::to_vec(&easing).unwrap_or_default()
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
            if a.easing == StandardEasing::Step {
                return a.value;
            }
            let span = (b.frame - a.frame).max(1) as f32;
            let t = (frame - a.frame) as f32 / span;
            a.value + (b.value - a.value) * ease(a.easing, t)
        }
    }
}

/// サードパーティ.dll/.so向けFFI経路。ホスト(NeoUtl本体)は本クレートをrlibとして
/// 直接リンクしており、標準エンジンのカーブ編集UIは`src/ui/properties/easing_editor.rs`
/// が`StandardEasing`/`parse_payload`/`encode_payload`をインプロセス直接呼び出しする
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
            .map(|p| (p.frame, p.value, p.easing))
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
    let raw: Vec<(i32, f32, StandardEasing)> = serde_json::from_slice(slice).unwrap_or_default();

    let mut out_vec = Vec::with_capacity(raw.len());
    for (frame, value, easing) in raw {
        let payload = encode_payload(easing);
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
