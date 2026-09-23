use serde::Deserialize;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

const RELEASES_API_URL: &str =
    "https://api.github.com/repos/microsoft/DirectXShaderCompiler/releases/latest";
const VERSION_MARKER_FILENAME: &str = ".dxc-version";

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
    workspace_root.join("dxc")
}

fn arch_dir() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "x64"
    }
}

pub fn bin_dir(workspace_root: &Path) -> PathBuf {
    install_dir(workspace_root).join("bin").join(arch_dir())
}

pub fn dxcompiler_path(workspace_root: &Path) -> PathBuf {
    bin_dir(workspace_root).join("dxcompiler.dll")
}

fn version_marker_path(dxc_dir: &Path) -> PathBuf {
    dxc_dir.join(VERSION_MARKER_FILENAME)
}

fn read_installed_tag(dxc_dir: &Path) -> Option<String> {
    let text = fs::read_to_string(version_marker_path(dxc_dir)).ok()?;
    let tag = text.trim();
    (!tag.is_empty()).then(|| tag.to_string())
}

fn write_installed_tag(dxc_dir: &Path, tag_name: &str) {
    if let Err(err) = fs::write(version_marker_path(dxc_dir), tag_name) {
        eprintln!("[xtask][dxc] バージョン記録失敗: {err}");
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

fn select_asset(assets: &[ReleaseAsset]) -> Option<&ReleaseAsset> {
    assets.iter().find(|asset| {
        let name = asset.name.to_ascii_lowercase();
        name.starts_with("dxc_") && name.ends_with(".zip")
    })
}

fn download_asset_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut req = ureq::get(url).header("User-Agent", "NeoUtl-xtask");
    if let Some(token) = github_token() {
        req = req.header("Authorization", &format!("Bearer {token}"));
    }
    let mut response = req
        .call()
        .map_err(|err| format!("dxcアセットダウンロード失敗: {err}"))?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut bytes)
        .map_err(|err| format!("dxcアセット読込失敗: {err}"))?;
    Ok(bytes)
}

fn extract_zip(bytes: &[u8], dxc_dir: &Path) -> Result<(), String> {
    if dxc_dir.exists() {
        fs::remove_dir_all(dxc_dir).map_err(|err| format!("既存dxc削除失敗: {err}"))?;
    }
    fs::create_dir_all(dxc_dir).map_err(|err| format!("dxcディレクトリ作成失敗: {err}"))?;

    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|err| format!("zip解析失敗: {err}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("zipエントリ読込失敗: {err}"))?;
        let Some(relative_path) = entry.enclosed_name() else {
            eprintln!("[xtask][dxc] 不正なzipエントリを無視: index={index}");
            continue;
        };
        let dest_path = dxc_dir.join(&relative_path);

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
    }
    Ok(())
}

pub fn ensure_installed(workspace_root: &Path, offline: bool) {
    if !cfg!(windows) {
        return;
    }

    let dxc_dir = install_dir(workspace_root);
    let installed_tag = read_installed_tag(&dxc_dir);
    let local_installed = installed_tag.is_some() && dxcompiler_path(workspace_root).is_file();

    if offline {
        if local_installed {
            eprintln!(
                "[xtask][dxc] --offline指定のため更新確認をスキップ: 導入済み {}",
                installed_tag.as_deref().unwrap_or("")
            );
            return;
        }
        panic!(
            "[xtask][dxc] --offline指定だがdxc未導入（{}が存在しません）",
            dxc_dir.display()
        );
    }

    let release = match fetch_latest_release() {
        Ok(release) => release,
        Err(err) => {
            if local_installed {
                eprintln!(
                    "[xtask][dxc] 更新確認をスキップ（{err}）。導入済みのdxcを継続利用します"
                );
                return;
            }
            panic!("[xtask][dxc] dxc未導入かつ取得失敗のためビルド続行不可: {err}");
        }
    };

    if installed_tag.as_deref() == Some(release.tag_name.as_str()) && local_installed {
        eprintln!("[xtask][dxc] 最新版導入済み: {}", release.tag_name);
        return;
    }

    let Some(asset) = select_asset(&release.assets) else {
        panic!(
            "[xtask][dxc] リリース{}に対応アセットなし",
            release.tag_name
        );
    };

    eprintln!(
        "[xtask][dxc] 導入開始: {} ({})",
        release.tag_name, asset.browser_download_url
    );
    let bytes = download_asset_bytes(&asset.browser_download_url)
        .unwrap_or_else(|err| panic!("[xtask][dxc] ダウンロード失敗: {err}"));
    extract_zip(&bytes, &dxc_dir).unwrap_or_else(|err| panic!("[xtask][dxc] 展開失敗: {err}"));
    write_installed_tag(&dxc_dir, &release.tag_name);
    eprintln!("[xtask][dxc] 導入完了: {}", dxc_dir.display());
}
