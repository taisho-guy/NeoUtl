use arc_swap::ArcSwap;
use libloading::{Library, Symbol};
use neoutl_effect_api::{ENTRY_SYMBOL, EffectVTable, EntryFn};
use neoutl_effect_lua::LuaEffectSource;
use neoutl_shared_abi::ParamRowOwned;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

pub struct EffectPlugin {
    pub id: String,
    pub name: String,
    pub category: String,
    pub vtable: &'static EffectVTable,
    _lib: Library,
}

pub enum EffectSource {
    Native(EffectPlugin),
    Lua(LuaEffectSource),
}

impl EffectSource {
    pub fn id(&self) -> &str {
        match self {
            Self::Native(p) => &p.id,
            Self::Lua(s) => &s.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Native(p) => &p.name,
            Self::Lua(s) => &s.name,
        }
    }

    pub fn category(&self) -> &str {
        match self {
            Self::Native(p) => &p.category,
            Self::Lua(s) => &s.category,
        }
    }

    pub fn wgsl_bytes(&self) -> &[u8] {
        match self {
            Self::Native(p) => {
                let src = unsafe { (p.vtable.wgsl)() };
                if src.ptr.is_null() {
                    &[]
                } else {
                    unsafe { std::slice::from_raw_parts(src.ptr, src.len) }
                }
            }
            Self::Lua(s) => s.wgsl.as_bytes(),
        }
    }

    pub fn param_schema(&self) -> Vec<ParamRowOwned> {
        match self {
            Self::Native(p) => {
                let meta = unsafe { &*((p.vtable.meta)()) };
                if meta.param_schema_ptr.is_null() || meta.param_schema_len == 0 {
                    return Vec::new();
                }
                let raw = unsafe {
                    std::slice::from_raw_parts(meta.param_schema_ptr, meta.param_schema_len)
                };
                raw.iter().map(|s| unsafe { s.to_owned_row() }).collect()
            }
            Self::Lua(s) => s.param_schema.clone(),
        }
    }

    pub fn requires_texture_param_index(&self) -> Option<u32> {
        match self {
            Self::Native(p) => p.vtable.requires_texture_param.map(|f| unsafe { f() }),
            Self::Lua(_) => None,
        }
    }

    pub fn uniform_size(&self) -> u32 {
        match self {
            Self::Native(p) => unsafe { (p.vtable.uniform_size)() },
            Self::Lua(s) => neoutl_effect_api::uniform_size_std(s.param_schema.len() as u32),
        }
    }

    pub fn pack_uniform(&self, params: &[f32], out: &mut [u8]) {
        match self {
            Self::Native(p) => unsafe {
                (p.vtable.pack_uniform)(params.as_ptr(), params.len() as u32, out.as_mut_ptr());
            },
            Self::Lua(_) => unsafe {
                neoutl_effect_api::pack_uniform_std(
                    params.as_ptr(),
                    params.len() as u32,
                    out.as_mut_ptr(),
                );
            },
        }
    }
}

fn registry_swap() -> &'static ArcSwap<Vec<Arc<EffectSource>>> {
    static SWAP: OnceLock<ArcSwap<Vec<Arc<EffectSource>>>> = OnceLock::new();
    SWAP.get_or_init(|| ArcSwap::new(Arc::new(Vec::new())))
}

pub fn load_all(effects_dir: &Path, scripts_dir: &Path) {
    let mut ids = std::collections::HashSet::new();
    let mut sources: Vec<Arc<EffectSource>> = Vec::new();

    for plugin in load_native(effects_dir) {
        if ids.insert(plugin.id.clone()) {
            sources.push(Arc::new(EffectSource::Native(plugin)));
        } else {
            eprintln!("{}", t!("[NeoUtl] エフェクトID重複、除外: %{arg0}"));
        }
    }
    for lua_source in neoutl_effect_lua::load_dir(scripts_dir) {
        crate::localization::load_plugin_catalog(&lua_source.script_path);
        if ids.insert(lua_source.id.clone()) {
            sources.push(Arc::new(EffectSource::Lua(lua_source)));
        } else {
            eprintln!("{}", t!("[NeoUtl] エフェクトID重複、除外: %{arg0}"));
        }
    }

    sources.sort_by(|a, b| a.id().cmp(b.id()));
    for s in &sources {
        let kind = match s.as_ref() {
            EffectSource::Native(_) => "native",
            EffectSource::Lua(_) => "lua",
        };
        eprintln!(
            "{}",
            t!(
                "[NeoUtl] エフェクト登録: %{arg0} (%{arg1}) [%{arg2}]",
                arg0 = format!("{}", s.id()),
                arg1 = format!("{}", s.name()),
                arg2 = format!("{}", kind)
            )
        );
    }
    registry_swap().store(Arc::new(sources));
}

