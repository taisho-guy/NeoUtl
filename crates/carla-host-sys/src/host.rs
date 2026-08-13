use std::ffi::{CStr, CString};
use std::path::Path;

use crate::error::{CarlaError, get_last_error};
use crate::ffi;
use crate::types::*;

pub struct CarlaHost {
    handle: ffi::CarlaHostHandle,
    engine_initialized: bool,
}

unsafe impl Send for CarlaHost {}

impl CarlaHost {
    pub fn new() -> Result<Self, CarlaError> {
        let handle = unsafe { ffi::carla_standalone_host_init() };
        if handle.is_null() {
            Err(CarlaError::HostInitFailed)
        } else {
            let host = Self {
                handle,
                engine_initialized: false,
            };
            let _ = host.set_engine_option(
                EngineOption::ProcessMode,
                EngineProcessMode::ContinuousRack as i32,
                None,
            );
            let _ = host.set_engine_option(
                EngineOption::TransportMode,
                EngineTransportMode::Internal as i32,
                None,
            );
            Ok(host)
        }
    }

    pub fn raw_handle(&self) -> ffi::CarlaHostHandle {
        self.handle
    }

    pub fn last_error(&self) -> String {
        get_last_error(self.handle)
    }

    pub fn set_engine_option(
        &self,
        option: EngineOption,
        value: i32,
        str_value: Option<&str>,
    ) -> Result<(), CarlaError> {
        let c_str = match str_value {
            Some(s) => Some(CString::new(s).map_err(|e| CarlaError::InvalidString(e.to_string()))?),
            None => None,
        };
        let c_ptr = c_str
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(std::ptr::null());

        unsafe {
            ffi::carla_set_engine_option(self.handle, option as u32, value, c_ptr);
        }
        Ok(())
    }

    pub fn init_engine(&mut self, driver_name: &str, client_name: &str) -> Result<(), CarlaError> {
        let c_driver =
            CString::new(driver_name).map_err(|e| CarlaError::InvalidString(e.to_string()))?;
        let c_client =
            CString::new(client_name).map_err(|e| CarlaError::InvalidString(e.to_string()))?;

        let ok =
            unsafe { ffi::carla_engine_init(self.handle, c_driver.as_ptr(), c_client.as_ptr()) };

        if ok {
            self.engine_initialized = true;
            Ok(())
        } else {
            Err(CarlaError::EngineInitFailed(self.last_error()))
        }
    }

    pub fn close_engine(&mut self) -> Result<(), CarlaError> {
        if !self.engine_initialized {
            return Ok(());
        }

        let ok = unsafe { ffi::carla_engine_close(self.handle) };
        if ok {
            self.engine_initialized = false;
            Ok(())
        } else {
            Err(CarlaError::EngineCloseFailed(self.last_error()))
        }
    }

    pub fn idle(&self) {
        unsafe {
            ffi::carla_engine_idle(self.handle);
        }
    }

    pub fn is_running(&self) -> bool {
        unsafe { ffi::carla_is_engine_running(self.handle) }
    }

    pub fn set_buffer_size_and_sample_rate(
        &self,
        buffer_size: u32,
        sample_rate: f64,
    ) -> Result<(), CarlaError> {
        let ok = unsafe {
            ffi::carla_set_engine_buffer_size_and_sample_rate(self.handle, buffer_size, sample_rate)
        };
        if ok {
            Ok(())
        } else {
            Err(CarlaError::OperationFailed(self.last_error()))
        }
    }

    pub fn buffer_size(&self) -> u32 {
        unsafe { ffi::carla_get_buffer_size(self.handle) }
    }

    pub fn sample_rate(&self) -> f64 {
        unsafe { ffi::carla_get_sample_rate(self.handle) }
    }

    pub fn clear_xruns(&self) {
        unsafe {
            ffi::carla_clear_engine_xruns(self.handle);
        }
    }

    pub fn cancel_action(&self) {
        unsafe {
            ffi::carla_cancel_engine_action(self.handle);
        }
    }

    pub fn load_file(&self, path: impl AsRef<Path>) -> Result<(), CarlaError> {
        let filename_str = path.as_ref().to_string_lossy().to_string();
        let c_filename = CString::new(filename_str.as_str())
            .map_err(|e| CarlaError::InvalidString(e.to_string()))?;

        let ok = unsafe { ffi::carla_load_file(self.handle, c_filename.as_ptr()) };
        if ok {
            Ok(())
        } else {
            Err(CarlaError::LoadFileFailed {
                filename: filename_str,
                message: self.last_error(),
            })
        }
    }

