// crates/objects/tetrahedron/src/lib.rs
use neoutl_object_api::{EntryFn, ObjectMeta, ObjectVTable, RenderContext};
use std::sync::OnceLock;

pub const WGSL: &str = include_str!("../tetrahedron.wgsl");

static META: ObjectMeta = ObjectMeta {
    name: "Tetrahedron",
};

static VTABLE: OnceLock<ObjectVTable> = OnceLock::new();

unsafe extern "C" fn meta() -> *const ObjectMeta {
    &raw const META
}
unsafe extern "C" fn vertex_count() -> u32 {
    12
}
unsafe extern "C" fn render(_ctx: *const RenderContext) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn neoutl_object_entry() -> *const ObjectVTable {
    VTABLE.get_or_init(|| ObjectVTable {
        meta,
        vertex_count,
        render,
    })
}

const _: EntryFn = neoutl_object_entry;
