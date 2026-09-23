use crate::csputils::{matrix_index, range_index};
use crate::format::describe;
use crate::graph::ConvertPlan;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SwscaleUniforms {
    pub color_matrix: u32,
    pub color_range: u32,
    pub bit_depth: u32,
    pub storage_bits: u32,
    pub src_width: u32,
    pub src_height: u32,
    pub dst_width: u32,
    pub dst_height: u32,
    pub tap_count_h: u32,
    pub tap_count_v: u32,
    pub _pad: [u32; 2],
}

pub fn pack_uniforms(plan: &ConvertPlan) -> SwscaleUniforms {
    let src_desc = describe(plan.src_fmt);
    let tap_count_h = plan
        .ops
        .iter()
        .find_map(|op| op.taps_h.as_ref().map(|t| t.tap_count as u32))
        .unwrap_or(1);
    let tap_count_v = plan
        .ops
        .iter()
        .find_map(|op| op.taps_v.as_ref().map(|t| t.tap_count as u32))
        .unwrap_or(1);

    SwscaleUniforms {
        color_matrix: matrix_index(plan.matrix),
        color_range: range_index(plan.full_range),
        bit_depth: src_desc.bit_depth,
        storage_bits: src_desc.storage_bits,
        src_width: plan.src_size.0,
        src_height: plan.src_size.1,
        dst_width: plan.dst_size.0,
        dst_height: plan.dst_size.1,
        tap_count_h,
        tap_count_v,
        _pad: [0, 0],
    }
}

pub fn pack_tap_weights(plan: &ConvertPlan) -> (Vec<f32>, Vec<f32>) {
    let h = plan
        .ops
        .iter()
        .find_map(|op| op.taps_h.as_ref().map(|t| t.weights.clone()))
        .unwrap_or_default();
    let v = plan
        .ops
        .iter()
        .find_map(|op| op.taps_v.as_ref().map(|t| t.weights.clone()))
        .unwrap_or_default();
    (h, v)
}
