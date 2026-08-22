use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

rust_i18n::i18n!("../../i18n");
#[macro_use]
extern crate rust_i18n;

mod slang;

struct DiscoveredCrate {
    package_name: String,
    lib_name: String,
    source_dir: PathBuf,
}

const WORKSPACE_EXCLUDED_DIRS: &[&str] =
    &["gstreamer-encoder", "gpuvideo-decoder", "gpuvideo-encoder"];

fn discover_crates(workspace_root: &Path, subdir: &str) -> Vec<DiscoveredCrate> {
    let scan_dir = workspace_root.join(subdir);
    let mut result = Vec::new();

    let entries = match fs::read_dir(&scan_dir) {
        Ok(e) => e,
        Err(err) => {
            eprintln!(
                "{}",
                t!(
                    "[xtask] %{arg0} 読取失敗: %{arg1}",
                    arg0 = format!("{}", scan_dir.display()),
                    arg1 = format!("{err}")
                )
            );
            return result;
        }
    };

    for entry in entries.flatten() {
        let manifest_dir = entry.path();
        if !manifest_dir.is_dir() {
            continue;
        }
        if manifest_dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| WORKSPACE_EXCLUDED_DIRS.contains(&name))
        {
            continue;
        }
        let manifest_path = manifest_dir.join("Cargo.toml");
        let Ok(text) = fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(doc) = text.parse::<toml::Table>() else {
            eprintln!(
                "{}",
                t!(
                    "[xtask] 解析失敗: %{arg0}",
                    arg0 = format!("{}", manifest_path.display())
                )
            );
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
            source_dir: manifest_dir,
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

fn target_dir(workspace_root: &Path, profile: &str, target: Option<&str>) -> PathBuf {
    match target {
        Some(triple) => workspace_root.join("target").join(triple).join(profile),
        None => workspace_root.join("target").join(profile),
    }
}

fn exe_filename(bin_name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{bin_name}.exe")
    } else {
        bin_name.to_owned()
    }
}

fn build_all<'a>(
    workspace_root: &Path,
    profile: &str,
    target: Option<&str>,
    offline: bool,
    groups: &[(&str, &'a [DiscoveredCrate])],
    extra_packages: &[&str],
    lua_feature: &str,
) {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(workspace_root).arg("build").arg("--locked");
    if profile == "release" {
        cmd.arg("--release");
    }
    if offline {
        cmd.arg("--offline");
    }
    if let Some(triple) = target {
        cmd.arg("--target").arg(triple);
    }
    const MLUA_CONSUMER_CRATES: &[&str] = &["neoutl-lua-runtime", "neoutl-effect-lua"];
    for pkg in MLUA_CONSUMER_CRATES {
        cmd.arg("-p")
            .arg(pkg)
            .arg("--features")
            .arg(format!("{pkg}/{lua_feature}"));
    }

    let mut package_count = 0usize;
    for (label, crates) in groups {
        if crates.is_empty() {
            eprintln!(
                "{}",
                t!("[xtask] %{arg0}クレート0件", arg0 = format!("{label}"))
            );
            continue;
        }
        for c in *crates {
            cmd.arg("-p").arg(&c.package_name);
            package_count += 1;
        }
    }
    for pkg in extra_packages {
        cmd.arg("-p").arg(pkg);
        package_count += 1;
    }

    if package_count == 0 {
        eprintln!(
            "{}",
            t!("[xtask] ビルド対象パッケージ0件のためcargo呼び出しを省略")
        );
        return;
    }

    slang::apply_build_env(&mut cmd, workspace_root);

    let status = cmd.status().expect(&t!("cargo build 起動失敗"));
    if !status.success() {
        panic!(
            "{}",
            t!(
                "[xtask] 統合ビルド失敗: exit=%{arg0}",
                arg0 = format!("{status}")
            )
        );
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
    fs::create_dir_all(&dest_dir).expect(&t!("配置先ディレクトリ作成失敗"));

    for c in crates {
        let filename = dylib_filename(&c.lib_name);
        let src = out_dir.join(&filename);
        let dst = dest_dir.join(&filename);
        match fs::copy(&src, &dst) {
            Ok(_) => eprintln!(
                "{}",
                t!(
                    "[xtask] 配置: %{arg0}/%{arg1}",
                    arg0 = format!("{dest_subdir}"),
                    arg1 = format!("{filename}")
                )
            ),
            Err(err) => eprintln!(
                "{}",
                t!(
                    "[xtask] 配置失敗 %{arg0}: %{arg1} (src=%{arg2})",
                    arg0 = format!("{filename}"),
                    arg1 = format!("{err}"),
                    arg2 = format!("{}", src.display())
                )
            ),
        }
        let catalog_src = c.source_dir.join("i18n");
        if catalog_src.is_dir() {
            let catalog_dst = dest_dir.join("i18n").join(&c.lib_name);
            if let Err(err) = copy_i18n(&catalog_src, &catalog_dst) {
                eprintln!("[xtask] 翻訳配置失敗 {filename}: {err}");
            }
        }
    }
}

fn copy_i18n(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yml") {
            fs::copy(&path, dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn stage_scripts(workspace_root: &Path, profile: &str, target: Option<&str>) {
    let src_dir = workspace_root.join("scripts");
    if !src_dir.is_dir() {
        return;
    }
    let dst_dir = target_dir(workspace_root, profile, target).join("scripts");
    copy_dir_recursive(&src_dir, &dst_dir).expect(&t!("Luaスクリプト配置失敗"));
    eprintln!("{}", t!("[xtask] 配置: scripts/（Lua）"));
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("lua") {
            fs::copy(path, target)?;
        }
    }
    Ok(())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect(&t!("workspace root解決失敗"))
        .to_path_buf()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut release = false;
    let mut offline = false;
    let mut task = "run".to_string();
    let mut target: Option<String> = None;
    let mut lua_feature = "luajit".to_string();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--release" => release = true,
            "--offline" => offline = true,
            "build" | "run" => task = args[i].clone(),
            "--target" => {
                i += 1;
                target = args.get(i).cloned();
            }
            "--lua-feature" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    lua_feature = v.clone();
                }
            }
            _ => {}
        }
        i += 1;
    }
    let profile = if release { "release" } else { "debug" };
    let target = target.as_deref();

    let root = workspace_root();
    generate_japanese_i18n(&root);

    slang::ensure_installed(&root, offline);
    let objects = discover_crates(&root, "crates/objects");
    let effects = discover_crates(&root, "crates/effects");
    let decoders = discover_crates(&root, "crates/media");
    build_all(
        &root,
        profile,
        target,
        offline,
        &[
            ("objects", objects.as_slice()),
            ("effects", effects.as_slice()),
            ("decoders", decoders.as_slice()),
        ],
        &["NeoUtl"],
        &lua_feature,
    );

    stage_crates(&root, profile, target, "objects", &objects);
    stage_crates(&root, profile, target, "effects", &effects);
    stage_crates(&root, profile, target, "decoders", &decoders);
    fs::create_dir_all(target_dir(&root, profile, target).join("easings"))
        .expect("easings配置先ディレクトリ作成失敗");
    stage_scripts(&root, profile, target);

    if task != "run" {
        return;
    }

    let bin_path = target_dir(&root, profile, target).join(exe_filename("NeoUtl"));
    let status = Command::new(&bin_path)
        .current_dir(&root)
        .status()
        .unwrap_or_else(|e| panic!("[xtask] バイナリ起動失敗 ({}): {e}", bin_path.display()));
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn generate_japanese_i18n(root: &Path) {
    let mut messages = std::collections::BTreeSet::new();
    let mut files = Vec::new();
    collect_source_files(&root.join("src"), &mut files);
    collect_source_files(&root.join("crates"), &mut files);
    collect_source_files(&root.join("scripts"), &mut files);
    for path in files {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        for literal in string_literals(&source) {
            if literal.chars().any(|c| ('ぁ'..='龯').contains(&c)) {
                messages.insert(literal);
            }
        }
    }
    let mut output = String::from("# Generated by `cargo run -p xtask -- i18n`.\n");
    for message in messages {
        let escaped = message
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
            .replace('\t', "\\t");
        output.push_str(&format!("\"{escaped}\": \"{escaped}\"\n"));
    }
    let dir = root.join("i18n");
    fs::create_dir_all(&dir).expect(&t!("i18nディレクトリ作成失敗"));
    let catalog = dir.join("ja.yml");
    let unchanged = fs::read_to_string(&catalog)
        .map(|current| current == output)
        .unwrap_or(false);
    if !unchanged {
        fs::write(&catalog, output).expect(&t!("日本語翻訳ファイル作成失敗"));
        eprintln!("{}", t!("[xtask] i18n/ja.ymlを生成しました"));
    } else {
        eprintln!("{}", t!("[xtask] i18n/ja.ymlに変更なし"));
    }
}

fn collect_source_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(&path, files);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "lua")
        ) {
            files.push(path);
        }
    }
}

fn string_literals(source: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut value = String::new();
        let mut escaped = false;
        while let Some((_, c)) = chars.next() {
            if escaped {
                if c == '\r' || c == '\n' {
                    if c == '\r' && matches!(chars.peek(), Some((_, '\n'))) {
                        chars.next();
                    }
                    while matches!(chars.peek(), Some((_, ' ' | '\t'))) {
                        chars.next();
                    }
                    escaped = false;
                    continue;
                }
                value.push(c);
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == '"' {
                result.push(value);
                break;
            }
            value.push(c);
        }
    }
    result
}
