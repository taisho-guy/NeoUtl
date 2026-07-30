#![allow(non_camel_case_types)]

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
    FilePath = 5,
    Track = 6,
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

/// enum_optionsのNUL区切り結合文字列を選択肢列へ分解する。空文字列は空Vecとする。
pub fn split_enum_options(joined: &str) -> Vec<&str> {
    if joined.is_empty() {
        Vec::new()
    } else {
        joined.split('\0').collect()
    }
}

/// float既定値のみ格納。Bool/Enumはdefault_floatを0/1として解釈する。
/// Text/FilePath/Trackはdefault_floatを不使用（0.0固定）とし、ホスト側の初期値は
/// それぞれ空文字列/空文字列/未選択(-1)とする。
/// enum_optionsはkind==EnumのときのみNUL区切り文字列（例"A\0B\0C"）として解釈する
/// （'static配列参照をC ABI越しに渡す手段がないため、単一StrRefへ結合する）。
/// kind!=Enumのenum_optionsは空StrRefとする。
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
