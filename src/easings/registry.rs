use neoutl_easing_standard::CurveKind;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Serialize, Deserialize)]
pub struct PresetEntry {
    pub name: String,
    pub kind: CurveKind,
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

    pub fn entries(&self) -> &[PresetEntry] {
        &self.entries
    }

    pub fn push_unique(&mut self, kind: CurveKind) -> usize {
        let mut n = self.entries.len() + 1;
        while self
            .entries
            .iter()
            .any(|e| e.name == format!("カスタム {n}"))
        {
            n += 1;
        }
        self.entries.push(PresetEntry {
            name: format!("カスタム {n}"),
            kind,
        });
        self.flush();
        self.entries.len() - 1
    }

    pub fn rename(&mut self, index: usize, name: String) {
        let name = name.trim().to_owned();
        let taken = self
            .entries
            .iter()
            .enumerate()
            .any(|(i, e)| i != index && e.name == name);
        if let Some(entry) = self.entries.get_mut(index)
            && !name.is_empty()
            && !taken
        {
            entry.name = name;
            self.flush();
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
            self.flush();
        }
    }

    pub fn move_by(&mut self, index: usize, delta: isize) {
        let target = index.saturating_add_signed(delta);
        if index < self.entries.len() && target < self.entries.len() {
            self.entries.swap(index, target);
            self.flush();
        }
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

pub fn shared() -> &'static Mutex<CurveRegistry> {
    static SHARED: OnceLock<Mutex<CurveRegistry>> = OnceLock::new();
    SHARED.get_or_init(|| {
        let dir = super::loader::default_easings_dir();
        Mutex::new(CurveRegistry::load(&default_presets_path(&dir)))
    })
}
