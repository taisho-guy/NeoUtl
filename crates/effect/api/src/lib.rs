pub use neoutl_shared_abi::{
    AcceleratorBackend, AcceleratorHandle, EffectKind, FfiSlice, ParamKind, PropertyWriteback, Roi,
    StrRef, WgslSource,
};
pub type EffectParamSchema = neoutl_shared_abi::ParamSchema;

#[repr(C)]
pub struct EffectMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub param_schema: FfiSlice<EffectParamSchema>,
    pub kind: EffectKind,
    pub author: StrRef,
    pub description: StrRef,
    pub uuid: StrRef,
    pub is_dummy: u8,
    pub use_composition_camera: u8,
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

    pub calc_roi: Option<
        unsafe extern "C" fn(
            base: Roi,
            params_ptr: *const f32,
            count: u32,
            layer_time_us: i64,
            downsample_x: f32,
            downsample_y: f32,
        ) -> Roi,
    >,

    pub is_need_render_frame:
        Option<unsafe extern "C" fn(params_ptr: *const f32, count: u32, layer_time_us: i64) -> u32>,

    pub process_audio: Option<
        unsafe extern "C" fn(
            samples_ptr: *mut f32,
            sample_count: u32,
            params_ptr: *const f32,
            param_count: u32,
        ) -> u32,
    >,

    pub on_property_edited: Option<unsafe extern "C" fn(params_ptr: *const f32, count: u32)>,

    pub on_property_restored: Option<unsafe extern "C" fn(params_ptr: *const f32, count: u32)>,

    pub poll_writeback:
        Option<unsafe extern "C" fn(out_ptr: *mut PropertyWriteback, out_cap: u32) -> u32>,

    pub setup_accelerator:
        Option<unsafe extern "C" fn(accelerator: *const AcceleratorHandle) -> u32>,
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

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn dummy_setup_accelerator(acc: *const AcceleratorHandle) -> u32 {
        if acc.is_null() {
            return 1;
        }
        let h = unsafe { &*acc };
        if h.version != AcceleratorHandle::CURRENT_VERSION {
            return 2;
        }
        0
    }

    unsafe extern "C" fn dummy_meta() -> *const EffectMeta {
        std::ptr::null()
    }
    unsafe extern "C" fn dummy_wgsl() -> WgslSource {
        WgslSource {
            ptr: std::ptr::null(),
            len: 0,
        }
    }
    unsafe extern "C" fn dummy_uniform_size() -> u32 {
        0
    }
    unsafe extern "C" fn dummy_pack_uniform(_: *const f32, _: u32, _: *mut u8) {}

    #[test]
    fn test_vtable_setup_accelerator_invocation() {
        let vtable = EffectVTable {
            meta: dummy_meta,
            wgsl: dummy_wgsl,
            uniform_size: dummy_uniform_size,
            pack_uniform: dummy_pack_uniform,
            requires_texture_param: None,
            calc_roi: None,
            is_need_render_frame: None,
            process_audio: None,
            on_property_edited: None,
            on_property_restored: None,
            poll_writeback: None,
            setup_accelerator: Some(dummy_setup_accelerator),
        };

        let handle = AcceleratorHandle::new(
            AcceleratorBackend::Vulkan,
            0x10 as *const (),
            0x20 as *const (),
        );
        let f = vtable.setup_accelerator.unwrap();
        assert_eq!(unsafe { f(&handle as *const _) }, 0);
        assert_eq!(unsafe { f(std::ptr::null()) }, 1);
    }
}
