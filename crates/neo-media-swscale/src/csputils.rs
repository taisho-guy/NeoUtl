use neo_media_core::MatrixCoefficients;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatrixCoeffs {
    pub kr: f32,
    pub kb: f32,
}

pub fn matrix_coeffs(mc: MatrixCoefficients) -> MatrixCoeffs {
    match mc {
        MatrixCoefficients::Smpte170m => MatrixCoeffs {
            kr: 0.299,
            kb: 0.114,
        },
        MatrixCoefficients::Bt2020Ncl => MatrixCoeffs {
            kr: 0.2627,
            kb: 0.0593,
        },
        MatrixCoefficients::Bt709 | MatrixCoefficients::Unknown => MatrixCoeffs {
            kr: 0.2126,
            kb: 0.0722,
        },
    }
}

pub fn matrix_index(mc: MatrixCoefficients) -> u32 {
    match mc {
        MatrixCoefficients::Smpte170m => 0,
        MatrixCoefficients::Bt709 | MatrixCoefficients::Unknown => 1,
        MatrixCoefficients::Bt2020Ncl => 2,
    }
}

pub fn range_index(full_range: bool) -> u32 {
    if full_range {
        1
    } else {
        0
    }
}
