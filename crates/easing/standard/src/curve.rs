use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CurveKind {
    Linear,
    Bezier {
        handle_left: [f32; 2],
        handle_right: [f32; 2],
    },
    Bounce {
        cor: f32,
        period: f32,
        reversed: bool,
    },
    Elastic {
        amplitude: f32,
        frequency: f32,
        decay: f32,
        reversed: bool,
    },
    Standard {
        name: String,
    },
    Normal {
        segments: Vec<CurveSegment>,
    },
    Script {
        source: String,
    },
}

impl Default for CurveKind {
    fn default() -> Self {
        CurveKind::Linear
    }
}

impl CurveKind {
    pub fn label(&self) -> &'static str {
        match self {
            CurveKind::Linear => "Linear",
            CurveKind::Bezier { .. } => "Bezier",
            CurveKind::Bounce { .. } => "Bounce",
            CurveKind::Elastic { .. } => "Elastic",
            CurveKind::Standard { .. } => "Standard",
            CurveKind::Normal { .. } => "Normal",
            CurveKind::Script { .. } => "Script",
        }
    }

    pub fn default_bezier() -> Self {
        CurveKind::Bezier {
            handle_left: [0.42, 0.0],
            handle_right: [0.58, 1.0],
        }
    }

    pub fn default_bounce() -> Self {
        CurveKind::Bounce {
            cor: 0.5,
            period: 0.3,
            reversed: false,
        }
    }

    pub fn default_elastic() -> Self {
        CurveKind::Elastic {
            amplitude: 1.0,
            frequency: 3.0,
            decay: 6.0,
            reversed: false,
        }
    }

    pub fn default_normal() -> Self {
        CurveKind::Normal {
            segments: vec![CurveSegment {
                anchor_start: [0.0, 0.0],
                anchor_end: [1.0, 1.0],
                kind: CurveKind::Linear,
                modifiers: Vec::new(),
            }],
        }
    }

    pub fn default_script() -> Self {
        CurveKind::Script {
            source: "return t".to_owned(),
        }
    }

    pub fn standard(name: impl Into<String>) -> Self {
        CurveKind::Standard { name: name.into() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CurveSegment {
    pub anchor_start: [f32; 2],
    pub anchor_end: [f32; 2],
    pub kind: CurveKind,
    pub modifiers: Vec<Modifier>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Modifier {
    Discretization {
        sampling_resolution: u32,
        quantization_resolution: u32,
    },
    Noise {
        seed: i32,
        amplitude: f32,
        frequency: f32,
        phase: f32,
        octaves: i32,
        decay_sharpness: f32,
    },
    SineWave {
        amplitude: f32,
        frequency: f32,
        phase: f32,
    },
    SquareWave {
        amplitude: f32,
        frequency: f32,
        phase: f32,
        duty: f32,
    },
}

impl Modifier {
    pub fn label(&self) -> &'static str {
        match self {
            Modifier::Discretization { .. } => "Discretization",
            Modifier::Noise { .. } => "Noise",
            Modifier::SineWave { .. } => "SineWave",
            Modifier::SquareWave { .. } => "SquareWave",
        }
    }

    fn wrap(&self, base: &dyn Fn(f32) -> f32, t: f32) -> f32 {
        match self {
            Modifier::Discretization {
                sampling_resolution,
                quantization_resolution,
            } => {
                let s = (*sampling_resolution).max(1) as f32;
                let q = (*quantization_resolution).max(1) as f32;
                let sampled_t = (t * s).round() / s;
                let raw = base(sampled_t);
                (raw * q).round() / q
            }
            Modifier::Noise {
                seed,
                amplitude,
                frequency,
                phase,
                octaves,
                decay_sharpness,
            } => {
                let raw = base(t);
                let n = fbm_noise1(
                    *seed,
                    t * *frequency + *phase,
                    (*octaves).max(1) as u32,
                    *decay_sharpness,
                );
                raw + n * *amplitude
            }
            Modifier::SineWave {
                amplitude,
                frequency,
                phase,
            } => {
                let raw = base(t);
                raw + (std::f32::consts::TAU * (*frequency * t + *phase)).sin() * *amplitude
            }
            Modifier::SquareWave {
                amplitude,
                frequency,
                phase,
                duty,
            } => {
                let raw = base(t);
                let cycle = (*frequency * t + *phase).fract();
                let cycle = if cycle < 0.0 { cycle + 1.0 } else { cycle };
                let sq = if cycle < duty.clamp(0.0, 1.0) {
                    1.0
                } else {
                    -1.0
                };
                raw + sq * *amplitude
            }
        }
    }
}

pub fn apply_modifiers(
    base: impl Fn(f32) -> f32 + 'static,
    mods: &[Modifier],
) -> Box<dyn Fn(f32) -> f32> {
    mods.iter()
        .cloned()
        .fold(Box::new(base) as Box<dyn Fn(f32) -> f32>, |acc, m| {
            Box::new(move |t: f32| m.wrap(&acc, t))
        })
}

fn splitmix32(seed: u32) -> u32 {
    let mut z = seed.wrapping_add(0x9E3779B9);
    z = (z ^ (z >> 16)).wrapping_mul(0x85EBCA6B);
    z = (z ^ (z >> 13)).wrapping_mul(0xC2B2AE35);
    z ^ (z >> 16)
}

fn value_noise1(seed: i32, x: f32) -> f32 {
    let x0 = x.floor();
    let frac = x - x0;
    let hash = |i: i32| -> f32 {
        let combined = (seed as u32) ^ (i as u32).wrapping_mul(0x27D4_EB2F);
        (splitmix32(combined) as f64 / (u32::MAX as f64 + 1.0)) as f32 * 2.0 - 1.0
    };
    let a = hash(x0 as i32);
    let b = hash(x0 as i32 + 1);
    let u = frac * frac * (3.0 - 2.0 * frac);
    a + (b - a) * u
}

fn fbm_noise1(seed: i32, x: f32, octaves: u32, decay_sharpness: f32) -> f32 {
    let mut total = 0.0f32;
    let mut freq = 1.0f32;
    let mut amp = 1.0f32;
    let mut norm = 0.0f32;
    for i in 0..octaves {
        total += value_noise1(seed.wrapping_add(i as i32), x * freq) * amp;
        norm += amp;
        freq *= 2.0;
        amp *= (0.5f32).powf(decay_sharpness.max(0.01));
    }
    if norm > 0.0 { total / norm } else { 0.0 }
}

fn bezier_ease(t: f32, h1: [f32; 2], h2: [f32; 2]) -> f32 {
    let sample_x = |u: f32| {
        let mu = 1.0 - u;
        3.0 * mu * mu * u * h1[0] + 3.0 * mu * u * u * h2[0] + u * u * u
    };
    let sample_y = |u: f32| {
        let mu = 1.0 - u;
        3.0 * mu * mu * u * h1[1] + 3.0 * mu * u * u * h2[1] + u * u * u
    };
    let mut u = t;
    for _ in 0..8 {
        let mu = 1.0 - u;
        let err = sample_x(u) - t;
        if err.abs() < 1e-5 {
            break;
        }
        let dx =
            3.0 * mu * mu * h1[0] + 6.0 * mu * u * (h2[0] - h1[0]) + 3.0 * u * u * (1.0 - h2[0]);
        if dx.abs() < 1e-6 {
            break;
        }
        u -= err / dx;
    }
    sample_y(u.clamp(0.0, 1.0))
}

fn bounce_ease(t: f32, cor: f32, period: f32, reversed: bool) -> f32 {
    let t = if reversed { 1.0 - t } else { t };
    let cor = cor.clamp(0.01, 0.99);
    let period = period.max(0.01);
    let mut drop_start = 0.0f32;
    let mut amp = 1.0f32;
    let mut half_period = period;
    let mut y;
    loop {
        let seg_end = drop_start + half_period;
        if t <= seg_end || amp < 1e-4 {
            let local = (t - drop_start) / half_period.max(1e-6);
            let bounce_y = 1.0 - (2.0 * local - 1.0).powi(2);
            y = 1.0 - amp * (1.0 - bounce_y);
            break;
        }
        drop_start = seg_end;
        amp *= cor * cor;
        half_period *= cor;
    }
    y = y.clamp(0.0, 1.0);
    if reversed { 1.0 - y } else { y }
}

fn elastic_ease(t: f32, amplitude: f32, frequency: f32, decay: f32, reversed: bool) -> f32 {
    let t = if reversed { 1.0 - t } else { t };
    let envelope = (-decay * t).exp();
    let osc = (std::f32::consts::TAU * frequency * t).sin();
    let y = 1.0 - envelope * osc * amplitude;
    let y = y.clamp(-2.0, 2.0);
    if reversed { 1.0 - y } else { y }
}

pub fn evaluate_kind(kind: &CurveKind, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        CurveKind::Linear => t,
        CurveKind::Bezier {
            handle_left,
            handle_right,
        } => bezier_ease(t, *handle_left, *handle_right),
        CurveKind::Bounce {
            cor,
            period,
            reversed,
        } => bounce_ease(t, *cor, *period, *reversed),
        CurveKind::Elastic {
            amplitude,
            frequency,
            decay,
            reversed,
        } => elastic_ease(t, *amplitude, *frequency, *decay, *reversed),
        CurveKind::Standard { name } => evaluate_standard(name, t),
        CurveKind::Normal { segments } => evaluate_normal(segments, t),
        CurveKind::Script { source } => crate::script::evaluate(source, t).unwrap_or(t),
    }
}

fn evaluate_standard(name: &str, t: f32) -> f32 {
    if name == "linear" {
        return t;
    }
    let family = if name.contains("Sine") {
        1
    } else if name.contains("Quad") {
        2
    } else if name.contains("Cubic") {
        3
    } else if name.contains("Quart") {
        4
    } else if name.contains("Quint") {
        5
    } else if name.contains("Expo") {
        6
    } else if name.contains("Circ") {
        7
    } else {
        8
    };
    let base = |x: f32| match family {
        1 => 1.0 - (x * std::f32::consts::FRAC_PI_2).cos(),
        2 => x * x,
        3 => x * x * x,
        4 => x.powi(4),
        5 => x.powi(5),
        6 => {
            if x == 0.0 {
                0.0
            } else {
                (2.0_f32).powf(10.0 * x - 10.0)
            }
        }
        7 => 1.0 - (1.0 - x * x).sqrt(),
        _ => 2.70158 * x * x * x - 1.70158 * x * x,
    };
    if name.starts_with("easeInOut") {
        if t < 0.5 {
            base(t * 2.0) / 2.0
        } else {
            1.0 - base(2.0 - 2.0 * t) / 2.0
        }
    } else if name.starts_with("easeOutIn") {
        if t < 0.5 {
            (1.0 - base(1.0 - 2.0 * t)) / 2.0
        } else {
            base(2.0 * t - 1.0) / 2.0 + 0.5
        }
    } else if name.starts_with("easeOut") {
        1.0 - base(1.0 - t)
    } else {
        base(t)
    }
}

pub fn evaluate_kind_with_modifiers(kind: &CurveKind, mods: &[Modifier], t: f32) -> f32 {
    let base_t = t.clamp(0.0, 1.0);
    if mods.is_empty() {
        return evaluate_kind(kind, base_t);
    }
    let kind = kind.clone();
    apply_modifiers(move |u| evaluate_kind(&kind, u), mods)(base_t)
}

fn evaluate_normal(segments: &[CurveSegment], t: f32) -> f32 {
    if segments.is_empty() {
        return t;
    }
    if segments.len() == 1 {
        return evaluate_segment(&segments[0], t);
    }
    let idx = segments.partition_point(|s| s.anchor_end[0] < t);
    let idx = idx.min(segments.len() - 1);
    evaluate_segment(&segments[idx], t)
}

fn evaluate_segment(seg: &CurveSegment, t: f32) -> f32 {
    let x0 = seg.anchor_start[0];
    let x1 = seg.anchor_end[0];
    let span = (x1 - x0).max(1e-6);
    let local_t = ((t - x0) / span).clamp(0.0, 1.0);
    let y0 = seg.anchor_start[1];
    let y1 = seg.anchor_end[1];
    let local_y = evaluate_kind_with_modifiers(&seg.kind, &seg.modifiers, local_t);
    y0 + (y1 - y0) * local_y
}

pub fn add_segment(segments: &mut Vec<CurveSegment>, at_x: f32) {
    let insert_at = segments.partition_point(|s| s.anchor_start[0] < at_x);
    let (start, end) = if insert_at == 0 {
        (0.0, segments.first().map_or(1.0, |s| s.anchor_start[0]))
    } else if insert_at >= segments.len() {
        (segments.last().map_or(0.0, |s| s.anchor_end[0]), 1.0)
    } else {
        (
            segments[insert_at - 1].anchor_end[0],
            segments[insert_at].anchor_start[0],
        )
    };
    segments.insert(
        insert_at,
        CurveSegment {
            anchor_start: [start, 0.0],
            anchor_end: [end, 1.0],
            kind: CurveKind::Linear,
            modifiers: Vec::new(),
        },
    );
}

pub fn drag_anchor_x(segments: &mut [CurveSegment], boundary_index: usize, new_x: f32) {
    if boundary_index == 0 || boundary_index >= segments.len() {
        return;
    }
    let lower = segments[boundary_index - 1].anchor_start[0] + 1e-3;
    let upper = segments
        .get(boundary_index + 1)
        .map_or(1.0, |s| s.anchor_end[0] - 1e-3);
    let clamped = new_x.clamp(lower, upper);
    segments[boundary_index - 1].anchor_end[0] = clamped;
    segments[boundary_index].anchor_start[0] = clamped;
}

pub fn remove_segment(segments: &mut Vec<CurveSegment>, index: usize) {
    if segments.len() > 1 && index < segments.len() {
        segments.remove(index);
    }
}

pub fn replace_segment_kind(segments: &mut [CurveSegment], index: usize, kind: CurveKind) {
    if let Some(seg) = segments.get_mut(index) {
        seg.kind = kind;
    }
}
