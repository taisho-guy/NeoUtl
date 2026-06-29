use std::path::PathBuf;
use std::process::Command;

fn main() {
    slint_build::compile("src/app.slint").unwrap();

    // Prevent recursive building when cargo is compiling the sub-crates
    if std::env::var("NEOUTL_BUILDING_PLUGINS").is_ok() {
        return;
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Find the target profile directory (e.g. target/debug or target/release) by locating the parent of the 'build' directory
    let target_profile_dir = out_dir
        .ancestors()
        .find(|p| p.file_name().and_then(|s| s.to_str()) == Some("build"))
        .and_then(|p| p.parent())
        .expect("Failed to find target profile directory");

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    // Map cargo profile name to standard cargo build profiles
    let cargo_profile = if profile == "debug" { "dev" } else { &profile };

    let plugins = [
        ("crates/objects/tetrahedron", "libneoutl_object_tetrahedron"),
        ("crates/objects/cube", "libneoutl_object_cube"),
        ("crates/objects/text", "libneoutl_object_text"),
    ];

    let ext = if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };

    // Use a separate target directory to avoid Cargo target locking issues
    let plugin_target_dir = manifest_dir.join("target").join("plugin_target");
    let dest_objects_dir = target_profile_dir.join("objects");
    std::fs::create_dir_all(&dest_objects_dir).unwrap();

    // Re-run whenever plugin source code changes
    println!("cargo:rerun-if-changed=crates/objects");

    for (plugin_path_rel, lib_name) in &plugins {
        let plugin_manifest = manifest_dir.join(plugin_path_rel).join("Cargo.toml");

        let status = Command::new("cargo")
            .args(&[
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
            panic!("Failed to build plugin: {}", plugin_path_rel);
        }

        // Try to find the compiled library in the separate target directory or its deps
        let search_dirs = [
            plugin_target_dir.join(&profile),
            plugin_target_dir.join(&profile).join("deps"),
        ];

        let mut copied = false;
        let candidate_names = [
            format!("{}.{}", lib_name, ext),
            format!(
                "{}.{}",
                lib_name.strip_prefix("lib").unwrap_or(lib_name),
                ext
            ),
        ];

        for search_dir in &search_dirs {
            for name in &candidate_names {
                let src_path = search_dir.join(name);
                if src_path.exists() {
                    let dest_path = dest_objects_dir.join(name);
                    std::fs::copy(&src_path, &dest_path).expect("Failed to copy plugin library");
                    copied = true;
                    break;
                }
            }
            if copied {
                break;
            }
        }

        if !copied {
            panic!(
                "Could not find compiled plugin library for {} in search paths",
                lib_name
            );
        }
    }
}
