pub use neoutl_shared_abi::{
    AcceleratorBackend, AcceleratorHandle, CacheHandle, CacheImageRef, ComputeDispatch, EffectKind,
    FfiSlice, ParamKind, PropertyWriteback, ResourceKind, ResourceRef, Roi, StrRef, WgslSource,
};
pub type EffectParamSchema = neoutl_shared_abi::ParamSchema;

#[repr(C)]
pub struct EffectMeta {
    pub id: StrRef,
    pub name: StrRef,
    pub category: StrRef,
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

/// フィルタ処理中に呼び出せるGPUリソースアクセス関数群。
/// AviUtl2の `exec_pixelshader_data` / `exec_computeshader_data` / `get_image_resource_*` /
/// `set_image_resource_data` に相当する。CPU転送はRGBA8、GPU target/storageはNamed/Temp/CacheのRGBA8Unormに限定。すべて戻り値は成功時0、失敗時非0。
#[repr(C)]
pub struct EffectRenderContext {
    /// フレーム間キャッシュ。`cache_identifier` は呼び出し元エフェクトの識別に使う任意の静的ポインタ。
    pub cache: *const CacheHandle,
    pub cache_identifier: *const (),

    /// コンパイル済みピクセルシェーダーを実行する。
    /// `data` はWGSLテキスト。ホストはvs_main/fs_mainとbinding 0/1 source+sampler、2 uniform、3/4 auxiliary+samplerを使う。targetはNamed/Temp/CacheのRGBA8UnormまたはFramebuffer(現在のRGBA16F出力)です。
    pub exec_pixelshader: unsafe extern "C" fn(
        data: *const u8,
        data_len: usize,
        target: ResourceRef,
        resources: *const ResourceRef,
        resource_count: u32,
        constant: *const u8,
        constant_len: usize,
    ) -> u32,

    /// コンパイル済みコンピュートシェーダーを実行する。
    pub exec_computeshader: unsafe extern "C" fn(
        data: *const u8,
        data_len: usize,
        targets: *const ResourceRef,
        target_count: u32,
        resources: *const ResourceRef,
        resource_count: u32,
        constant: *const u8,
        constant_len: usize,
        dispatch: ComputeDispatch,
    ) -> u32,

    pub get_resource_size:
        unsafe extern "C" fn(resource: ResourceRef, width: *mut u32, height: *mut u32) -> u32,

    pub get_resource_data: unsafe extern "C" fn(
        resource: ResourceRef,
        buffer: *mut u8,
        width: u32,
        height: u32,
        pitch: u32,
    ) -> u32,

    pub set_resource_data: unsafe extern "C" fn(
        resource: ResourceRef,
        buffer: *const u8,
        width: u32,
        height: u32,
        pitch: u32,
    ) -> u32,

    /// `Named`/`TempBuffer`/`Cache` 種別のリソースを明示的に解放する。
    pub release_resource: unsafe extern "C" fn(resource: ResourceRef) -> u32,
}
unsafe impl Send for EffectRenderContext {}
unsafe impl Sync for EffectRenderContext {}

#[repr(C)]
pub struct EffectVTable {
    pub meta: unsafe extern "C" fn() -> *const EffectMeta,
    pub wgsl: unsafe extern "C" fn() -> WgslSource,
    pub uniform_size: unsafe extern "C" fn() -> u32,
    pub pack_uniform: unsafe extern "C" fn(params_ptr: *const f32, count: u32, out_ptr: *mut u8),
    pub requires_texture_param: Option<unsafe extern "C" fn() -> u32>,

    /// 頂点シェーダーソース。`None` の場合ホスト標準の全画面矩形頂点シェーダーを使う。
    pub vertex_wgsl: Option<unsafe extern "C" fn() -> WgslSource>,

    /// コンピュートシェーダーソース。指定時は `compute_dispatch` も併せて指定する。ホストはcs_mainを使い、RGBA8Unorm storage targetsをbinding 0..N、sampled texture群を次のbinding、uniformを最後のbindingに置く。
    pub compute_wgsl: Option<unsafe extern "C" fn() -> WgslSource>,

    /// `compute_wgsl` 実行時のスレッドグループ数を算出する。現行ホストではcs_main、RGBA8Unorm storage targets、sampled resources、最後にuniformのbinding順。
    pub compute_dispatch: Option<
        unsafe extern "C" fn(
            params_ptr: *const f32,
            count: u32,
            width: u32,
            height: u32,
        ) -> ComputeDispatch,
    >,

    /// 固定パイプライン(`wgsl`/`uniform_size`/`pack_uniform`)を介さず、
    /// `EffectRenderContext` を通じてホスト対応範囲のリソースアクセスとシェーダー実行を行う。pixel shaderはvs_main/fs_main、binding 0/1入力画像とsampler、2 uniform、3/4補助画像とsampler。
    /// `Some` の場合、ホストは `wgsl` の代わりにこちらを呼び出す。
    pub custom_render: Option<
        unsafe extern "C" fn(
            ctx: *const EffectRenderContext,
            params_ptr: *const f32,
            count: u32,
        ) -> u32,
    >,

    /// ROIを計算する。戻り値の領域は後続の画像効果に累積され、ホストのフラグメント描画範囲に反映される。
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

    /// 0を返すフレームでは画像/音声処理を省略する。
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

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_effect_entry_v2\0";
pub type EntryFn = unsafe extern "C" fn() -> *const EffectVTable;

pub const fn uniform_size_std(count: u32) -> u32 {
    count.div_ceil(4) * 16
}

/// Packs scalar parameters into the standard 16-byte uniform layout.
///
/// # Safety
///
/// `params_ptr` must point to at least `count` initialized `f32` values, and
/// `out_ptr` must point to a writable buffer of at least
/// `uniform_size_std(count)` bytes. The source and destination must be valid
/// for the duration of this call and must not overlap.
pub unsafe fn pack_uniform_std(params_ptr: *const f32, count: u32, out_ptr: *mut u8) {
    let total = uniform_size_std(count) as usize;
    if total == 0 {
        return;
    }
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
            vertex_wgsl: None,
            compute_wgsl: None,
            compute_dispatch: None,
            custom_render: None,
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
