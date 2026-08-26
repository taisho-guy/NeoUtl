pub use neoutl_shared_abi::{ParamKind, StrRef, WgslSource};
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

#[repr(C)]
pub struct EffectVTable {
    pub meta: unsafe extern "C" fn() -> *const EffectMeta,
    pub wgsl: unsafe extern "C" fn() -> WgslSource,
    pub uniform_size: unsafe extern "C" fn() -> u32,
    pub pack_uniform: unsafe extern "C" fn(params_ptr: *const f32, count: u32, out_ptr: *mut u8),
    pub requires_texture_param: Option<unsafe extern "C" fn() -> u32>,
}

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_effect_entry\0";
pub type EntryFn = unsafe extern "C" fn() -> *const EffectVTable;

pub const fn uniform_size_std(count: u32) -> u32 {
    count.div_ceil(4) * 16
}

pub unsafe fn pack_uniform_std(params_ptr: *const f32, count: u32, out_ptr: *mut u8) {
    let total = uniform_size_std(count) as usize;
    unsafe {
        std::ptr::write_bytes(out_ptr, 0, total);
        let params = std::slice::from_raw_parts(params_ptr, count as usize);
        std::ptr::copy_nonoverlapping(params.as_ptr() as *const u8, out_ptr, params.len() * 4);
    }
}
