use mlua::{Lua, RegistryKey, StdLib, Table, Value as LuaValue};
use neoutl_effect_lua::LuaEffectSource;
use std::path::Path;
use std::sync::{Arc, Mutex};

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
}

#[derive(Debug)]
pub enum SystemError {
    Lua(mlua::Error),
}
impl std::fmt::Display for SystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lua(err) => write!(f, "Lua実行エラー: {err}"),
        }
    }
}
impl std::error::Error for SystemError {}
impl From<mlua::Error> for SystemError {
    fn from(err: mlua::Error) -> Self {
        Self::Lua(err)
    }
}

impl LuaSystem {
    pub fn new() -> Result<Self, SystemError> {
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH,
            mlua::LuaOptions::new(),
        )?;
        let regs = Arc::new(Mutex::new(Registrations::default()));
        install_system_table(&lua, &regs)?;
        Ok(Self { lua, regs })
    }

    pub fn load_script(&self, src: &str, chunk_name: &str) -> Result<(), SystemError> {
        self.lua.load(src).set_name(chunk_name).exec()?;
        Ok(())
    }

    pub fn load_file(&self, path: &Path) -> Result<(), SystemError> {
        let src = std::fs::read_to_string(path).map_err(|err| {
            SystemError::Lua(mlua::Error::RuntimeError(format!(
                "スクリプト読込失敗 {}: {err}",
                path.display()
            )))
        })?;
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

    pub fn run_pre_render_hooks(&self) -> Result<(), SystemError> {
        let len = self.regs.lock().unwrap().pre_render_hooks.len();
        for i in 0..len {
            let f: mlua::Function = {
                let regs = self.regs.lock().unwrap();
                let Some(key) = regs.pre_render_hooks.get(i) else {
                    continue;
                };
                self.lua.registry_value(key)?
            };
            f.call::<()>(())?;
        }
        Ok(())
    }

    pub fn run_post_export_hooks(&self) -> Result<(), SystemError> {
        let len = self.regs.lock().unwrap().post_export_hooks.len();
        for i in 0..len {
            let f: mlua::Function = {
                let regs = self.regs.lock().unwrap();
                let Some(key) = regs.post_export_hooks.get(i) else {
                    continue;
                };
                self.lua.registry_value(key)?
            };
            f.call::<()>(())?;
        }
        Ok(())
    }

    fn clear_hooks(&self) -> Result<(), SystemError> {
        let mut regs = self.regs.lock().unwrap();
        for key in regs.pre_render_hooks.drain(..) {
            self.lua.remove_registry_value(key)?;
        }
        for key in regs.post_export_hooks.drain(..) {
            self.lua.remove_registry_value(key)?;
        }
        Ok(())
    }

    pub fn reload_dir(&self, dir: &Path) -> Result<(), SystemError> {
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

fn install_system_table(lua: &Lua, regs: &Arc<Mutex<Registrations>>) -> mlua::Result<()> {
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

    lua.globals().set("system", system)?;
    Ok(())
}
rust_i18n::i18n!("../../i18n");
#[macro_use]
extern crate rust_i18n;
