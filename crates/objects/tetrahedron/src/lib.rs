use neoutl_object_api::{EntryFn, ObjectMeta, ObjectVTable, RenderContext, WgslSource};
use std::sync::OnceLock;

static META: ObjectMeta = ObjectMeta {
    stable_id: "neoutl.object.tetrahedron",
    name: "Tetrahedron",
};
static VTABLE: OnceLock<ObjectVTable> = OnceLock::new();
static WGSL: &str = include_str!("../tetrahedron.wgsl");

unsafe extern "C" fn meta() -> *const ObjectMeta {
    &raw const META
}
unsafe extern "C" fn vertex_count() -> u32 {
    12
}
unsafe extern "C" fn wgsl() -> WgslSource {
    WgslSource {
        ptr: WGSL.as_ptr(),
        len: WGSL.len(),
    }
}
unsafe extern "C" fn render(_ctx: *const RenderContext) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn neoutl_object_entry() -> *const ObjectVTable {
    VTABLE.get_or_init(|| ObjectVTable {
        meta,
        vertex_count,
        wgsl,
        render,
    })
}

const _: EntryFn = neoutl_object_entry;
