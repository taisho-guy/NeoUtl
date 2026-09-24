use std::os::raw::{c_char, c_double, c_float, c_int, c_void};

pub const EXTENSION_ENTRY_SYMBOL: &[u8] = b"neoutl_extension_create\0";
pub const EXTENSION_API_VERSION: u32 = 1;

pub type ExtensionCreateFn = fn() -> Box<dyn crate::ExtensionPlugin>;

pub type AviUtl2ObjectHandle = *mut c_void;
pub type AviUtl2EffectHandle = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AviUtl2ObjectLayerFrame {
    pub layer: c_int,
    pub start: c_int,
    pub end: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AviUtl2MediaInfo {
    pub video_track_num: c_int,
    pub audio_track_num: c_int,
    pub total_time: c_double,
    pub width: c_int,
    pub height: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AviUtl2TrackInfo {
    pub mode: *const u16,
    pub param: *mut c_double,
    pub param_num: c_int,
    pub accelerate: bool,
    pub decelerate: bool,
    pub twopoint: bool,
    pub timecontrol: bool,
    pub group_num: c_int,
    pub group_index: c_int,
    pub group_name: *const u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AviUtl2BpmInfo {
    pub tempo: c_float,
    pub beat: c_int,
    pub start: c_double,
    pub offset: c_float,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AviUtl2Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AviUtl2EditInfo {
    pub width: c_int,
    pub height: c_int,
    pub rate: c_int,
    pub scale: c_int,
    pub sample_rate: c_int,
    pub frame: c_int,
    pub layer: c_int,
    pub frame_max: c_int,
    pub layer_max: c_int,
    pub display_frame_start: c_int,
    pub display_layer_start: c_int,
    pub display_frame_num: c_int,
    pub display_layer_num: c_int,
    pub select_range_start: c_int,
    pub select_range_end: c_int,
    pub grid_bpm_tempo: c_float,
    pub grid_bpm_beat: c_int,
    pub grid_bpm_offset: c_float,
    pub scene_id: c_int,
    pub background: AviUtl2Color,
}

#[repr(C)]
pub struct AviUtl2EditSection {
    pub info: *mut AviUtl2EditInfo,
    pub create_object_from_alias: Option<
        unsafe extern "C" fn(
            alias: *const c_char,
            layer: c_int,
            frame: c_int,
            length: c_int,
        ) -> AviUtl2ObjectHandle,
    >,
    pub find_object:
        Option<unsafe extern "C" fn(layer: c_int, frame: c_int) -> AviUtl2ObjectHandle>,
    pub count_object_effect:
        Option<unsafe extern "C" fn(object: AviUtl2ObjectHandle, effect: *const u16) -> c_int>,
    pub get_object_layer_frame:
        Option<unsafe extern "C" fn(object: AviUtl2ObjectHandle) -> AviUtl2ObjectLayerFrame>,
    pub get_object_alias:
        Option<unsafe extern "C" fn(object: AviUtl2ObjectHandle) -> *const c_char>,
    pub get_object_item_value: Option<
        unsafe extern "C" fn(
            object: AviUtl2ObjectHandle,
            effect: *const u16,
            item: *const u16,
        ) -> *const c_char,
    >,
    pub set_object_item_value: Option<
        unsafe extern "C" fn(
            object: AviUtl2ObjectHandle,
            effect: *const u16,
            item: *const u16,
            value: *const c_char,
        ) -> bool,
    >,
    pub move_object: Option<
        unsafe extern "C" fn(object: AviUtl2ObjectHandle, layer: c_int, frame: c_int) -> bool,
    >,
    pub delete_object: Option<unsafe extern "C" fn(object: AviUtl2ObjectHandle)>,
    pub get_focus_object: Option<unsafe extern "C" fn() -> AviUtl2ObjectHandle>,
    pub set_focus_object: Option<unsafe extern "C" fn(object: AviUtl2ObjectHandle)>,
    pub get_project_file:
        Option<unsafe extern "C" fn(edit: *mut AviUtl2EditHandle) -> *mut AviUtl2ProjectFile>,
    pub get_selected_object: Option<unsafe extern "C" fn(index: c_int) -> AviUtl2ObjectHandle>,
    pub get_selected_object_num: Option<unsafe extern "C" fn() -> c_int>,
}

#[repr(C)]
pub struct AviUtl2ProjectFile {
    pub get_param_string: Option<unsafe extern "C" fn(key: *const c_char) -> *const c_char>,
    pub set_param_string: Option<unsafe extern "C" fn(key: *const c_char, value: *const c_char)>,
    pub get_param_binary:
        Option<unsafe extern "C" fn(key: *const c_char, data: *mut c_void, size: c_int) -> bool>,
    pub set_param_binary:
        Option<unsafe extern "C" fn(key: *const c_char, data: *mut c_void, size: c_int)>,
    pub clear_params: Option<unsafe extern "C" fn()>,
    pub get_project_file_path: Option<unsafe extern "C" fn() -> *const u16>,
}

#[repr(C)]
pub struct AviUtl2EditHandle {
    pub call_edit_section: Option<
        unsafe extern "C" fn(func_proc_edit: unsafe extern "C" fn(*mut AviUtl2EditSection)) -> bool,
    >,
    pub get_edit_info: Option<unsafe extern "C" fn(info: *mut AviUtl2EditInfo, info_size: c_int)>,
    pub restart_host_app: Option<unsafe extern "C" fn()>,
}

#[repr(C)]
pub struct AviUtl2HostAppTable {
    pub set_plugin_information: Option<unsafe extern "C" fn(info: *const u16)>,
    pub register_import_menu: Option<
        unsafe extern "C" fn(name: *const u16, func: unsafe extern "C" fn(*mut AviUtl2EditSection)),
    >,
    pub register_export_menu: Option<
        unsafe extern "C" fn(name: *const u16, func: unsafe extern "C" fn(*mut AviUtl2EditSection)),
    >,
    pub register_layer_menu: Option<
        unsafe extern "C" fn(name: *const u16, func: unsafe extern "C" fn(*mut AviUtl2EditSection)),
    >,
    pub register_object_menu: Option<
        unsafe extern "C" fn(name: *const u16, func: unsafe extern "C" fn(*mut AviUtl2EditSection)),
    >,
    pub register_edit_menu: Option<
        unsafe extern "C" fn(name: *const u16, func: unsafe extern "C" fn(*mut AviUtl2EditSection)),
    >,
    pub register_file_drop_handler: Option<
        unsafe extern "C" fn(
            name: *const u16,
            filefilter: *const u16,
            func: unsafe extern "C" fn(*mut AviUtl2EditSection, *const u16),
        ),
    >,
}

#[repr(C)]
pub struct AviUtl2CommonPluginTable {
    pub name: *const u16,
    pub information: *const u16,
}

#[repr(C)]
pub struct AviUtl2LogHandle {
    pub log: Option<unsafe extern "C" fn(handle: *mut AviUtl2LogHandle, message: *const u16)>,
    pub info: Option<unsafe extern "C" fn(handle: *mut AviUtl2LogHandle, message: *const u16)>,
    pub warn: Option<unsafe extern "C" fn(handle: *mut AviUtl2LogHandle, message: *const u16)>,
    pub error: Option<unsafe extern "C" fn(handle: *mut AviUtl2LogHandle, message: *const u16)>,
}

#[repr(C)]
pub struct AviUtl2ConfigHandle {
    pub app_data_path: *const u16,
    pub translate: Option<
        unsafe extern "C" fn(handle: *mut AviUtl2ConfigHandle, text: *const u16) -> *const u16,
    >,
}
