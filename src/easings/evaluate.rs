use crate::easings::payload::{EasingPayload, ease, parse_payload};

struct Point {
    frame: i32,
    value: f32,
    easing: EasingPayload,
}

pub fn evaluate_track(keyframes: &[(i32, f32, Vec<u8>)], frame: i32, fallback: f32) -> f32 {
    let points: Vec<Point> = keyframes
        .iter()
        .map(|(f, v, payload)| Point {
            frame: *f,
            value: *v,
            easing: parse_payload(payload),
        })
        .collect();

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
