use neoutl_shared_abi::StrRef;
use std::os::raw::c_void;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct KeyframeC {
    pub frame: i32,
    pub value: f32,
    pub payload_ptr: *const u8,
    pub payload_len: usize,
}
unsafe impl Send for KeyframeC {}
unsafe impl Sync for KeyframeC {}

#[repr(C)]
pub struct EasingEngineMeta {
    pub id: StrRef,
    pub name: StrRef,
}
unsafe impl Send for EasingEngineMeta {}
unsafe impl Sync for EasingEngineMeta {}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditResultCode {
    Cancel = 0,
    Success = 1,
}

#[repr(C)]
pub struct EditResultC {
    pub code: EditResultCode,
    pub keyframes_ptr: *mut KeyframeC,
    pub count: usize,
}
unsafe impl Send for EditResultC {}
unsafe impl Sync for EditResultC {}

#[repr(C)]
pub struct EasingEngineVTable {
    pub meta: unsafe extern "C" fn() -> *const EasingEngineMeta,
    pub evaluate: unsafe extern "C" fn(
        keyframes_ptr: *const KeyframeC,
        count: usize,
        frame: i32,
        fallback: f32,
    ) -> f32,
    pub open_editor_window: unsafe extern "C" fn(
        host_window_handle: *const c_void,
        keyframes_ptr: *const KeyframeC,
        count: usize,
        on_complete: unsafe extern "C" fn(user_data: *mut c_void, result: EditResultC),
        user_data: *mut c_void,
    ),
    pub serialize: unsafe extern "C" fn(
        keyframes_ptr: *const KeyframeC,
        count: usize,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ),
    pub deserialize: unsafe extern "C" fn(
        bytes_ptr: *const u8,
        len: usize,
        out_keyframes: *mut *mut KeyframeC,
        out_count: *mut usize,
    ),
    pub free_bytes: unsafe extern "C" fn(ptr: *mut u8, len: usize),
    pub free_keyframes: unsafe extern "C" fn(ptr: *mut KeyframeC, count: usize),
}
unsafe impl Send for EasingEngineVTable {}
unsafe impl Sync for EasingEngineVTable {}

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_easing_engine_entry\0";
pub type EntryFn = unsafe extern "C" fn() -> *const EasingEngineVTable;
