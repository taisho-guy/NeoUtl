//! 中間点（キーフレーム）補間の純粋計算クレート。
//!
//! ECS・UI・永続化のいずれにも依存しない（`f32`と`i32`のみを扱う）。
//! 評価はShipyardシステム側（描画直前）でのみ行う。UI層はここを呼び出さない。
#![forbid(unsafe_code)]

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// 区間補間の種別。AviUtl系エディタの標準イージング一式を網羅する。
/// `Bezier`のみ制御点を持つ可変長ヴァリアント。
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Easing {
    Linear,
    /// 次の中間点まで値を保持し、到達点で瞬時に切り替える（無補間）。
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
    /// 3次ベジェイージング（CSS `cubic-bezier()`と同一定義）。
    Bezier {
        cp1: (f32, f32),
        cp2: (f32, f32),
    },
    /// 疑似乱数補間。同一`seed`・同一区間内位置は常に同一値を返す（再生毎に値が
    /// 変わる真の乱数は不採用、Undo再生・レンダリング再現性を破壊するため）。
    /// `step`は区間内を何分割の階段として量子化するかを表す（AviUtl系の
    /// 「ランダム移動」のコマ送り幅に相当）。1未満は1へ丸める。
    Random {
        seed: u32,
        step: i32,
    },
}

