//! Commonly used types, traits, and macros across all NeoUtl plugin kinds.

pub use crate::abi::{
    AcceleratorBackend, AcceleratorHandle, Dimensionality, EffectKind, FfiSlice, ParamKind,
    ParamSchema, Roi, StrRef, WgslSource, empty_str_ref, ffi_slice, str_ref,
};

pub use crate::effect::{
    EffectMeta, EffectParamSchema, EffectVTable, pack_uniform_std, uniform_size_std,
};

pub use crate::object::{ObjectMeta, ObjectVTable, PropertyGroup, RenderContext};

pub use crate::easing::{EasingEngineMeta, EasingEngineVTable, KeyframeC};

pub use crate::expression::{
    ExpressionEngineMeta, ExpressionEngineVTable, ExpressionEvalContext, ExpressionHostVTable,
};

pub use crate::extension::{
    CommandDescriptor, EditSession, EffectInfo, ExtensionContext, ExtensionEvent,
    ExtensionMetadata, ExtensionPlugin, FileDropDescriptor, MenuBarSection, MenuItemDescriptor,
    MenuLocation, ObjectInfo, PanelDescriptor, PanelPlacement, PreviewOverlayContext,
    ProjectStorage, TimelineOverlayContext,
};

pub use crate::{export_easing, export_effect, export_expression, export_extension, export_object};
