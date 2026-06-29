// crates/objects/text/src/lib.rs
use neoutl_object_api::{EntryFn, ObjectMeta, ObjectVTable, RenderContext};
use std::sync::OnceLock;

static META: ObjectMeta = ObjectMeta { name: "Text" };

static VTABLE: OnceLock<ObjectVTable> = OnceLock::new();

unsafe extern "C" fn meta() -> *const ObjectMeta {
    &raw const META
}
unsafe extern "C" fn vertex_count() -> u32 {
    0
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
