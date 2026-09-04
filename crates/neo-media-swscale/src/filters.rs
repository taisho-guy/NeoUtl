#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterKind {
    Bilinear,
    Bicubic,
    Lanczos3,
}

pub struct FilterTaps {
    pub offsets: Vec<i32>,
    pub weights: Vec<f32>,
    pub tap_count: usize,
}

fn sinc(x: f32) -> f32 {
    if x.abs() < 1e-6 {
        1.0
    } else {
        (std::f32::consts::PI * x).sin() / (std::f32::consts::PI * x)
    }
}

fn lanczos3(x: f32) -> f32 {
    if x.abs() >= 3.0 {
        0.0
    } else {
        sinc(x) * sinc(x / 3.0)
    }
}

fn bicubic(x: f32) -> f32 {
    let a = -0.5;
    let x = x.abs();
    if x <= 1.0 {
        (a + 2.0) * x.powi(3) - (a + 3.0) * x.powi(2) + 1.0
    } else if x < 2.0 {
        a * x.powi(3) - 5.0 * a * x.powi(2) + 8.0 * a * x - 4.0 * a
    } else {
        0.0
    }
}

fn bilinear(x: f32) -> f32 {
    let x = x.abs();
    if x < 1.0 {
        1.0 - x
    } else {
        0.0
    }
}

pub fn build_taps(kind: FilterKind, scale_ratio: f32) -> FilterTaps {
    let radius = match kind {
        FilterKind::Bilinear => 1,
        FilterKind::Bicubic => 2,
        FilterKind::Lanczos3 => 3,
    };
    let support = if scale_ratio < 1.0 {
        (radius as f32 / scale_ratio).ceil() as i32
    } else {
        radius
    };
    let mut offsets = Vec::new();
    let mut weights = Vec::new();
    let step = if scale_ratio < 1.0 { scale_ratio } else { 1.0 };
    let mut sum = 0.0f32;
    for i in -support..=support {
        let x = i as f32 * step;
        let w = match kind {
            FilterKind::Bilinear => bilinear(x),
            FilterKind::Bicubic => bicubic(x),
            FilterKind::Lanczos3 => lanczos3(x),
        };
        offsets.push(i);
        weights.push(w);
        sum += w;
    }
    if sum.abs() > 1e-6 {
        for w in weights.iter_mut() {
            *w /= sum;
        }
    }
    let tap_count = offsets.len();
    FilterTaps {
        offsets,
        weights,
        tap_count,
    }
}
