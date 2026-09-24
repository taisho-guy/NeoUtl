//! Macros for exporting NeoUtl plugin entry points.

/// Exports an Extension plugin entry point (`neoutl_extension_create`).
///
/// # Example
/// ```rust,ignore
/// #[derive(Default)]
/// struct MySuperPlugin;
/// impl ExtensionPlugin for MySuperPlugin { ... }
///
/// neoutl_sdk::export_extension!(MySuperPlugin);
/// ```
#[macro_export]
macro_rules! export_extension {
    ($plugin_ty:ty) => {
        #[no_mangle]
        pub extern "C" fn neoutl_extension_create() -> Box<dyn $crate::extension::ExtensionPlugin> {
            Box::new(<$plugin_ty as ::std::default::Default>::default())
        }
    };
    (new: $constructor:expr) => {
        #[no_mangle]
        pub extern "C" fn neoutl_extension_create() -> Box<dyn $crate::extension::ExtensionPlugin> {
            Box::new($constructor)
        }
    };
}

/// Exports an Effect plugin entry point (`neoutl_effect_entry`).
///
/// # Example
/// ```rust,ignore
/// neoutl_sdk::export_effect!(get_vtable);
/// ```
#[macro_export]
macro_rules! export_effect {
    ($vtable_getter:path) => {
        #[no_mangle]
        pub unsafe extern "C" fn neoutl_effect_entry() -> *const $crate::effect::EffectVTable {
            $vtable_getter()
        }
    };
}

/// Exports an Object plugin entry point (`neoutl_object_entry`).
///
/// # Example
/// ```rust,ignore
/// neoutl_sdk::export_object!(get_vtable);
/// ```
#[macro_export]
macro_rules! export_object {
    ($vtable_getter:path) => {
        #[no_mangle]
        pub unsafe extern "C" fn neoutl_object_entry() -> *const $crate::object::ObjectVTable {
            $vtable_getter()
        }
    };
}

/// Exports an Easing engine plugin entry point (`neoutl_easing_engine_entry`).
///
/// # Example
/// ```rust,ignore
/// neoutl_sdk::export_easing!(get_vtable);
/// ```
#[macro_export]
macro_rules! export_easing {
    ($vtable_getter:path) => {
        #[no_mangle]
        pub unsafe extern "C" fn neoutl_easing_engine_entry()
        -> *const $crate::easing::EasingEngineVTable {
            $vtable_getter()
        }
    };
}

/// Exports an Expression engine plugin entry point (`neoutl_expression_engine_entry`).
///
/// # Example
/// ```rust,ignore
/// neoutl_sdk::export_expression!(get_vtable);
/// ```
#[macro_export]
macro_rules! export_expression {
    ($vtable_getter:path) => {
        #[no_mangle]
        pub unsafe extern "C" fn neoutl_expression_engine_entry()
        -> *const $crate::expression::ExpressionEngineVTable {
            $vtable_getter()
        }
    };
}
