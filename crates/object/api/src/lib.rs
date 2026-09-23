pub use neoutl_shared_abi::{
    AcceleratorBackend, AcceleratorHandle, Dimensionality, FfiSlice, ParamKind, ParamSchema,
    StrRef, WgslSource,
};

#[repr(C)]
pub struct PropertyGroup {
    pub group_id: StrRef,
    pub schema: FfiSlice<ParamSchema>,
}
unsafe impl Send for PropertyGroup {}
unsafe impl Sync for PropertyGroup {}

pub const DEFAULT_PROPERTY_GROUP_ID: &str = "default";

#[repr(C)]
pub struct ObjectMeta {
    pub stable_id: &'static str,
    pub name: &'static str,
    pub dimensionality: Dimensionality,
    pub property_groups: FfiSlice<PropertyGroup>,
}
unsafe impl Send for ObjectMeta {}
unsafe impl Sync for ObjectMeta {}

#[repr(C)]
pub struct RenderContext {
    pub version: u32,
    pub render_pass_ptr: *mut (),
    pub bind_group_ptr: *const (),
    pub vertex_count: u32,
    pub mvp_matrix: [f32; 16],
    pub opacity: f32,
    pub depth_enabled: bool,
    pub ref_layer_texture_ptr: *const (),
    pub ref_layer_texture_count: u32,
}

#[repr(C)]
pub struct ObjectVTable {
    pub meta: unsafe extern "C" fn() -> *const ObjectMeta,
    pub vertex_count: unsafe extern "C" fn() -> u32,
    pub wgsl: unsafe extern "C" fn() -> WgslSource,
    pub render: unsafe extern "C" fn(ctx: *const RenderContext),

    pub read_ref_layer:
        Option<unsafe extern "C" fn(ctx: *const RenderContext, index: u32) -> *const ()>,

    pub setup_accelerator:
        Option<unsafe extern "C" fn(accelerator: *const AcceleratorHandle) -> u32>,
}

pub const UNIT_SIZE_PX: f32 = 200.0;

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_object_entry\0";
pub type EntryFn = unsafe extern "C" fn() -> *const ObjectVTable;

pub const TEXT_STABLE_ID: &str = "neoutl.object.text";

pub const VIDEO_STABLE_ID: &str = "neoutl.object.video";

pub const IMAGE_STABLE_ID: &str = "neoutl.object.image";

pub const AUDIO_STABLE_ID: &str = "neoutl.object.audio";

pub const SCENE_STABLE_ID: &str = "neoutl.object.scene";

pub const GROUP_CONTROL_STABLE_ID: &str = "neoutl.object.group_control";

pub const CAMERA_STABLE_ID: &str = "neoutl.object.camera";

pub const LIGHT_STABLE_ID: &str = "neoutl.object.light";

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

    unsafe extern "C" fn dummy_meta() -> *const ObjectMeta {
        std::ptr::null()
    }
    unsafe extern "C" fn dummy_vertex_count() -> u32 {
        0
    }
    unsafe extern "C" fn dummy_wgsl() -> WgslSource {
        WgslSource {
            ptr: std::ptr::null(),
            len: 0,
        }
    }
    unsafe extern "C" fn dummy_render(_: *const RenderContext) {}

    #[test]
    fn test_object_vtable_setup_accelerator_invocation() {
        let vtable = ObjectVTable {
            meta: dummy_meta,
            vertex_count: dummy_vertex_count,
            wgsl: dummy_wgsl,
            render: dummy_render,
            read_ref_layer: None,
            setup_accelerator: Some(dummy_setup_accelerator),
        };

        let handle = AcceleratorHandle::new(
            AcceleratorBackend::Metal,
            0x100 as *const (),
            0x200 as *const (),
        );
        let f = vtable.setup_accelerator.unwrap();
        assert_eq!(unsafe { f(&handle as *const _) }, 0);
        assert_eq!(unsafe { f(std::ptr::null()) }, 1);
    }
}
