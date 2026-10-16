//! Experimental directional-blur evidence, independent of the face models.
//! A line blur leaves negative lobes in the directional derivative's
//! autocorrelation. Require anisotropy and agreement across several patches;
//! a low sharpness score alone is never evidence of motion blur.

const SIDE: usize = 64;
pub const VERSION: u8 = 2;
const DIRECTIONS: [(isize, isize); 8] = [(1, 0), (2, 1), (1, 1), (1, 2), (0, 1), (-1, 2), (-1, 1), (-2, 1)];
#[derive(Clone, Copy)]
struct Evidence { strength: f64, angle: f64, length: f64 }

pub fn annotate_pixels(rgb: &[u8], width: u32, height: u32, record: &mut super::store::Record) {
    let mut subjects: Vec<_> = record.faces.iter().filter(|f| f.subject).collect();
    subjects.sort_by(|a, b| (b.w * b.h).total_cmp(&(a.w * a.h)));
    let regions: Vec<_> = if subjects.is_empty() {
        record.key.as_ref().map(|k| vec![[k.x as f64, k.y as f64, k.w as f64, k.h as f64]]).unwrap_or_default()
    } else {
        subjects.iter().take(3).map(|f| [(f.x - f.w * 0.15) as f64, (f.y - f.h * 0.15) as f64, (f.w * 1.3) as f64, (f.h * 1.3) as f64]).collect()
    };
    record.motion_blur = assess(rgb, width, height, &regions);
    record.motion_version = VERSION;
}

/// Enrich an older face cache without running ONNX again.
pub fn annotate_file(path: &std::path::Path, record: &mut super::store::Record) {
    match crate::preview::decode_upright(path, u32::MAX) {
        Ok(image) => annotate_pixels(&image.rgb, image.width, image.height, record),
        Err(error) => { record.motion_version = VERSION; eprintln!("[ai] motion-blur check unavailable: {error}"); }
    }
}

fn patch(grey: &[f64]) -> Option<Evidence> {
    let mean = grey.iter().sum::<f64>() / grey.len() as f64;
    if grey.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (grey.len() as f64) < 36.0 { return None; }
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for y in 2..SIDE - 2 { for x in 2..SIDE - 2 {
        let gx = grey[y * SIDE + x + 1] - grey[y * SIDE + x - 1];
        let gy = grey[(y + 1) * SIDE + x] - grey[(y - 1) * SIDE + x];
        xx += gx * gx; yy += gy * gy; xy += gx * gy;
    }}
    let spread = ((xx - yy).powi(2) + 4.0 * xy * xy).sqrt();
    let strong = (xx + yy + spread) / 2.0;
    let weak = (xx + yy - spread) / 2.0;
    // Flat patches, one-dimensional graphics and isotropic defocus cannot
    // establish motion. Directionality alone is not enough either.
    if weak < 1.0 || strong <= 0.0 || weak / strong > 0.45 { return None; }
    let &(dx, dy) = DIRECTIONS.iter().min_by(|&&(ax, ay), &&(bx, by)| {
        let energy = |x: isize, y: isize| (xx * (x * x) as f64 + 2.0 * xy * (x * y) as f64 + yy * (y * y) as f64) / (x * x + y * y) as f64;
        energy(ax, ay).total_cmp(&energy(bx, by))
    })?;
    let gradient = |x: isize, y: isize| grey[((y + dy) as usize) * SIDE + (x + dx) as usize] - grey[((y - dy) as usize) * SIDE + (x - dx) as usize];
    let mut best = (0.0f64, 0);
    for lag in 3..=16isize {
        let (mut ab, mut aa, mut bb, mut count) = (0.0, 0.0, 0.0, 0);
        for y in 2..SIDE as isize - 2 { for x in 2..SIDE as isize - 2 {
            let (nx, ny) = (x + lag * dx, y + lag * dy);
            if nx < 2 || ny < 2 || nx >= SIDE as isize - 2 || ny >= SIDE as isize - 2 { continue; }
            let (a, b) = (gradient(x, y), gradient(nx, ny));
            ab += a * b; aa += a * a; bb += b * b; count += 1;
        }}
        if count < 512 || aa <= 0.0 || bb <= 0.0 { continue; }
        let correlation = ab / (aa * bb).sqrt();
        if correlation < best.0 { best = (correlation, lag); }
    }
    if best.0 > -0.24 { return None; }
    Some(Evidence { strength: (0.8 + (-best.0 - 0.24) * 0.5).min(0.99), angle: (dy as f64).atan2(dx as f64), length: best.1 as f64 * ((dx * dx + dy * dy) as f64).sqrt() })
}

