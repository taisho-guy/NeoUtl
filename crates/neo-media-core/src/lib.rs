#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    Nv12,
    P010,
    P012,
    P016,
    Yuv444,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorPrimaries {
    Bt709,
    Bt2020,
    Smpte170m,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferCharacteristics {
    Bt709,
    Smpte2084,
    AribStdB67,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixCoefficients {
    Bt709,
    Bt2020Ncl,
    Smpte170m,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromaSiting {
    Left,
    Center,
    TopLeft,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceBackend {
    Vaapi,
    D3d11va,
    VideoToolbox,
}

pub struct NeoFrame {
    pub texture: wgpu::Texture,
    pub width: u32,
    pub height: u32,
    pub coded_size: Size,
    pub visible_rect: Rect,
    pub color_primaries: ColorPrimaries,
    pub transfer_characteristics: TransferCharacteristics,
    pub matrix_coefficients: MatrixCoefficients,
    pub full_range: bool,
    pub chroma_siting: ChromaSiting,
    pub pts: i64,
    pub duration: i64,
    pub progressive: bool,
    pub source_backend: SourceBackend,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    DeviceInit(String),
    ConfigUnsupported(String),
    DecodeFailed(String),
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferError {
    UnsupportedFormat(PixelFormat),
    PoolExhausted,
    SyncFailed(String),
    CopyFailed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoolError {
    UnsupportedFormat(PixelFormat),
    Exhausted,
}

pub trait DecodedHwFrame: Send {
    fn pixel_format(&self) -> PixelFormat;
    fn coded_size(&self) -> Size;
    fn visible_rect(&self) -> Rect;
    fn pts(&self) -> i64;
    fn duration(&self) -> i64;
    fn progressive(&self) -> bool;
}

pub trait HwDecoder: Send {
    type Frame: DecodedHwFrame;

    fn decode_next(&mut self) -> Result<Self::Frame, DecodeError>;
    fn flush(&mut self);
}

pub trait NeoFramePool: Send + Sync {
    fn acquire(&self, format: PixelFormat, width: u32, height: u32)
        -> Result<wgpu::Texture, PoolError>;
    fn release(&self, texture: wgpu::Texture);
}

pub trait TransferBackend: Send {
    type Input: DecodedHwFrame;

    fn source_backend(&self) -> SourceBackend;

    fn transfer(
        &mut self,
        input: &Self::Input,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pool: &dyn NeoFramePool,
    ) -> Result<NeoFrame, TransferError>;
}
