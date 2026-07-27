#![allow(non_camel_case_types)]

pub use neoutl_shared_abi::{ParamKind, StrRef, WgslSource};
/// ParamSchema型自体はneoutl-object-apiと完全共有する（EffectParamSchemaという別名は導入しない）。
pub type EffectParamSchema = neoutl_shared_abi::ParamSchema;

#[repr(C)]
pub struct EffectMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub param_schema_ptr: *const EffectParamSchema,
    pub param_schema_len: usize,
}
unsafe impl Send for EffectMeta {}
unsafe impl Sync for EffectMeta {}

/// エフェクトシェーダの頂点契約はslang/effect_prelude.slangへ一本化した
/// （フルスクリーン三角形、@group(0)@binding(0)=入力テクスチャ、@binding(1)=サンプラーを固定契約とする）。
/// 各エフェクトクレートのbuild.rsがneoutl-effect-shader-build経由でこのファイルを
/// fs_main本体と連結しSPIR-Vへコンパイルするため、ランタイム側の文字列連結は不要になった。

#[repr(C)]
pub struct EffectVTable {
    pub meta: unsafe extern "C" fn() -> *const EffectMeta,
    pub wgsl: unsafe extern "C" fn() -> WgslSource,
    /// エフェクトUniforms構造体の必要バイト数（uniform_size_std(param_schema_len)と一致させること）。
    pub uniform_size: unsafe extern "C" fn() -> u32,
    /// params_ptr[0..count)をUniforms構造体のバイト表現へ詰める。
    /// 全実装はpack_uniform_stdへ委譲し、per-effect独自実装を持たない
    /// （@binding(2)のUniforms.pは`array<vec4<f32>, N>`で共有レイアウト固定のため）。
    pub pack_uniform: unsafe extern "C" fn(params_ptr: *const f32, count: u32, out_ptr: *mut u8),
}

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_effect_entry\0";
pub type EntryFn = unsafe extern "C" fn() -> *const EffectVTable;

/// `array<vec4<f32>, N>` レイアウト契約下でのUniformsバイト数（N = ceil(count/4) * 16）。
pub const fn uniform_size_std(count: u32) -> u32 {
    count.div_ceil(4) * 16
}

/// # Safety
/// params_ptrはcount個のf32を指し、out_ptrはuniform_size_std(count)バイト以上書き込み可能であること。
pub unsafe fn pack_uniform_std(params_ptr: *const f32, count: u32, out_ptr: *mut u8) {
    let total = uniform_size_std(count) as usize;
    unsafe {
        std::ptr::write_bytes(out_ptr, 0, total);
        let params = std::slice::from_raw_parts(params_ptr, count as usize);
        std::ptr::copy_nonoverlapping(params.as_ptr() as *const u8, out_ptr, params.len() * 4);
    }
}
