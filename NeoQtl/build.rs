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
    println!("cargo:rerun-if-changed=src/ui/qml");
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

    let qrc_path = PathBuf::from("src/ui/qml/qml.qrc")
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
    collect_qml_files(Path::new("src/ui/qml"), &mut qml_files);

    let qml_root = Path::new("src/ui/qml");
    verify_bridge_surface(
        qml_root,
        Path::new("src/ui/timeline/bridge_qt.h"),
        "Workspace.currentTimeline",
    );
    verify_bridge_surface(
        qml_root,
        Path::new("src/ui/timeline/bridge_qt.h"),
        "TimelineBridge",
    );
    verify_bridge_surface(qml_root, Path::new("src/ui/workspace_qt.h"), "Workspace");
    verify_bridge_surface(
        qml_root,
        Path::new("src/ui/settings_manager_qt.h"),
        "SettingsManager",
    );

    let qml_only: Vec<_> = qml_files
        .iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("qml"))
        .cloned()
        .collect();
    run_qmllint(&qml_only);

    let mut generated_cpp_files = Vec::new();
    for qml in &qml_files {
        let abs_qml = qml
            .canonicalize()
            .expect("Failed to canonicalize QML/JS file");
        let stem = abs_qml
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("item");
        let ext = abs_qml
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("qml");
        let parent_name = abs_qml
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let cpp_name = format!("{parent_name}_{stem}_{ext}.cpp");
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
        .cpp_file("src/ui/timeline/bridge_qt.h")
        .cpp_file("src/ui/workspace_qt.h")
        .cpp_file("src/ui/settings_manager_qt.h")
        .cpp_file("include/app_main.h")
        .file("src/ffi.rs")
        .file("src/ui/timeline/bridge/timeline_bridge.rs")
        .file("src/ui/workspace_bridge.rs");

    builder = unsafe {
        builder.cc_builder(move |cc| {
            cc.file("src/app_main.cpp")
                .file("src/ui/settings_manager_qt.cpp")
                .file("src/ui/timeline/bridge_qt.cpp")
                .file("src/ui/workspace_qt.cpp")
                .include("include")
                .include("src/ui")
                .include("src/ui/timeline")
                .include(format!("{gui_private_include}/QtGui/{qt_version}"))
                .include(format!("{gui_private_include}/QtGui/{qt_version}/QtGui"))
                .flag_if_supported("-std=c++17");

            for generated in &generated_cpp_files {
                cc.file(generated);
            }
        })
    };

    builder.build();

    println!("cargo:rustc-link-lib=static:+whole-archive=cxx-qt-cxxqt-generated");
    println!("cargo:rustc-link-lib=static:+whole-archive=cxx-qt-lib-cxxqt-generated");
}

fn declared_members(header_src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in header_src.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Q_PROPERTY(") {
            if let Some(name) = rest.split_whitespace().nth(1) {
                out.push(name.trim_start_matches('*').to_string());
            }
        } else if let Some(pos) = line.find("Q_INVOKABLE ") {
            let tail = &line[pos + "Q_INVOKABLE ".len()..];
            if let Some(paren) = tail.find('(') {
                if let Some(name) = tail[..paren].rsplit([' ', '*', '&']).next() {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

fn referenced_members(qml_root: &Path, receiver: &str) -> Vec<String> {
    let mut files = Vec::new();
    collect_qml_files(qml_root, &mut files);
    let prefix = format!("{receiver}.");
    let mut out = Vec::new();
    for file in files {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let mut rest = text.as_str();
        while let Some(pos) = rest.find(&prefix) {
            rest = &rest[pos + prefix.len()..];
            let len = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            if len > 0 {
                out.push(rest[..len].to_string());
            }
            rest = &rest[len..];
        }
    }
    out.sort();
    out.dedup();
    out
}

fn verify_bridge_surface(qml_root: &Path, header_path: &Path, receiver: &str) {
    let header = fs::read_to_string(header_path)
        .unwrap_or_else(|_| panic!("{}読込失敗", header_path.display()));
    let declared = declared_members(&header);
    let missing: Vec<String> = referenced_members(qml_root, receiver)
        .into_iter()
        .filter(|m| !declared.contains(m))
        .collect();
    assert!(
        missing.is_empty(),
        "{receiver}未実装メンバ {}件: {}",
        missing.len(),
        missing.join(", ")
    );
}

fn run_qmllint(qml_files: &[PathBuf]) {
    let qmllint = find_qt6_tool("qmllint");
    if !qmllint.is_file() {
        println!(
            "cargo:warning=qmllint not found, skipping static QML check (path tried: {})",
            qmllint.display()
        );
        return;
    }

    let qml_root = Path::new("src/ui/qml")
        .canonicalize()
        .expect("Failed to canonicalize src/ui/qml");

    for qml in qml_files {
        let abs_qml = qml.canonicalize().expect("Failed to canonicalize QML file");
        let output = Command::new(&qmllint)
            .arg("-I")
            .arg(&qml_root)
            .arg("--silent")
            .arg(&abs_qml)
            .output()
            .expect("Failed to run qmllint");
        if !output.status.success() {
            println!(
                "cargo:warning=qmllint warning on {}: {}",
                abs_qml.display(),
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .next()
                    .unwrap_or("diagnostic")
            );
        }
    }
}

fn collect_qml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_qml_files(&path, out);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("qml") | Some("js")
            ) {
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
