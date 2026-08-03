use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

pub enum ReloadEvent {
    Object(PathBuf),
    Effect(PathBuf),
    Script(PathBuf),
}

const DEBOUNCE: Duration = Duration::from_millis(250);

/// objects_dir・effects_dir・scripts_dir（非再帰）を監視するバックグラウンドスレッドを
/// 起動する。dylib対象(so/dylib/dll)は objects_dir/effects_dir 配下、lua対象(*.lua)は
/// scripts_dir配下のCreate/Modifyイベントをデバウンスの上、呼び出し元へ通知する。
/// デバウンスはパス単位で直近発火時刻を保持し、ビルドツールの連続書き込み・
/// リネーム移動パターンによる重複通知を抑制する。
pub fn spawn_watcher(
    objects_dir: PathBuf,
    effects_dir: PathBuf,
    scripts_dir: PathBuf,
) -> mpsc::Receiver<ReloadEvent> {
    let (out_tx, out_rx) = mpsc::channel::<ReloadEvent>();

    std::thread::spawn(move || {
        let (raw_tx, raw_rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher = match RecommendedWatcher::new(
            move |res| {
                let _ = raw_tx.send(res);
            },
            notify::Config::default(),
        ) {
            Ok(w) => w,
            Err(err) => {
                eprintln!("[NeoUtl] ホットリロード監視初期化失敗: {err}");
                return;
            }
        };

        for dir in [&objects_dir, &effects_dir, &scripts_dir] {
            if let Err(err) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                eprintln!(
                    "[NeoUtl] ホットリロード監視対象追加失敗 {}: {err}",
                    dir.display()
                );
            }
        }

        let mut last_fired: std::collections::HashMap<PathBuf, Instant> =
            std::collections::HashMap::new();

        for res in raw_rx {
            let Ok(event) = res else { continue };
            if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                continue;
            }
            for path in event.paths {
                let is_script = path.starts_with(&scripts_dir) && is_target_lua(&path);
                let is_dylib = is_target_dylib(&path);
                if !is_script && !is_dylib {
                    continue;
                }
                let now = Instant::now();
                if let Some(prev) = last_fired.get(&path)
                    && now.duration_since(*prev) < DEBOUNCE
                {
                    continue;
                }
                last_fired.insert(path.clone(), now);

                let reload_event = if is_script {
                    ReloadEvent::Script(path)
                } else if path.starts_with(&objects_dir) {
                    ReloadEvent::Object(path)
                } else {
                    ReloadEvent::Effect(path)
                };
                if out_tx.send(reload_event).is_err() {
                    return;
                }
            }
        }
    });

    out_rx
}

fn is_target_dylib(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("so" | "dylib" | "dll")
    )
}

fn is_target_lua(path: &Path) -> bool {
    path.extension().and_then(OsStr::to_str) == Some("lua")
}
