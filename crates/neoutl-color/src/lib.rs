use half::f16;

pub const CHANNELS: usize = 4;

pub fn u8_to_rgba16f(data: &[u8]) -> Vec<f16> {
    data.iter()
        .map(|&c| f16::from_f32(c as f32 / 255.0))
        .collect()
}

pub fn rgba16f_to_u8(data: &[f16]) -> Vec<u8> {
    data.iter()
        .map(|&c| (c.to_f32().clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_endpoints() {
        let src = [0u8, 128, 255, 255];
        let f = u8_to_rgba16f(&src);
        assert_eq!(f[0].to_f32(), 0.0);
        assert_eq!(f[2].to_f32(), 1.0);
        let back = rgba16f_to_u8(&f);
        assert_eq!(back[0], 0);
        assert_eq!(back[2], 255);
    }
}