/// Regions are upright, normalized subject boxes. `None` means insufficient
/// consistent evidence, not a guarantee that the photograph has no blur.
pub fn assess(rgb: &[u8], width: u32, height: u32, regions: &[[f64; 4]]) -> Option<f32> {
    let (w, h) = (width as usize, height as usize);
    if w < 96 || h < 96 || rgb.len() < w.checked_mul(h)?.checked_mul(3)? { return None; }
    let mut strongest: Option<f32> = None;
    for &[x, y, rw, rh] in regions.iter().take(3) {
        if ![x, y, rw, rh].iter().all(|v| v.is_finite()) || rw <= 0.0 || rh <= 0.0 { continue; }
        let (x0, y0) = ((x.clamp(0.0, 1.0) * w as f64) as usize, (y.clamp(0.0, 1.0) * h as f64) as usize);
        let (x1, y1) = (((x + rw).clamp(0.0, 1.0) * w as f64) as usize, ((y + rh).clamp(0.0, 1.0) * h as f64) as usize);
        if x1.saturating_sub(x0) < 96 || y1.saturating_sub(y0) < 96 { continue; }
        // Check longer streaks without weakening the correlation test.
        // Require agreement within each sampling scale.
        for step in [1usize, 2, 3] {
        let span = SIDE * step;
        if x1 - x0 < span || y1 - y0 < span { continue; }
        let mut evidence = Vec::new();
        let mut origins: Vec<(usize, usize)> = Vec::new();
        for (cx, cy) in [(0.5, 0.5), (0.3, 0.3), (0.7, 0.3), (0.3, 0.7), (0.7, 0.7)] {
            let left = (x0 + ((x1 - x0) as f64 * cx) as usize).saturating_sub(span / 2).clamp(x0, x1 - span);
            let top = (y0 + ((y1 - y0) as f64 * cy) as usize).saturating_sub(span / 2).clamp(y0, y1 - span);
            // Clamped footprints can otherwise count the same patch repeatedly.
            if origins.iter().any(|&(x, y)| x.abs_diff(left).max(y.abs_diff(top)) < span / 8) { continue; }
            origins.push((left, top));
            let mut grey = vec![0.0; SIDE * SIDE];
            for py in 0..SIDE { for px in 0..SIDE {
                let at = ((top + py * step) * w + left + px * step) * 3;
                grey[py * SIDE + px] = 0.299 * rgb[at] as f64 + 0.587 * rgb[at + 1] as f64 + 0.114 * rgb[at + 2] as f64;
            }}
            if let Some(found) = patch(&grey) { evidence.push(found); }
        }
        for e in &evidence {
            let agreeing: Vec<_> = evidence.iter().filter(|other| (2.0 * (other.angle - e.angle)).cos() > 0.7 && (other.length - e.length).abs() <= 3.0f64.max(e.length * 0.35)).collect();
            if agreeing.len() >= 3 {
                let score = (agreeing.iter().map(|e| e.strength).sum::<f64>() / agreeing.len() as f64) as f32;
                strongest = Some(strongest.map_or(score, |old| old.max(score)));
            }
        }
        }
    }
    strongest
}

#[cfg(test)] mod tests {
    use super::*;
    fn texture() -> Vec<u8> {
        let mut seed = 1234567u32;
        (0..256 * 256).flat_map(|_| { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; [32 + (seed % 192) as u8; 3] }).collect()
    }
    fn blur(image: &[u8], dx: isize, dy: isize) -> Vec<u8> { blur_radius(image, dx, dy, 4) }
    fn blur_radius(image: &[u8], dx: isize, dy: isize, radius: isize) -> Vec<u8> {
        let mut out = vec![0; image.len()];
        for y in 0..256isize { for x in 0..256isize {
            let value = (-radius..=radius).map(|k| { let xx = (x + k * dx).clamp(0, 255); let yy = (y + k * dy).clamp(0, 255); image[(yy * 256 + xx) as usize * 3] as u32 }).sum::<u32>() / (2 * radius + 1) as u32;
            out[(y * 256 + x) as usize * 3..][..3].fill(value as u8);
        }} out
    }
    #[test] fn detects_consistent_horizontal_vertical_and_diagonal_streaks() {
        let sharp = texture();
        for (dx, dy) in [(1, 0), (0, 1), (1, 1)] {
            assert!(assess(&blur(&sharp, dx, dy), 256, 256, &[[0.0, 0.0, 1.0, 1.0]]).is_some(), "direction {dx},{dy}");
        }
    }
    #[test] fn detects_long_streaks_beyond_the_original_patch_lag_range() {
        let sharp = texture();
        for radius in [12, 15, 20] {
            assert!(assess(&blur_radius(&sharp, 1, 0, radius), 256, 256, &[[0.0, 0.0, 1.0, 1.0]]).is_some(), "radius {radius}");
        }
    }
    #[test] fn does_not_relabel_sharp_texture_defocus_flat_patches_or_sharp_lines() {
        let sharp = texture();
        let defocus = blur(&blur(&sharp, 1, 0), 0, 1);
        let lines: Vec<_> = (0..256 * 256).flat_map(|i| [if i % 256 % 16 < 8 { 30 } else { 220 }; 3]).collect();
        for image in [sharp, defocus, vec![128; 256 * 256 * 3], lines] {
            assert_eq!(assess(&image, 256, 256, &[[0.0, 0.0, 1.0, 1.0]]), None);
        }
    }
    #[test] fn ignores_blur_outside_the_subject_and_unusable_input() {
        let sharp = texture(); let mut mixed = blur(&sharp, 1, 0);
        for y in 0..128 { mixed[y * 256 * 3..(y + 1) * 256 * 3].copy_from_slice(&sharp[y * 256 * 3..(y + 1) * 256 * 3]); }
        assert_eq!(assess(&mixed, 256, 256, &[[0.0, 0.0, 1.0, 0.45]]), None);
        assert_eq!(assess(&[], 256, 256, &[[0.0, 0.0, 1.0, 1.0]]), None);
        assert_eq!(assess(&sharp, 256, 256, &[[0.0, 0.0, 0.1, 0.1]]), None);
    }
    #[test] fn enriches_a_cached_record_without_changing_its_faces_or_original_score() {
        let mut record = super::super::store::Record { score: Some(75.0), key: Some(super::super::store::KeyRecord { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }), ..Default::default() };
        annotate_pixels(&blur(&texture(), 1, 0), 256, 256, &mut record);
        assert_eq!(record.score, Some(75.0));
        assert!(record.faces.is_empty());
        assert_eq!(record.motion_version, VERSION);
        assert!(record.motion_blur.is_some_and(|v| v >= 0.8));
        let encoded = serde_json::to_string(&record).unwrap();
        assert_eq!(serde_json::from_str::<super::super::store::Record>(&encoded).unwrap(), record);
    }
}
