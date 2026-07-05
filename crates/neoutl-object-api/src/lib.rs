#[repr(C)]
pub struct ObjectMeta {
    pub stable_id: &'static str,
    pub name: &'static str,
}

#[repr(C)]
pub struct RenderContext {
    pub version: u32,
    pub render_pass_ptr: *mut (),
    pub bind_group_ptr: *const (),
    pub vertex_count: u32,
    pub aspect: f32,
    pub angle: f32,
}

#[repr(C)]
pub struct WgslSource {
    pub ptr: *const u8,
    pub len: usize,
}

unsafe impl Send for WgslSource {}
unsafe impl Sync for WgslSource {}

#[repr(C)]
pub struct ObjectVTable {
    pub meta: unsafe extern "C" fn() -> *const ObjectMeta,
    pub vertex_count: unsafe extern "C" fn() -> u32,
    pub wgsl: unsafe extern "C" fn() -> WgslSource,
    pub render: unsafe extern "C" fn(ctx: *const RenderContext),
}

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_object_entry\0";
pub type EntryFn = unsafe extern "C" fn() -> *const ObjectVTable;
