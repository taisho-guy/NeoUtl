use neoutl_easing_standard::{CurveKind, CurveSegment};

pub const SNAP_STEP: f32 = 0.25;
const AMP_EDGE_INSET: f32 = 0.08;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PointRef {
    BezierLeft,
    BezierRight,
    BounceHandle,
    ElasticAmp,
    ElasticFreqDecay,
    NormalBoundary(usize),
}

pub struct ControlPoint {
    pub point: PointRef,
    pub pos: [f32; 2],
    pub label: String,
}

pub fn control_points(kind: &CurveKind) -> Vec<ControlPoint> {
    use neoutl_easing_standard as es;
    let cp = |point, pos: [f32; 2], label: String| ControlPoint { point, pos, label };
    match kind {
        CurveKind::Bezier {
            handle_left: l,
            handle_right: r,
        } => vec![
            cp(
                PointRef::BezierLeft,
                *l,
                format!("P1 ({:.2}, {:.2})", l[0], l[1]),
            ),
            cp(
                PointRef::BezierRight,
                *r,
                format!("P2 ({:.2}, {:.2})", r[0], r[1]),
            ),
        ],
        CurveKind::Bounce {
            cor,
            period,
            reversed,
        } => {
            let (x, y) = es::bounce_handle(*cor, *period, *reversed);
            vec![cp(
                PointRef::BounceHandle,
                [x, y],
                format!("反発 {cor:.2} / 周期 {period:.2}"),
            )]
        }
        CurveKind::Elastic {
            amplitude,
            frequency,
            decay,
            reversed,
        } => {
            let amp_x = if *reversed {
                1.0 - AMP_EDGE_INSET
            } else {
                AMP_EDGE_INSET
            };
            let (fx, fy) = es::elastic_freq_decay_handle(*frequency, *decay, *reversed);
            vec![
                cp(
                    PointRef::ElasticAmp,
                    [amp_x, es::elastic_amp_handle_y(*amplitude)],
                    format!("振幅 {amplitude:.2}"),
                ),
                cp(
                    PointRef::ElasticFreqDecay,
                    [fx, fy],
                    format!("周波数 {frequency:.2} / 減衰 {decay:.2}"),
                ),
            ]
        }
        CurveKind::Normal { segments } => segments
            .iter()
            .enumerate()
            .take(segments.len().saturating_sub(1))
            .map(|(i, s)| {
                cp(
                    PointRef::NormalBoundary(i),
                    s.anchor_end,
                    format!(
                        "頂点{} ({:.2}, {:.2})",
                        i + 1,
                        s.anchor_end[0],
                        s.anchor_end[1]
                    ),
                )
            })
            .collect(),
        CurveKind::Linear | CurveKind::Standard { .. } | CurveKind::Script { .. } => Vec::new(),
    }
}

fn snap(v: f32, on: bool) -> f32 {
    if on {
        (v / SNAP_STEP).round() * SNAP_STEP
    } else {
        v
    }
}

pub fn apply_drag(kind: &mut CurveKind, point: PointRef, data: (f32, f32), mods: egui::Modifiers) {
    use neoutl_easing_standard as es;
    let (px, py) = (snap(data.0, mods.shift), snap(data.1, mods.shift));
    match (kind, point) {
        (
            CurveKind::Bezier {
                handle_left: l,
                handle_right: r,
            },
            p @ (PointRef::BezierLeft | PointRef::BezierRight),
        ) => {
            let (moved, other) = if p == PointRef::BezierLeft {
                (l, r)
            } else {
                (r, l)
            };
            *moved = [px.clamp(0.0, 1.0), py.clamp(-2.0, 3.0)];
            if mods.command {
                *other = [1.0 - moved[0], 1.0 - moved[1]];
            }
        }
        (
            CurveKind::Bounce {
                cor,
                period,
                reversed,
            },
            PointRef::BounceHandle,
        ) => {
            (*cor, *period) = es::bounce_set_handle(px, py, *reversed);
        }
        (CurveKind::Elastic { amplitude, .. }, PointRef::ElasticAmp) => {
            *amplitude = es::elastic_set_amp(py);
        }
        (
            CurveKind::Elastic {
                frequency,
                decay,
                reversed,
                ..
            },
            PointRef::ElasticFreqDecay,
        ) => (*frequency, *decay) = es::elastic_set_freq_decay(px, py, *reversed),
        (CurveKind::Normal { segments }, PointRef::NormalBoundary(i)) => {
            set_boundary(segments, i, px.clamp(0.0, 1.0), py.clamp(-2.0, 3.0))
        }
        _ => {}
    }
}

pub fn set_boundary(segments: &mut [CurveSegment], i: usize, x: f32, y: f32) {
    neoutl_easing_standard::drag_anchor_x(segments, i + 1, x);
    if i + 1 < segments.len() {
        segments[i].anchor_end[1] = y;
        segments[i + 1].anchor_start[1] = y;
    }
}

pub fn nearest(
    points: &[ControlPoint],
    to_screen: impl Fn([f32; 2]) -> egui::Pos2,
    at: egui::Pos2,
    radius: f32,
) -> Option<usize> {
    points
        .iter()
        .map(|p| to_screen(p.pos).distance(at))
        .enumerate()
        .filter(|(_, d)| *d <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

pub fn reset_label(kind: &CurveKind) -> &'static str {
    if matches!(kind, CurveKind::Normal { .. }) {
        "頂点を結合 (削除)"
    } else {
        "ハンドルを初期値へ"
    }
}

pub fn reset_point(kind: &mut CurveKind, point: PointRef) -> bool {
    match (kind, point) {
        (CurveKind::Bezier { handle_left, .. }, PointRef::BezierLeft) => *handle_left = [0.42, 0.0],
        (CurveKind::Bezier { handle_right, .. }, PointRef::BezierRight) => {
            *handle_right = [0.58, 1.0]
        }
        (CurveKind::Bounce { cor, period, .. }, PointRef::BounceHandle) => {
            (*cor, *period) = (0.6, 0.5)
        }
        (CurveKind::Elastic { amplitude, .. }, PointRef::ElasticAmp) => *amplitude = 1.0,
        (
            CurveKind::Elastic {
                frequency, decay, ..
            },
            PointRef::ElasticFreqDecay,
        ) => (*frequency, *decay) = (5.0, 6.0),
        (CurveKind::Normal { segments }, PointRef::NormalBoundary(i)) => {
            if segments.len() <= 1 || i + 1 >= segments.len() {
                return false;
            }
            segments[i].anchor_end = segments[i + 1].anchor_end;
            segments.remove(i + 1);
        }
        _ => return false,
    }
    true
}
