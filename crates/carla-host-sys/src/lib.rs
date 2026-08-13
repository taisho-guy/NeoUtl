#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(unused_unsafe)]

pub mod error;
pub mod host;
pub mod types;

#[cfg(feature = "egui")]
pub mod egui_embed;

pub mod ffi {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use error::{CarlaError, get_last_error};
pub use host::{
    CarlaHost, get_engine_driver_count, get_engine_driver_device_names, get_engine_driver_name,
};
pub use types::*;

#[cfg(feature = "egui")]
pub use egui_embed::{EmbeddedPluginUi, extract_raw_window_ptr};
