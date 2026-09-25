use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 各オブジェクトの解決済みソースサイズ (ピクセル単位) を保持するプロセスグローバルキャッシュ。
/// `get_active_objects_system` が毎フレーム書き込み、UI層 (`hit_test` / `gizmo`) が読み取る。
/// `neoutl_media_runtime::cache::global()` と同一の運用方式 (プロセス内シングルトン、`OnceLock` 初期化)。
/// `EcsWorld` / Shipyard `Unique` には登録しない: 登録には world 初期化コードの変更が必要になり、
/// 読み取り専用の `&EcsWorld` からも書き込みたいという要件と噛み合わないため。
#[derive(Default)]
pub struct SourceSizeCache {
    sizes: Mutex<HashMap<usize, (f32, f32)>>,
}

impl SourceSizeCache {
    /// オブジェクトのソースサイズを登録する。
    /// メディア: 画像/動画の実ピクセル数。
    /// ネストシーン: シーン解像度。
    /// シェイプ・テキスト等、外形が未解決の種別は登録しない
    /// (呼び出し側 `hit_test`/`gizmo` は `get` が `None` を返した場合、
    /// 原点距離判定・固定オフセットへフォールバックする)。
    pub fn insert(&self, object_id: usize, width: f32, height: f32) {
        if let Ok(mut sizes) = self.sizes.lock() {
            sizes.insert(object_id, (width, height));
        }
    }

    pub fn get(&self, object_id: usize) -> Option<(f32, f32)> {
        self.sizes.lock().ok()?.get(&object_id).copied()
    }
}

/// プロセス内シングルトンを返す。
pub fn global() -> &'static SourceSizeCache {
    static INSTANCE: OnceLock<SourceSizeCache> = OnceLock::new();
    INSTANCE.get_or_init(SourceSizeCache::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_then_get_round_trips() {
        let cache = SourceSizeCache::default();
        cache.insert(7, 1920.0, 1080.0);
        assert_eq!(cache.get(7), Some((1920.0, 1080.0)));
    }

    #[test]
    fn get_missing_returns_none() {
        let cache = SourceSizeCache::default();
        assert_eq!(cache.get(999), None);
    }
}
