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

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiSlice<T> {
    pub ptr: *const T,
    pub len: usize,
}

impl<T> FfiSlice<T> {
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    pub const fn from_static(items: &'static [T]) -> Self {
        Self {
            ptr: items.as_ptr(),
            len: items.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ptr.is_null() || self.len == 0
    }

    pub unsafe fn as_slice(&self) -> &'static [T] {
        if self.is_empty() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}
unsafe impl<T> Send for FfiSlice<T> {}
unsafe impl<T> Sync for FfiSlice<T> {}

pub type WgslSource = FfiSlice<u8>;

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

#[derive(Debug)]
pub enum PluginError {
    Load(String),
    Runtime(String),
    MissingField(&'static str),
    InvalidField(&'static str),
    Unknown(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(msg) => write!(f, "読み込み失敗: {msg}"),
            Self::Runtime(msg) => write!(f, "実行エラー: {msg}"),
            Self::MissingField(name) => write!(f, "必須フィールド欠落: {name}"),
            Self::InvalidField(name) => write!(f, "フィールド型不正: {name}"),
            Self::Unknown(what) => write!(f, "未知の識別子: {what}"),
        }
    }
}
impl std::error::Error for PluginError {}
