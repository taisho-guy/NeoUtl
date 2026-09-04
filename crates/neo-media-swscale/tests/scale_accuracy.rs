use neo_media_core::{MatrixCoefficients, PixelFormat};
use neo_media_swscale::{build_plan, is_identity, FilterKind};

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let mse: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = *x as f64 - *y as f64;
            d * d
        })
        .sum::<f64>()
        / a.len() as f64;
    if mse == 0.0 {
        return f64::INFINITY;
    }
    20.0 * (255.0f64).log10() - 10.0 * mse.log10()
}

#[test]
fn identity_plan_has_no_scale_or_convert_ops() {
    let plan = build_plan(
        PixelFormat::Rgba8,
        PixelFormat::Rgba8,
        (1920, 1080),
        (1920, 1080),
        MatrixCoefficients::Bt709,
        false,
        FilterKind::Lanczos3,
    );
    assert!(is_identity(&plan));
}

#[test]
fn downscale_plan_includes_scale_ops() {
    let plan = build_plan(
        PixelFormat::P010,
        PixelFormat::Rgba8,
        (3840, 2160),
        (1920, 1080),
        MatrixCoefficients::Bt2020Ncl,
        false,
        FilterKind::Lanczos3,
    );
    assert!(!is_identity(&plan));
    assert_eq!(plan.ops.len(), 4);
}

#[test]
fn psnr_threshold_placeholder() {
    const MIN_PSNR_DB: f64 = 40.0;
    let identical = vec![128u8; 64];
    assert!(psnr(&identical, &identical).is_infinite());
    assert!(MIN_PSNR_DB > 0.0);
}
