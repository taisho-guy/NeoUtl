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
    /// コールバック実行ボタン。呼び出し先は `ParamSchema::button_callback` を参照する。
    Button = 10,
    /// レイヤー単位で独立した真偽値。値の意味は `ParamKind::Bool` と同一。
    CheckSection = 11,
    /// UI非表示の汎用バイト列。サイズは可変で `ParamSchema::data_size` が初期サイズを表す。
    Data = 12,
    /// `ParamSchema::group_member_count` 個の直後の `Track` 項目を1グループとして束ねる。
    TrackGroup = 13,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    Image = 0,
    Audio = 1,
    Both = 2,
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

    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    /// Reads the referenced bytes as a UTF-8 string with static lifetime.
    ///
    /// # Safety
    ///
    /// `ptr` must point to `len` readable bytes that remain valid for the
    /// returned string's lifetime, and those bytes must contain valid UTF-8.
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

    /// Reads the referenced memory as a slice with static lifetime.
    ///
    /// # Safety
    ///
    /// When the slice is non-empty, `ptr` must be non-null and point to at
    /// least `len` properly initialized values that remain valid for the
    /// returned slice's lifetime.
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

/// GPUリソース参照先の種別。文字列プレフィックス解析ではなくタグ付き共用体で表現する。
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    /// 現在処理対象のオブジェクト。
    Object = 0,
    /// `name` で識別される標準リソース。処理完了後に破棄される。
    Named = 1,
    /// フレームバッファ。
    Framebuffer = 2,
    /// 仮想バッファ。
    TempBuffer = 3,
    /// `name` で識別されるキャッシュバッファ(VRAM常駐)。
    Cache = 4,
    /// `name` が指す画像ファイル。読み込み結果はVRAMにキャッシュされる。
    Image = 5,
    /// 0.0〜1.0の乱数値を格納する256x256領域(読み取り専用)。
    Random = 6,
}

/// GPUリソースへの参照。`kind` が `Named` `Cache` `Image` の場合のみ `name` を参照する。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResourceRef {
    pub kind: ResourceKind,
    pub name: StrRef,
}
unsafe impl Send for ResourceRef {}
unsafe impl Sync for ResourceRef {}

impl ResourceRef {
    pub const fn object() -> Self {
        Self {
            kind: ResourceKind::Object,
            name: StrRef::empty(),
        }
    }
    pub const fn named(name: StrRef) -> Self {
        Self {
            kind: ResourceKind::Named,
            name,
        }
    }
    pub const fn framebuffer() -> Self {
        Self {
            kind: ResourceKind::Framebuffer,
            name: StrRef::empty(),
        }
    }
    pub const fn temp_buffer() -> Self {
        Self {
            kind: ResourceKind::TempBuffer,
            name: StrRef::empty(),
        }
    }
    pub const fn cache(name: StrRef) -> Self {
        Self {
            kind: ResourceKind::Cache,
            name,
        }
    }
    pub const fn image(path: StrRef) -> Self {
        Self {
            kind: ResourceKind::Image,
            name: path,
        }
    }
    pub const fn random() -> Self {
        Self {
            kind: ResourceKind::Random,
            name: StrRef::empty(),
        }
    }
}

/// シェーダー段階。
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderStage {
    Vertex = 0,
    Fragment = 1,
    Compute = 2,
}

/// コンピュートシェーダーのスレッドグループ数。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComputeDispatch {
    pub group_count_x: u32,
    pub group_count_y: u32,
    pub group_count_z: u32,
}

/// VRAM常駐の画像キャッシュ参照。`valid == 0` はキャッシュ未存在を表す。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CacheImageRef {
    pub resource: ResourceRef,
    pub width: u32,
    pub height: u32,
    pub valid: u8,
}
unsafe impl Send for CacheImageRef {}
unsafe impl Sync for CacheImageRef {}

impl CacheImageRef {
    pub const fn empty() -> Self {
        Self {
            resource: ResourceRef::cache(StrRef::empty()),
            width: 0,
            height: 0,
            valid: 0,
        }
    }
}

/// フレーム間画像キャッシュハンドル。データはVRAM(テクスチャ)に確保され、CPU側へのコピーを伴わない。
/// `identifier` はキャッシュ名前空間を分離するための任意の静的ポインタ(呼び出し元エフェクトの `EffectMeta` アドレス等)。
#[repr(C)]
pub struct CacheHandle {
    pub get_image: unsafe extern "C" fn(identifier: *const (), name: StrRef) -> CacheImageRef,
    pub create_image: unsafe extern "C" fn(
        identifier: *const (),
        name: StrRef,
        width: u32,
        height: u32,
    ) -> CacheImageRef,
    pub clear_image: unsafe extern "C" fn(identifier: *const (), name: StrRef),
}
unsafe impl Send for CacheHandle {}
unsafe impl Sync for CacheHandle {}

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
    /// Converts the FFI schema into owned Rust data.
    ///
    /// # Safety
    ///
    /// Every string reference in the schema must satisfy the safety contract
    /// of [`StrRef::as_str`].
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

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roi {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PropertyWriteback {
    pub key: StrRef,
    pub value: f32,
    pub is_user_action: u8,
}
unsafe impl Send for PropertyWriteback {}
unsafe impl Sync for PropertyWriteback {}

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

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceleratorBackend {
    Unknown = 0,
    Vulkan = 1,
    Metal = 2,
    Dx12 = 3,
    Cpu = 4,
    Mock = 5,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AcceleratorHandle {
    pub version: u32,
    pub backend_kind: u32,
    pub device_ptr: *const (),
    pub queue_ptr: *const (),
}
unsafe impl Send for AcceleratorHandle {}
unsafe impl Sync for AcceleratorHandle {}

impl AcceleratorHandle {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new(
        backend_kind: AcceleratorBackend,
        device_ptr: *const (),
        queue_ptr: *const (),
    ) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            backend_kind: backend_kind as u32,
            device_ptr,
            queue_ptr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accelerator_handle_creation() {
        let dev = 0x1000 as *const ();
        let q = 0x2000 as *const ();
        let handle = AcceleratorHandle::new(AcceleratorBackend::Vulkan, dev, q);
        assert_eq!(handle.version, AcceleratorHandle::CURRENT_VERSION);
        assert_eq!(handle.backend_kind, AcceleratorBackend::Vulkan as u32);
        assert_eq!(handle.device_ptr, dev);
        assert_eq!(handle.queue_ptr, q);
    }
}
