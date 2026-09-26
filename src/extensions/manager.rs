use crate::app::state::SharedAppState;
use crate::extensions::builtin::{QuickPaletteExtension, SubtitleImporterExtension};
use crate::extensions::session::AppEditSession;
use libloading::{Library, Symbol};
use neoutl_sdk::extension::*;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_int};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub struct LoadedPlugin {
    pub instance: Box<dyn ExtensionPlugin>,
    pub metadata: ExtensionMetadata,
    pub _lib: Option<Library>,
}

#[repr(C)]
struct CHostAppTable {
    struct_size: c_int,
    app_name: *const c_char,
    version: c_int,
    show_toast: Option<unsafe extern "C" fn(*const c_char)>,
    log_info: Option<unsafe extern "C" fn(*const c_char)>,
    log_error: Option<unsafe extern "C" fn(*const c_char)>,
}
unsafe impl Sync for CHostAppTable {}

#[repr(C)]
struct CCommonPluginTable {
    struct_size: c_int,
    plugin_id: *const c_char,
    name: *const c_char,
    version: *const c_char,
    author: *const c_char,
    description: *const c_char,
    init: Option<unsafe extern "C" fn(*const CHostAppTable) -> c_int>,
    exit: Option<unsafe extern "C" fn() -> c_int>,
    command: Option<unsafe extern "C" fn(*const c_char) -> c_int>,
}

static C_HOST_TABLE: CHostAppTable = CHostAppTable {
    struct_size: std::mem::size_of::<CHostAppTable>() as c_int,
    app_name: c"NeoUtl".as_ptr(),
    version: 0x0008_0009,
    show_toast: Some(c_host_toast),
    log_info: Some(c_host_log_info),
    log_error: Some(c_host_log_error),
};

unsafe extern "C" fn c_host_toast(message: *const c_char) {
    if let Some(message) = unsafe { c_string(message) } {
        eprintln!("[NeoUtl C plugin notification] {message}");
    }
}
unsafe extern "C" fn c_host_log_info(message: *const c_char) {
    if let Some(message) = unsafe { c_string(message) } {
        eprintln!("[NeoUtl C plugin] {message}");
    }
}
unsafe extern "C" fn c_host_log_error(message: *const c_char) {
    if let Some(message) = unsafe { c_string(message) } {
        eprintln!("[NeoUtl C plugin error] {message}");
    }
}
unsafe fn c_string<'a>(value: *const c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned(),
    )
}

struct CExtensionAdapter {
    table: *const CCommonPluginTable,
    metadata: ExtensionMetadata,
    command_id: String,
    initialized: bool,
}
unsafe impl Send for CExtensionAdapter {}
unsafe impl Sync for CExtensionAdapter {}

impl CExtensionAdapter {
    unsafe fn from_table(table: *const CCommonPluginTable) -> Result<Self, String> {
        let table_ref = unsafe { table.as_ref() }.ok_or("C extension entry returned null")?;
        let required = std::mem::size_of::<CCommonPluginTable>() as c_int;
        if table_ref.struct_size < required {
            return Err(format!(
                "C extension table is too small: {} < {required}",
                table_ref.struct_size
            ));
        }
        let id = unsafe { c_string(table_ref.plugin_id) }
            .filter(|s| !s.is_empty())
            .ok_or("C extension has no plugin_id")?;
        let name = unsafe { c_string(table_ref.name) }
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| id.clone());
        let mut metadata = ExtensionMetadata::new(id.clone(), name.clone());
        if let Some(value) = unsafe { c_string(table_ref.version) } {
            metadata.version = value;
        }
        if let Some(value) = unsafe { c_string(table_ref.author) } {
            metadata.author = value;
        }
        if let Some(value) = unsafe { c_string(table_ref.description) } {
            metadata.description = value;
        }
        Ok(Self {
            table,
            metadata,
            command_id: format!("{id}.command"),
            initialized: false,
        })
    }
}
impl ExtensionPlugin for CExtensionAdapter {
    fn metadata(&self) -> ExtensionMetadata {
        self.metadata.clone()
    }

