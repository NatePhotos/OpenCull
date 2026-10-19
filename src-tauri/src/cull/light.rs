//! How light or dark a photo is, from a small copy of it. A thumbnail a few
//! hundred pixels across is plenty for a distribution, and is usually made
//! already, so folders analysed before auto-cull existed need no new scan.

use fast_image_resize as fr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Light {
    /// Average brightness as stored in the picture, 0 black to 1 white.
    pub mean: f32,
    /// Average amount of light, 0 black to 1 white: each value decoded from
    /// the picture's brightness curve (a power of 2.2) before averaging, so
    /// half the light reads half as much. Exposure differences are measured on this.
    pub linear: f32,
    /// The brightness 5% and 95% of the way up the picture's tones.
    pub p05: f32,
    pub p95: f32,
    /// Share of the picture crushed to black, and blown to white.
    pub shadows: f32,
    pub highlights: f32,
    /// How much fine structure there is: the average brightness step between
    /// neighbouring pixels, 0 for a flat picture to 1 for a very busy one.
    pub detail: f32,
}

impl Light {
    /// What to assume of a photo whose brightness could not be read.
    pub fn neutral() -> Light {
        Light { mean: 0.5, linear: 0.217_637_64, p05: 0.05, p95: 0.95, shadows: 0.0, highlights: 0.0, detail: 0.5 }
    }
}

/// For when the picture's width is not known: assumes it is square. Only a
/// fallback for tests; real pictures go through `light_of_sized`.
pub fn light_of(rgb: &[u8]) -> Light {
    let pixels = rgb.len() / 3;
    let side = (pixels as f64).sqrt() as usize;
    light_of_sized(rgb, side.max(1), (pixels / side.max(1)).max(1))
}

/// The long side of the thumbnail brightness is read from (the smallest of `serve::THUMB_SIZES`).
pub const THUMB_EDGE: u32 = 320;

/// Brightness of a larger picture, read as if from its thumbnail: first
/// shrunk by averaging to the size the thumbnail would be, so the readings,
/// detail most of all, come out as the thumbnail's would. For the picture the
/// face scan has already decoded, so no photo is decoded twice. A picture no
/// larger than a thumbnail is read as it is.
pub fn light_of_picture(rgb: &[u8], w: u32, h: u32) -> Light {
    let long = w.max(h);
    if long <= THUMB_EDGE {
        return light_of_sized(rgb, w as usize, h as usize);
    }
    // Sized as the thumbnail is (`preview::shrink_to`).
    let k = THUMB_EDGE as f64 / long as f64;
    let (tw, th) = (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1));
    let shrunk = fr::images::ImageRef::new(w, h, rgb, fr::PixelType::U8x3).ok().and_then(|src| {
        let mut dst = fr::images::Image::new(tw, th, fr::PixelType::U8x3);
        fr::Resizer::new().resize(&src, &mut dst, &fr::ResizeOptions::new().resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Box))).ok()?;
        Some(dst.into_vec())
    });
    match shrunk {
        Some(small) => light_of_sized(&small, tw as usize, th as usize),
        None => light_of_sized(rgb, w as usize, h as usize),
    }
}

pub fn light_of_sized(rgb: &[u8], w: usize, h: usize) -> Light {
    let mut counts = [0u32; 256];
    let mut total = 0u32;
    let mut sum = 0f64;
    let mut lumas = Vec::with_capacity(rgb.len() / 3);
    for px in rgb.chunks_exact(3) {
        let luma = (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32).round().min(255.0) as usize;
        lumas.push(luma as u8);
        counts[luma] += 1;
        total += 1;
        sum += luma as f64;
    }
    if total == 0 {
        return Light::default();
    }
    let at = |share: f32| {
        let wanted = (share * total as f32).ceil().max(1.0) as u32;
        let mut seen = 0;
        for (value, n) in counts.iter().enumerate() {
            seen += n;
            if seen >= wanted { return value as f32 / 255.0; }
        }
        1.0
    };
    let share = |range: std::ops::RangeInclusive<usize>| counts[range].iter().sum::<u32>() as f32 / total as f32;
    // From the counts, so it costs 256 steps rather than one per pixel.
    let light: f64 = counts.iter().enumerate().map(|(v, &n)| n as f64 * (v as f64 / 255.0).powf(2.2)).sum();
    Light { mean: (sum / total as f64 / 255.0) as f32, linear: (light / total as f64) as f32, p05: at(0.05), p95: at(0.95), shadows: share(0..=8), highlights: share(247..=255), detail: detail_of(&lumas, w, h) }
}

