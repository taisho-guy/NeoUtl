use std::path::PathBuf;
use std::process::Command;

fn main() {
    slint_build::compile("src/app.slint").unwrap();

    if std::env::var("NEOUTL_BUILDING_PLUGINS").is_ok() {
        return;
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let target_profile_dir = out_dir
        .ancestors()
        .find(|p| p.file_name().and_then(|s| s.to_str()) == Some("build"))
        .and_then(|p| p.parent())
        .expect("Failed to find target profile directory");

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let cargo_profile = if profile == "debug" { "dev" } else { &profile };

    let ext = if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };

    let plugin_target_dir = manifest_dir.join("target").join("plugin_target");
    let dest_objects_dir = target_profile_dir.join("objects");
    std::fs::create_dir_all(&dest_objects_dir).unwrap();

    println!("cargo:rerun-if-changed=crates/objects");

    let objects_root = manifest_dir.join("crates").join("objects");
    let plugin_dirs: Vec<PathBuf> = std::fs::read_dir(&objects_root)
        .expect("crates/objects ディレクトリが存在しません")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("Cargo.toml").exists())
        .collect();

    for plugin_dir in &plugin_dirs {
        let plugin_manifest = plugin_dir.join("Cargo.toml");

        let status = Command::new("cargo")
            .args([
                "build",
                "--manifest-path",
                plugin_manifest.to_str().unwrap(),
                "--profile",
                cargo_profile,
                "--target-dir",
                plugin_target_dir.to_str().unwrap(),
            ])
            .env("NEOUTL_BUILDING_PLUGINS", "1")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .env_remove("CARGO_MANIFEST_DIR")
            .status()
            .expect("Failed to execute cargo build for plugin");

        if !status.success() {
            panic!("Failed to build plugin: {}", plugin_dir.display());
        }

        let search_dirs = [
            plugin_target_dir.join(&profile),
            plugin_target_dir.join(&profile).join("deps"),
        ];

        let dylibs: Vec<PathBuf> = search_dirs
            .iter()
            .flat_map(|d| std::fs::read_dir(d).into_iter().flatten())
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(ext))
            .collect();

        let manifest_text = std::fs::read_to_string(&plugin_manifest).unwrap();
        let crate_name = manifest_text
            .lines()
            .find_map(|l| l.trim().strip_prefix("name").map(|_| l))
            .and_then(|l| l.split('"').nth(1))
            .expect("crate name を Cargo.toml から取得できません")
            .replace('-', "_");

        let matched = dylibs
            .iter()
            .find(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.contains(&crate_name))
                    .unwrap_or(false)
            })
            .unwrap_or_else(|| panic!("プラグイン生成物が見つかりません: {crate_name}"));

        let dest_path = dest_objects_dir.join(matched.file_name().unwrap());
        std::fs::copy(matched, &dest_path).expect("Failed to copy plugin library");
    }
}
