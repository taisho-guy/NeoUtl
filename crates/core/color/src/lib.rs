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

pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { delta / max };
    (h, s, max)
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match (h.rem_euclid(360.0) / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r1 + m, g1 + m, b1 + m)
}

pub fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

#[cfg(test)]
mod hsv_tests {
    use super::*;

    #[test]
    fn round_trip_pure_red() {
        let (h, s, v) = rgb_to_hsv(1.0, 0.0, 0.0);
        let (r, g, b) = hsv_to_rgb(h, s, v);
        assert!((r - 1.0).abs() < 1e-5 && g.abs() < 1e-5 && b.abs() < 1e-5);
    }

    #[test]
    fn hue_distance_wraps_at_360() {
        assert!((hue_distance(350.0, 10.0) - 20.0).abs() < 1e-5);
    }

    #[test]
    fn hue_distance_zero_for_equal() {
        assert_eq!(hue_distance(120.0, 120.0), 0.0);
    }
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
