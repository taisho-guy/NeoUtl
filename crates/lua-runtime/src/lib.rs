use mlua::{Lua, MultiValue, RegistryKey, StdLib, Table, Value as LuaValue, Variadic};
use neoutl_effect_lua::LuaEffectSource;
use neoutl_sdk::extension::{ScriptArgs, ScriptValue};
use neoutl_shared_abi::PluginError;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// `system.call_script_function` からホスト側(`ExtensionManager::call_script_function`)へ
/// 橋渡しする関数。第1引数は関数名、第2引数はLuaから渡された引数列。
pub type ScriptBridgeFn =
    Arc<dyn Fn(&str, &ScriptArgs) -> Result<Vec<ScriptValue>, String> + Send + Sync>;

fn lua_value_to_script_value(value: &LuaValue) -> ScriptValue {
    match value {
        LuaValue::Nil => ScriptValue::Nil,
        LuaValue::Boolean(b) => ScriptValue::Boolean(*b),
        LuaValue::Integer(i) => ScriptValue::Integer(*i),
        LuaValue::Number(n) => ScriptValue::Number(*n),
        LuaValue::String(s) => ScriptValue::String(s.to_string_lossy()),
        _ => ScriptValue::Nil,
    }
}

fn script_value_to_lua_value(lua: &Lua, value: &ScriptValue) -> mlua::Result<LuaValue> {
    Ok(match value {
        ScriptValue::Nil => LuaValue::Nil,
        ScriptValue::Boolean(b) => LuaValue::Boolean(*b),
        ScriptValue::Integer(i) => LuaValue::Integer(*i),
        ScriptValue::Number(n) => LuaValue::Number(*n),
        ScriptValue::String(s) => LuaValue::String(lua.create_string(s)?),
    })
}

fn mlua_err(err: mlua::Error) -> PluginError {
    PluginError::Runtime(err.to_string())
}

#[derive(Clone, Debug)]
pub struct ComputeDef {
    pub id: String,
    pub wgsl: String,
}

#[derive(Default)]
struct Registrations {
    effects: Vec<LuaEffectSource>,
    computes: Vec<ComputeDef>,
    pre_render_hooks: Vec<RegistryKey>,
    post_export_hooks: Vec<RegistryKey>,
    reduce_results: std::collections::HashMap<String, Vec<f32>>,
}

pub struct LuaSystem {
    lua: Lua,
    regs: Arc<Mutex<Registrations>>,
    bridge: Arc<Mutex<Option<ScriptBridgeFn>>>,
}

impl LuaSystem {
    pub fn new() -> Result<Self, PluginError> {
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH,
            mlua::LuaOptions::new(),
        )
        .map_err(mlua_err)?;
        let regs = Arc::new(Mutex::new(Registrations::default()));
        let bridge: Arc<Mutex<Option<ScriptBridgeFn>>> = Arc::new(Mutex::new(None));
        install_system_table(&lua, &regs, &bridge).map_err(mlua_err)?;
        Ok(Self { lua, regs, bridge })
    }

    /// ホスト側の`ExtensionManager::call_script_function`を`system.call_script_function`
    /// として公開する。Lua側からの呼び出しはこの関数を経由してホストへ橋渡しされる。
    pub fn set_script_bridge(&self, bridge: ScriptBridgeFn) {
        *self.bridge.lock().unwrap() = Some(bridge);
    }

    pub fn load_script(&self, src: &str, chunk_name: &str) -> Result<(), PluginError> {
        self.lua
            .load(src)
            .set_name(chunk_name)
            .exec()
            .map_err(mlua_err)?;
        Ok(())
    }

    pub fn load_file(&self, path: &Path) -> Result<(), PluginError> {
        let src = std::fs::read_to_string(path)
            .map_err(|err| PluginError::Load(format!("{}: {err}", path.display())))?;
        self.load_script(&src, &path.to_string_lossy())
    }

    pub fn load_dir(&self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("lua"))
            .collect();
        paths.sort();
        for path in paths {
            if let Err(err) = self.load_file(&path) {
                eprintln!(
                    "{}",
                    t!(
                        "[NeoUtl] systemスクリプト実行失敗 %{arg0}: %{arg1}",
                        arg1 = format!("{}", err)
                    )
                );
            }
        }
    }

    pub fn drain_effects(&self) -> Vec<LuaEffectSource> {
        std::mem::take(&mut self.regs.lock().unwrap().effects)
    }

    pub fn drain_computes(&self) -> Vec<ComputeDef> {
        std::mem::take(&mut self.regs.lock().unwrap().computes)
    }

    pub fn run_pre_render_hooks(&self) -> Result<(), PluginError> {
        let len = self.regs.lock().unwrap().pre_render_hooks.len();
        for i in 0..len {
            let f: mlua::Function = {
                let regs = self.regs.lock().unwrap();
                let Some(key) = regs.pre_render_hooks.get(i) else {
                    continue;
                };
                self.lua.registry_value(key).map_err(mlua_err)?
            };
            f.call::<()>(()).map_err(mlua_err)?;
        }
        Ok(())
    }

    pub fn run_post_export_hooks(&self) -> Result<(), PluginError> {
        let len = self.regs.lock().unwrap().post_export_hooks.len();
        for i in 0..len {
            let f: mlua::Function = {
                let regs = self.regs.lock().unwrap();
                let Some(key) = regs.post_export_hooks.get(i) else {
                    continue;
                };
                self.lua.registry_value(key).map_err(mlua_err)?
            };
            f.call::<()>(()).map_err(mlua_err)?;
        }
        Ok(())
    }

    fn clear_hooks(&self) -> Result<(), PluginError> {
        let mut regs = self.regs.lock().unwrap();
        for key in regs.pre_render_hooks.drain(..) {
            self.lua.remove_registry_value(key).map_err(mlua_err)?;
        }
        for key in regs.post_export_hooks.drain(..) {
            self.lua.remove_registry_value(key).map_err(mlua_err)?;
        }
        Ok(())
    }

    pub fn reload_dir(&self, dir: &Path) -> Result<(), PluginError> {
        self.clear_hooks()?;
        {
            let mut regs = self.regs.lock().unwrap();
            regs.effects.clear();
            regs.computes.clear();
        }
        self.load_dir(dir);
        Ok(())
    }

    pub fn publish_reduce_result(&self, name: &str, values: &[f32]) {
        self.regs
            .lock()
            .unwrap()
            .reduce_results
            .insert(name.to_owned(), values.to_vec());
    }
}

