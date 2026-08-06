//! `Curve_Editor移植計画.md` 3.3節・フェーズ5対応。
//! AviUtl `curves_normal_`(連番ID)はNeoUtlでは名前付きプリセットへ置換する。

use neoutl_easing_standard::CurveKind;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
struct PresetEntry {
    name: String,
    kind: CurveKind,
}

#[derive(Default)]
pub struct CurveRegistry {
    entries: Vec<PresetEntry>,
    path: PathBuf,
}

impl CurveRegistry {
    pub fn load(path: &Path) -> Self {
        let entries = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<PresetEntry>>(&bytes).ok())
            .unwrap_or_default();
        Self {
            entries,
            path: path.to_path_buf(),
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    pub fn get(&self, name: &str) -> Option<&CurveKind> {
        self.entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| &e.kind)
    }

    /// 既存同名は上書きする。保存失敗はログのみ(呼び出し側の編集操作は継続)。
    pub fn save_as(&mut self, name: &str, kind: CurveKind) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.name == name) {
            existing.kind = kind;
        } else {
            self.entries.push(PresetEntry {
                name: name.to_owned(),
                kind,
            });
        }
        self.flush();
    }

    fn flush(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_vec_pretty(&self.entries) {
            Ok(bytes) => {
                if let Err(err) = std::fs::write(&self.path, bytes) {
                    eprintln!(
                        "{}",
                        t!(
                            "[NeoUtl] カーブプリセット保存失敗 %{arg0}: %{arg1}",
                            arg0 = self.path.display().to_string(),
                            arg1 = format!("{}", err)
                        )
                    );
                }
            }
            Err(err) => eprintln!(
                "{}",
                t!(
                    "[NeoUtl] カーブプリセット直列化失敗: %{arg0}",
                    arg0 = format!("{}", err)
                )
            ),
        }
    }
}

pub fn default_presets_path(easings_dir: &Path) -> PathBuf {
    easings_dir.join("curve_presets.json")
}
