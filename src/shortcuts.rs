#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Timeline,
    Properties,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    ShowTimeline,
    ShowProperties,
    ShowSystemSettings,
    ShowProjectSettings,
    NextProjectTab,
    PrevProjectTab,
    CloseProjectTab,
    DeleteSelected,
    SplitAtPlayhead,
    Duplicate,
    Cut,
    Copy,
    Paste,
    ToggleRipple,
    ZoomIn,
    ZoomOut,
    SeekHome,
    SeekEnd,
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
];

pub fn resolve(scope: Scope, ctrl: bool, shift: bool, alt: bool, key: &str) -> Option<CommandId> {
    DEFAULT_KEYMAP
        .iter()
        .find(|(_, s, b)| {
            (*s == scope || *s == Scope::Global)
                && b.ctrl == ctrl
                && b.shift == shift
                && b.alt == alt
                && b.key.eq_ignore_ascii_case(key)
        })
        .map(|(cmd, _, _)| *cmd)
}
