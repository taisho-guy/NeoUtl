#![allow(non_camel_case_types)]
// neoutl-object-api / neoutl-effect-api 共有C ABI型。
// 両APIが同一のプラグイン設定UI（properties.rs）から参照されるため、
// ParamKind/ParamSchema/StrRef/WgslSameを二重定義せずここへ一本化する。

/// オブジェクト・エフェクト双方が対応する次元。ホストはこの値でカメラ行列を切替える。
/// エフェクトは現状常時2Dパス（フルスクリーンポストプロセス）で適用するため、
/// EffectMetaはこの型を保持しない（ObjectMeta専用）。
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimensionality {
    TwoD = 0,
    ThreeD = 1,
    Both = 2,
}

/// 設定ダイアログUI生成用のパラメータ種別。
/// Enumはオブジェクト側、Textはエフェクト側で導入されたが、
/// 型共有方針により両APIが同一列挙を参照する。
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Float = 0,
    Bool = 1,
    Color = 2,
    Enum = 3,
    Text = 4,
}

/// C ABI越しに渡す固定長文字列参照。
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

    /// # Safety
    /// ptr/lenが生成元の'static文字列バイト列を指し続けていること。
    pub unsafe fn as_str(&self) -> &'static str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.ptr, self.len)) }
    }
}
unsafe impl Send for StrRef {}
unsafe impl Sync for StrRef {}

/// float既定値のみ格納。Bool/Enumはdefault_floatを0/1として解釈する。
/// Textはdefault_floatを不使用（0.0固定）としホスト側の初期文字列は空文字とする。
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
}

#[repr(C)]
pub struct WgslSource {
    pub ptr: *const u8,
    pub len: usize,
}
unsafe impl Send for WgslSource {}
unsafe impl Sync for WgslSource {}
