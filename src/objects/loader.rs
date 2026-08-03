use arc_swap::ArcSwap;
use libloading::{Library, Symbol};
use neoutl_object_api::{ENTRY_SYMBOL, EntryFn, ObjectVTable};
use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub struct ObjectPlugin {
    pub stable_id: String,
    pub name: String,
    pub kind_id: u32,
    pub vtable: &'static ObjectVTable,
    _lib: Library,
}

fn registry_swap() -> &'static ArcSwap<Vec<Arc<ObjectPlugin>>> {
    static SWAP: OnceLock<ArcSwap<Vec<Arc<ObjectPlugin>>>> = OnceLock::new();
    SWAP.get_or_init(|| ArcSwap::new(Arc::new(Vec::new())))
}

/// stable_id -> kind_id 対応表。初回発行順を保持し、再ロードでもstable_id一致の限り
/// 同一kind_idを再利用する（プロセス内キャッシュのみ、保存形式は既存の数値kind_idのまま）。
fn kind_id_table() -> &'static Mutex<(HashMap<String, u32>, u32)> {
    static TABLE: OnceLock<Mutex<(HashMap<String, u32>, u32)>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new((HashMap::new(), 0)))
}

fn assign_kind_id(stable_id: &str) -> u32 {
    let mut guard = kind_id_table().lock().expect("kind_id_table poisoned");
    if let Some(existing) = guard.0.get(stable_id) {
        return *existing;
    }
    let next = guard.1;
    guard.1 = guard.1.checked_add(1).expect("kind_id空間枯渇");
    guard.0.insert(stable_id.to_owned(), next);
    next
}

pub fn load_all(objects_dir: &Path) {
    let entries = match std::fs::read_dir(objects_dir) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("[NeoUtl] objects/ 読み込み失敗: {err}");
            return;
        }
    };
    let candidates: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_dylib(p))
        .collect();

    let mut plugins: Vec<Arc<ObjectPlugin>> = candidates
        .iter()
        .filter_map(|path| match load_one(path) {
            Ok(p) => Some(Arc::new(p)),
            Err(err) => {
                eprintln!("[NeoUtl] プラグイン読み込み失敗 {}: {err}", path.display());
                None
            }
        })
        .collect();

    plugins.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    for plugin in &mut plugins {
        let kind_id = assign_kind_id(&plugin.stable_id);
        Arc::get_mut(plugin)
            .expect("初回ロード直後は単一所有")
            .kind_id = kind_id;
        eprintln!(
            "[NeoUtl] プラグイン登録: {} ({}, kind_id={})",
            plugin.name, plugin.stable_id, plugin.kind_id
        );
    }
    registry_swap().store(Arc::new(plugins));
}

/// 現行スナップショットを返す。呼び出し側はフレーム内のみ保持し、以降は破棄する
/// （旧Libraryを無参照後即解放させる設計を担保するため、長期保持しない）。
pub fn registry() -> Arc<Vec<Arc<ObjectPlugin>>> {
    registry_swap().load_full()
}

pub fn by_kind_id(kind_id: u32) -> Option<Arc<ObjectPlugin>> {
    registry().iter().find(|p| p.kind_id == kind_id).cloned()
}

pub fn by_stable_id(stable_id: &str) -> Option<Arc<ObjectPlugin>> {
    registry()
        .iter()
        .find(|p| p.stable_id == stable_id)
        .cloned()
}

/// 単一プラグインファイルの再ロード。既存stable_id一致エントリのみ差し替える。
/// 未一致（新規プラグイン）は対象外としエラーを返す（kind_id空間の実行時拡張は別課題）。
/// シンボル欠落・メタ不整合時は現行registryを変更せずエラーを返す。
pub fn reload_one(path: &Path) -> Result<(), String> {
    let new_plugin = load_one(path).map_err(|e| e.to_string())?;
    let current = registry_swap().load_full();
    let Some(pos) = current
        .iter()
        .position(|p| p.stable_id == new_plugin.stable_id)
    else {
        return Err(format!(
            "既存プラグイン未検出、新規追加は対象外: {}",
            new_plugin.stable_id
        ));
    };

    let kind_id = current
        .get(pos)
        .map(|p| p.kind_id)
        .unwrap_or_else(|| assign_kind_id(&new_plugin.stable_id));
    let mut new_plugin = new_plugin;
    new_plugin.kind_id = kind_id;
    let stable_id = new_plugin.stable_id.clone();
    let mut new_plugin = Some(new_plugin);

    let next: Vec<Arc<ObjectPlugin>> = current
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i == pos {
                Arc::new(new_plugin.take().expect("posは一度のみ一致"))
            } else {
                Arc::clone(p)
            }
        })
        .collect();
    registry_swap().store(Arc::new(next));
    eprintln!("[NeoUtl] プラグイン再ロード完了: {stable_id} (kind_id={kind_id})");
    Ok(())
}

pub fn default_objects_dir() -> PathBuf {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from("objects");
    };

    #[cfg(target_os = "macos")]
    {
        let resources_dir = exe_dir.join("../Resources/objects");
        if resources_dir.is_dir() {
            return resources_dir;
        }
    }

    exe_dir.join("objects")
}

fn load_one(path: &Path) -> Result<ObjectPlugin, Box<dyn std::error::Error>> {
    let lib = unsafe { Library::new(path) }?;
    let entry: Symbol<EntryFn> = unsafe { lib.get(ENTRY_SYMBOL) }?;
    let vtable: &'static ObjectVTable = unsafe { &*entry() };
    let meta = unsafe { &*((vtable.meta)()) };
    Ok(ObjectPlugin {
        stable_id: meta.stable_id.to_owned(),
        name: meta.name.to_owned(),
        kind_id: 0,
        vtable,
        _lib: lib,
    })
}

fn is_dylib(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("so" | "dylib" | "dll")
    )
}
