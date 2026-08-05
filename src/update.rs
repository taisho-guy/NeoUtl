//! Codeberg(Gitea) `taisho-guy/NeoUtl` リリースAPIを用いた自己アップデート機構。
//! GitHub非互換のため`self_update`のbuiltin backend（github/gitlab）は使用せず、
//! リリースメタデータ取得は`ureq`直叩き、ダウンロード・展開は`zip`直接処理、
//! 自己置換のみ`self_replace`クレートへ委譲する。
//!
//! 注意: 自己置換は`self_replace`クレートのAPI表面に依存する。

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const API_BASE: &str = "https://codeberg.org/api/v1/repos/taisho-guy/NeoUtl";

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub asset_url: String,
    pub asset_name: String,
}

#[derive(Clone, Debug, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading(f32),
    Installed,
    Error(String),
}

#[derive(serde::Deserialize)]
struct ReleaseAssetResponse {
    name: String,
    browser_download_url: String,
}

#[derive(serde::Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    body: String,
    assets: Vec<ReleaseAssetResponse>,
}

/// build.yml matrix.artifact_nameと一致させる。prerelease限定運用のため
/// prerelease判定は行わず、常に先頭（最新publish）を採用する。
fn current_asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("neoutl-linux-x86_64.zip"),
        ("linux", "aarch64") => Some("neoutl-linux-arm64.zip"),
        ("macos", "aarch64") => Some("neoutl-macos-aarch64.zip"),
        ("macos", "x86_64") => Some("neoutl-macos-x86_64.zip"),
        ("windows", "x86_64") => Some("neoutl-windows-x86_64.zip"),
        ("windows", "aarch64") => Some("neoutl-windows-arm64.zip"),
        _ => None,
    }
}

fn current_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "NeoUtl.exe"
    } else {
        "NeoUtl"
    }
}

fn fetch_latest_release() -> Result<UpdateInfo, String> {
    let asset_name =
        current_asset_name().ok_or_else(|| t!("このOS/アーキテクチャ向けの配布物がありません"))?;

    let url = format!("{API_BASE}/releases?limit=1");
    let releases: Vec<ReleaseResponse> = ureq::get(&url)
        .call()
        .map_err(|err| t!("リリース情報取得失敗: %{arg0}", arg0 = format!("{err}")))?
        .body_mut()
        .read_json()
        .map_err(|err| t!("リリース情報解析失敗: %{arg0}", arg0 = format!("{err}")))?;

    let release = releases
        .into_iter()
        .next()
        .ok_or_else(|| t!("リリースが見つかりません"))?;

    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| {
            t!(
                "対象アセット未検出: %{arg0}",
                arg0 = format!("{asset_name}")
            )
        })?;

    Ok(UpdateInfo {
        version: release.tag_name.trim_start_matches('v').to_string(),
        notes: release.body,
        asset_url: asset.browser_download_url,
        asset_name: asset.name,
    })
}

fn is_newer(remote_version: &str) -> Result<bool, String> {
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|err| t!("現行バージョン解析失敗: %{arg0}", arg0 = format!("{err}")))?;
    let remote = semver::Version::parse(remote_version).map_err(|err| {
        t!(
            "リリースバージョン解析失敗: %{arg0}",
            arg0 = format!("{err}")
        )
    })?;
    Ok(remote > current)
}

/// 起動時/手動チェック共通。バックグラウンドスレッドで完結し、結果を`state`へ書き込む。
/// UI側は`show()`毎フレーム`state.lock()`で読むのみ（ExportDialog::progressと同型）。
pub fn spawn_check(state: Arc<Mutex<UpdateStatus>>) {
    *state.lock().unwrap() = UpdateStatus::Checking;
    std::thread::spawn(move || {
        let result = fetch_latest_release().and_then(|info| {
            let newer = is_newer(&info.version)?;
            Ok(if newer {
                UpdateStatus::Available(info)
            } else {
                UpdateStatus::UpToDate
            })
        });
        *state.lock().unwrap() = match result {
            Ok(status) => status,
            Err(err) => UpdateStatus::Error(err),
        };
    });
}

