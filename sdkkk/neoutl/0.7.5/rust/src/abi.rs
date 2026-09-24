//! NeoUtl Shared ABI types and low-level data structures.

pub use neoutl_shared_abi::{
    AcceleratorBackend, AcceleratorHandle, Dimensionality, EffectKind, FfiSlice, ParamKind,
    ParamRowOwned, ParamSchema, PropertyWriteback, Roi, StrRef, WgslSource, split_enum_options,
};

/// Helper function to create a static StrRef from a string literal.
#[inline(always)]
pub const fn str_ref(s: &'static str) -> StrRef {
    StrRef::from_str(s)
}

/// Helper function to create an empty StrRef.
#[inline(always)]
pub const fn empty_str_ref() -> StrRef {
    StrRef::empty()
}

/// Helper function to create an FfiSlice from a static slice.
#[inline(always)]
pub const fn ffi_slice<T>(items: &'static [T]) -> FfiSlice<T> {
    FfiSlice::from_static(items)
}
