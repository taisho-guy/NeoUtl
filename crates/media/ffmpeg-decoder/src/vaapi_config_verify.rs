use std::ffi::CString;
use std::os::raw::{c_int, c_uint};

use libva::VAProfile;

use crate::vaapi_sys as va_sys;

const VA_CONFIG_ATTRIB_RT_FORMAT: c_uint = 0;
const VA_RT_FORMAT_YUV420: c_uint = 0x0000_0001;
const VA_RT_FORMAT_YUV420_10: c_uint = 0x0000_0100;
const VA_STATUS_SUCCESS: c_int = 0;

pub fn verify_va_config_creatable(path: &str, profile: VAProfile::Type, want_10bit: bool) -> bool {
    let c_path = match CString::new(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[vaapi-config-verify] CString変換失敗 path={path} err={e:?}");
            return false;
        }
    };

    unsafe {
        let fd = libc::open(c_path.as_ptr(), libc::O_RDWR);
        if fd < 0 {
            eprintln!(
                "[vaapi-config-verify] open失敗 path={path} errno={}",
                std::io::Error::last_os_error()
            );
            return false;
        }

        let dpy = va_sys::vaGetDisplayDRM(fd);
        if dpy.is_null() {
            eprintln!("[vaapi-config-verify] vaGetDisplayDRM失敗 path={path}");
            libc::close(fd);
            return false;
        }

        let mut major: c_int = 0;
        let mut minor: c_int = 0;
        let init_ret = va_sys::vaInitialize(dpy, &mut major, &mut minor);
        if init_ret != VA_STATUS_SUCCESS {
            eprintln!("[vaapi-config-verify] vaInitialize失敗 path={path} ret={init_ret}");
            libc::close(fd);
            return false;
        }
        eprintln!("[vaapi-config-verify] vaInitialize成功 path={path} version={major}.{minor}");

        let rt_format = if want_10bit {
            VA_RT_FORMAT_YUV420_10
        } else {
            VA_RT_FORMAT_YUV420
        };
        let mut attrib = va_sys::VAConfigAttrib {
            type_: VA_CONFIG_ATTRIB_RT_FORMAT,
            value: rt_format,
        };

        let mut config_id: va_sys::VAConfigID = 0;
        let create_ret = va_sys::vaCreateConfig(
            dpy,
            profile as va_sys::VAProfileType,
            va_sys::VA_ENTRYPOINT_VLD,
            &mut attrib,
            1,
            &mut config_id,
        );
        let ok = create_ret == VA_STATUS_SUCCESS;
        eprintln!(
            "[vaapi-config-verify] vaCreateConfig結果 path={path} profile={profile:?} rt_format={rt_format:#x} ret={create_ret} ok={ok}"
        );

        if ok {
            let destroy_ret = va_sys::vaDestroyConfig(dpy, config_id);
            if destroy_ret != VA_STATUS_SUCCESS {
                eprintln!(
                    "[vaapi-config-verify] vaDestroyConfig失敗 path={path} ret={destroy_ret}(継続)"
                );
            }
        }

        va_sys::vaTerminate(dpy);
        libc::close(fd);
        ok
    }
}
