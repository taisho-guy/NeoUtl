use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Global,
    Timeline,
    Properties,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommandId {
    NewProject,
    OpenProject,
    SaveProject,
    SaveProjectAs,
    ExportMedia,
    Quit,
    Undo,
    Redo,
    TogglePlay,
    StepFrameFwd,
    StepFrameBack,
    SeekHome,
    SeekEnd,
    ShowTimeline,
    ShowProperties,
    ShowPreview,
    ShowSystemSettings,
    ShowProjectSettings,
    ShowSceneSettings,
    ShowKeybindings,
    NextProjectTab,
    PrevProjectTab,
    CloseProjectTab,
    NewScene,
    CloseScene,
    NextScene,
    PrevScene,
    DeleteSelected,
    SplitAtPlayhead,
    Duplicate,
    Cut,
    Copy,
    Paste,
    ToggleRipple,
    ZoomIn,
    ZoomOut,
}

pub const ALL_COMMANDS: &[CommandId] = &[
    CommandId::NewProject,
    CommandId::OpenProject,
    CommandId::SaveProject,
    CommandId::SaveProjectAs,
    CommandId::ExportMedia,
    CommandId::Quit,
    CommandId::Undo,
    CommandId::Redo,
    CommandId::TogglePlay,
    CommandId::StepFrameFwd,
    CommandId::StepFrameBack,
    CommandId::SeekHome,
    CommandId::SeekEnd,
    CommandId::ShowTimeline,
    CommandId::ShowProperties,
    CommandId::ShowPreview,
    CommandId::ShowSystemSettings,
    CommandId::ShowProjectSettings,
    CommandId::ShowSceneSettings,
    CommandId::ShowKeybindings,
    CommandId::NextProjectTab,
    CommandId::PrevProjectTab,
    CommandId::CloseProjectTab,
    CommandId::NewScene,
    CommandId::CloseScene,
    CommandId::NextScene,
    CommandId::PrevScene,
    CommandId::DeleteSelected,
    CommandId::SplitAtPlayhead,
    CommandId::Duplicate,
    CommandId::Cut,
    CommandId::Copy,
    CommandId::Paste,
    CommandId::ToggleRipple,
    CommandId::ZoomIn,
    CommandId::ZoomOut,
];

pub fn label(id: CommandId) -> &'static str {
    match id {
        CommandId::NewProject => "新規プロジェクト",
        CommandId::OpenProject => "プロジェクトを開く",
        CommandId::SaveProject => "プロジェクトを保存",
        CommandId::SaveProjectAs => "名前を付けて保存",
        CommandId::ExportMedia => "書き出し",
        CommandId::Quit => "終了",
        CommandId::Undo => "元に戻す",
        CommandId::Redo => "やり直し",
        CommandId::TogglePlay => "再生/停止",
        CommandId::StepFrameFwd => "1フレーム進む",
        CommandId::StepFrameBack => "1フレーム戻る",
        CommandId::SeekHome => "先頭へ",
        CommandId::SeekEnd => "末尾へ",
        CommandId::ShowTimeline => "タイムライン表示",
        CommandId::ShowProperties => "設定ダイアログ表示",
        CommandId::ShowPreview => "プレビュー表示",
        CommandId::ShowSystemSettings => "システム設定表示",
        CommandId::ShowProjectSettings => "プロジェクト設定表示",
        CommandId::ShowSceneSettings => "シーン設定表示",
        CommandId::ShowKeybindings => "ショートカット設定表示",
        CommandId::NextProjectTab => "次のプロジェクトタブ",
        CommandId::PrevProjectTab => "前のプロジェクトタブ",
        CommandId::CloseProjectTab => "プロジェクトタブを閉じる",
        CommandId::NewScene => "新規シーン",
        CommandId::CloseScene => "シーンを閉じる",
        CommandId::NextScene => "次のシーンタブ",
        CommandId::PrevScene => "前のシーンタブ",
        CommandId::DeleteSelected => "選択オブジェクト削除",
        CommandId::SplitAtPlayhead => "再生位置で分割",
        CommandId::Duplicate => "複製",
        CommandId::Cut => "切り取り",
        CommandId::Copy => "コピー",
        CommandId::Paste => "貼り付け",
        CommandId::ToggleRipple => "リップル編集切替",
        CommandId::ZoomIn => "拡大",
        CommandId::ZoomOut => "縮小",
    }
}

#[derive(Debug, Clone, Copy)]
pub struct KeyBinding {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: &'static str,
}

