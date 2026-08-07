use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

const RELEASES_API_URL: &str = "https://api.github.com/repos/shader-slang/slang/releases/latest";
const VERSION_MARKER_FILENAME: &str = ".slang-version";

fn slangc_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "slangc.exe"
    } else {
        "slangc"
    }
}

fn find_system_slangc() -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    let filename = slangc_filename();
    env::split_paths(&path_var)
        .map(|dir| dir.join(filename))
        .find(|candidate| candidate.is_file())
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct ReleaseInfo {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

pub fn install_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join("slang")
}

pub fn bin_dir(workspace_root: &Path) -> PathBuf {
    install_dir(workspace_root).join("bin")
}

fn version_marker_path(slang_dir: &Path) -> PathBuf {
    slang_dir.join(VERSION_MARKER_FILENAME)
}

fn read_installed_tag(slang_dir: &Path) -> Option<String> {
    let text = fs::read_to_string(version_marker_path(slang_dir)).ok()?;
    let tag = text.trim();
    (!tag.is_empty()).then(|| tag.to_string())
}

fn write_installed_tag(slang_dir: &Path, tag_name: &str) {
    if let Err(err) = fs::write(version_marker_path(slang_dir), tag_name) {
        eprintln!(
            "{}",
            t!(
                "[xtask][slang] バージョン記録失敗: %{arg0}",
                arg0 = format!("{}", err)
            )
        );
    }
}

fn platform_tag() -> &'static str {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    match (os, arch) {
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("macos", "aarch64") => "macos-aarch64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        _ => unreachable!("未対応OS/CPUアーキテクチャ組合せ"),
    }
}

fn github_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .or_else(|_| std::env::var("GH_TOKEN"))
        .ok()
        .filter(|t| !t.is_empty())
}

fn fetch_latest_release() -> Result<ReleaseInfo, String> {
    let mut req = ureq::get(RELEASES_API_URL)
        .header("User-Agent", "NeoUtl-xtask")
        .header("Accept", "application/vnd.github+json");
    if let Some(token) = github_token() {
        req = req.header("Authorization", &format!("Bearer {token}"));
    }
    req.call()
        .map_err(|err| format!("GitHub Releases API取得失敗: {err}"))?
        .body_mut()
        .read_json::<ReleaseInfo>()
        .map_err(|err| format!("GitHub Releases APIレスポンス解析失敗: {err}"))
}

fn select_asset<'a>(assets: &'a [ReleaseAsset], platform_tag: &str) -> Option<&'a ReleaseAsset> {
    let suffix = format!("-{platform_tag}.zip");
    assets.iter().find(|asset| asset.name.ends_with(&suffix))
}

fn download_asset_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut req = ureq::get(url).header("User-Agent", "NeoUtl-xtask");
    if let Some(token) = github_token() {
        req = req.header("Authorization", &format!("Bearer {token}"));
    }
    let mut response = req
        .call()
        .map_err(|err| format!("Slangアセットダウンロード失敗: {err}"))?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut bytes)
        .map_err(|err| format!("Slangアセット読込失敗: {err}"))?;
    Ok(bytes)
}

fn extract_zip(bytes: &[u8], slang_dir: &Path) -> Result<(), String> {
    if slang_dir.exists() {
        fs::remove_dir_all(slang_dir).map_err(|err| format!("既存slang削除失敗: {err}"))?;
    }
    fs::create_dir_all(slang_dir).map_err(|err| format!("slangディレクトリ作成失敗: {err}"))?;

    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|err| format!("zip解析失敗: {err}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("zipエントリ読込失敗: {err}"))?;
        let Some(relative_path) = entry.enclosed_name() else {
            eprintln!("{}", t!("[xtask][slang] 不正なzipエントリを無視: %{arg0}"));
            continue;
        };
        let dest_path = slang_dir.join(&relative_path);

        if entry.is_dir() {
            fs::create_dir_all(&dest_path).map_err(|err| format!("展開先作成失敗: {err}"))?;
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("展開先作成失敗: {err}"))?;
        }
        let mut out_file =
            fs::File::create(&dest_path).map_err(|err| format!("展開ファイル作成失敗: {err}"))?;
        std::io::copy(&mut entry, &mut out_file).map_err(|err| format!("展開失敗: {err}"))?;

        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dest_path, fs::Permissions::from_mode(mode));
        }
    }
    Ok(())
}

