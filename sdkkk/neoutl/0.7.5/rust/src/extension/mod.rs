//! Extension (Super-Mod) plugin API for GUI customization, timeline automation, and tool suites.

pub mod abi;
pub mod types;

pub use abi::*;
pub use types::*;

#[derive(Debug)]
pub enum ExtensionError {
    Load(String),
    Init(String),
    Runtime(String),
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(s) => write!(f, "拡張読み込み失敗: {s}"),
            Self::Init(s) => write!(f, "拡張初期化失敗: {s}"),
            Self::Runtime(s) => write!(f, "拡張実行時エラー: {s}"),
        }
    }
}

impl std::error::Error for ExtensionError {}
