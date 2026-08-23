use std::env;
use std::path::PathBuf;

fn find_versioned_dir(base: &std::path::Path, prefix: &str) -> PathBuf {
    std::fs::read_dir(base)
        .unwrap_or_else(|e| panic!("ディレクトリ読み取り失敗 {}: {}", base.display(), e))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(prefix))
                    .unwrap_or(false)
        })
        .unwrap_or_else(|| {
            panic!(
                "{}配下に接頭辞{}のディレクトリ未検出",
                base.display(),
                prefix
            )
        })
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let carla_root = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Carla");

    let carla_cmake_dir = carla_root.join("cmake");
    let carla_source_dir = carla_root.join("source");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    let windows_lilv_includes: Option<[PathBuf; 4]> = if target_os == "windows" {
        let lilv_dir = carla_source_dir.join("modules").join("lilv");
        Some([
            find_versioned_dir(&lilv_dir, "serd-"),
            find_versioned_dir(&lilv_dir, "sord-"),
            find_versioned_dir(&lilv_dir, "sratom-"),
            find_versioned_dir(&lilv_dir, "lilv-"),
        ])
    } else {
        None
    };

    println!("cargo:rerun-if-changed={}", carla_cmake_dir.display());
    println!("cargo:rerun-if-changed={}", carla_source_dir.display());
    println!("cargo:rerun-if-changed=src/wrapper.h");

    let mut cmake_config = cmake::Config::new(&carla_cmake_dir);
    cmake_config
        .define("CMAKE_INTERPROCEDURAL_OPTIMIZATION", "OFF")
        .define("CARLA_BUILD_STATIC", "ON")
        .define("CARLA_BUILD_FRAMEWORKS", "OFF")
        .define("CARLA_USE_JACK", "OFF")
        .define("CARLA_USE_OSC", "OFF")
        .define("CARLA_ENABLE_JSFX", "ON");
    if let Some([serd, sord, sratom, lilv]) = &windows_lilv_includes {
        for dir in [serd, sord, sratom, lilv] {
            cmake_config.cflag(format!("-I{}", dir.display()));
            cmake_config.cxxflag(format!("-I{}", dir.display()));
        }
    }

    cmake_config.build_target("carla-standalone");
    let dst = cmake_config.build();

    let build_dir = dst.join("build");
    let mut search_dirs = vec![
        dst.clone(),
        build_dir.clone(),
        dst.join("lib"),
        build_dir.join("Release"),
        build_dir.join("Debug"),
    ];

    if let Ok(entries) = std::fs::read_dir(&build_dir) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    search_dirs.push(entry.path());
                }
            }
        }
    }

    for dir in &search_dirs {
        if dir.exists() {
            println!("cargo:rustc-link-search=native={}", dir.display());
        }
    }

    let static_libs = [
        "carla_standalone2",
        "carla-sfzero",
        "carla-lilv_sratom",
        "carla-jackbridge",
        "carla-lilv_lilv",
        "carla-lilv_sord",
        "carla-lilv_serd",
        "carla-ysfx",
        "carla-audio-decoder",
        "carla-water",
        "carla-native-plugins",
        "carla-rtmempool",
        "carla-zita-resampler",
    ];

    let use_whole_archive = matches!(target_os.as_str(), "linux" | "android");
    for lib in &static_libs {
        if use_whole_archive {
            println!("cargo:rustc-link-lib=static:+whole-archive={}", lib);
        } else {
            println!("cargo:rustc-link-lib=static={}", lib);
        }
    }

    match target_os.as_str() {
        "linux" | "android" => {
            println!("cargo:rustc-link-lib=dylib=pthread");
            println!("cargo:rustc-link-lib=dylib=dl");
            println!("cargo:rustc-link-lib=dylib=m");
            println!("cargo:rustc-link-lib=dylib=rt");
            println!("cargo:rustc-link-lib=dylib=stdc++");
            println!("cargo:rustc-link-lib=dylib=fluidsynth");
            println!("cargo:rustc-link-lib=dylib=sndfile");
            println!("cargo:rustc-link-lib=dylib=X11");
            println!("cargo:rustc-link-lib=dylib=magic");
        }
        "macos" | "ios" => {
            println!("cargo:rustc-link-lib=framework=Cocoa");
            println!("cargo:rustc-link-lib=framework=CoreAudio");
            println!("cargo:rustc-link-lib=framework=CoreMIDI");
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
            println!("cargo:rustc-link-lib=dylib=c++");
        }
        "windows" => {
            println!("cargo:rustc-link-lib=dylib=user32");
            println!("cargo:rustc-link-lib=dylib=gdi32");
            println!("cargo:rustc-link-lib=dylib=shell32");
            println!("cargo:rustc-link-lib=dylib=ole32");
            println!("cargo:rustc-link-lib=dylib=uuid");
            println!("cargo:rustc-link-lib=dylib=winmm");
            println!("cargo:rustc-link-lib=dylib=ws2_32");
            println!("cargo:rustc-link-lib=dylib=shlwapi");
            if target_env == "msvc" {
                println!("cargo:rustc-link-lib=static=pthreads4w");
            } else {
                println!("cargo:rustc-link-lib=dylib=stdc++");
                println!("cargo:rustc-link-lib=dylib=pthread");
            }
        }
        _ => {}
    }

    let mut include_dirs = vec![
        carla_source_dir.clone(),
        carla_source_dir.join("backend"),
        carla_source_dir.join("backend").join("engine"),
        carla_source_dir.join("backend").join("plugin"),
        carla_source_dir.join("includes"),
        carla_source_dir.join("modules"),
        carla_source_dir.join("modules").join("distrho"),
        carla_source_dir.join("modules").join("water"),
        carla_source_dir.join("utils"),
    ];
    if let Some(dirs) = &windows_lilv_includes {
        include_dirs.extend(dirs.iter().cloned());
    }

    println!("cargo:rerun-if-changed=src/carla_bridge_ext.cpp");
    let mut cc_builder = cc::Build::new();
    cc_builder
        .cpp(true)
        .std("c++11")
        .file("src/carla_bridge_ext.cpp");
    if target_os == "windows" {
        cc_builder.define("BUILDING_CARLA", None);
    }
    for inc in &include_dirs {
        cc_builder.include(inc);
    }
    cc_builder.compile("carla_bridge_ext");

    let mut bindgen_builder = bindgen::Builder::default()
        .header("src/wrapper.h")
        .clang_args(["-x", "c++", "-std=c++11"]);
    if target_os == "windows" {
        bindgen_builder = bindgen_builder.clang_arg("-DBUILDING_CARLA");
    }
    let mut bindgen_builder = bindgen_builder
        .allowlist_function("carla_.*")
        .allowlist_type(".*Carla.*")
        .allowlist_type("Carla.*")
        .allowlist_type(".*carla.*")
        .allowlist_type("Plugin.*")
        .allowlist_type("Engine.*")
        .allowlist_type("BinaryType")
        .allowlist_type("CustomData")
        .allowlist_type("Parameter.*")
        .allowlist_type("MidiProgram.*")
        .allowlist_var(".*CARLA_.*")
        .allowlist_var(".*ENGINE_.*")
        .allowlist_var(".*PLUGIN_.*")
        .allowlist_var(".*MAX_.*")
        .allowlist_var(".*CUSTOM_.*")
        .allowlist_var(".*MAIN_CARLA_.*")
        .derive_default(true)
        .derive_debug(true)
        .derive_copy(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    for inc in &include_dirs {
        bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", inc.display()));
    }

    let bindings = bindgen_builder
        .generate()
        .expect("Unable to generate Carla host bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings to OUT_DIR");
}
