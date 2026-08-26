pub use neoutl_shared_abi::{Dimensionality, ParamKind, ParamSchema, StrRef, WgslSource};

#[repr(C)]
pub struct ObjectMeta {
    pub stable_id: &'static str,
    pub name: &'static str,
    pub dimensionality: Dimensionality,
    pub property_schema_ptr: *const ParamSchema,
    pub property_schema_len: usize,
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
}

#[repr(C)]
pub struct ObjectVTable {
    pub meta: unsafe extern "C" fn() -> *const ObjectMeta,
    pub vertex_count: unsafe extern "C" fn() -> u32,
    pub wgsl: unsafe extern "C" fn() -> WgslSource,
    pub render: unsafe extern "C" fn(ctx: *const RenderContext),
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