    pub fn load_project(&self, path: impl AsRef<Path>) -> Result<(), CarlaError> {
        let filename_str = path.as_ref().to_string_lossy().to_string();
        let c_filename = CString::new(filename_str.as_str())
            .map_err(|e| CarlaError::InvalidString(e.to_string()))?;

        let ok = unsafe { ffi::carla_load_project(self.handle, c_filename.as_ptr()) };
        if ok {
            Ok(())
        } else {
            Err(CarlaError::LoadProjectFailed {
                filename: filename_str,
                message: self.last_error(),
            })
        }
    }

    pub fn save_project(&self, path: impl AsRef<Path>) -> Result<(), CarlaError> {
        let filename_str = path.as_ref().to_string_lossy().to_string();
        let c_filename = CString::new(filename_str.as_str())
            .map_err(|e| CarlaError::InvalidString(e.to_string()))?;

        let ok = unsafe { ffi::carla_save_project(self.handle, c_filename.as_ptr()) };
        if ok {
            Ok(())
        } else {
            Err(CarlaError::SaveProjectFailed {
                filename: filename_str,
                message: self.last_error(),
            })
        }
    }

    pub fn current_project_folder(&self) -> Option<String> {
        unsafe {
            let p = ffi::carla_get_current_project_folder(self.handle);
            if p.is_null() {
                None
            } else {
                Some(CStr::from_ptr(p).to_string_lossy().into_owned())
            }
        }
    }

    pub fn current_project_filename(&self) -> Option<String> {
        unsafe {
            let p = ffi::carla_get_current_project_filename(self.handle);
            if p.is_null() {
                None
            } else {
                Some(CStr::from_ptr(p).to_string_lossy().into_owned())
            }
        }
    }

    pub fn clear_project_filename(&self) {
        unsafe {
            ffi::carla_clear_project_filename(self.handle);
        }
    }

    pub fn transport_play(&self) {
        unsafe {
            ffi::carla_transport_play(self.handle);
        }
    }

    pub fn transport_pause(&self) {
        unsafe {
            ffi::carla_transport_pause(self.handle);
        }
    }

    pub fn transport_bpm(&self, bpm: f64) {
        unsafe {
            ffi::carla_transport_bpm(self.handle, bpm);
        }
    }

    pub fn transport_relocate(&self, frame: u64) {
        unsafe {
            ffi::carla_transport_relocate(self.handle, frame);
        }
    }

    pub fn current_transport_frame(&self) -> u64 {
        unsafe { ffi::carla_get_current_transport_frame(self.handle) }
    }

    pub fn transport_info(&self) -> TransportInfo {
        unsafe {
            let raw = ffi::carla_get_transport_info(self.handle);
            if raw.is_null() {
                TransportInfo::default()
            } else {
                (*raw).into()
            }
        }
    }

    pub fn add_plugin(
        &self,
        binary_type: BinaryType,
        plugin_type: PluginType,
        filename: Option<&str>,
        name: Option<&str>,
        label: Option<&str>,
        unique_id: i64,
        options: u32,
    ) -> Result<u32, CarlaError> {
        let c_filename = filename
            .map(|s| CString::new(s).map_err(|e| CarlaError::InvalidString(e.to_string())))
            .transpose()?;
        let c_name = name
            .map(|s| CString::new(s).map_err(|e| CarlaError::InvalidString(e.to_string())))
            .transpose()?;
        let c_label = label
            .map(|s| CString::new(s).map_err(|e| CarlaError::InvalidString(e.to_string())))
            .transpose()?;

        let ok = unsafe {
            ffi::carla_add_plugin(
                self.handle,
                binary_type as u32,
                plugin_type as u32,
                c_filename.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                c_name.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                c_label.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                unique_id,
                std::ptr::null_mut(),
                options,
            )
        };

        if ok {
            let count = self.plugin_count();
            if count > 0 { Ok(count - 1) } else { Ok(0) }
        } else {
            Err(CarlaError::AddPluginFailed {
                name: name.unwrap_or("").to_string(),
                filename: filename.unwrap_or("").to_string(),
                message: self.last_error(),
            })
        }
    }