const fn bind(ctrl: bool, shift: bool, alt: bool, key: &'static str) -> KeyBinding {
    KeyBinding {
        ctrl,
        shift,
        alt,
        key,
    }
}

pub const DEFAULT_KEYMAP: &[(CommandId, Scope, KeyBinding)] = &[
    (
        CommandId::NewProject,
        Scope::Global,
        bind(true, false, false, "n"),
    ),
    (
        CommandId::OpenProject,
        Scope::Global,
        bind(true, false, false, "o"),
    ),
    (
        CommandId::SaveProject,
        Scope::Global,
        bind(true, false, false, "s"),
    ),
    (
        CommandId::SaveProjectAs,
        Scope::Global,
        bind(true, true, false, "s"),
    ),
    (
        CommandId::ExportMedia,
        Scope::Global,
        bind(true, false, false, "e"),
    ),
    (
        CommandId::Quit,
        Scope::Global,
        bind(true, false, false, "q"),
    ),
    (
        CommandId::Undo,
        Scope::Global,
        bind(true, false, false, "z"),
    ),
    (
        CommandId::Redo,
        Scope::Global,
        bind(true, false, false, "y"),
    ),
    (CommandId::Redo, Scope::Global, bind(true, true, false, "z")),
    (
        CommandId::TogglePlay,
        Scope::Global,
        bind(false, false, false, "Space"),
    ),
    (
        CommandId::StepFrameFwd,
        Scope::Global,
        bind(false, false, false, "Right"),
    ),
    (
        CommandId::StepFrameBack,
        Scope::Global,
        bind(false, false, false, "Left"),
    ),
    (
        CommandId::SeekHome,
        Scope::Timeline,
        bind(false, false, false, "Home"),
    ),
    (
        CommandId::SeekEnd,
        Scope::Timeline,
        bind(false, false, false, "End"),
    ),
    (
        CommandId::ShowTimeline,
        Scope::Global,
        bind(false, false, false, "F2"),
    ),
    (
        CommandId::ShowProperties,
        Scope::Global,
        bind(false, false, false, "F3"),
    ),
    (
        CommandId::ShowPreview,
        Scope::Global,
        bind(false, false, false, "F4"),
    ),
    (
        CommandId::ShowSystemSettings,
        Scope::Global,
        bind(false, false, false, "F9"),
    ),
    (
        CommandId::ShowProjectSettings,
        Scope::Global,
        bind(false, false, false, "F10"),
    ),
    (
        CommandId::ShowSceneSettings,
        Scope::Global,
        bind(false, false, false, "F11"),
    ),
    (
        CommandId::ShowKeybindings,
        Scope::Global,
        bind(false, false, false, "F12"),
    ),
    (
        CommandId::NextProjectTab,
        Scope::Global,
        bind(true, false, false, "Tab"),
    ),
    (
        CommandId::PrevProjectTab,
        Scope::Global,
        bind(true, true, false, "Tab"),
    ),
    (
        CommandId::CloseProjectTab,
        Scope::Global,
        bind(true, false, false, "w"),
    ),
    (
        CommandId::NewScene,
        Scope::Global,
        bind(true, true, false, "n"),
    ),
    (
        CommandId::CloseScene,
        Scope::Global,
        bind(true, true, false, "w"),
    ),
    (
        CommandId::NextScene,
        Scope::Global,
        bind(true, false, false, "PageDown"),
    ),
    (
        CommandId::PrevScene,
        Scope::Global,
        bind(true, false, false, "PageUp"),
    ),
    (
        CommandId::DeleteSelected,
        Scope::Timeline,
        bind(false, false, false, "Delete"),
    ),
    (
        CommandId::SplitAtPlayhead,
        Scope::Timeline,
        bind(false, false, false, "s"),
    ),
    (
        CommandId::Duplicate,
        Scope::Timeline,
        bind(true, false, false, "d"),
    ),
    (
        CommandId::Cut,
        Scope::Timeline,
        bind(true, false, false, "x"),
    ),
    (
        CommandId::Copy,
        Scope::Timeline,
        bind(true, false, false, "c"),
    ),
    (
        CommandId::Paste,
        Scope::Timeline,
        bind(true, false, false, "v"),
    ),
    (
        CommandId::ToggleRipple,
        Scope::Timeline,
        bind(false, false, false, "r"),
    ),
    (
        CommandId::ZoomIn,
        Scope::Timeline,
        bind(true, false, false, "="),
    ),
    (
        CommandId::ZoomOut,
        Scope::Timeline,
        bind(true, false, false, "-"),
    ),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedBinding {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: String,
}

impl OwnedBinding {
    fn matches(&self, ctrl: bool, shift: bool, alt: bool, key: &str) -> bool {
        self.ctrl == ctrl
            && self.shift == shift
            && self.alt == alt
            && self.key.eq_ignore_ascii_case(key)
    }
}

impl From<KeyBinding> for OwnedBinding {
    fn from(b: KeyBinding) -> Self {
        Self {
            ctrl: b.ctrl,
            shift: b.shift,
            alt: b.alt,
            key: b.key.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Override {
    pub command: CommandId,
    pub scope: Scope,
    pub binding: OwnedBinding,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeymapResource {
    pub overrides: Vec<Override>,
}

impl KeymapResource {
    pub fn binding_of(&self, command: CommandId) -> (Scope, OwnedBinding) {
        if let Some(o) = self.overrides.iter().find(|o| o.command == command) {
            return (o.scope, o.binding.clone());
        }
        DEFAULT_KEYMAP
            .iter()
            .find(|(c, _, _)| *c == command)
            .map(|(_, s, b)| (*s, OwnedBinding::from(*b)))
            .unwrap_or((
                Scope::Global,
                OwnedBinding {
                    ctrl: false,
                    shift: false,
                    alt: false,
                    key: String::new(),
                },
            ))
    }

    pub fn set_binding(&mut self, command: CommandId, scope: Scope, binding: OwnedBinding) {
        if let Some(o) = self.overrides.iter_mut().find(|o| o.command == command) {
            o.scope = scope;
            o.binding = binding;
        } else {
            self.overrides.push(Override {
                command,
                scope,
                binding,
            });
        }
    }

    pub fn reset_to_default(&mut self, command: CommandId) {
        self.overrides.retain(|o| o.command != command);
    }

    pub fn reset_all(&mut self) {
        self.overrides.clear();
    }

    pub fn conflict_of(
        &self,
        exclude: CommandId,
        scope: Scope,
        binding: &OwnedBinding,
    ) -> Option<CommandId> {
        for command in ALL_COMMANDS {
            if *command == exclude {
                continue;
            }
            let (s, b) = self.binding_of(*command);
            if (s == scope || s == Scope::Global || scope == Scope::Global)
                && b.matches(binding.ctrl, binding.shift, binding.alt, &binding.key)
            {
                return Some(*command);
            }
        }
        None
    }

    pub fn resolve(
        &self,
        scope: Scope,
        ctrl: bool,
        shift: bool,
        alt: bool,
        key: &str,
    ) -> Option<CommandId> {
        for o in &self.overrides {
            if (o.scope == scope || o.scope == Scope::Global)
                && o.binding.matches(ctrl, shift, alt, key)
            {
                return Some(o.command);
            }
        }
        DEFAULT_KEYMAP
            .iter()
            .find(|(c, s, b)| {
                !self.overrides.iter().any(|o| o.command == *c)
                    && (*s == scope || *s == Scope::Global)
                    && b.ctrl == ctrl
                    && b.shift == shift
                    && b.alt == alt
                    && b.key.eq_ignore_ascii_case(key)
            })
            .map(|(c, _, _)| *c)
    }
}

fn keymap_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("settings").join("keymap.yaml")))
        .unwrap_or_else(|| PathBuf::from("settings/keymap.yaml"))
}

pub fn save_to_disk(k: &KeymapResource) -> std::io::Result<()> {
    let path = keymap_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let yaml = rust_yaml::to_string(k).map_err(std::io::Error::other)?;
    std::fs::write(path, yaml)
}

pub fn load_from_disk() -> Option<KeymapResource> {
    let content = std::fs::read_to_string(keymap_path()).ok()?;
    rust_yaml::from_str(&content).ok()
}

static ACTIVE_KEYMAP: OnceLock<Mutex<KeymapResource>> = OnceLock::new();

pub fn active_keymap() -> &'static Mutex<KeymapResource> {
    ACTIVE_KEYMAP.get_or_init(|| Mutex::new(load_from_disk().unwrap_or_default()))
}

pub fn resolve_active(
    scope: Scope,
    ctrl: bool,
    shift: bool,
    alt: bool,
    key: &str,
) -> Option<CommandId> {
    active_keymap()
        .lock()
        .unwrap()
        .resolve(scope, ctrl, shift, alt, key)
}
