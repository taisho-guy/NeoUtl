pub mod abi;
pub mod types;

pub use abi::*;
pub use types::*;

#[derive(Debug)]
pub enum ExtensionError {
    Load(String),
    Init(String),
    Runtime(String),
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(s) => write!(f, "拡張読み込み失敗: {s}"),
            Self::Init(s) => write!(f, "拡張初期化失敗: {s}"),
            Self::Runtime(s) => write!(f, "拡張実行時エラー: {s}"),
        }
    }
}

impl std::error::Error for ExtensionError {}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyExtension {
        meta: ExtensionMetadata,
    }

    impl DummyExtension {
        fn new() -> Self {
            Self {
                meta: ExtensionMetadata::new("dummy.ext", "Dummy Extension"),
            }
        }
    }

    impl ExtensionPlugin for DummyExtension {
        fn metadata(&self) -> ExtensionMetadata {
            self.meta.clone()
        }

        fn init(&mut self, ctx: &mut ExtensionContext) -> Result<(), String> {
            ctx.register_panel(PanelDescriptor {
                id: "dummy_panel".to_owned(),
                title: "Dummy Panel".to_owned(),
                default_size: [300.0, 200.0],
                default_open: true,
                placement: PanelPlacement::FloatingWindow,
            });
            ctx.register_command(CommandDescriptor {
                id: "dummy.hello".to_owned(),
                name: "Hello Command".to_owned(),
                description: "Prints hello".to_owned(),
                default_shortcut: Some("Ctrl+Shift+H".to_owned()),
            });
            Ok(())
        }
    }

    #[test]
    fn test_extension_init_and_storage() {
        let mut ext = DummyExtension::new();
        let mut storage = ProjectStorage::new();
        storage.set_string("my_key", "my_value");
        assert_eq!(storage.get_string("my_key"), Some("my_value"));

        let mut panels = Vec::new();
        let mut menus = Vec::new();
        let mut commands = Vec::new();
        let mut file_drops = Vec::new();
        let mut toasts = Vec::new();
        let mut events = Vec::new();

        let mut ctx = ExtensionContext {
            plugin_id: "dummy.ext",
            storage: &mut storage,
            edit_session: None,
            registered_panels: &mut panels,
            registered_menus: &mut menus,
            registered_commands: &mut commands,
            registered_file_drops: &mut file_drops,
            toast_messages: &mut toasts,
            outgoing_events: &mut events,
        };

        ext.init(&mut ctx).expect("init should succeed");
        assert_eq!(panels.len(), 1);
        assert_eq!(panels[0].id, "dummy_panel");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].id, "dummy.hello");
    }
}