impl Default for Easing {
    fn default() -> Self {
        Easing::Linear
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

/// 3次ベジェのX(時間)からY(進捗)を求める。ニュートン法で t を解いてから Y を評価する。
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

/// 32bit整数から32bit整数への決定論的拡散関数（splitmix32）。
/// 同一入力は常に同一出力を返す（システム時刻・スレッド状態等の外部要因に非依存）。
fn splitmix32(seed: u32) -> u32 {
    let mut z = seed.wrapping_add(0x9E3779B9);
    z = (z ^ (z >> 16)).wrapping_mul(0x85EBCA6B);
    z = (z ^ (z >> 13)).wrapping_mul(0xC2B2AE35);
    z ^ (z >> 16)
}

/// `seed`と区間内の量子化インデックス`idx`から[0, 1)の値を1個決定論的に導く。
fn random_unit(seed: u32, idx: i64) -> f32 {
    let idx_bits = (idx as i64 as u64 as u32).wrapping_mul(0x27D4_EB2F);
    let combined = seed ^ idx_bits;
    (splitmix32(combined) as f64 / (u32::MAX as f64 + 1.0)) as f32
}

/// t(区間内進捗, 0..1)を補間種別に応じたイージング後の進捗へ変換する。
pub fn ease(kind: Easing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        Easing::Linear => t,
        Easing::Step => 0.0,
        Easing::EaseInSine => 1.0 - (t * std::f32::consts::FRAC_PI_2).cos(),
        Easing::EaseOutSine => (t * std::f32::consts::FRAC_PI_2).sin(),
        Easing::EaseInOutSine => -((std::f32::consts::PI * t).cos() - 1.0) / 2.0,
        Easing::EaseInQuad => t * t,
        Easing::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
        Easing::EaseInOutQuad => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        }
        Easing::EaseInCubic => t * t * t,
        Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
        Easing::EaseInOutCubic => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        Easing::EaseInQuart => t.powi(4),
        Easing::EaseOutQuart => 1.0 - (1.0 - t).powi(4),
        Easing::EaseInOutQuart => {
            if t < 0.5 {
                8.0 * t.powi(4)
            } else {
                1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
            }
        }
        Easing::EaseInExpo => {
            if t == 0.0 {
                0.0
            } else {
                2f32.powf(10.0 * t - 10.0)
            }
        }
        Easing::EaseOutExpo => {
            if t == 1.0 {
                1.0
            } else {
                1.0 - 2f32.powf(-10.0 * t)
            }
        }
        Easing::EaseInOutExpo => {
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
        Easing::EaseInBack => {
            const C1: f32 = 1.70158;
            const C3: f32 = C1 + 1.0;
            C3 * t * t * t - C1 * t * t
        }
        Easing::EaseOutBack => {
            const C1: f32 = 1.70158;
            const C3: f32 = C1 + 1.0;
            let u = t - 1.0;
            1.0 + C3 * u * u * u + C1 * u * u
        }
        Easing::EaseInOutBack => {
            const C2: f32 = 1.70158 * 1.525;
            if t < 0.5 {
                (2.0 * t).powi(2) * ((C2 + 1.0) * 2.0 * t - C2) / 2.0
            } else {
                let u = 2.0 * t - 2.0;
                (u * u * ((C2 + 1.0) * u + C2) + 2.0) / 2.0
            }
        }
        Easing::EaseInBounce => 1.0 - bounce_out(1.0 - t),
        Easing::EaseOutBounce => bounce_out(t),
        Easing::EaseInOutBounce => {
            if t < 0.5 {
                (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
            } else {
                (1.0 + bounce_out(2.0 * t - 1.0)) / 2.0
            }
        }
        Easing::Bezier { cp1, cp2 } => bezier_ease(t, cp1, cp2),
        Easing::Random { seed, step } => {
            let step = step.max(1) as f32;
            let idx = (t * 16.0 / step).floor() as i64;
            random_unit(seed, idx)
        }
    }
}

/// タイムライン上の1中間点。`easing`はこの点から次の点への区間補間種別を持つ
/// （AviUtl系エディタと同じく「左点が区間の補間方式を決める」方式）。
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Keyframe {
    pub frame: i32,
    pub value: f32,
    pub easing: Easing,
}

/// `points`（frame昇順であること）と現在フレームから実効値を求める。
/// 空: fallback。範囲外: 端点の値でクランプ。区間内: 左点のeasingで補間。
pub fn evaluate(points: &[Keyframe], frame: i32, fallback: f32) -> f32 {
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
            for pair in points.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                if frame >= a.frame && frame <= b.frame {
                    if a.easing == Easing::Step {
                        return a.value;
                    }
                    let span = (b.frame - a.frame).max(1) as f32;
                    let t = (frame - a.frame) as f32 / span;
                    return a.value + (b.value - a.value) * ease(a.easing, t);
                }
            }
            last.value
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_fallback() {
        assert_eq!(evaluate(&[], 10, 5.0), 5.0);
    }

    #[test]
    fn single_point_is_constant() {
        let kf = [Keyframe {
            frame: 5,
            value: 3.0,
            easing: Easing::Linear,
        }];
        assert_eq!(evaluate(&kf, 0, 0.0), 3.0);
        assert_eq!(evaluate(&kf, 100, 0.0), 3.0);
    }

    #[test]
    fn linear_midpoint() {
        let kf = [
            Keyframe {
                frame: 0,
                value: 0.0,
                easing: Easing::Linear,
            },
            Keyframe {
                frame: 10,
                value: 100.0,
                easing: Easing::Linear,
            },
        ];
        assert_eq!(evaluate(&kf, 5, 0.0), 50.0);
        assert_eq!(evaluate(&kf, -5, 0.0), 0.0);
        assert_eq!(evaluate(&kf, 15, 0.0), 100.0);
    }

    #[test]
    fn random_seed_determinism() {
        let kind = Easing::Random { seed: 1, step: 1 };
        let a = ease(kind, 0.37);
        let b = ease(kind, 0.37);
        assert_eq!(a, b);
        assert!((0.0..1.0).contains(&a));
    }

    #[test]
    fn random_seed_changes_output() {
        let t = 0.37;
        let a = ease(Easing::Random { seed: 1, step: 1 }, t);
        let b = ease(Easing::Random { seed: 2, step: 1 }, t);
        assert_ne!(a, b);
    }

    #[test]
    fn step_holds_until_next_point() {
        let kf = [
            Keyframe {
                frame: 0,
                value: 1.0,
                easing: Easing::Step,
            },
            Keyframe {
                frame: 10,
                value: 9.0,
                easing: Easing::Linear,
            },
        ];
        assert_eq!(evaluate(&kf, 9, 0.0), 1.0);
        assert_eq!(evaluate(&kf, 10, 0.0), 9.0);
    }

    #[test]
    fn ease_bounds_are_stable() {
        for kind in [
            Easing::EaseOutBounce,
            Easing::EaseInExpo,
            Easing::EaseInOutBack,
        ] {
            assert!((ease(kind, 0.0) - 0.0).abs() < 1e-3);
            assert!((ease(kind, 1.0) - 1.0).abs() < 1e-3);
        }
    }
}
