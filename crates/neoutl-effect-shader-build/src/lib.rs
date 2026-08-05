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

/// エフェクトクレートのbuild.rsから呼ぶ唯一の関数。
/// neoutl-effect-api/slang/effect_prelude.slang（vs_main・input_tex・input_sampler契約）と
/// fragment_path（fs_main本体）を同一slangc呼び出しへ両方渡し、単一翻訳単位としてWGSLへ
/// コンパイルし、OUT_DIR/{label}.wgslへ出力する。
/// ビルド時にslangc終了コードを検査するため、コンパイル失敗はcargo build自体を失敗させる
/// （実行時にシェーダ不正へ到達することがない）。
pub fn compile_effect_fragment(label: &str, fragment_relpath: &str) {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIRが未設定");
    let prelude_path =
        Path::new(&manifest_dir).join("../../neoutl-effect-api/slang/effect_prelude.slang");
    let fragment_path = Path::new(&manifest_dir).join(fragment_relpath);
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIRが未設定");
    let dest_path = PathBuf::from(&out_dir).join(format!("{label}.wgsl"));

    let output = Command::new(resolve_slangc())
        .arg(&prelude_path)
        .arg(&fragment_path)
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
            "Slang→WGSLコンパイル失敗: label={label} prelude={} fragment={} stderr={stderr}",
            prelude_path.display(),
            fragment_path.display()
        );
    }

    println!("cargo:rerun-if-changed={}", prelude_path.display());
    println!("cargo:rerun-if-changed={}", fragment_path.display());
    println!("cargo:rerun-if-env-changed=SLANG_DIR");
}
rust_i18n::i18n!("../../i18n");
#[macro_use]
extern crate rust_i18n;