/// Mean absolute brightness difference between horizontally and vertically
/// adjacent pixels, over 64, kept within 0 to 1. `w` and `h` are the picture's width and height.
fn detail_of(lumas: &[u8], w: usize, h: usize) -> f32 {
    if w == 0 { return 0.0; }
    let rows = h.min(lumas.len() / w);
    let (mut total, mut steps) = (0u64, 0u64);
    for y in 0..rows {
        let row = &lumas[y * w..(y + 1) * w];
        for x in 0..w {
            if x + 1 < w {
                total += row[x].abs_diff(row[x + 1]) as u64;
                steps += 1;
            }
            if y + 1 < rows {
                total += row[x].abs_diff(lumas[(y + 1) * w + x]) as u64;
                steps += 1;
            }
        }
    }
    if steps == 0 { 0.0 } else { (total as f32 / steps as f32 / 64.0).clamp(0.0, 1.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(value: u8, pixels: usize) -> Vec<u8> { vec![value; pixels * 3] }

    #[test]
    fn a_black_frame_is_dark_and_clipped_in_the_shadows() {
        let l = light_of(&flat(0, 1000));
        assert!(l.mean < 0.01 && l.p95 < 0.01);
        assert!((l.shadows - 1.0).abs() < 1e-6);
        assert_eq!(l.highlights, 0.0);
    }

    #[test]
    fn a_white_frame_is_clipped_in_the_highlights() {
        let l = light_of(&flat(255, 1000));
        assert!(l.mean > 0.99);
        assert!((l.highlights - 1.0).abs() < 1e-6);
    }

    #[test]
    fn percentiles_follow_the_spread() {
        let mut rgb = flat(20, 900);
        rgb.extend(flat(230, 100));
        let l = light_of(&rgb);
        assert!(l.p05 < 0.1);
        assert!(l.p95 > 0.85);
    }

    #[test]
    fn a_flat_picture_has_little_detail_and_a_busy_one_a_lot() {
        let flat = vec![128u8; 64 * 64 * 3];
        let mut busy = Vec::with_capacity(64 * 64 * 3);
        for y in 0..64 { for x in 0..64 { let v = if (x + y) % 2 == 0 { 30 } else { 220 }; busy.extend([v, v, v]); } }
        assert!(light_of_sized(&flat, 64, 64).detail < 0.05);
        assert!(light_of_sized(&busy, 64, 64).detail > 0.5);
    }

    #[test]
    fn the_linear_mean_averages_the_light_itself_not_the_encoded_values() {
        // Half black, half white: half the light, though the encoded mean is the same as flat mid-grey's.
        let mut rgb = flat(0, 500);
        rgb.extend(flat(255, 500));
        assert!((light_of(&rgb).linear - 0.5).abs() < 0.01);
        // Mid-grey carries about a fifth of white's light.
        let grey = light_of(&flat(128, 1000)).linear;
        assert!((grey - (128.0f32 / 255.0).powf(2.2)).abs() < 0.01, "{grey}");
        assert!((Light::neutral().linear - 0.5f32.powf(2.2)).abs() < 0.01);
    }

    #[test]
    fn a_large_picture_is_read_at_thumbnail_size() {
        // Single-pixel stripes too fine to survive shrinking to 320 px: read
        // at full size they are all detail; at thumbnail size, flat grey.
        let (w, h) = (1280usize, 640usize);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for _ in 0..h { for x in 0..w { let v = if x % 2 == 0 { 40 } else { 200 }; rgb.extend([v, v, v]); } }
        assert!(light_of_sized(&rgb, w, h).detail > 0.5);
        let l = light_of_picture(&rgb, w as u32, h as u32);
        assert!(l.detail < 0.05, "{}", l.detail);
        assert!((l.mean - 120.0 / 255.0).abs() < 0.01, "{}", l.mean);
        // A flat picture reads the same either way.
        let flat = vec![90u8; w * h * 3];
        assert!((light_of_picture(&flat, w as u32, h as u32).mean - light_of_sized(&flat, w, h).mean).abs() < 1e-6);
    }

    #[test]
    fn a_picture_no_larger_than_a_thumbnail_is_read_as_it_is() {
        let mut rgb = Vec::new();
        for y in 0..200 { for x in 0..300 { let v = ((x * 7 + y * 3) % 256) as u8; rgb.extend([v, v, v]); } }
        assert_eq!(light_of_picture(&rgb, 300, 200), light_of_sized(&rgb, 300, 200));
    }

    #[test]
    fn nothing_to_read_is_neutral() {
        assert_eq!(light_of(&[]), Light::default());
    }
}