    pub fn remove_plugin(&self, plugin_id: u32) -> Result<(), CarlaError> {
        let ok = unsafe { ffi::carla_remove_plugin(self.handle, plugin_id) };
        if ok {
            Ok(())
        } else {
            Err(CarlaError::OperationFailed(self.last_error()))
        }
    }

    pub fn remove_all_plugins(&self) -> Result<(), CarlaError> {
        let ok = unsafe { ffi::carla_remove_all_plugins(self.handle) };
        if ok {
            Ok(())
        } else {
            Err(CarlaError::OperationFailed(self.last_error()))
        }
    }

    pub fn plugin_count(&self) -> u32 {
        unsafe { ffi::carla_get_current_plugin_count(self.handle) }
    }

    pub fn max_plugin_number(&self) -> u32 {
        unsafe { ffi::carla_get_max_plugin_number(self.handle) }
    }

    pub fn plugin_info(&self, plugin_id: u32) -> Option<PluginInfo> {
        unsafe {
            let raw = ffi::carla_get_plugin_info(self.handle, plugin_id);
            if raw.is_null() {
                None
            } else {
                Some(PluginInfo::from_raw(&*raw))
            }
        }
    }

    pub fn audio_port_count(&self, plugin_id: u32) -> PortCountInfo {
        unsafe {
            let raw = ffi::carla_get_audio_port_count_info(self.handle, plugin_id);
            if raw.is_null() {
                PortCountInfo::default()
            } else {
                (*raw).into()
            }
        }
    }

    pub fn midi_port_count(&self, plugin_id: u32) -> PortCountInfo {
        unsafe {
            let raw = ffi::carla_get_midi_port_count_info(self.handle, plugin_id);
            if raw.is_null() {
                PortCountInfo::default()
            } else {
                (*raw).into()
            }
        }
    }

    pub fn parameter_count(&self, plugin_id: u32) -> PortCountInfo {
        unsafe {
            let raw = ffi::carla_get_parameter_count_info(self.handle, plugin_id);
            if raw.is_null() {
                PortCountInfo::default()
            } else {
                (*raw).into()
            }
        }
    }

    pub fn parameter_info(&self, plugin_id: u32, param_id: u32) -> Option<ParameterInfo> {
        unsafe {
            let raw = ffi::carla_get_parameter_info(self.handle, plugin_id, param_id);
            if raw.is_null() {
                None
            } else {
                Some(ParameterInfo::from_raw(&*raw))
            }
        }
    }

    pub fn parameter_value(&self, plugin_id: u32, param_id: u32) -> f32 {
        unsafe { ffi::carla_get_current_parameter_value(self.handle, plugin_id, param_id) }
    }

    pub fn set_parameter_value(&self, plugin_id: u32, param_id: u32, value: f32) {
        unsafe {
            ffi::carla_set_parameter_value(self.handle, plugin_id, param_id, value);
        }
    }

    pub fn program_count(&self, plugin_id: u32) -> u32 {
        unsafe { ffi::carla_get_program_count(self.handle, plugin_id) }
    }

    pub fn program_name(&self, plugin_id: u32, index: u32) -> Option<String> {
        unsafe {
            let p = ffi::carla_get_program_name(self.handle, plugin_id, index);
            if p.is_null() {
                None
            } else {
                Some(CStr::from_ptr(p).to_string_lossy().into_owned())
            }
        }
    }

    pub fn current_program(&self, plugin_id: u32) -> i32 {
        unsafe { ffi::carla_get_current_program_index(self.handle, plugin_id) }
    }

    pub fn set_program(&self, plugin_id: u32, index: u32) {
        unsafe {
            ffi::carla_set_program(self.handle, plugin_id, index);
        }
    }

    pub fn show_custom_ui(&self, plugin_id: u32, show: bool) {
        unsafe {
            ffi::carla_show_custom_ui(self.handle, plugin_id, show);
        }
    }

