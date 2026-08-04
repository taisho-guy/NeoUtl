use crate::localization::{effect_category, effect_name};
use crate::ui::effect_add_dialog::EffectCatalogSource;
use crate::ui::types::CatalogRow;
use std::sync::Mutex;

/// 直近に追加したエフェクトIDの履歴（新しい順、最大8件）。プロセス生存中のみ保持し
/// ディスク永続化はしない（最近使用ソートは同一セッション内の利便性のためのもの）。
static RECENT_EFFECT_IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn mark_effect_used(id: &str) {
    let mut recent = RECENT_EFFECT_IDS.lock().unwrap();
    recent.retain(|x| x != id);
    recent.insert(0, id.to_owned());
    recent.truncate(8);
}

/// エフェクトカタログの全件と、カテゴリ一覧（重複除去・昇順）を起動時に一度構築する。
/// フィルタ・ソートは`filtered()`が都度算出し、EffectAddDialog表示のたびに反映する。
pub struct EffectCatalogState {
    all: Vec<CatalogRow>,
    categories: Vec<String>,
}

impl EffectCatalogState {
    pub fn build() -> Self {
        let mut all: Vec<CatalogRow> = crate::effects::loader::registry()
            .iter()
            .map(|p| CatalogRow {
                id: p.id().to_owned(),
                name: effect_name(p.name()),
                category: effect_category(p.category()),
            })
            .collect();
        all.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));

        let mut categories: Vec<String> = all.iter().map(|r| r.category.clone()).collect();
        categories.sort();
        categories.dedup();

        Self { all, categories }
    }

    pub fn categories_inner(&self) -> &[String] {
        &self.categories
    }

    /// sort_mode: 0=カテゴリ順, 1=名前順, 2=最近使用順
    pub fn filtered(&self, query: &str, sort_mode: i32, category: &str) -> Vec<CatalogRow> {
        let q = query.to_lowercase();
        let mut rows: Vec<CatalogRow> = self
            .all
            .iter()
            .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q))
            .filter(|r| category.is_empty() || r.category.as_str() == category)
            .cloned()
            .collect();

        match sort_mode {
            1 => rows.sort_by(|a, b| a.name.cmp(&b.name)),
            2 => {
                let recent = RECENT_EFFECT_IDS.lock().unwrap();
                rows.sort_by_key(|r| {
                    recent
                        .iter()
                        .position(|id| id.as_str() == r.id.as_str())
                        .unwrap_or(usize::MAX)
                });
            }
            _ => rows.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name))),
        }
        rows
    }
}

impl EffectCatalogSource for EffectCatalogState {
    fn categories(&self) -> &[String] {
        self.categories_inner()
    }
    fn filtered(&self, query: &str, sort_mode: i32, category: &str) -> Vec<CatalogRow> {
        EffectCatalogState::filtered(self, query, sort_mode, category)
    }
}

/// audio-plugin-hostのRegistryから構築するカタログ。EffectCatalogStateと同型で、
/// categoryにはPluginFormat（"Vst3"/"Clap"）を流用する（フィルタ・カテゴリタブの
/// 挙動をエフェクト側と完全に共通化するため、専用フィールドを設けない）。
pub struct PluginCatalogState {
    all: Vec<CatalogRow>,
    categories: Vec<String>,
}

impl PluginCatalogState {
    pub fn build() -> Self {
        let mut all: Vec<CatalogRow> = crate::audio::plugin_registry::registry()
            .iter()
            .map(|e| CatalogRow {
                id: e.plugin_id.clone(),
                name: e.name.clone(),
                category: format!("{:?}", e.format),
            })
            .collect();
        all.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));

        let mut categories: Vec<String> = all.iter().map(|r| r.category.clone()).collect();
        categories.sort();
        categories.dedup();

        Self { all, categories }
    }

    pub fn categories(&self) -> &[String] {
        &self.categories
    }

    pub fn filtered(&self, query: &str, sort_mode: i32, category: &str) -> Vec<CatalogRow> {
        let q = query.to_lowercase();
        let mut rows: Vec<CatalogRow> = self
            .all
            .iter()
            .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q))
            .filter(|r| category.is_empty() || r.category.as_str() == category)
            .cloned()
            .collect();

        match sort_mode {
            1 => rows.sort_by(|a, b| a.name.cmp(&b.name)),
            _ => rows.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name))),
        }
        rows
    }
}
