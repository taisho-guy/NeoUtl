use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_qt6_tool(name: &str) -> PathBuf {
    let candidates = [
        format!("/usr/lib/qt6/{name}"),
        format!("/usr/lib/qt6/bin/{name}"),
        format!("/usr/libexec/{name}"),
        format!("/usr/bin/{name}6"),
        format!("/usr/bin/{name}"),
    ];
    for c in &candidates {
        let p = PathBuf::from(c);
        if p.is_file() {
            if let Ok(output) = Command::new(&p).arg("-v").output() {
                let v = String::from_utf8_lossy(&output.stdout);
                let v_err = String::from_utf8_lossy(&output.stderr);
                if v.contains(" 6.") || v_err.contains(" 6.") {
                    return p;
                }
            }
        }
    }
    for c in &candidates {
        let p = PathBuf::from(c);
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(name)
}

fn main() {
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=assets/themes");
    println!("cargo:rerun-if-changed=src/renderer/slang/media.slang");
    println!("cargo:rerun-if-changed=src/renderer/slang/media_video.slang");

    copy_data_themes();

    neoutl_object_shader_build::compile_object_shader("media", "src/renderer/slang/media.slang");
    neoutl_object_shader_build::compile_object_shader(
        "media_video",
        "src/renderer/slang/media_video.slang",
    );

    let gui_private_include = Command::new("pkg-config")
        .args(["--variable=includedir", "Qt6Gui"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "/usr/include/qt6".to_string());

    let qt_version = Command::new("pkg-config")
        .args(["--modversion", "Qt6Gui"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR must be set"));
    let qml_cache_dir = out_dir.join("qmlcache");
    fs::create_dir_all(&qml_cache_dir).expect("Failed to create qmlcache dir");

    let qrc_path = PathBuf::from("qml/qml.qrc")
        .canonicalize()
        .expect("Failed to canonicalize qml.qrc");
    let qmlcachegen = find_qt6_tool("qmlcachegen");
    let rcc = find_qt6_tool("rcc");

    let filtered_qrc = qml_cache_dir.join("filtered.qrc");
    let status = Command::new(&qmlcachegen)
        .arg("--filter-resource-file")
        .arg("-o")
        .arg(&filtered_qrc)
        .arg(&qrc_path)
        .status()
        .expect("Failed to run qmlcachegen --filter-resource-file");
    assert!(status.success(), "qmlcachegen filter failed");

    let mut qml_files = Vec::new();
    collect_qml_files(Path::new("qml"), &mut qml_files);

    let mut generated_cpp_files = Vec::new();
    for qml in &qml_files {
        let abs_qml = qml.canonicalize().expect("Failed to canonicalize QML file");
        let stem = abs_qml
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("item");
        let parent_name = abs_qml
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let cpp_name = format!("{parent_name}_{stem}_qml.cpp");
        let out_cpp = qml_cache_dir.join(cpp_name);

        let status = Command::new(&qmlcachegen)
            .arg(format!("--resource={}", qrc_path.display()))
            .arg("-o")
            .arg(&out_cpp)
            .arg(&abs_qml)
            .status()
            .unwrap_or_else(|_| panic!("Failed to run qmlcachegen on {}", abs_qml.display()));
        assert!(status.success(), "qmlcachegen failed for {:?}", abs_qml);
        generated_cpp_files.push(out_cpp);
    }

    let loader_cpp = qml_cache_dir.join("qmlcache_loader.cpp");
    let status = Command::new(&qmlcachegen)
        .arg(format!(
            "--resource-file-mapping={}={}",
            qrc_path.display(),
            filtered_qrc.display()
        ))
        .arg("-o")
        .arg(&loader_cpp)
        .arg(&qrc_path)
        .status()
        .expect("Failed to run qmlcachegen for loader");
    assert!(status.success(), "qmlcachegen loader failed");

    {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&loader_cpp)
            .expect("Failed to open loader_cpp for appending");
        writeln!(
            file,
            r#"
extern "C" void ensure_qml_aot_cache_registered() {{
    unitRegistry();
}}
"#
        )
        .expect("Failed to append trigger to loader_cpp");
    }

    generated_cpp_files.push(loader_cpp);

    let filtered_rcc_cpp = qml_cache_dir.join("filtered_rcc.cpp");
    let status = Command::new(&rcc)
        .arg("--name")
        .arg("filtered")
        .arg("-o")
        .arg(&filtered_rcc_cpp)
        .arg(&filtered_qrc)
        .status()
        .expect("Failed to run rcc");
    assert!(status.success(), "rcc failed");
    generated_cpp_files.push(filtered_rcc_cpp);

    let mut builder = cxx_qt_build::CxxQtBuilder::new()
        .qt_module("Quick")
        .qt_module("Qml")
        .cpp_file("include/wgpu_rhi_item.h")
        .file("src/ffi.rs");

    builder = unsafe {
        builder.cc_builder(move |cc| {
            cc.file("src/wgpu_rhi_item.cpp")
                .include("include")
                .include(format!("{gui_private_include}/QtGui/{qt_version}"))
                .include(format!("{gui_private_include}/QtGui/{qt_version}/QtGui"))
                .flag_if_supported("-std=c++17");

            for generated in &generated_cpp_files {
                cc.file(generated);
            }
        })
    };

    builder.build();
}

fn collect_qml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_qml_files(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("qml") {
                out.push(path);
            }
        }
    }
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
}
