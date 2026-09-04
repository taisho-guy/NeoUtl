use neo_media_core::{MatrixCoefficients, PixelFormat};

use crate::filters::{build_taps, FilterKind, FilterTaps};
use crate::format::describe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    Read,
    ScaleH,
    ScaleV,
    Convert,
    Write,
}

pub struct OpNode {
    pub kind: OpKind,
    pub taps_h: Option<FilterTaps>,
    pub taps_v: Option<FilterTaps>,
}

pub struct ConvertPlan {
    pub src_fmt: PixelFormat,
    pub dst_fmt: PixelFormat,
    pub src_size: (u32, u32),
    pub dst_size: (u32, u32),
    pub matrix: MatrixCoefficients,
    pub full_range: bool,
    pub ops: Vec<OpNode>,
}

pub fn build_plan(
    src_fmt: PixelFormat,
    dst_fmt: PixelFormat,
    src_size: (u32, u32),
    dst_size: (u32, u32),
    matrix: MatrixCoefficients,
    full_range: bool,
    filter: FilterKind,
) -> ConvertPlan {
    let mut ops = vec![OpNode {
        kind: OpKind::Read,
        taps_h: None,
        taps_v: None,
    }];

    let ratio_h = dst_size.0 as f32 / src_size.0 as f32;
    let ratio_v = dst_size.1 as f32 / src_size.1 as f32;

    if (ratio_h - 1.0).abs() > 1e-4 {
        ops.push(OpNode {
            kind: OpKind::ScaleH,
            taps_h: Some(build_taps(filter, ratio_h)),
            taps_v: None,
        });
    }
    if (ratio_v - 1.0).abs() > 1e-4 {
        ops.push(OpNode {
            kind: OpKind::ScaleV,
            taps_h: None,
            taps_v: Some(build_taps(filter, ratio_v)),
        });
    }

    let src_desc = describe(src_fmt);
    let dst_desc = describe(dst_fmt);
    if src_desc.is_rgb != dst_desc.is_rgb || src_fmt != dst_fmt {
        ops.push(OpNode {
            kind: OpKind::Convert,
            taps_h: None,
            taps_v: None,
        });
    }

    ops.push(OpNode {
        kind: OpKind::Write,
        taps_h: None,
        taps_v: None,
    });

    ConvertPlan {
        src_fmt,
        dst_fmt,
        src_size,
        dst_size,
        matrix,
        full_range,
        ops,
    }
}

pub fn is_identity(plan: &ConvertPlan) -> bool {
    plan.ops.len() == 2
}
