//! `0.5.x`系列限定の後方互換モジュール。次回メジャーバージョンで削除する。
//! `parse_payload`が新形式デコード失敗時のみ経由する読込専用パス。

use serde::{Deserialize, Serialize};

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