    fn init(&mut self, ctx: &mut ExtensionContext) -> Result<(), String> {
        let table = unsafe { &*self.table };
        if let Some(init) = table.init {
            let code = unsafe { init(&C_HOST_TABLE) };
            if code != 0 {
                return Err(format!("C extension init returned {code}"));
            }
        }
        self.initialized = true;
        if table.command.is_some() {
            ctx.register_menu(MenuItemDescriptor {
                id: self.command_id.clone(),
                location: MenuLocation::MenuBar(MenuBarSection::Plugins),
                label: self.metadata.name.clone(),
                shortcut_hint: None,
                command_id: self.command_id.clone(),
                enabled: true,
                checked: None,
            });
        }
        Ok(())
    }

    fn on_command(&mut self, command_id: &str, _ctx: &mut ExtensionContext) -> Result<(), String> {
        if command_id != self.command_id {
            return Err("unknown C extension command".to_owned());
        }
        let table = unsafe { &*self.table };
        let Some(command) = table.command else {
            return Err("C extension has no command callback".to_owned());
        };
        let command_name = CString::new(command_id).map_err(|e| e.to_string())?;
        let code = unsafe { command(command_name.as_ptr()) };
        if code == 0 {
            Ok(())
        } else {
            Err(format!("C extension command returned {code}"))
        }
    }
}
impl Drop for CExtensionAdapter {
    fn drop(&mut self) {
        if self.initialized {
            if let Some(exit) = unsafe { &*self.table }.exit {
                let _ = unsafe { exit() };
            }
            self.initialized = false;
        }
    }
}

pub struct ExtensionManager {
    plugins: Vec<LoadedPlugin>,
    open_panels: HashMap<String, bool>,
    registered_panels: Vec<(String, PanelDescriptor)>,
    registered_menus: Vec<(String, MenuItemDescriptor)>,
    registered_commands: Vec<(String, CommandDescriptor)>,
    registered_file_drops: Vec<(String, FileDropDescriptor)>,
    registered_script_functions: HashMap<String, (String, ScriptFn)>,
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
            registered_script_functions: HashMap::new(),
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
        let mut script_functions = Vec::new();
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
            registered_script_functions: &mut script_functions,
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
        for s in script_functions {
            if let Some((owner, _)) = self.registered_script_functions.get(s.name) {
                eprintln!(
                    "[NeoUtl] スクリプト関数 '{}' は '{}' により登録済みのため '{}' の登録を拒否",
                    s.name, owner, plugin_id
                );
                continue;
            }
            self.registered_script_functions
                .insert(s.name.to_owned(), (plugin_id.clone(), s.func));
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
            if let Ok(entry) = lib.get::<ExtensionCreateFn>(EXTENSION_ENTRY_SYMBOL) {
                let entry = *entry;
                self.register_plugin(entry(), Some(lib));
                return Ok(());
            }
            let c_entry: Symbol<unsafe extern "C" fn() -> *const CCommonPluginTable> = lib
                .get(b"neoutl_c_extension_entry\0")
                .map_err(|e| format!("Rust/C拡張エントリシンボル未検出: {e}"))?;
            let table = c_entry();
            let plugin = CExtensionAdapter::from_table(table)?;
            self.register_plugin(Box::new(plugin), Some(lib));
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

    /// スクリプト(Lua)側から呼び出し可能な登録関数を実行する。
    /// `name`未登録時は`Err`を返す。呼び出し自体の橋渡し(引数/戻り値の型変換)は
    /// crates/lua-runtime側でこの関数をラップして行う。
    pub fn call_script_function(
        &self,
        name: &str,
        args: &ScriptArgs,
    ) -> Result<Vec<ScriptValue>, String> {
        match self.registered_script_functions.get(name) {
            Some((_, func)) => func(args),
            None => Err(format!("未登録のスクリプト関数: {name}")),
        }
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
            let mut script_functions = Vec::new();
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
                registered_script_functions: &mut script_functions,
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
            let mut script_functions = Vec::new();
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
                registered_script_functions: &mut script_functions,
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
                let mut script_functions = Vec::new();
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
                    registered_script_functions: &mut script_functions,
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
            let mut script_functions = Vec::new();
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
                registered_script_functions: &mut script_functions,
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
            let mut script_functions = Vec::new();
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
                registered_script_functions: &mut script_functions,
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
                    let mut script_functions = Vec::new();
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
                        registered_script_functions: &mut script_functions,
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
