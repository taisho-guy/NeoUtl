use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

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
    "gstreamer-decoder",
];

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

/// objects/effects/decoders/themes/NeoUtl本体を単一のcargo呼び出しへ集約してビルドする。
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

    slang::apply_build_env(&mut cmd, workspace_root);

    let status = cmd.status().expect("cargo build 起動失敗");
    if !status.success() {
        panic!("[xtask] 統合ビルド失敗: exit={status}");
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
    let status = cmd.status().expect("vst3-host-probeビルド起動失敗");
    if !status.success() {
        panic!("[xtask] vst3-host-probeビルド失敗: exit={status}");
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
    let status = helper.status().expect("vst3-host-helperビルド起動失敗");
    if !status.success() {
        panic!("[xtask] vst3-host-helperビルド失敗: exit={status}");
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

    slang::ensure_installed(&root, offline);
    build_vst3_helpers(&root, profile, target, offline);

    let objects = discover_crates(&root, "crates/objects");
    let effects = discover_crates(&root, "crates/effects");
    let decoders = discover_crates(&root, "crates/media");
    let themes = discover_crates(&root, "crates/themes");

    build_all(
        &root,
        profile,
        target,
        offline,
        &[
            ("objects", objects.as_slice()),
            ("effects", effects.as_slice()),
            ("decoders", decoders.as_slice()),
            ("themes", themes.as_slice()),
        ],
        &["NeoUtl"],
        &lua_feature,
    );

    stage_crates(&root, profile, target, "objects", &objects);
    stage_crates(&root, profile, target, "effects", &effects);
    stage_crates(&root, profile, target, "decoders", &decoders);
    stage_crates(&root, profile, target, "themes", &themes);
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
