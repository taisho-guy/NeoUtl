use std::path::{Path, PathBuf};
use std::process::Command;

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

    println!("{}", t!("cargo:rerun-if-changed=%{arg0}"));
    println!("{}", t!("cargo:rerun-if-changed=%{arg0}"));
    println!("{}", t!("cargo:rerun-if-env-changed=SLANG_DIR"));
}
rust_i18n::i18n!("../../i18n");
#[macro_use]
extern crate rust_i18n;
