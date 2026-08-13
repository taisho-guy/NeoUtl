use std::ffi::CStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum EngineProcessMode {
    SingleClient = crate::ffi::CarlaBackend_EngineProcessMode_ENGINE_PROCESS_MODE_SINGLE_CLIENT,
    MultipleClients =
        crate::ffi::CarlaBackend_EngineProcessMode_ENGINE_PROCESS_MODE_MULTIPLE_CLIENTS,
    ContinuousRack = crate::ffi::CarlaBackend_EngineProcessMode_ENGINE_PROCESS_MODE_CONTINUOUS_RACK,
    Patchbay = crate::ffi::CarlaBackend_EngineProcessMode_ENGINE_PROCESS_MODE_PATCHBAY,
    Bridge = crate::ffi::CarlaBackend_EngineProcessMode_ENGINE_PROCESS_MODE_BRIDGE,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum EngineTransportMode {
    Disabled = crate::ffi::CarlaBackend_EngineTransportMode_ENGINE_TRANSPORT_MODE_DISABLED,
    Internal = crate::ffi::CarlaBackend_EngineTransportMode_ENGINE_TRANSPORT_MODE_INTERNAL,
    Jack = crate::ffi::CarlaBackend_EngineTransportMode_ENGINE_TRANSPORT_MODE_JACK,
    Plugin = crate::ffi::CarlaBackend_EngineTransportMode_ENGINE_TRANSPORT_MODE_PLUGIN,
    Bridge = crate::ffi::CarlaBackend_EngineTransportMode_ENGINE_TRANSPORT_MODE_BRIDGE,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum EngineOption {
    Debug = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_DEBUG,
    ProcessMode = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_PROCESS_MODE,
    TransportMode = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_TRANSPORT_MODE,
    ForceStereo = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_FORCE_STEREO,
    PreferPluginBridges = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_PREFER_PLUGIN_BRIDGES,
    PreferUiBridges = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_PREFER_UI_BRIDGES,
    UisAlwaysOnTop = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_UIS_ALWAYS_ON_TOP,
    MaxParameters = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_MAX_PARAMETERS,
    ResetXruns = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_RESET_XRUNS,
    UiBridgesTimeout = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_UI_BRIDGES_TIMEOUT,
    AudioBufferSize = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_AUDIO_BUFFER_SIZE,
    AudioSampleRate = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_AUDIO_SAMPLE_RATE,
    AudioTripleBuffer = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_AUDIO_TRIPLE_BUFFER,
    AudioDriver = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_AUDIO_DRIVER,
    AudioDevice = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_AUDIO_DEVICE,
    OscEnabled = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_OSC_ENABLED,
    OscPortTcp = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_OSC_PORT_TCP,
    OscPortUdp = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_OSC_PORT_UDP,
    FilePath = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_FILE_PATH,
    PluginPath = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_PLUGIN_PATH,
    PathBinaries = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_PATH_BINARIES,
    PathResources = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_PATH_RESOURCES,
    PreventBadBehaviour = crate::ffi::CarlaBackend_EngineOption_ENGINE_OPTION_PREVENT_BAD_BEHAVIOUR,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum BinaryType {
    None = crate::ffi::CarlaBackend_BinaryType_BINARY_NONE,
    Posix32 = crate::ffi::CarlaBackend_BinaryType_BINARY_POSIX32,
    Posix64 = crate::ffi::CarlaBackend_BinaryType_BINARY_POSIX64,
    Win32 = crate::ffi::CarlaBackend_BinaryType_BINARY_WIN32,
    Win64 = crate::ffi::CarlaBackend_BinaryType_BINARY_WIN64,
    Other = crate::ffi::CarlaBackend_BinaryType_BINARY_OTHER,
}

impl BinaryType {
    pub const NATIVE: BinaryType = {
        #[cfg(all(unix, target_pointer_width = "64"))]
        {
            BinaryType::Posix64
        }
        #[cfg(all(unix, target_pointer_width = "32"))]
        {
            BinaryType::Posix32
        }
        #[cfg(all(windows, target_pointer_width = "64"))]
        {
            BinaryType::Win64
        }
        #[cfg(all(windows, target_pointer_width = "32"))]
        {
            BinaryType::Win32
        }
        #[cfg(not(any(unix, windows)))]
        {
            BinaryType::Other
        }
    };

    pub fn native() -> Self {
        Self::NATIVE
    }
}

impl From<crate::ffi::CarlaBackend_BinaryType> for BinaryType {
    fn from(v: crate::ffi::CarlaBackend_BinaryType) -> Self {
        match v {
            crate::ffi::CarlaBackend_BinaryType_BINARY_NONE => Self::None,
            crate::ffi::CarlaBackend_BinaryType_BINARY_POSIX32 => Self::Posix32,
            crate::ffi::CarlaBackend_BinaryType_BINARY_POSIX64 => Self::Posix64,
            crate::ffi::CarlaBackend_BinaryType_BINARY_WIN32 => Self::Win32,
            crate::ffi::CarlaBackend_BinaryType_BINARY_WIN64 => Self::Win64,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum PluginType {
    None = crate::ffi::CarlaBackend_PluginType_PLUGIN_NONE,
    Internal = crate::ffi::CarlaBackend_PluginType_PLUGIN_INTERNAL,
    Ladspa = crate::ffi::CarlaBackend_PluginType_PLUGIN_LADSPA,
    Dssi = crate::ffi::CarlaBackend_PluginType_PLUGIN_DSSI,
    Lv2 = crate::ffi::CarlaBackend_PluginType_PLUGIN_LV2,
    Vst2 = crate::ffi::CarlaBackend_PluginType_PLUGIN_VST2,
    Vst3 = crate::ffi::CarlaBackend_PluginType_PLUGIN_VST3,
    Au = crate::ffi::CarlaBackend_PluginType_PLUGIN_AU,
    Dls = crate::ffi::CarlaBackend_PluginType_PLUGIN_DLS,
    Gig = crate::ffi::CarlaBackend_PluginType_PLUGIN_GIG,
    Sf2 = crate::ffi::CarlaBackend_PluginType_PLUGIN_SF2,
    Sfz = crate::ffi::CarlaBackend_PluginType_PLUGIN_SFZ,
    Jack = crate::ffi::CarlaBackend_PluginType_PLUGIN_JACK,
    Jsfx = crate::ffi::CarlaBackend_PluginType_PLUGIN_JSFX,
    Clap = crate::ffi::CarlaBackend_PluginType_PLUGIN_CLAP,
}

impl From<crate::ffi::CarlaBackend_PluginType> for PluginType {
    fn from(v: crate::ffi::CarlaBackend_PluginType) -> Self {
        match v {
            crate::ffi::CarlaBackend_PluginType_PLUGIN_NONE => Self::None,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_INTERNAL => Self::Internal,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_LADSPA => Self::Ladspa,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_DSSI => Self::Dssi,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_LV2 => Self::Lv2,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_VST2 => Self::Vst2,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_VST3 => Self::Vst3,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_AU => Self::Au,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_DLS => Self::Dls,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_GIG => Self::Gig,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_SF2 => Self::Sf2,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_SFZ => Self::Sfz,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_JACK => Self::Jack,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_JSFX => Self::Jsfx,
            crate::ffi::CarlaBackend_PluginType_PLUGIN_CLAP => Self::Clap,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum PluginCategory {
    None = crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_NONE,
    Synth = crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_SYNTH,
    Delay = crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_DELAY,
    Eq = crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_EQ,
    Filter = crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_FILTER,
    Distortion = crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_DISTORTION,
    Dynamics = crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_DYNAMICS,
    Modulator = crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_MODULATOR,
    Utility = crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_UTILITY,
    Other = crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_OTHER,
}

impl From<crate::ffi::CarlaBackend_PluginCategory> for PluginCategory {
    fn from(v: crate::ffi::CarlaBackend_PluginCategory) -> Self {
        match v {
            crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_NONE => Self::None,
            crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_SYNTH => Self::Synth,
            crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_DELAY => Self::Delay,
            crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_EQ => Self::Eq,
            crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_FILTER => Self::Filter,
            crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_DISTORTION => Self::Distortion,
            crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_DYNAMICS => Self::Dynamics,
            crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_MODULATOR => Self::Modulator,
            crate::ffi::CarlaBackend_PluginCategory_PLUGIN_CATEGORY_UTILITY => Self::Utility,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PortCountInfo {
    pub ins: u32,
    pub outs: u32,
}

impl From<crate::ffi::CarlaPortCountInfo> for PortCountInfo {
    fn from(raw: crate::ffi::CarlaPortCountInfo) -> Self {
        Self {
            ins: raw.ins,
            outs: raw.outs,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub plugin_type: PluginType,
    pub category: PluginCategory,
    pub hints: u32,
    pub options_available: u32,
    pub options_enabled: u32,
    pub filename: String,
    pub name: String,
    pub label: String,
    pub maker: String,
    pub copyright: String,
    pub icon_name: String,
    pub unique_id: i64,
}

impl PluginInfo {
    pub(crate) unsafe fn from_raw(raw: &crate::ffi::CarlaPluginInfo) -> Self {
        let to_str = |p: *const std::os::raw::c_char| -> String {
            if p.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
            }
        };

        Self {
            plugin_type: raw.type_.into(),
            category: raw.category.into(),
            hints: raw.hints,
            options_available: raw.optionsAvailable,
            options_enabled: raw.optionsEnabled,
            filename: to_str(raw.filename),
            name: to_str(raw.name),
            label: to_str(raw.label),
            maker: to_str(raw.maker),
            copyright: to_str(raw.copyright),
            icon_name: to_str(raw.iconName),
            unique_id: raw.uniqueId,
        }
    }

    pub fn has_custom_ui(&self) -> bool {
        (self.hints & crate::ffi::CarlaBackend_PLUGIN_HAS_CUSTOM_UI) != 0
    }

    pub fn can_embed_custom_ui(&self) -> bool {
        (self.hints & crate::ffi::CarlaBackend_PLUGIN_HAS_CUSTOM_EMBED_UI) != 0
    }

    pub fn has_inline_display(&self) -> bool {
        (self.hints & crate::ffi::CarlaBackend_PLUGIN_HAS_INLINE_DISPLAY) != 0
    }

    pub fn has_resizable_custom_ui(&self) -> bool {
        (self.hints & crate::ffi::CarlaBackend_PLUGIN_HAS_CUSTOM_RESIZABLE_UI) != 0
    }
}

#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub symbol: String,
    pub unit: String,
    pub comment: String,
    pub group_name: String,
    pub scale_point_count: u32,
}

impl ParameterInfo {
    pub(crate) unsafe fn from_raw(raw: &crate::ffi::CarlaParameterInfo) -> Self {
        let to_str = |p: *const std::os::raw::c_char| -> String {
            if p.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
            }
        };

        Self {
            name: to_str(raw.name),
            symbol: to_str(raw.symbol),
            unit: to_str(raw.unit),
            comment: to_str(raw.comment),
            group_name: to_str(raw.groupName),
            scale_point_count: raw.scalePointCount,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TransportInfo {
    pub playing: bool,
    pub frame: u64,
    pub bar: i32,
    pub beat: i32,
    pub tick: i32,
    pub bpm: f64,
}

impl From<crate::ffi::CarlaTransportInfo> for TransportInfo {
    fn from(raw: crate::ffi::CarlaTransportInfo) -> Self {
        Self {
            playing: raw.playing,
            frame: raw.frame,
            bar: raw.bar,
            beat: raw.beat,
            tick: raw.tick,
            bpm: raw.bpm,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InlineDisplaySurface {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub data: Vec<u8>,
}

impl InlineDisplaySurface {
    pub(crate) unsafe fn from_raw(
        raw: &crate::ffi::CarlaInlineDisplayImageSurface,
    ) -> Option<Self> {
        if raw.data.is_null() || raw.width <= 0 || raw.height <= 0 {
            return None;
        }
        let byte_len = (raw.stride as usize) * (raw.height as usize);
        let slice = std::slice::from_raw_parts(raw.data, byte_len);
        Some(Self {
            width: raw.width as u32,
            height: raw.height as u32,
            stride: raw.stride as u32,
            data: slice.to_vec(),
        })
    }

    #[cfg(feature = "egui")]
    pub fn to_color_image(&self) -> egui::ColorImage {
        let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
        for y in 0..self.height {
            let row_start = (y * self.stride) as usize;
            for x in 0..self.width {
                let px_start = row_start + (x * 4) as usize;
                if px_start + 3 < self.data.len() {
                    let b = self.data[px_start];
                    let g = self.data[px_start + 1];
                    let r = self.data[px_start + 2];
                    let a = self.data[px_start + 3];
                    pixels.push(r);
                    pixels.push(g);
                    pixels.push(b);
                    pixels.push(a);
                } else {
                    pixels.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
        }
        egui::ColorImage::from_rgba_unmultiplied(
            [self.width as usize, self.height as usize],
            &pixels,
        )
    }
}
