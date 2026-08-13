use std::ffi::CStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CarlaError {
    #[error("Failed to initialize Carla standalone host")]
    HostInitFailed,

    #[error("Failed to initialize Carla engine: {0}")]
    EngineInitFailed(String),

    #[error("Failed to close Carla engine: {0}")]
    EngineCloseFailed(String),

    #[error("Failed to load file '{filename}': {message}")]
    LoadFileFailed { filename: String, message: String },

    #[error("Failed to load project '{filename}': {message}")]
    LoadProjectFailed { filename: String, message: String },

    #[error("Failed to save project '{filename}': {message}")]
    SaveProjectFailed { filename: String, message: String },

    #[error("Failed to add plugin '{name}' ({filename}): {message}")]
    AddPluginFailed {
        name: String,
        filename: String,
        message: String,
    },

    #[error("Plugin with ID {0} not found or invalid")]
    PluginNotFound(u32),

    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("Null pointer encountered")]
    NullPointer,

    #[error("String contains invalid UTF-8 or null byte: {0}")]
    InvalidString(String),
}

pub fn get_last_error(handle: crate::ffi::CarlaHostHandle) -> String {
    unsafe {
        let err_ptr = crate::ffi::carla_get_last_error(handle);
        if err_ptr.is_null() {
            "Unknown error".to_string()
        } else {
            CStr::from_ptr(err_ptr).to_string_lossy().into_owned()
        }
    }
}
