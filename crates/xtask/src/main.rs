// crates/xtask/src/main.rs
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

struct DiscoveredCrate {
    package_name: String,
    lib_name: String,
}

/// workspace_root/subdir 直下の各ディレクトリのCargo.tomlを走査し、
/// package.name と lib.name（未指定時はpackage.nameの'-'を'_'置換）を収集する。
/// 新規追加クレートはディレクトリを置くだけで自動検出対象になる。
/// subdirには"crates/objects"・"crates/effects"のいずれも渡せる（両者は同一走査規則）。
fn discover_crates(workspace_root: &Path, subdir: &str) -> Vec<DiscoveredCrate> {
    let scan_dir = workspace_root.join(subdir);
    let mut result = Vec::new();

    let entries = match fs::read_dir(&scan_dir) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("[xtask] {} 読取失敗: {err}", scan_dir.display());
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

        result.push(DiscoveredCrate {
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

/// --targetの有無でcargoの出力先が target/{profile} と target/{triple}/{profile} に分かれるため、
/// ビルド・配置の両方で参照する実出力ディレクトリをここで一元的に解決する。
fn target_dir(workspace_root: &Path, profile: &str, target: Option<&str>) -> PathBuf {
    match target {
        Some(triple) => workspace_root.join("target").join(triple).join(profile),
        None => workspace_root.join("target").join(profile),
    }
}

fn build_crates(
    workspace_root: &Path,
    profile: &str,
    target: Option<&str>,
    label: &str,
    crates: &[DiscoveredCrate],
) {
    if crates.is_empty() {
        eprintln!("[xtask] {label}クレート0件");
        return;
    }

    let mut cmd = Command::new("cargo");
    cmd.current_dir(workspace_root).arg("build").arg("--locked");
    if profile == "release" {
        cmd.arg("--release");
    }
    if let Some(triple) = target {
        cmd.arg("--target").arg(triple);
    }
    for c in crates {
        cmd.arg("-p").arg(&c.package_name);
    }

    let status = cmd.status().expect("cargo build 起動失敗");
    if !status.success() {
        panic!("[xtask] {label}ビルド失敗: exit={status}");
    }
}

fn stage_crates(
    workspace_root: &Path,
    profile: &str,
    target: Option<&str>,
    dest_subdir: &str,
    crates: &[DiscoveredCrate],
) {
    let out_dir = target_dir(workspace_root, profile, target);
    let dest_dir = out_dir.join(dest_subdir);
    fs::create_dir_all(&dest_dir).expect("配置先ディレクトリ作成失敗");

    for c in crates {
        let filename = dylib_filename(&c.lib_name);
        let src = out_dir.join(&filename);
        let dst = dest_dir.join(&filename);
        match fs::copy(&src, &dst) {
            Ok(_) => eprintln!("[xtask] 配置: {dest_subdir}/{filename}"),
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
    let mut target: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--release" => release = true,
            "build" | "run" => task = args[i].clone(),
            "--target" => {
                i += 1;
                target = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }
    let profile = if release { "release" } else { "debug" };
    let target = target.as_deref();

    let root = workspace_root();

    let objects = discover_crates(&root, "crates/objects");
    build_crates(&root, profile, target, "objects", &objects);
    stage_crates(&root, profile, target, "objects", &objects);

    let effects = discover_crates(&root, "crates/effects");
    build_crates(&root, profile, target, "effects", &effects);
    stage_crates(&root, profile, target, "effects", &effects);

    // 本体(NeoUtl)は build タスクではビルドのみ、run タスクでは実行まで行う。
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&root)
        .arg(if task == "run" { "run" } else { "build" })
        .arg("--locked")
        .arg("-p")
        .arg("NeoUtl");
    if release {
        cmd.arg("--release");
    }
    if let Some(triple) = target {
        cmd.arg("--target").arg(triple);
    }
    let status = cmd.status().expect("cargo build/run 起動失敗");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
