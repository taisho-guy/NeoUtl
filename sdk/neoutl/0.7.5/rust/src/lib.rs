//! # NeoUtl All-In-One SDK (v0.7.5)
//!
//! The official Rust SDK for extending NeoUtl with:
//! - **Extensions (Super-Mods)**: Custom egui panels, floating windows, timeline transactions, canvas overlays, and file drop handlers.
//! - **Effects**: Hardware-accelerated WGSL video filters, transitions, and audio DSP.
//! - **Objects**: Custom timeline media elements, 2D/3D procedural shapes, and shaders.
//! - **Easing Engines**: Custom interpolation curves, easing algorithms, and UI curve editors.
//! - **Expression Engines**: Custom formula parsers, scripting runtimes, and parameter math.
//!
//! ## Quick Start: Writing an Extension Plugin
//!
//! ```rust,ignore
//! use neoutl_sdk::prelude::*;
//!
//! #[derive(Default)]
//! pub struct MyToolExtension;
//!
//! impl ExtensionPlugin for MyToolExtension {
//!     fn metadata(&self) -> ExtensionMetadata {
//!         ExtensionMetadata::new("com.example.my_tool", "My Custom Tool")
//!     }
//!
//!     fn init(&mut self, ctx: &mut ExtensionContext) -> Result<(), String> {
//!         ctx.register_panel(PanelDescriptor {
//!             id: "my_tool_panel".to_owned(),
//!             title: "My Tool".to_owned(),
//!             default_size: [300.0, 200.0],
//!             default_open: true,
//!             placement: PanelPlacement::FloatingWindow,
//!         });
//!         Ok(())
//!     }
//!
//!     fn ui(&mut self, panel_id: &str, ui: &mut egui::Ui, ctx: &mut ExtensionContext) {
//!         if panel_id == "my_tool_panel" {
//!             ui.label("Hello from NeoUtl Extension!");
//!             if ui.button("Create Text Clip").clicked() {
//!                 if let Some(session) = &mut ctx.edit_session {
//!                     let _ = session.create_text_object("Hello NeoUtl", 1, 0, 150);
//!                 }
//!             }
//!         }
//!     }
//! }
//!
//! neoutl_sdk::export_extension!(MyToolExtension);
//! ```
//!
//! ## Quick Start: Writing a Video Effect Plugin
//!
//! ```rust,ignore
//! use neoutl_sdk::prelude::*;
//! use std::sync::OnceLock;
//!
//! static PARAM_SCHEMA: &[EffectParamSchema] = &[
//!     EffectParamSchema {
//!         key: str_ref("intensity"),
//!         label: str_ref("強度"),
//!         kind: ParamKind::Float,
//!         min: 0.0,
//!         max: 1.0,
//!         step: 0.01,
//!         default_float: 0.5,
//!         enum_options: empty_str_ref(),
//!     },
//! ];
//!
//! static META: EffectMeta = EffectMeta {
//!     id: "my_effect",
//!     name: "My Custom Effect",
//!     category: "Filter",
//!     param_schema: ffi_slice(PARAM_SCHEMA),
//!     kind: EffectKind::Image,
//!     author: str_ref("Author"),
//!     description: empty_str_ref(),
//!     uuid: str_ref("com.example.my_effect"),
//!     is_dummy: 0,
//!     use_composition_camera: 0,
//! };
//!
//! static WGSL: &[u8] = b"/* WGSL Shader code */";
//! static VTABLE: OnceLock<EffectVTable> = OnceLock::new();
//!
//! unsafe extern "C" fn meta() -> *const EffectMeta { &raw const META }
//! unsafe extern "C" fn wgsl() -> WgslSource { WgslSource { ptr: WGSL.as_ptr(), len: WGSL.len() } }
//! unsafe extern "C" fn uniform_size() -> u32 { uniform_size_std(PARAM_SCHEMA.len() as u32) }
//! unsafe extern "C" fn pack_uniform(params_ptr: *const f32, count: u32, out_ptr: *mut u8) {
//!     unsafe { pack_uniform_std(params_ptr, count, out_ptr) }
//! }
//!
//! pub fn get_vtable() -> *const EffectVTable {
//!     VTABLE.get_or_init(|| EffectVTable {
//!         meta,
//!         wgsl,
//!         uniform_size,
//!         pack_uniform,
//!         requires_texture_param: None,
//!         calc_roi: None,
//!         is_need_render_frame: None,
//!         process_audio: None,
//!         on_property_edited: None,
//!         on_property_restored: None,
//!         poll_writeback: None,
//!         setup_accelerator: None,
//!     })
//! }
//!
//! neoutl_sdk::export_effect!(get_vtable);
//! ```

pub mod abi;
pub mod easing;
pub mod effect;
pub mod expression;
pub mod extension;
pub mod macros;
pub mod object;
pub mod prelude;

// Re-export third-party crates for convenience
pub use egui;
pub use serde;
pub use serde_json;

#[cfg(test)]
mod tests {
    use super::prelude::*;

    #[test]
    fn test_sdk_prelude_types() {
        let s = str_ref("test");
        assert_eq!(unsafe { s.as_str() }, "test");

        let schema = ParamSchema {
            key: str_ref("key"),
            label: str_ref("label"),
            kind: ParamKind::Float,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            default_float: 50.0,
            enum_options: empty_str_ref(),
        };
        assert_eq!(schema.kind, ParamKind::Float);
    }
}