pub fn registry() -> Arc<Vec<Arc<EffectSource>>> {
    registry_swap().load_full()
}

pub fn by_id(id: &str) -> Option<Arc<EffectSource>> {
    registry().iter().find(|p| p.id() == id).cloned()
}

pub fn reload_one(path: &Path) -> Result<(), String> {
    let new_plugin = load_one(path).map_err(|e| e.to_string())?;
    let current = registry_swap().load_full();
    let Some(pos) = current.iter().position(|s| s.id() == new_plugin.id) else {
        return Err(format!(
            "既存エフェクト未検出、新規追加は対象外: {}",
            new_plugin.id
        ));
    };

    let id = new_plugin.id.clone();
    let mut new_plugin = Some(new_plugin);
    let next: Vec<Arc<EffectSource>> = current
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if i == pos {
                Arc::new(EffectSource::Native(
                    new_plugin.take().expect("posは一度のみ一致"),
                ))
            } else {
                Arc::clone(s)
            }
        })
        .collect();
    registry_swap().store(Arc::new(next));
    eprintln!(
        "{}",
        t!(
            "[NeoUtl] エフェクト再ロード完了: %{arg0}",
            arg0 = format!("{}", id)
        )
    );
    Ok(())
}

pub fn reload_lua(sources: Vec<LuaEffectSource>) {
    let current = registry_swap().load_full();
    let mut ids = std::collections::HashSet::new();
    let mut next: Vec<Arc<EffectSource>> = Vec::new();

    for s in current.iter() {
        if matches!(s.as_ref(), EffectSource::Native(_)) && ids.insert(s.id().to_owned()) {
            next.push(Arc::clone(s));
        }
    }
    for lua_source in sources {
        if ids.insert(lua_source.id.clone()) {
            next.push(Arc::new(EffectSource::Lua(lua_source)));
        } else {
            eprintln!("{}", t!("[NeoUtl] エフェクトID重複、除外: %{arg0}"));
        }
    }

    next.sort_by(|a, b| a.id().cmp(b.id()));
    let count = next.len();
    registry_swap().store(Arc::new(next));
    eprintln!(
        "{}",
        t!(
            "[NeoUtl] Luaエフェクト再ロード完了: %{arg0}件",
            arg0 = format!("{}", count)
        )
    );
}

fn load_native(effects_dir: &Path) -> Vec<EffectPlugin> {
    let entries = match std::fs::read_dir(effects_dir) {
        Ok(e) => e,
        Err(err) => {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] effects/ 読み込み失敗: %{arg0}",
                    arg0 = format!("{}", err)
                )
            );
            return Vec::new();
        }
    };
    let candidates: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_dylib(p))
        .collect();

    candidates
        .iter()
        .filter_map(|path| match load_one(path) {
            Ok(p) => Some(p),
            Err(err) => {
                eprintln!(
                    "{}",
                    t!(
                        "[NeoUtl] エフェクト読み込み失敗 %{arg0}: %{arg1}",
                        arg1 = format!("{}", err)
                    )
                );
                None
            }
        })
        .collect()
}

pub fn default_effects_dir() -> PathBuf {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from("effects");
    };

    #[cfg(target_os = "macos")]
    {
        let resources_dir = exe_dir.join("../Resources/effects");
        if resources_dir.is_dir() {
            return resources_dir;
        }
    }

    exe_dir.join("effects")
}

pub fn default_effects_lua_dir() -> PathBuf {
    default_effects_dir()
        .parent()
        .map(|p| p.join("scripts"))
        .unwrap_or_else(|| PathBuf::from("scripts"))
}

fn load_one(path: &Path) -> Result<EffectPlugin, Box<dyn std::error::Error>> {
    crate::localization::load_plugin_catalog(path);
    let lib = unsafe { Library::new(path) }?;
    let entry: Symbol<EntryFn> = unsafe { lib.get(ENTRY_SYMBOL) }?;
    let vtable: &'static EffectVTable = unsafe { &*entry() };
    let meta = unsafe { &*((vtable.meta)()) };
    Ok(EffectPlugin {
        id: meta.id.to_owned(),
        name: meta.name.to_owned(),
        category: meta.category.to_owned(),
        vtable,
        _lib: lib,
    })
}

fn is_dylib(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("so" | "dylib" | "dll")
    )
}
