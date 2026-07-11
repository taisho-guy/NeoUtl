// crates/xtask/src/main.rs
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

struct ObjectCrate {
    package_name: String,
    lib_name: String,
}

/// crates/objects/直下の各ディレクトリのCargo.tomlを走査し、
/// package.name と lib.name（未指定時はpackage.nameの'-'を'_'置換）を収集する。
/// 新規追加クレートはディレクトリを置くだけで自動検出対象になる。
fn discover_object_crates(workspace_root: &Path) -> Vec<ObjectCrate> {
    let objects_dir = workspace_root.join("crates/objects");
    let mut result = Vec::new();

    let entries = match fs::read_dir(&objects_dir) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("[xtask] {} 読取失敗: {err}", objects_dir.display());
            return result;
        }
    };

    for entry in entries.flatten() {
        let manifest_dir = entry.path();
        if !manifest_dir.is_dir() {
            continue;
        }
        let manifest_path = manifest_dir.join("Cargo.toml");
        let Ok(text) = fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(doc) = text.parse::<toml::Table>() else {
            eprintln!("[xtask] 解析失敗: {}", manifest_path.display());
            continue;
        };

        let package_name = doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(str::to_owned);

        let Some(package_name) = package_name else {
            continue;
        };

        let lib_name = doc
            .get("lib")
            .and_then(|l| l.get("name"))
            .and_then(|n| n.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| package_name.replace('-', "_"));

        result.push(ObjectCrate {
            package_name,
            lib_name,
        });
    }

    result.sort_by(|a, b| a.package_name.cmp(&b.package_name));
    result
}

fn dylib_filename(lib_name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{lib_name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{lib_name}.dylib")
    } else {
        format!("lib{lib_name}.so")
    }
}

fn build_objects(workspace_root: &Path, profile: &str, crates: &[ObjectCrate]) {
    if crates.is_empty() {
        eprintln!("[xtask] objectsクレート0件: crates/objects/* を確認");
        return;
    }

    let mut cmd = Command::new("cargo");
    cmd.current_dir(workspace_root).arg("build");
    if profile == "release" {
        cmd.arg("--release");
    }
    for c in crates {
        cmd.arg("-p").arg(&c.package_name);
    }

    let status = cmd.status().expect("cargo build 起動失敗");
    if !status.success() {
        panic!("[xtask] objectsビルド失敗: exit={status}");
    }
}

fn stage_objects(workspace_root: &Path, profile: &str, crates: &[ObjectCrate]) {
    let target_dir = workspace_root.join("target").join(profile);
    let objects_dir = target_dir.join("objects");
    fs::create_dir_all(&objects_dir).expect("target/{profile}/objects 作成失敗");

    for c in crates {
        let filename = dylib_filename(&c.lib_name);
        let src = target_dir.join(&filename);
        let dst = objects_dir.join(&filename);
        match fs::copy(&src, &dst) {
            Ok(_) => eprintln!("[xtask] 配置: {filename}"),
            Err(err) => eprintln!("[xtask] 配置失敗 {filename}: {err} (src={})", src.display()),
        }
    }
}

fn workspace_root() -> PathBuf {
    // crates/xtask/Cargo.toml から見て2階層上がワークスペースルート。
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root解決失敗")
        .to_path_buf()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut release = false;
    let mut task = "run".to_string();
    for a in &args {
        match a.as_str() {
            "--release" => release = true,
            "build" | "run" => task = a.clone(),
            _ => {}
        }
    }
    let profile = if release { "release" } else { "debug" };

    let root = workspace_root();
    let crates = discover_object_crates(&root);
    build_objects(&root, profile, &crates);
    stage_objects(&root, profile, &crates);

    if task == "run" {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(&root).arg("run").arg("-p").arg("NeoUtl");
        if release {
            cmd.arg("--release");
        }
        let status = cmd.status().expect("cargo run 起動失敗");
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
    }
}
