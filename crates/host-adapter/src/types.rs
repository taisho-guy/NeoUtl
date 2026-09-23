#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PluginFormat {
    Vst3,
    Clap,
    Lv2,
    Vst2,
    Au,
    Sf2,
    Sfz,
    Jsfx,
    Internal,
}

impl PluginFormat {
    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        let ext = path.extension().and_then(|e| e.to_str())?;
        match ext.to_lowercase().as_str() {
            "vst3" => Some(Self::Vst3),
            "clap" => Some(Self::Clap),
            "lv2" => Some(Self::Lv2),
            "vst" | "dll" | "so" | "dylib" => Some(Self::Vst2),
            "component" => Some(Self::Au),
            "sf2" => Some(Self::Sf2),
            "sfz" => Some(Self::Sfz),
            "jsfx" => Some(Self::Jsfx),
            _ => None,
        }
    }

    pub fn host_format_tag(&self) -> Result<&'static str, HostError> {
        match self {
            Self::Clap => Ok("clap"),
            Self::Vst3 => Ok("vst3"),
            Self::Lv2 => Ok("lv2"),
            _ => Err(HostError::UnsupportedFormat(*self)),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PluginParamInfo {
    pub id: u32,
    pub name: String,
    pub symbol: String,
    pub unit: String,
    pub comment: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PluginCatalogEntry {
    pub format: PluginFormat,
    pub name: String,
    pub vendor: String,
    pub plugin_id: String,
    pub path: std::path::PathBuf,
}

#[derive(Debug)]
pub enum HostError {
    UnsupportedFormat(PluginFormat),
    Blocked(String),
    ShmCreateFailed(String),
    EventPairFailed(std::io::Error),
    SpawnFailed(std::io::Error),
    ReadyTimeout,
    RequestTimeout,
    RequestFailed(String),
    ScratchDecodeFailed,
    UnknownInstance(u32),
    BlockTooLarge { frames: usize, max: usize },
    ProcessDead,
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat(fmt_) => write!(f, "unsupported plugin format: {fmt_:?}"),
            Self::Blocked(spec) => write!(f, "plugin blocklisted after repeated failures: {spec}"),
            Self::ShmCreateFailed(e) => write!(f, "shared-memory allocation failed: {e}"),
            Self::EventPairFailed(e) => write!(f, "event-pipe creation failed: {e}"),
            Self::SpawnFailed(e) => write!(f, "maolan-plugin-host spawn failed: {e}"),
            Self::ReadyTimeout => write!(f, "plugin-host did not signal ready before timeout"),
            Self::RequestTimeout => write!(f, "plugin-host did not answer request before timeout"),
            Self::RequestFailed(e) => write!(f, "plugin-host reported request error: {e}"),
            Self::ScratchDecodeFailed => write!(f, "scratch buffer decode failed"),
            Self::UnknownInstance(id) => write!(f, "unknown plugin instance id: {id}"),
            Self::BlockTooLarge { frames, max } => {
                write!(f, "block size {frames} exceeds protocol maximum {max}")
            }
            Self::ProcessDead => write!(f, "plugin-host process is no longer responding"),
        }
    }
}

impl std::error::Error for HostError {}