    pub fn embed_custom_ui(
        &self,
        plugin_id: u32,
        parent_window_ptr: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void {
        unsafe { ffi::carla_embed_custom_ui(self.handle, plugin_id, parent_window_ptr) }
    }

    pub fn set_custom_ui_title(&self, plugin_id: u32, title: &str) -> Result<(), CarlaError> {
        let c_title = CString::new(title).map_err(|e| CarlaError::InvalidString(e.to_string()))?;
        unsafe {
            ffi::carla_set_custom_ui_title(self.handle, plugin_id, c_title.as_ptr());
        }
        Ok(())
    }

    pub fn has_custom_ui(&self, plugin_id: u32) -> bool {
        if let Some(info) = self.plugin_info(plugin_id) {
            info.has_custom_ui()
        } else {
            false
        }
    }

    pub fn can_embed_custom_ui(&self, plugin_id: u32) -> bool {
        if let Some(info) = self.plugin_info(plugin_id) {
            info.can_embed_custom_ui()
        } else {
            false
        }
    }

    pub fn has_inline_display(&self, plugin_id: u32) -> bool {
        if let Some(info) = self.plugin_info(plugin_id) {
            info.has_inline_display()
        } else {
            false
        }
    }

    pub fn has_resizable_custom_ui(&self, plugin_id: u32) -> bool {
        if let Some(info) = self.plugin_info(plugin_id) {
            info.has_resizable_custom_ui()
        } else {
            false
        }
    }

    pub fn render_inline_display(
        &self,
        plugin_id: u32,
        width: u32,
        height: u32,
    ) -> Option<InlineDisplaySurface> {
        unsafe {
            let ptr = ffi::carla_render_inline_display(self.handle, plugin_id, width, height);
            if ptr.is_null() {
                None
            } else {
                InlineDisplaySurface::from_raw(&*ptr)
            }
        }
    }

    pub fn process_stereo(
        &self,
        plugin_id: u32,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        frames: usize,
    ) {
        if frames == 0 {
            return;
        }
        unsafe {
            ffi::carla_plugin_process_stereo(
                self.handle,
                plugin_id,
                in_l.as_ptr(),
                in_r.as_ptr(),
                out_l.as_mut_ptr(),
                out_r.as_mut_ptr(),
                frames as u32,
            );
        }
    }

    pub fn parameter_ranges(&self, plugin_id: u32, param_id: u32) -> Option<ParameterRanges> {
        unsafe {
            let ptr = ffi::carla_get_parameter_ranges(self.handle, plugin_id, param_id);
            if ptr.is_null() {
                None
            } else {
                Some(ParameterRanges {
                    default: (*ptr).def,
                    min: (*ptr).min,
                    max: (*ptr).max,
                    step: (*ptr).step,
                    step_small: (*ptr).stepSmall,
                    step_large: (*ptr).stepLarge,
                })
            }
        }
    }

    pub fn full_param_info_list(&self, plugin_id: u32) -> Vec<PluginParamInfo> {
        let count_info = self.parameter_count(plugin_id);
        let count = count_info.ins + count_info.outs;
        let mut list = Vec::with_capacity(count as usize);
        for id in 0..count {
            if let Some(info) = self.parameter_info(plugin_id, id) {
                let ranges = self.parameter_ranges(plugin_id, id);
                list.push(PluginParamInfo {
                    id,
                    name: info.name,
                    symbol: info.symbol,
                    unit: info.unit,
                    comment: info.comment,
                    min: ranges.as_ref().map_or(0.0, |r| r.min as f64),
                    max: ranges.as_ref().map_or(1.0, |r| r.max as f64),
                    default: ranges.as_ref().map_or(0.0, |r| r.default as f64),
                });
            }
        }
        list
    }
}

impl Drop for CarlaHost {
    fn drop(&mut self) {
        let _ = self.close_engine();
    }
}

pub fn get_engine_driver_count() -> u32 {
    unsafe { ffi::carla_get_engine_driver_count() }
}

pub fn get_engine_driver_name(index: u32) -> Option<String> {
    unsafe {
        let p = ffi::carla_get_engine_driver_name(index);
        if p.is_null() {
            None
        } else {
            Some(CStr::from_ptr(p).to_string_lossy().into_owned())
        }
    }
}

pub fn get_engine_driver_device_names(index: u32) -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        let mut ptr = ffi::carla_get_engine_driver_device_names(index);
        if !ptr.is_null() {
            while !(*ptr).is_null() {
                names.push(CStr::from_ptr(*ptr).to_string_lossy().into_owned());
                ptr = ptr.add(1);
            }
        }
    }
    names
}
