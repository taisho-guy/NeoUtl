use std::path::{Path, PathBuf};
use std::process::Command;

/// slangc実行ファイルのパスを解決する。
/// SLANG_DIR環境変数（<SLANG_DIR>/bin/slangc[.exe]）を優先し、
/// 未設定時はPATH上のslangcをそのまま起動する。
fn resolve_slangc() -> PathBuf {
    let exe_name = if cfg!(windows) {
        "slangc.exe"
    } else {
        "slangc"
    };
    match std::env::var("SLANG_DIR") {
        Ok(dir) => Path::new(&dir).join("bin").join(exe_name),
        Err(_) => PathBuf::from(exe_name),
    }
}

/// オブジェクトクレートのbuild.rsから呼ぶ唯一の関数。
/// vs_main・fs_mainを単一.slangファイル内に完結させる形式（エフェクトと異なり共有頂点契約を持たない）を
/// slangc単体呼び出しでWGSLへコンパイルし、OUT_DIR/{label}.wgslへ出力する。
/// ビルド時にslangc終了コードを検査するため、コンパイル失敗はcargo build自体を失敗させる
/// （実行時にシェーダ不正へ到達することがない）。
pub fn compile_object_shader(label: &str, source_relpath: &str) {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIRが未設定");
    let source_path = Path::new(&manifest_dir).join(source_relpath);
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIRが未設定");
    let dest_path = PathBuf::from(&out_dir).join(format!("{label}.wgsl"));

    let output = Command::new(resolve_slangc())
        .arg(&source_path)
        .arg("-entry")
        .arg("vs_main")
        .arg("-stage")
        .arg("vertex")
        .arg("-entry")
        .arg("fs_main")
        .arg("-stage")
        .arg("fragment")
        .arg("-target")
        .arg("wgsl")
        .arg("-o")
        .arg(&dest_path)
        .output()
        .expect("slangc起動失敗。SLANG_DIR環境変数またはPATHを確認してください");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "Slang→WGSLコンパイル失敗: label={label} source={} stderr={stderr}",
            source_path.display()
        );
    }

    println!("cargo:rerun-if-changed={}", source_path.display());
    println!("cargo:rerun-if-env-changed=SLANG_DIR");
}
rust_i18n::i18n!("../../i18n");
#[macro_use]
extern crate rust_i18n;
