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
}

/// workspace_root/subdir 直下の各ディレクトリのCargo.tomlを走査し、
/// package.name と lib.name（未指定時はpackage.nameの'-'を'_'置換）を収集する。
/// 新規追加クレートはディレクトリを置くだけで自動検出対象になる。
/// subdirには"crates/objects"・"crates/effects"・"crates/media"のいずれも渡せる
/// （3者は同一走査規則）。
/// workspace([workspace].members)から意図的に除外されたクレートディレクトリ。
/// cargo build -p はworkspace非対象パッケージを解決できないため、ディレクトリが
/// 存在していてもここに含まれるものはxtaskの検出対象から外す。
/// ffmpeg-decoder: gstreamer/symphonia経路で代替、要件確定まで凍結（Cargo.toml側の
/// [workspace].membersコメントアウトと対で管理する）。
/// gstreamer-encoder: NeoUtl本体へ直接静的リンク（export.rsから使用）。dlsymプラグイン
/// ではないため除外。
/// gpuvideo-decoder/gpuvideo-encoder: NeoUtl本体からgpu_video共有デバイス注入のため
/// path依存として直接静的リンクされ、native_plugins()/native_vtables()経由で自己登録する。
/// dlsymプラグインではないため、decoders/への配置対象から外す。
const WORKSPACE_EXCLUDED_DIRS: &[&str] = &[
    "audio-plugin-host",
    "ffmpeg-decoder",
    "gstreamer-encoder",
    "gpuvideo-decoder",
    "gpuvideo-encoder",
];

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

fn exe_filename(bin_name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{bin_name}.exe")
    } else {
        bin_name.to_owned()
    }
}

/// objects/effects/decoders/NeoUtl本体を単一のcargo呼び出しへ集約してビルドする。
/// 呼び出しを分割すると、cargoのfeature unification（resolver 2）が呼び出し単位で
/// 独立に行われるため、要求パッケージ集合の違い（例: decoders単体呼び出しと
/// NeoUtl本体呼び出しでwgpu等の要求feature集合が食い違う）により同一依存クレートが
/// 呼び出しごとに異なるfingerprintで再ビルドされ、互いのキャッシュを破棄し合う。
/// 全パッケージを1回のcargo build -pの列挙に含めることでfeature解決を1本化し、
/// この相互キャッシュ破棄を排除する。
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
    // [target.'cfg(...)']による自動切替ではなく、常にこの明示featureで選択する。
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

fn build_vst3_helpers(workspace_root: &Path, profile: &str, target: Option<&str>, offline: bool) {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(workspace_root)
        .arg("build")
        .arg("--locked")
        .arg("-p")
        .arg("vst3-host")
        .arg("--bin")
        .arg("vst3-host-probe");
    if profile == "release" {
        cmd.arg("--release");
    }
    if let Some(target) = target {
        cmd.arg("--target").arg(target);
    }
    if offline {
        cmd.arg("--offline");
    }
    let status = cmd.status().expect(&t!("vst3-host-probeビルド起動失敗"));
    if !status.success() {
        panic!(
            "{}",
            t!(
                "[xtask] vst3-host-probeビルド失敗: exit=%{arg0}",
                arg0 = format!("{status}")
            )
        );
    }

    let mut helper = Command::new("cargo");
    helper
        .current_dir(workspace_root)
        .arg("build")
        .arg("--locked")
        .arg("-p")
        .arg("vst3-host")
        .arg("--bin")
        .arg("vst3-host-helper");
    if profile == "release" {
        helper.arg("--release");
    }
    if let Some(target) = target {
        helper.arg("--target").arg(target);
    }
    if offline {
        helper.arg("--offline");
    }
    let status = helper
        .status()
        .expect(&t!("vst3-host-helperビルド起動失敗"));
    if !status.success() {
        panic!(
            "{}",
            t!(
                "[xtask] vst3-host-helperビルド失敗: exit=%{arg0}",
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
    }
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
    build_vst3_helpers(&root, profile, target, offline);

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

/// Collect Japanese string literals from application and plugin sources and
/// write the canonical Japanese catalog. Translation keys are deliberately
/// the source text itself; internal IDs and numeric effect constraints are
/// never inspected or modified here.
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
            .replace('\n', "\\n");
        output.push_str(&format!("\"{escaped}\": \"{escaped}\"\n"));
    }
    let dir = root.join("i18n");
    fs::create_dir_all(&dir).expect(&t!("i18nディレクトリ作成失敗"));
    fs::write(dir.join("ja.yml"), output).expect(&t!("日本語翻訳ファイル作成失敗"));
    eprintln!("{}", t!("[xtask] i18n/ja.ymlを生成しました"));
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
        for (_, c) in chars.by_ref() {
            if escaped {
                value.push(c);
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                value.push(c);
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
