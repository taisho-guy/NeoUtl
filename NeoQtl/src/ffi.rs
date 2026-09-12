#[cxx::bridge]
mod ffi {
    #[derive(Clone, Copy, Debug, Default)]
    struct NativeDeviceHandles {
        backend: u8,
        instance: u64,
        physical_device: u64,
        device: u64,
        queue: u64,
        queue_family_index: u32,
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct NativeTextureHandle {
        object: u64,
        layout_or_state: u32,
        width: i32,
        height: i32,
    }

    unsafe extern "C++" {
        include!("wgpu_rhi_item.h");
        fn register_wgpu_rhi_item();
    }

    extern "Rust" {
        fn wgpu_renderer_create() -> usize;
        fn wgpu_renderer_bind_device(context_id: usize, handles: NativeDeviceHandles);
        fn wgpu_renderer_bind_texture(context_id: usize, texture: NativeTextureHandle);
        fn wgpu_renderer_render(context_id: usize);
        fn wgpu_renderer_destroy(context_id: usize);
    }
}

pub fn register() {
    ffi::register_wgpu_rhi_item();
}

pub use crate::renderer::{
    wgpu_renderer_bind_device, wgpu_renderer_bind_texture, wgpu_renderer_create,
    wgpu_renderer_destroy, wgpu_renderer_render,
};
pub use ffi::{NativeDeviceHandles, NativeTextureHandle};