fn install_system_table(
    lua: &Lua,
    regs: &Arc<Mutex<Registrations>>,
    bridge: &Arc<Mutex<Option<ScriptBridgeFn>>>,
) -> mlua::Result<()> {
    let system = lua.create_table()?;

    {
        let regs = regs.clone();
        let register_effect = lua.create_function(move |_, table: Table| {
            let path = Path::new("<system.register_effect>");
            match neoutl_effect_lua::build_effect_source(&table, path) {
                Ok(src) => {
                    regs.lock().unwrap().effects.push(src);
                    Ok(())
                }
                Err(err) => Err(mlua::Error::RuntimeError(err.to_string())),
            }
        })?;
        system.set("register_effect", register_effect)?;
    }

    {
        let regs = regs.clone();
        let register_compute = lua.create_function(move |_, (id, wgsl): (String, String)| {
            regs.lock().unwrap().computes.push(ComputeDef { id, wgsl });
            Ok(())
        })?;
        system.set("register_compute", register_compute)?;
    }

    {
        let regs = regs.clone();
        let on_pre_render = lua.create_function(move |lua, f: mlua::Function| {
            let key = lua.create_registry_value(f)?;
            regs.lock().unwrap().pre_render_hooks.push(key);
            Ok(())
        })?;
        system.set("on_pre_render", on_pre_render)?;
    }

    {
        let regs = regs.clone();
        let on_post_export = lua.create_function(move |lua, f: mlua::Function| {
            let key = lua.create_registry_value(f)?;
            regs.lock().unwrap().post_export_hooks.push(key);
            Ok(())
        })?;
        system.set("on_post_export", on_post_export)?;
    }

    {
        let regs = regs.clone();
        let reduce_result = lua.create_function(move |lua, name: String| {
            match regs.lock().unwrap().reduce_results.get(&name) {
                Some(values) => {
                    let t = lua.create_table()?;
                    for (i, v) in values.iter().enumerate() {
                        t.set(i + 1, *v)?;
                    }
                    Ok(LuaValue::Table(t))
                }
                None => Ok(LuaValue::Nil),
            }
        })?;
        system.set("reduce_result", reduce_result)?;
    }

    {
        let bridge = bridge.clone();
        let call_script_function = lua.create_function(
            move |lua, mut args: Variadic<LuaValue>| -> mlua::Result<MultiValue> {
                if args.is_empty() {
                    return Err(mlua::Error::RuntimeError(
                        "call_script_function: 関数名が必要".to_owned(),
                    ));
                }
                let name_value = args.remove(0);
                let name = match &name_value {
                    LuaValue::String(s) => s.to_string_lossy(),
                    _ => {
                        return Err(mlua::Error::RuntimeError(
                            "call_script_function: 第1引数は文字列であること".to_owned(),
                        ));
                    }
                };
                let script_args: Vec<ScriptValue> =
                    args.iter().map(lua_value_to_script_value).collect();

                let result = {
                    let guard = bridge.lock().unwrap();
                    match guard.as_ref() {
                        Some(f) => f(&name, &ScriptArgs::new(&script_args)),
                        None => Err("スクリプトブリッジ未接続".to_owned()),
                    }
                };

                match result {
                    Ok(values) => {
                        let mut out = MultiValue::new();
                        for v in &values {
                            out.push_back(script_value_to_lua_value(lua, v)?);
                        }
                        Ok(out)
                    }
                    Err(err) => Err(mlua::Error::RuntimeError(err)),
                }
            },
        )?;
        system.set("call_script_function", call_script_function)?;
    }

    lua.globals().set("system", system)?;
    Ok(())
}
rust_i18n::i18n!("../../i18n");
#[macro_use]
extern crate rust_i18n;
