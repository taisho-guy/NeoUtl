// build.rs
use std::{collections::HashMap, fs, path::PathBuf};

fn main() {
    let library = HashMap::from([("lucide".to_string(), PathBuf::from(lucide_slint::lib()))]);
    let config = slint_build::CompilerConfiguration::new().with_library_paths(library);
    slint_build::compile_with_config("src/app.slint", config).expect("Slint build failed");
    copy_data_themes();
}

/// assets/themes/*.json,*.toml をビルド不要のままtarget/{profile}/themes/へコピーする。
/// ネイティブテーマクレートはcrates/themes配下からxtask側でステージングされる。
fn copy_data_themes() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR未設定");
    let profile_dir = PathBuf::from(&out_dir)
        .ancestors()
        .nth(3)
        .expect("target/{profile}解決失敗")
        .to_path_buf();
    let dest = profile_dir.join("themes");
    let src = PathBuf::from("assets/themes");
    if !src.is_dir() {
        return;
    }
    fs::create_dir_all(&dest).expect("themes配置先作成失敗");
    for entry in fs::read_dir(&src).into_iter().flatten().flatten() {
        let path = entry.path();
        let ext_ok = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("json") | Some("toml")
        );
        if ext_ok && let Some(name) = path.file_name() {
            let _ = fs::copy(&path, dest.join(name));
        }
    }
    println!("cargo:rerun-if-changed=assets/themes");
}
