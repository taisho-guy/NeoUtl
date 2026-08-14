use std::os::raw::{c_int, c_uint, c_void};

pub type VADisplay = *mut c_void;
pub type VAStatus = c_int;
pub type VAConfigID = c_uint;
pub type VAProfileType = c_int;
pub type VAEntrypointType = c_int;

pub const VA_ENTRYPOINT_VLD: VAEntrypointType = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VAConfigAttrib {
    pub type_: c_uint,
    pub value: c_uint,
}

#[link(name = "va")]
unsafe extern "C" {
    pub fn vaGetDisplayDRM(fd: c_int) -> VADisplay;
    pub fn vaInitialize(dpy: VADisplay, major: *mut c_int, minor: *mut c_int) -> VAStatus;
    pub fn vaTerminate(dpy: VADisplay) -> VAStatus;
    pub fn vaCreateConfig(
        dpy: VADisplay,
        profile: VAProfileType,
        entrypoint: VAEntrypointType,
        attrib_list: *mut VAConfigAttrib,
        num_attribs: c_int,
        config_id: *mut VAConfigID,
    ) -> VAStatus;
    pub fn vaDestroyConfig(dpy: VADisplay, config_id: VAConfigID) -> VAStatus;
}
