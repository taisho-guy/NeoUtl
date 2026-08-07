#![allow(non_camel_case_types)]

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimensionality {
    TwoD = 0,
    ThreeD = 1,
    Both = 2,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Float = 0,
    Bool = 1,
    Color = 2,
    Enum = 3,
    Text = 4,
    FilePath = 5,
    Track = 6,
    Separator = 7,
    Group = 8,
    Folder = 9,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StrRef {
    pub ptr: *const u8,
    pub len: usize,
}

impl StrRef {
    pub const fn from_str(s: &'static str) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }

    pub unsafe fn as_str(&self) -> &'static str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.ptr, self.len)) }
    }
}
unsafe impl Send for StrRef {}
unsafe impl Sync for StrRef {}

pub fn split_enum_options(joined: &str) -> Vec<&str> {
    if joined.is_empty() {
        Vec::new()
    } else {
        joined.split('\0').collect()
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ParamSchema {
    pub key: StrRef,
    pub label: StrRef,
    pub kind: ParamKind,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default_float: f32,
    pub enum_options: StrRef,
}

#[repr(C)]
pub struct WgslSource {
    pub ptr: *const u8,
    pub len: usize,
}
unsafe impl Send for WgslSource {}
unsafe impl Sync for WgslSource {}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamRowOwned {
    pub key: String,
    pub label: String,
    pub kind: ParamKind,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default_float: f32,
    pub enum_options: Vec<String>,
}

impl ParamSchema {
    pub unsafe fn to_owned_row(&self) -> ParamRowOwned {
        unsafe {
            ParamRowOwned {
                key: self.key.as_str().to_owned(),
                label: self.label.as_str().to_owned(),
                kind: self.kind,
                min: self.min,
                max: self.max,
                step: self.step,
                default_float: self.default_float,
                enum_options: if self.kind == ParamKind::Enum {
                    split_enum_options(self.enum_options.as_str())
                        .into_iter()
                        .map(str::to_owned)
                        .collect()
                } else {
                    Vec::new()
                },
            }
        }
    }
}
