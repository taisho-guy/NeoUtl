use crate::app::state::SharedAppState;
use crate::extensions::builtin::{QuickPaletteExtension, SubtitleImporterExtension};
use crate::extensions::session::AppEditSession;
use libloading::{Library, Symbol};
use neoutl_extension_api::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub struct LoadedPlugin {
    pub instance: Box<dyn ExtensionPlugin>,
    pub metadata: ExtensionMetadata,
    pub _lib: Option<Library>,
}

pub struct ExtensionManager {
    plugins: Vec<LoadedPlugin>,
    open_panels: HashMap<String, bool>,
    registered_panels: Vec<(String, PanelDescriptor)>,
    registered_menus: Vec<(String, MenuItemDescriptor)>,
    registered_commands: Vec<(String, CommandDescriptor)>,
    registered_file_drops: Vec<(String, FileDropDescriptor)>,
    plugin_storages: HashMap<String, ProjectStorage>,
    pub toasts: Vec<String>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            plugins: Vec::new(),
            open_panels: HashMap::new(),
            registered_panels: Vec::new(),
            registered_menus: Vec::new(),
            registered_commands: Vec::new(),
            registered_file_drops: Vec::new(),
            plugin_storages: HashMap::new(),
            toasts: Vec::new(),
        };

        mgr.register_plugin(Box::new(QuickPaletteExtension::new()), None);
        mgr.register_plugin(Box::new(SubtitleImporterExtension::new()), None);

        mgr
    }

    pub fn register_plugin(&mut self, mut plugin: Box<dyn ExtensionPlugin>, lib: Option<Library>) {
        let meta = plugin.metadata();
        let plugin_id = meta.id.clone();
        let storage = self.plugin_storages.entry(plugin_id.clone()).or_default();

        let mut panels = Vec::new();
        let mut menus = Vec::new();
        let mut commands = Vec::new();
        let mut file_drops = Vec::new();
        let mut toasts = Vec::new();
        let mut outgoing = Vec::new();

        let mut ctx = ExtensionContext {
            plugin_id: &plugin_id,
            storage,
            edit_session: None,
            registered_panels: &mut panels,
            registered_menus: &mut menus,
            registered_commands: &mut commands,
            registered_file_drops: &mut file_drops,
            toast_messages: &mut toasts,
            outgoing_events: &mut outgoing,
        };

        if let Err(err) = plugin.init(&mut ctx) {
            eprintln!("[NeoUtl] プラグイン '{}' の初期化失敗: {err}", plugin_id);
            return;
        }

        for p in panels {
            self.open_panels.insert(p.id.clone(), p.default_open);
            self.registered_panels.push((plugin_id.clone(), p));
        }
        for m in menus {
            self.registered_menus.push((plugin_id.clone(), m));
        }
        for c in commands {
            self.registered_commands.push((plugin_id.clone(), c));
        }
        for f in file_drops {
            self.registered_file_drops.push((plugin_id.clone(), f));
        }

        self.plugins.push(LoadedPlugin {
            instance: plugin,
            metadata: meta,
            _lib: lib,
        });
    }

    pub fn load_native(&mut self, path: &Path) -> Result<(), String> {
        unsafe {
            let lib = Library::new(path).map_err(|e| format!("ライブラリ読み込み失敗: {e}"))?;
            let entry: Symbol<ExtensionCreateFn> = lib
                .get(EXTENSION_ENTRY_SYMBOL)
                .map_err(|e| format!("エントリシンボル未検出: {e}"))?;
            let boxed_plugin = entry();
            self.register_plugin(boxed_plugin, Some(lib));
            Ok(())
        }
    }

    pub fn load_dir(&mut self, dir: &Path) {
        if !dir.exists() {
            let _ = std::fs::create_dir_all(dir);
            return;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_ext = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext == "so" || ext == "dll" || ext == "dylib" || ext == "aux2");
            if is_ext {
                if let Err(err) = self.load_native(&path) {
                    eprintln!(
                        "[NeoUtl] ネイティブ拡張 '{}' 読み込み失敗: {err}",
                        path.display()
                    );
                } else {
                    eprintln!("[NeoUtl] ネイティブ拡張ロード完了: {}", path.display());
                }
            }
        }
    }

    pub fn menus_for_location(&self, location: &MenuLocation) -> Vec<&MenuItemDescriptor> {
        self.registered_menus
            .iter()
            .filter_map(|(_, m)| {
                if &m.location == location {
                    Some(m)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn is_panel_open(&self, panel_id: &str) -> bool {
        self.open_panels.get(panel_id).copied().unwrap_or(false)
    }

    pub fn toggle_panel(&mut self, panel_id: &str) {
        let entry = self.open_panels.entry(panel_id.to_owned()).or_insert(false);
        *entry = !*entry;
    }

    #[allow(dead_code)]
    pub fn set_panel_open(&mut self, panel_id: &str, open: bool) {
        self.open_panels.insert(panel_id.to_owned(), open);
    }

    pub fn registered_panels(&self) -> &[(String, PanelDescriptor)] {
        &self.registered_panels
    }

    pub fn execute_command(
        &mut self,
        command_id: &str,
        state: Option<&SharedAppState>,
    ) -> Result<(), String> {
        let panel_to_toggle = self.registered_panels.iter().find_map(|(_, panel)| {
            if command_id == format!("{}.toggle", panel.id) || command_id == panel.id {
                Some(panel.id.clone())
            } else {
                None
            }
        });
        if let Some(id) = panel_to_toggle {
            self.toggle_panel(&id);
            return Ok(());
        }

        let mut edit_session = state.map(AppEditSession::new);
        let mut executed = false;

        for plugin in &mut self.plugins {
            let plugin_id = plugin.metadata.id.clone();
            let storage = self.plugin_storages.entry(plugin_id.clone()).or_default();
            let mut panels = Vec::new();
            let mut menus = Vec::new();
            let mut commands = Vec::new();
            let mut file_drops = Vec::new();
            let mut toasts = Vec::new();
            let mut outgoing = Vec::new();

            let mut ctx = ExtensionContext {
                plugin_id: &plugin_id,
                storage,
                edit_session: edit_session.as_mut().map(|s| s as &mut dyn EditSession),
                registered_panels: &mut panels,
                registered_menus: &mut menus,
                registered_commands: &mut commands,
                registered_file_drops: &mut file_drops,
                toast_messages: &mut toasts,
                outgoing_events: &mut outgoing,
            };

            if plugin.instance.on_command(command_id, &mut ctx).is_ok() {
                executed = true;
            }
            self.toasts.extend(toasts);
        }

        if executed {
            Ok(())
        } else {
            Err(format!("未知のコマンド: {command_id}"))
        }
    }

    pub fn dispatch_event(&mut self, event: &ExtensionEvent, state: Option<&SharedAppState>) {
        let mut edit_session = state.map(AppEditSession::new);

        for plugin in &mut self.plugins {
            let plugin_id = plugin.metadata.id.clone();
            let storage = self.plugin_storages.entry(plugin_id.clone()).or_default();
            let mut panels = Vec::new();
            let mut menus = Vec::new();
            let mut commands = Vec::new();
            let mut file_drops = Vec::new();
            let mut toasts = Vec::new();
            let mut outgoing = Vec::new();

            let mut ctx = ExtensionContext {
                plugin_id: &plugin_id,
                storage,
                edit_session: edit_session.as_mut().map(|s| s as &mut dyn EditSession),
                registered_panels: &mut panels,
                registered_menus: &mut menus,
                registered_commands: &mut commands,
                registered_file_drops: &mut file_drops,
                toast_messages: &mut toasts,
                outgoing_events: &mut outgoing,
            };

            plugin.instance.on_event(event, &mut ctx);
            self.toasts.extend(toasts);
        }
    }

    pub fn draw_panels(&mut self, ctx: &egui::Context, state: Option<&SharedAppState>) {
        let mut edit_session = state.map(AppEditSession::new);

        for (plugin_id, panel_desc) in self.registered_panels.clone() {
            let is_open = self
                .open_panels
                .get(&panel_desc.id)
                .copied()
                .unwrap_or(false);
            if !is_open {
                continue;
            }

            let mut open_var = true;
            let mut close_requested = false;

            if let Some(plugin) = self.plugins.iter_mut().find(|p| p.metadata.id == plugin_id) {
                let storage = self.plugin_storages.entry(plugin_id.clone()).or_default();
                let mut panels = Vec::new();
                let mut menus = Vec::new();
                let mut commands = Vec::new();
                let mut file_drops = Vec::new();
                let mut toasts = Vec::new();
                let mut outgoing = Vec::new();

                let mut ext_ctx = ExtensionContext {
                    plugin_id: &plugin_id,
                    storage,
                    edit_session: edit_session.as_mut().map(|s| s as &mut dyn EditSession),
                    registered_panels: &mut panels,
                    registered_menus: &mut menus,
                    registered_commands: &mut commands,
                    registered_file_drops: &mut file_drops,
                    toast_messages: &mut toasts,
                    outgoing_events: &mut outgoing,
                };

                egui::Window::new(&panel_desc.title)
                    .id(egui::Id::new(&panel_desc.id))
                    .open(&mut open_var)
                    .default_size(panel_desc.default_size)
                    .show(ctx, |ui| {
                        plugin.instance.ui(&panel_desc.id, ui, &mut ext_ctx);
                    });

                if !open_var {
                    close_requested = true;
                }
                self.toasts.extend(toasts);
            }

            if close_requested {
                self.open_panels.insert(panel_desc.id, false);
            }
        }
    }

    pub fn draw_preview_overlay(
        &mut self,
        overlay: &mut PreviewOverlayContext,
        state: Option<&SharedAppState>,
    ) {
        let mut edit_session = state.map(AppEditSession::new);

        for plugin in &mut self.plugins {
            let plugin_id = plugin.metadata.id.clone();
            let storage = self.plugin_storages.entry(plugin_id.clone()).or_default();
            let mut panels = Vec::new();
            let mut menus = Vec::new();
            let mut commands = Vec::new();
            let mut file_drops = Vec::new();
            let mut toasts = Vec::new();
            let mut outgoing = Vec::new();

            let mut ext_ctx = ExtensionContext {
                plugin_id: &plugin_id,
                storage,
                edit_session: edit_session.as_mut().map(|s| s as &mut dyn EditSession),
                registered_panels: &mut panels,
                registered_menus: &mut menus,
                registered_commands: &mut commands,
                registered_file_drops: &mut file_drops,
                toast_messages: &mut toasts,
                outgoing_events: &mut outgoing,
            };

            plugin.instance.draw_preview_overlay(overlay, &mut ext_ctx);
            self.toasts.extend(toasts);
        }
    }

    pub fn draw_timeline_overlay(
        &mut self,
        overlay: &mut TimelineOverlayContext,
        state: Option<&SharedAppState>,
    ) {
        let mut edit_session = state.map(AppEditSession::new);

        for plugin in &mut self.plugins {
            let plugin_id = plugin.metadata.id.clone();
            let storage = self.plugin_storages.entry(plugin_id.clone()).or_default();
            let mut panels = Vec::new();
            let mut menus = Vec::new();
            let mut commands = Vec::new();
            let mut file_drops = Vec::new();
            let mut toasts = Vec::new();
            let mut outgoing = Vec::new();

            let mut ext_ctx = ExtensionContext {
                plugin_id: &plugin_id,
                storage,
                edit_session: edit_session.as_mut().map(|s| s as &mut dyn EditSession),
                registered_panels: &mut panels,
                registered_menus: &mut menus,
                registered_commands: &mut commands,
                registered_file_drops: &mut file_drops,
                toast_messages: &mut toasts,
                outgoing_events: &mut outgoing,
            };

            plugin.instance.draw_timeline_overlay(overlay, &mut ext_ctx);
            self.toasts.extend(toasts);
        }
    }

    pub fn handle_file_drop(
        &mut self,
        path: &str,
        layer: i32,
        frame: i32,
        state: Option<&SharedAppState>,
    ) -> bool {
        let p = Path::new(path);
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let mut edit_session = state.map(AppEditSession::new);

        for (plugin_id, drop_desc) in &self.registered_file_drops {
            if drop_desc
                .file_extensions
                .iter()
                .any(|e| e.eq_ignore_ascii_case(&ext))
            {
                if let Some(plugin) = self
                    .plugins
                    .iter_mut()
                    .find(|p| &p.metadata.id == plugin_id)
                {
                    let storage = self.plugin_storages.entry(plugin_id.clone()).or_default();
                    let mut panels = Vec::new();
                    let mut menus = Vec::new();
                    let mut commands = Vec::new();
                    let mut file_drops = Vec::new();
                    let mut toasts = Vec::new();
                    let mut outgoing = Vec::new();

                    let mut ext_ctx = ExtensionContext {
                        plugin_id,
                        storage,
                        edit_session: edit_session.as_mut().map(|s| s as &mut dyn EditSession),
                        registered_panels: &mut panels,
                        registered_menus: &mut menus,
                        registered_commands: &mut commands,
                        registered_file_drops: &mut file_drops,
                        toast_messages: &mut toasts,
                        outgoing_events: &mut outgoing,
                    };

                    if plugin
                        .instance
                        .on_file_drop(path, layer, frame, &mut ext_ctx)
                    {
                        self.toasts.extend(toasts);
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn save_project_storage(&self, project_dir: &Path) {
        let path = project_dir.join("extensions.json");
        if let Ok(json) = serde_json::to_string_pretty(&self.plugin_storages) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn load_project_storage(&mut self, project_dir: &Path) {
        let path = project_dir.join("extensions.json");
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(storages) = serde_json::from_str(&content) {
                    self.plugin_storages = storages;
                }
            }
        }
    }
}

pub fn global_extension_manager() -> &'static Mutex<ExtensionManager> {
    static MGR: OnceLock<Mutex<ExtensionManager>> = OnceLock::new();
    MGR.get_or_init(|| Mutex::new(ExtensionManager::new()))
}

pub fn default_extensions_dir() -> PathBuf {
    PathBuf::from("plugins")
}
