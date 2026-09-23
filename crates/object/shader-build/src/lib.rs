use std::path::{Path, PathBuf};
use std::process::Command;

fn resolve_slangc() -> PathBuf {
    let exe_name = if cfg!(windows) {
        "slangc.exe"
    } else {
        "slangc"
    };
    if let Ok(dir) = std::env::var("SLANG_DIR") {
        let p = Path::new(&dir).join("bin").join(exe_name);
        if p.exists() {
            return p;
        }
        let p = Path::new(&dir).join(exe_name);
        if p.exists() {
            return p;
        }
    }
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let mut cur = PathBuf::from(manifest);
        loop {
            let candidate = cur.join("slang").join("bin").join(exe_name);
            if candidate.exists() {
                return candidate;
            }
            if let Some(parent) = cur.parent() {
                cur = parent.to_path_buf();
            } else {
                break;
            }
        }
    }
    PathBuf::from(exe_name)
}

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

pub fn compile_compute_dxil(label: &str, source_relpath: &str, entry: &str) {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIRが未設定");
    let source_path = Path::new(&manifest_dir).join(source_relpath);
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIRが未設定");
    let dest_path = PathBuf::from(&out_dir).join(format!("{label}.dxil"));

    let output = Command::new(resolve_slangc())
        .arg(&source_path)
        .arg("-entry")
        .arg(entry)
        .arg("-stage")
        .arg("compute")
        .arg("-target")
        .arg("dxil")
        .arg("-profile")
        .arg("cs_6_0")
        .arg("-o")
        .arg(&dest_path)
        .output()
        .expect("slangc起動失敗。SLANG_DIR環境変数またはPATHを確認してください");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "Slang→DXILコンパイル失敗: label={label} source={} stderr={stderr}",
            source_path.display()
        );
    }

    println!("cargo:rerun-if-changed={}", source_path.display());
    println!("cargo:rerun-if-env-changed=SLANG_DIR");
}

pub fn compile_compute_spirv(label: &str, source_relpath: &str, entry: &str) {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIRが未設定");
    let source_path = Path::new(&manifest_dir).join(source_relpath);
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIRが未設定");
    let dest_path = PathBuf::from(&out_dir).join(format!("{label}.spv"));

    let output = Command::new(resolve_slangc())
        .arg(&source_path)
        .arg("-entry")
        .arg(entry)
        .arg("-stage")
        .arg("compute")
        .arg("-target")
        .arg("spirv")
        .arg("-o")
        .arg(&dest_path)
        .output()
        .expect("slangc起動失敗。SLANG_DIR環境変数またはPATHを確認してください");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "Slang→SPIR-Vコンパイル失敗: label={label} source={} stderr={stderr}",
            source_path.display()
        );
    }

    println!("cargo:rerun-if-changed={}", source_path.display());
    println!("cargo:rerun-if-env-changed=SLANG_DIR");
}
