//! Image comparison for golden tests.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DiffResult {
    pub max_abs: u8,
    pub mean_abs: f32,
    pub pct_pixels_over_tol: f32,
    pub passed: bool,
    #[serde(skip)]
    pub diff_png: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
#[error("image size mismatch or bad buffer")]
pub struct DiffError;

/// Compare two RGBA buffers. `tol` is the allowed fraction (0..1) of pixels that
/// may differ by any amount before the test is considered failed.
pub fn diff_images(w: u32, h: u32, a: &[u8], b: &[u8], tol: f32) -> Result<DiffResult, DiffError> {
    if a.len() != b.len() || a.len() != (w * h * 4) as usize {
        return Err(DiffError);
    }
    let mut max_abs = 0u8;
    let mut sum: u64 = 0;
    let mut differing = 0u64;
    let mut diff_img = image::RgbaImage::new(w, h);
    for i in 0..(w * h) as usize {
        let mut pix_diff = 0u8;
        for c in 0..4 {
            let d = a[i * 4 + c].abs_diff(b[i * 4 + c]);
            pix_diff = pix_diff.max(d);
            sum += d as u64;
            max_abs = max_abs.max(d);
        }
        if pix_diff > 0 {
            differing += 1;
        }
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        diff_img.put_pixel(
            x,
            y,
            image::Rgba([pix_diff, 0, if pix_diff == 0 { 40 } else { 0 }, 255]),
        );
    }
    let total = (w * h) as f32;
    let pct = differing as f32 / total;
    let mut diff_png = Vec::new();
    image::DynamicImage::ImageRgba8(diff_img)
        .write_to(
            &mut std::io::Cursor::new(&mut diff_png),
            image::ImageFormat::Png,
        )
        .map_err(|_| DiffError)?;
    Ok(DiffResult {
        max_abs,
        mean_abs: sum as f32 / (total * 4.0),
        pct_pixels_over_tol: pct,
        passed: pct <= tol,
        diff_png,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images_zero_diff() {
        let a = vec![10u8; 16 * 16 * 4];
        let r = diff_images(16, 16, &a, &a, 0.0).unwrap();
        assert_eq!(r.max_abs, 0);
        assert!(r.passed);
    }

    #[test]
    fn different_images_flag_fail() {
        let a = vec![0u8; 16 * 16 * 4];
        let b = vec![255u8; 16 * 16 * 4];
        let r = diff_images(16, 16, &a, &b, 0.01).unwrap();
        assert_eq!(r.max_abs, 255);
        assert!(!r.passed);
        assert_eq!(
            &r.diff_png[0..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
    }
}
