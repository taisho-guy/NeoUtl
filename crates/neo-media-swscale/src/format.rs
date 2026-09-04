use neo_media_core::PixelFormat;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneDesc {
    pub width_shift: u32,
    pub height_shift: u32,
    pub storage_bits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatDesc {
    pub plane_count: usize,
    pub bit_depth: u32,
    pub storage_bits: u32,
    pub planes: [PlaneDesc; 3],
    pub is_rgb: bool,
}

pub fn describe(fmt: PixelFormat) -> FormatDesc {
    const P2: PlaneDesc = PlaneDesc {
        width_shift: 1,
        height_shift: 1,
        storage_bits: 0,
    };
    const P0: PlaneDesc = PlaneDesc {
        width_shift: 0,
        height_shift: 0,
        storage_bits: 0,
    };
    match fmt {
        PixelFormat::Nv12 => FormatDesc {
            plane_count: 2,
            bit_depth: 8,
            storage_bits: 8,
            planes: [P0, P2, P0],
            is_rgb: false,
        },
        PixelFormat::P010 => FormatDesc {
            plane_count: 2,
            bit_depth: 10,
            storage_bits: 16,
            planes: [P0, P2, P0],
            is_rgb: false,
        },
        PixelFormat::P012 => FormatDesc {
            plane_count: 2,
            bit_depth: 12,
            storage_bits: 16,
            planes: [P0, P2, P0],
            is_rgb: false,
        },
        PixelFormat::P016 => FormatDesc {
            plane_count: 2,
            bit_depth: 16,
            storage_bits: 16,
            planes: [P0, P2, P0],
            is_rgb: false,
        },
        PixelFormat::Yuv420p => FormatDesc {
            plane_count: 3,
            bit_depth: 8,
            storage_bits: 8,
            planes: [P0, P2, P2],
            is_rgb: false,
        },
        PixelFormat::Yuv444 => FormatDesc {
            plane_count: 3,
            bit_depth: 8,
            storage_bits: 8,
            planes: [P0, P0, P0],
            is_rgb: false,
        },
        PixelFormat::Rgba8 => FormatDesc {
            plane_count: 1,
            bit_depth: 8,
            storage_bits: 8,
            planes: [P0, P0, P0],
            is_rgb: true,
        },
        PixelFormat::Rgba16Float => FormatDesc {
            plane_count: 1,
            bit_depth: 16,
            storage_bits: 16,
            planes: [P0, P0, P0],
            is_rgb: true,
        },
    }
}

pub fn normalize_shift(desc: FormatDesc) -> u32 {
    desc.storage_bits - desc.bit_depth
}

pub fn normalize_max(desc: FormatDesc) -> f32 {
    ((1u32 << desc.bit_depth) - 1) as f32
}