pub fn ensure_installed(workspace_root: &Path, offline: bool) {
    let slang_dir = install_dir(workspace_root);
    let installed_tag = read_installed_tag(&slang_dir);
    let local_installed = installed_tag.is_some() && slang_dir.is_dir();

    if offline {
        if local_installed {
            eprintln!(
                "{}",
                t!("[xtask][slang] --offline指定のため更新確認をスキップ: 導入済み %{arg0}")
            );
            return;
        }
        if let Some(system_slangc) = find_system_slangc() {
            eprintln!(
                "{}",
                t!(
                    "[xtask][slang] --offline指定のため更新確認をスキップ: システム導入済みslangcを利用 (%{arg0})",
                    arg0 = system_slangc.display()
                )
            );
            return;
        }
        panic!(
            "[xtask][slang] --offline指定だがSlang未導入（{}が存在せず、PATH上にもslangcが見つかりません）",
            slang_dir.display()
        );
    }

    let platform = platform_tag();

    let release = match fetch_latest_release() {
        Ok(release) => release,
        Err(err) => {
            if local_installed {
                eprintln!(
                    "{}",
                    t!(
                        "[xtask][slang] 更新確認をスキップ（%{arg0}）。導入済みのSlangを継続利用します",
                        arg0 = format!("{}", err)
                    )
                );
                return;
            }
            if let Some(system_slangc) = find_system_slangc() {
                eprintln!(
                    "{}",
                    t!(
                        "[xtask][slang] 更新確認失敗（%{arg0}）。システム導入済みslangcを利用します (%{arg1})",
                        arg0 = format!("{}", err),
                        arg1 = system_slangc.display()
                    )
                );
                return;
            }
            panic!(
                "[xtask][slang] Slang未導入（project_root/slang・システムPATH双方とも見つからず）かつ取得失敗のためビルド続行不可: {err}"
            );
        }
    };

    if installed_tag.as_deref() == Some(release.tag_name.as_str()) && slang_dir.is_dir() {
        eprintln!("{}", t!("[xtask][slang] 最新版導入済み: %{arg0}"));
        return;
    }

    let Some(asset) = select_asset(&release.assets, platform) else {
        panic!(
            "[xtask][slang] リリース{}に対応アセットなし（platform={platform}）",
            release.tag_name
        );
    };

    eprintln!("{}", t!("[xtask][slang] 導入開始: %{arg0} (%{arg1})"));
    let bytes = download_asset_bytes(&asset.browser_download_url)
        .unwrap_or_else(|err| panic!("[xtask][slang] ダウンロード失敗: {err}"));
    extract_zip(&bytes, &slang_dir).unwrap_or_else(|err| panic!("[xtask][slang] 展開失敗: {err}"));
    write_installed_tag(&slang_dir, &release.tag_name);
    eprintln!("{}", t!("[xtask][slang] 導入完了: %{arg0}"));
}

pub fn apply_build_env(cmd: &mut Command, workspace_root: &Path) {
    let slang_dir = install_dir(workspace_root);
    let bin_dir = bin_dir(workspace_root);
    let slangc_path = bin_dir.join(slangc_filename());
    if !slangc_path.is_file() {
        return;
    }

    cmd.env("SLANG_DIR", &slang_dir);

    let existing_path = env::var_os("PATH").unwrap_or_default();
    let mut paths: Vec<PathBuf> = vec![bin_dir];
    paths.extend(env::split_paths(&existing_path));
    let Ok(joined_path) = env::join_paths(paths) else {
        eprintln!(
            "{}",
            t!("[xtask][slang] PATH合成失敗、SLANG_DIRのみ設定します")
        );
        return;
    };
    cmd.env("PATH", joined_path);
}
