use egui::{Pos2, Rect, Vec2, pos2};
use neoutl_easing_standard::{CurveKind, EasingPayload, ease};

pub struct CurveView {
    origin: Pos2,
    scale: f32,
}

impl CurveView {
    pub const PAD: f32 = 28.0;

    pub fn fit(rect: Rect, y_lo: f32, y_hi: f32, zoom: f32, pan: Vec2) -> Self {
        let inner = rect.shrink(Self::PAD);
        let scale = inner.width().min(inner.height() / (y_hi - y_lo)) * zoom;
        Self {
            origin: pos2(
                inner.center().x - 0.5 * scale + pan.x,
                inner.center().y + 0.5 * (y_lo + y_hi) * scale + pan.y,
            ),
            scale,
        }
    }

    pub fn to_screen(&self, x: f32, y: f32) -> Pos2 {
        pos2(
            self.origin.x + x * self.scale,
            self.origin.y - y * self.scale,
        )
    }

    pub fn to_data(&self, p: Pos2) -> (f32, f32) {
        (
            (p.x - self.origin.x) / self.scale,
            (self.origin.y - p.y) / self.scale,
        )
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }
}

pub fn y_range(samples: &[[f32; 2]], handle_ys: impl Iterator<Item = f32>) -> (f32, f32) {
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    for y in samples.iter().map(|p| p[1]).chain(handle_ys) {
        if y.is_finite() {
            lo = lo.min(y);
            hi = hi.max(y);
        }
    }
    ((lo - 0.12).max(-4.0), (hi + 0.12).min(5.0))
}

pub fn sample(payload: &EasingPayload, resolution: usize) -> Vec<[f32; 2]> {
    if payload.modifiers.is_empty()
        && let CurveKind::Bezier {
            handle_left: l,
            handle_right: r,
        } = &payload.kind
    {
        let bez = kurbo::CubicBez::new(
            (0.0, 0.0),
            (l[0] as f64, l[1] as f64),
            (r[0] as f64, r[1] as f64),
            (1.0, 1.0),
        );
        return (0..=resolution)
            .map(|i| {
                let p = kurbo::ParamCurve::eval(&bez, i as f64 / resolution as f64);
                [p.x as f32, p.y as f32]
            })
            .collect();
    }
    (0..=resolution)
        .map(|i| {
            let t = i as f32 / resolution as f32;
            [t, ease(payload, t)]
        })
        .collect()
}

pub fn interpolate(samples: &[[f32; 2]], t: f32) -> f32 {
    let n = samples.len();
    if n < 2 {
        return t;
    }
    let f = t.clamp(0.0, 1.0) * (n - 1) as f32;
    let i = (f as usize).min(n - 2);
    let u = f - i as f32;
    samples[i][1] + (samples[i + 1][1] - samples[i][1]) * u
}

pub fn fmt(v: f32) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

pub fn grid_step(scale: f32) -> f32 {
    [0.05, 0.1, 0.25, 0.5, 1.0, 2.0]
        .into_iter()
        .find(|s| s * scale >= 32.0)
        .unwrap_or(2.0)
}