/// ダウンロード済みzipアーカイブをdest_dir直下へ展開する。
fn extract_zip_archive(
    archive_path: &std::path::Path,
    dest_dir: &std::path::Path,
) -> Result<(), String> {
    let file = std::fs::File::open(archive_path).map_err(|err| format!("{err}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| format!("{err}"))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| format!("{err}"))?;
        let Some(relative_path) = entry.enclosed_name() else {
            continue;
        };
        let dest_path = dest_dir.join(&relative_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest_path).map_err(|err| format!("{err}"))?;
            continue;
        }
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("{err}"))?;
        }
        let mut out_file = std::fs::File::create(&dest_path).map_err(|err| format!("{err}"))?;
        std::io::copy(&mut entry, &mut out_file).map_err(|err| format!("{err}"))?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest_path, std::fs::Permissions::from_mode(mode));
        }
    }
    Ok(())
}

fn apply_update(info: &UpdateInfo, state: &Arc<Mutex<UpdateStatus>>) -> Result<(), String> {
    let tmp_dir = tempfile::Builder::new()
        .prefix("neoutl-update")
        .tempdir()
        .map_err(|err| t!("一時ディレクトリ作成失敗: %{arg0}", arg0 = format!("{err}")))?;
    let archive_path: PathBuf = tmp_dir.path().join(&info.asset_name);

    {
        let mut resp = ureq::get(&info.asset_url)
            .call()
            .map_err(|err| t!("ダウンロード開始失敗: %{arg0}", arg0 = format!("{err}")))?;
        let total: u64 = resp
            .headers()
            .get("Content-Length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut reader = resp.body_mut().as_reader();
        let mut archive_file = std::fs::File::create(&archive_path)
            .map_err(|err| t!("一時ファイル作成失敗: %{arg0}", arg0 = format!("{err}")))?;
        let mut buf = [0u8; 64 * 1024];
        let mut downloaded: u64 = 0;
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|err| t!("ダウンロード読み出し失敗: %{arg0}", arg0 = format!("{err}")))?;
            if n == 0 {
                break;
            }
            archive_file
                .write_all(&buf[..n])
                .map_err(|err| t!("一時ファイル書き込み失敗: %{arg0}", arg0 = format!("{err}")))?;
            downloaded += n as u64;
            if total > 0 {
                *state.lock().unwrap() =
                    UpdateStatus::Downloading(downloaded as f32 / total as f32);
            }
        }
        archive_file
            .flush()
            .map_err(|err| t!("一時ファイル書き込み失敗: %{arg0}", arg0 = format!("{err}")))?;
    }

    extract_zip_archive(&archive_path, tmp_dir.path())
        .map_err(|err| t!("展開失敗: %{arg0}", arg0 = err))?;

    let extracted_binary = if cfg!(target_os = "macos") {
        tmp_dir
            .path()
            .join("NeoUtl.app/Contents/MacOS")
            .join(current_binary_name())
    } else {
        tmp_dir.path().join(current_binary_name())
    };
    if !extracted_binary.is_file() {
        return Err(t!(
            "展開後バイナリ未検出: %{arg0}",
            arg0 = format!("{}", extracted_binary.display())
        ));
    }

    self_replace::self_replace(&extracted_binary)
        .map_err(|err| t!("自己置換失敗: %{arg0}", arg0 = format!("{err}")))?;

    Ok(())
}

/// ダウンロード〜自己置換までを別スレッドで実行する。完了後は再起動が必要。
/// objects/effects/decoders/vst3-host-helper・macOS Resourcesは対象外（フェーズ1）。
pub fn spawn_apply(state: Arc<Mutex<UpdateStatus>>, info: UpdateInfo) {
    std::thread::spawn(move || {
        let result = apply_update(&info, &state);
        *state.lock().unwrap() = match result {
            Ok(()) => UpdateStatus::Installed,
            Err(err) => UpdateStatus::Error(err),
        };
    });
}
