#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("unknown plugin format: {0}")]
    UnknownFormat(String),
    #[error("vst3 error: {0}")]
    Vst3(String),
    #[error("clap error: {0}")]
    Clap(String),
}

impl From<vst3_host::Error> for PluginError {
    fn from(e: vst3_host::Error) -> Self {
        Self::Vst3(e.to_string())
    }
}
