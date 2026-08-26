use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

mod dxc;
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
            eprintln!("[xtask] {} 読取失敗: {err}", scan_dir.display());
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
            eprintln!("[xtask] {label}クレート0件");
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
        eprintln!("[xtask] ビルド対象パッケージ0件のためcargo呼び出しを省略");
        return;
    }

    apply_toolchain_env(&mut cmd, workspace_root);

    let status = cmd.status().expect("cargo build 起動失敗");
    if !status.success() {
        panic!("[xtask] 統合ビルド失敗: exit={status}");
    }
}

fn apply_toolchain_env(cmd: &mut Command, workspace_root: &Path) {
    let mut extra_paths = Vec::new();

    if slang::slangc_path(workspace_root).is_file() {
        cmd.env("SLANG_DIR", slang::install_dir(workspace_root));
        extra_paths.push(slang::bin_dir(workspace_root));
    }

    if dxc::dxcompiler_path(workspace_root).is_file() {
        extra_paths.push(dxc::bin_dir(workspace_root));
    }

    if extra_paths.is_empty() {
        return;
    }

    let existing_path = env::var_os("PATH").unwrap_or_default();
    let mut paths = extra_paths;
    paths.extend(env::split_paths(&existing_path));
    let Ok(joined_path) = env::join_paths(paths) else {
        eprintln!("[xtask] PATH合成失敗");
        return;
    };
    cmd.env("PATH", joined_path);
}

const PLUGIN_HOST_VERSION: &str = "0.0.7";

fn plugin_host_install_root(workspace_root: &Path) -> PathBuf {
    workspace_root
        .join("target")
        .join("maolan-plugin-host-install")
}

fn stage_plugin_host(workspace_root: &Path, profile: &str, target: Option<&str>, offline: bool) {
    let install_root = plugin_host_install_root(workspace_root);
    let bin_name = exe_filename("maolan-plugin-host");
    let installed_bin = install_root.join("bin").join(&bin_name);

    if !installed_bin.is_file() {
        let mut cmd = Command::new("cargo");
        cmd.arg("install")
            .arg("maolan-plugin-host")
            .arg("--version")
            .arg(PLUGIN_HOST_VERSION)
            .arg("--locked")
            .arg("--root")
            .arg(&install_root);
        if let Some(triple) = target {
            cmd.arg("--target").arg(triple);
        }
        if offline {
            cmd.arg("--offline");
        }
        apply_toolchain_env(&mut cmd, workspace_root);
        let status = cmd
            .status()
            .expect("maolan-plugin-hostインストール起動失敗");
        if !status.success() {
            panic!("[xtask] maolan-plugin-hostインストール失敗: exit={status}");
        }
    }

    let dest_dir = target_dir(workspace_root, profile, target);
    fs::create_dir_all(&dest_dir).expect("配置先ディレクトリ作成失敗");
    let dest = dest_dir.join(&bin_name);
    fs::copy(&installed_bin, &dest).unwrap_or_else(|err| {
        panic!(
            "[xtask] maolan-plugin-host配置失敗: {err} (src={})",
            installed_bin.display()
        )
    });
    eprintln!("[xtask] 配置: {bin_name}");
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
    copy_dir_recursive(&src_dir, &dst_dir).expect("Luaスクリプト配置失敗");
    eprintln!("[xtask] 配置: scripts/（Lua）");
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
        .expect("workspace root解決失敗")
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
    dxc::ensure_installed(&root, offline);
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

    stage_plugin_host(&root, profile, target, offline);
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
        let escaped = escape_yaml_double_quoted(&message);
        output.push_str(&format!("\"{escaped}\": \"{escaped}\"\n"));
    }
    let dir = root.join("i18n");
    fs::create_dir_all(&dir).expect("i18nディレクトリ作成失敗");
    let catalog = dir.join("ja.yml");
    let unchanged = fs::read_to_string(&catalog)
        .map(|current| current == output)
        .unwrap_or(false);
    if !unchanged {
        fs::write(&catalog, output).expect("日本語翻訳ファイル作成失敗");
        eprintln!("[xtask] i18n/ja.ymlを生成しました");
    } else {
        eprintln!("[xtask] i18n/ja.ymlに変更なし");
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

fn escape_yaml_double_quoted(message: &str) -> String {
    let mut escaped = String::with_capacity(message.len());
    for c in message.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if (c as u32) < 0x20 => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}

fn string_literals(source: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut value = String::new();
        loop {
            let Some((_, c)) = chars.next() else {
                break;
            };
            if c == '"' {
                result.push(value);
                break;
            }
            if c != '\\' {
                value.push(c);
                continue;
            }
            let Some((_, escape)) = chars.next() else {
                break;
            };
            match escape {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                'r' => value.push('\r'),
                '0' => value.push('\0'),
                '\\' => value.push('\\'),
                '"' => value.push('"'),
                '\'' => value.push('\''),
                '\n' => {
                    while let Some(&(_, w)) = chars.peek() {
                        if w == ' ' || w == '\t' || w == '\n' || w == '\r' {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                'x' => {
                    let hex: String = (0..2)
                        .filter_map(|_| chars.next().map(|(_, h)| h))
                        .collect();
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        value.push(byte as char);
                    }
                }
                'u' => {
                    if chars.peek().map(|&(_, c)| c) == Some('{') {
                        chars.next();
                        let mut hex = String::new();
                        for (_, h) in chars.by_ref() {
                            if h == '}' {
                                break;
                            }
                            hex.push(h);
                        }
                        if let Some(ch) =
                            u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            value.push(ch);
                        }
                    }
                }
                other => value.push(other),
            }
        }
    }
    result
}
