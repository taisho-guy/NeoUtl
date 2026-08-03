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

fn parse_payload(slice: &[u8]) -> StandardEasing {
    if slice.is_empty() {
        return StandardEasing::Linear;
    }
    serde_json::from_slice(slice).unwrap_or(StandardEasing::Linear)
}

fn encode_payload(easing: StandardEasing) -> Vec<u8> {
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

#[cfg(any())]
fn easing_kind_names() -> Vec<String> {
    [
        "Linear",
        "Step",
        "EaseInSine",
        "EaseOutSine",
        "EaseInOutSine",
        "EaseInQuad",
        "EaseOutQuad",
        "EaseInOutQuad",
        "EaseInCubic",
        "EaseOutCubic",
        "EaseInOutCubic",
        "EaseInQuart",
        "EaseOutQuart",
        "EaseInOutQuart",
        "EaseInExpo",
        "EaseOutExpo",
        "EaseInOutExpo",
        "EaseInBack",
        "EaseOutBack",
        "EaseInOutBack",
        "EaseInBounce",
        "EaseOutBounce",
        "EaseInOutBounce",
        "Bezier",
        "Random",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[cfg(any())]
fn easing_kind_index(kind: StandardEasing) -> i32 {
    match kind {
        StandardEasing::Linear => 0,
        StandardEasing::Step => 1,
        StandardEasing::EaseInSine => 2,
        StandardEasing::EaseOutSine => 3,
        StandardEasing::EaseInOutSine => 4,
        StandardEasing::EaseInQuad => 5,
        StandardEasing::EaseOutQuad => 6,
        StandardEasing::EaseInOutQuad => 7,
        StandardEasing::EaseInCubic => 8,
        StandardEasing::EaseOutCubic => 9,
        StandardEasing::EaseInOutCubic => 10,
        StandardEasing::EaseInQuart => 11,
        StandardEasing::EaseOutQuart => 12,
        StandardEasing::EaseInOutQuart => 13,
        StandardEasing::EaseInExpo => 14,
        StandardEasing::EaseOutExpo => 15,
        StandardEasing::EaseInOutExpo => 16,
        StandardEasing::EaseInBack => 17,
        StandardEasing::EaseOutBack => 18,
        StandardEasing::EaseInOutBack => 19,
        StandardEasing::EaseInBounce => 20,
        StandardEasing::EaseOutBounce => 21,
        StandardEasing::EaseInOutBounce => 22,
        StandardEasing::Bezier { .. } => 23,
        StandardEasing::Random { .. } => 24,
    }
}

#[cfg(any())]
fn easing_kind_from_index(
    index: i32,
    cp1: (f32, f32),
    cp2: (f32, f32),
    seed: u32,
    step: i32,
) -> StandardEasing {
    match index {
        0 => StandardEasing::Linear,
        1 => StandardEasing::Step,
        2 => StandardEasing::EaseInSine,
        3 => StandardEasing::EaseOutSine,
        4 => StandardEasing::EaseInOutSine,
        5 => StandardEasing::EaseInQuad,
        6 => StandardEasing::EaseOutQuad,
        7 => StandardEasing::EaseInOutQuad,
        8 => StandardEasing::EaseInCubic,
        9 => StandardEasing::EaseOutCubic,
        10 => StandardEasing::EaseInOutCubic,
        11 => StandardEasing::EaseInQuart,
        12 => StandardEasing::EaseOutQuart,
        13 => StandardEasing::EaseInOutQuart,
        14 => StandardEasing::EaseInExpo,
        15 => StandardEasing::EaseOutExpo,
        16 => StandardEasing::EaseInOutExpo,
        17 => StandardEasing::EaseInBack,
        18 => StandardEasing::EaseOutBack,
        19 => StandardEasing::EaseInOutBack,
        20 => StandardEasing::EaseInBounce,
        21 => StandardEasing::EaseOutBounce,
        22 => StandardEasing::EaseInOutBounce,
        23 => StandardEasing::Bezier { cp1, cp2 },
        24 => StandardEasing::Random { seed, step },
        _ => StandardEasing::Linear,
    }
}

#[cfg(any())]
fn preview_path_commands(kind: StandardEasing) -> String {
    const SAMPLES: i32 = 24;
    let mut cmd = String::new();
    for i in 0..=SAMPLES {
        let t = i as f32 / SAMPLES as f32;
        let v = ease(kind, t);
        let y = 1.0 - v.clamp(-1.0, 2.0).mul_add(0.5, 0.5).clamp(0.0, 1.0);
        cmd.push_str(if i == 0 { "M " } else { "L " });
        cmd.push_str(&format!("{t} {y} "));
    }
    cmd
}

#[cfg(any())]
unsafe extern "C" fn open_editor_window_c(
    _host_handle: *const c_void,
    keyframes_ptr: *const KeyframeC,
    count: usize,
    on_complete: unsafe extern "C" fn(*mut c_void, EditResultC),
    user_data: *mut c_void,
) {
    let points = unsafe { decode_keyframes(keyframes_ptr, count) };
    let Some(target) = points.first() else {
        unsafe {
            on_complete(
                user_data,
                EditResultC {
                    code: EditResultCode::Cancel,
                    keyframes_ptr: std::ptr::null_mut(),
                    count: 0,
                },
            );
        }
        return;
    };

    let Ok(window) = KeyframeEditorWindow::new() else {
        unsafe {
            on_complete(
                user_data,
                EditResultC {
                    code: EditResultCode::Cancel,
                    keyframes_ptr: std::ptr::null_mut(),
                    count: 0,
                },
            );
        }
        return;
    };

    let initial_kind = target.easing;
    let initial_frame = target.frame;
    let end_frame = points.get(1).map_or(target.frame, |k| k.frame);

    window.set_easing_names(ModelRc::new(VecModel::from(easing_kind_names())));
    window.set_easing_kind(easing_kind_index(initial_kind));
    window.set_segment_start_frame(initial_frame);
    window.set_segment_end_frame(end_frame);
    window.set_preview_path_commands(preview_path_commands(initial_kind).into());
    if let StandardEasing::Bezier { cp1, cp2 } = initial_kind {
        window.set_bezier_cp1_x(cp1.0);
        window.set_bezier_cp1_y(cp1.1);
        window.set_bezier_cp2_x(cp2.0);
        window.set_bezier_cp2_y(cp2.1);
    }
    if let StandardEasing::Random { seed, step } = initial_kind {
        window.set_random_seed(seed as i32);
        window.set_random_step(step);
    }

    let refresh_preview = {
        let window_weak = window.as_weak();
        move || {
            let Some(w) = window_weak.upgrade() else {
                return;
            };
            let kind = easing_kind_from_index(
                w.get_easing_kind(),
                (w.get_bezier_cp1_x(), w.get_bezier_cp1_y()),
                (w.get_bezier_cp2_x(), w.get_bezier_cp2_y()),
                w.get_random_seed().max(0) as u32,
                w.get_random_step().max(1),
            );
            w.set_preview_path_commands(preview_path_commands(kind).into());
        }
    };

    {
        let refresh_preview = refresh_preview.clone();
        window.on_easing_changed(move |_| refresh_preview());
    }
    {
        let refresh_preview = refresh_preview.clone();
        window.on_bezier_changed(move || refresh_preview());
    }
    {
        let refresh_preview = refresh_preview.clone();
        window.on_random_changed(move || refresh_preview());
    }

    let user_data_addr = user_data as usize;
    let payload_start = target.frame;
    let remaining_c: Vec<KeyframeC> = unsafe {
        std::slice::from_raw_parts(keyframes_ptr, count)
            .iter()
            .skip(1)
            .map(|k| KeyframeC {
                frame: k.frame,
                value: k.value,
                payload_ptr: k.payload_ptr,
                payload_len: k.payload_len,
            })
            .collect()
    };
    let target_value = target.value;

    {
        let window_weak = window.as_weak();
        window.on_apply(move || {
            let Some(w) = window_weak.upgrade() else {
                return;
            };
            let kind = easing_kind_from_index(
                w.get_easing_kind(),
                (w.get_bezier_cp1_x(), w.get_bezier_cp1_y()),
                (w.get_bezier_cp2_x(), w.get_bezier_cp2_y()),
                w.get_random_seed().max(0) as u32,
                w.get_random_step().max(1),
            );
            let payload = encode_payload(kind).into_boxed_slice();
            let payload_len = payload.len();
            let payload_ptr = Box::into_raw(payload) as *mut u8;

            let mut out = vec![KeyframeC {
                frame: payload_start,
                value: target_value,
                payload_ptr,
                payload_len,
            }];
            for k in &remaining_c {
                let dup_payload = if k.payload_ptr.is_null() || k.payload_len == 0 {
                    (std::ptr::null_mut(), 0)
                } else {
                    let slice = unsafe { std::slice::from_raw_parts(k.payload_ptr, k.payload_len) };
                    let mut b = slice.to_vec().into_boxed_slice();
                    let p = b.as_mut_ptr();
                    let l = b.len();
                    std::mem::forget(b);
                    (p, l)
                };
                out.push(KeyframeC {
                    frame: k.frame,
                    value: k.value,
                    payload_ptr: dup_payload.0,
                    payload_len: dup_payload.1,
                });
            }

            let len = out.len();
            let mut boxed = out.into_boxed_slice();
            let ptr = boxed.as_mut_ptr();
            std::mem::forget(boxed);

            let _ = w.hide();
            unsafe {
                on_complete(
                    user_data_addr as *mut c_void,
                    EditResultC {
                        code: EditResultCode::Success,
                        keyframes_ptr: ptr,
                        count: len,
                    },
                );
            }
        });
    }
    {
        let window_weak = window.as_weak();
        window.on_cancel(move || {
            if let Some(w) = window_weak.upgrade() {
                let _ = w.hide();
            }
            unsafe {
                on_complete(
                    user_data_addr as *mut c_void,
                    EditResultC {
                        code: EditResultCode::Cancel,
                        keyframes_ptr: std::ptr::null_mut(),
                        count: 0,
                    },
                );
            }
        });
    }

    let _ = window.show();
    std::mem::forget(window);
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
