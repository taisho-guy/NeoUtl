use mlua::{Lua, StdLib, Table, Value as LuaValue};
use neoutl_shared_abi::{ParamKind, ParamRowOwned};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct LuaEffectSource {
    pub id: String,
    pub name: String,
    pub category: String,
    pub param_schema: Vec<ParamRowOwned>,
    pub wgsl: String,
    pub script_path: PathBuf,
}

#[derive(Debug)]
pub enum LuaEffectError {
    Lua(mlua::Error),
    MissingField(&'static str),
    InvalidField(&'static str),
    UnknownParamKind(String),
}

impl std::fmt::Display for LuaEffectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lua(err) => write!(f, "Lua実行エラー: {err}"),
            Self::MissingField(name) => write!(f, "必須フィールド欠落: {name}"),
            Self::InvalidField(name) => write!(f, "フィールド型不正: {name}"),
            Self::UnknownParamKind(kind) => write!(f, "未知のparam kind: {kind}"),
        }
    }
}
impl std::error::Error for LuaEffectError {}
impl From<mlua::Error> for LuaEffectError {
    fn from(err: mlua::Error) -> Self {
        Self::Lua(err)
    }
}

fn sandboxed_lua() -> mlua::Result<Lua> {
    Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH,
        mlua::LuaOptions::new(),
    )
}

pub fn load(path: &Path) -> Result<LuaEffectSource, LuaEffectError> {
    let src = std::fs::read_to_string(path).map_err(|err| {
        LuaEffectError::Lua(mlua::Error::RuntimeError(format!(
            "スクリプト読込失敗 {}: {err}",
            path.display()
        )))
    })?;
    let lua = sandboxed_lua()?;
    let chunk_name = path.to_string_lossy().into_owned();
    let value: LuaValue = lua.load(&src).set_name(&chunk_name).eval()?;
    let table = match value {
        LuaValue::Table(t) => t,
        _ => return Err(LuaEffectError::InvalidField("(戻り値はtableであること)")),
    };
    build_effect_source(&table, path)
}

pub fn load_dir(dir: &Path) -> Vec<LuaEffectSource> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("lua"))
        .collect();
    paths.sort();

    paths
        .into_iter()
        .filter_map(|path| {
            if std::fs::read_to_string(&path)
                .map(|source| source.contains("system."))
                .unwrap_or(false)
            {
                return None;
            }
            match load(&path) {
                Ok(src) => Some(src),
                Err(err) => {
                    eprintln!("[NeoUtl] Luaエフェクト読込失敗 {}: {err}", path.display());
                    None
                }
            }
        })
        .collect()
}

pub fn build_effect_source(table: &Table, path: &Path) -> Result<LuaEffectSource, LuaEffectError> {
    let id: String = table
        .get("id")
        .map_err(|_| LuaEffectError::MissingField("id"))?;
    let name: String = table
        .get("name")
        .map_err(|_| LuaEffectError::MissingField("name"))?;
    let category: String = table
        .get("category")
        .map_err(|_| LuaEffectError::MissingField("category"))?;
    let wgsl: String = table
        .get("wgsl")
        .map_err(|_| LuaEffectError::MissingField("wgsl"))?;
    let params_table: Table = table
        .get("params")
        .map_err(|_| LuaEffectError::MissingField("params"))?;

    let mut param_schema = Vec::new();
    for pair in params_table.sequence_values::<Table>() {
        let row = pair.map_err(|_| LuaEffectError::InvalidField("params[i]"))?;
        param_schema.push(build_param_row(&row)?);
    }

    Ok(LuaEffectSource {
        id,
        name,
        category,
        param_schema,
        wgsl,
        script_path: path.to_path_buf(),
    })
}

fn build_param_row(row: &Table) -> Result<ParamRowOwned, LuaEffectError> {
    let key: String = row
        .get("key")
        .map_err(|_| LuaEffectError::MissingField("params[i].key"))?;
    let label: String = row.get("label").unwrap_or_else(|_| key.clone());
    let kind_str: String = row
        .get("kind")
        .map_err(|_| LuaEffectError::MissingField("params[i].kind"))?;
    let kind = parse_param_kind(&kind_str)?;
    let min: f32 = row.get("min").unwrap_or(0.0);
    let max: f32 = row.get("max").unwrap_or(1.0);
    let step: f32 = row.get("step").unwrap_or(0.01);
    let default_float: f32 = row.get("default").unwrap_or(0.0);
    let enum_options: Vec<String> = if kind == ParamKind::Enum {
        let opts: Table = row
            .get("options")
            .map_err(|_| LuaEffectError::MissingField("params[i].options"))?;
        opts.sequence_values::<String>()
            .collect::<Result<_, _>>()
            .map_err(|_| LuaEffectError::InvalidField("params[i].options"))?
    } else {
        Vec::new()
    };

    Ok(ParamRowOwned {
        key,
        label,
        kind,
        min,
        max,
        step,
        default_float,
        enum_options,
    })
}

fn parse_param_kind(s: &str) -> Result<ParamKind, LuaEffectError> {
    Ok(match s {
        "float" => ParamKind::Float,
        "bool" => ParamKind::Bool,
        "color" => ParamKind::Color,
        "enum" => ParamKind::Enum,
        "text" => ParamKind::Text,
        "filepath" => ParamKind::FilePath,
        "track" => ParamKind::Track,
        "separator" => ParamKind::Separator,
        "group" => ParamKind::Group,
        "folder" => ParamKind::Folder,
        other => return Err(LuaEffectError::UnknownParamKind(other.to_owned())),
    })
}
