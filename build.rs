use std::{fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=assets/icon-shadowed.svg");
    println!("cargo:rerun-if-changed=assets/icon.svg");
    copy_data_themes();
    neoutl_object_shader_build::compile_object_shader("media", "src/renderer/slang/media.slang");
    neoutl_object_shader_build::compile_object_shader(
        "media_video",
        "src/renderer/slang/media_video.slang",
    );
}

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
