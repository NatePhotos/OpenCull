//! How sharp a part of a photograph is.
//!
//! The Mac app's own measure, carried over: the spread of the picture's
//! second differences over the spread of its brightness. Measured on the
//! photograph at full size — shrinking it first smooths away exactly the
//! fine detail that focus is.

/// Below this spread of brightness a patch is flat, and the measure becomes
/// two tiny numbers divided by each other: noise that reads as a firm answer.
const MIN_CONTRAST: f64 = 25.0;
/// A patch smaller than this is mostly sensor noise at high ISO, which reads
/// as perfectly sharp. It is widened to this before measuring.
const MIN_EDGE: f64 = 64.0;
/// More than this is more than a spread needs. Larger patches are cut down,
/// not shrunk, so the pixels measured are still the photograph's own.
const MAX_EDGE: f64 = 512.0;

/// The focus measure of a rectangle (left, top, right, bottom, in pixels) of
/// an upright picture. `None` when it cannot be told: too small or too flat.
pub fn ratio(rgb: &[u8], width: u32, height: u32, rect: (f64, f64, f64, f64)) -> Option<f64> {
    let (fw, fh) = (width as f64, height as f64);
    let within = |(x0, y0, x1, y1): (f64, f64, f64, f64)| (x0.max(0.0).floor(), y0.max(0.0).floor(), x1.min(fw).ceil(), y1.min(fh).ceil());
    let resized = |(x0, y0, x1, y1): (f64, f64, f64, f64), w: f64, h: f64| {
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        within((cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0))
    };

    let mut r = within(rect);
    let (mut w, mut h) = (r.2 - r.0, r.3 - r.1);
    if w <= 8.0 || h <= 8.0 {
        return None;
    }
    if w < MIN_EDGE || h < MIN_EDGE {
        r = resized(r, w.max(MIN_EDGE), h.max(MIN_EDGE));
        (w, h) = (r.2 - r.0, r.3 - r.1);
    }
    if w > MAX_EDGE || h > MAX_EDGE {
        r = resized(r, w.min(MAX_EDGE), h.min(MAX_EDGE));
        (w, h) = (r.2 - r.0, r.3 - r.1);
    }
    if w <= 8.0 || h <= 8.0 {
        return None;
    }
    let (x0, y0, w, h) = (r.0 as usize, r.1 as usize, w as usize, h as usize);

    let stride = width as usize * 3;
    let mut grey = vec![0f32; w * h];
    for y in 0..h {
        let row = &rgb[(y0 + y) * stride + x0 * 3..];
        for x in 0..w {
            let p = &row[x * 3..x * 3 + 3];
            grey[y * w + x] = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
        }
    }
    let contrast = variance(&grey);
    if contrast < MIN_CONTRAST {
        return None;
    }

    // Each pixel against its four neighbours; at the edge, the edge pixel
    // stands in for the neighbour that is not there.
    let mut second = vec![0f32; w * h];
    for y in 0..h {
        let (up, down) = (y.saturating_sub(1), (y + 1).min(h - 1));
        for x in 0..w {
            let (left, right) = (x.saturating_sub(1), (x + 1).min(w - 1));
            second[y * w + x] = grey[up * w + x] + grey[down * w + x] + grey[y * w + left] + grey[y * w + right] - 4.0 * grey[y * w + x];
        }
    }
    Some(variance(&second) / contrast)
}

fn variance(values: &[f32]) -> f64 {
    let n = values.len() as f64;
    let mean = values.iter().map(|v| *v as f64).sum::<f64>() / n;
    let mean_square = values.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / n;
    (mean_square - mean * mean).max(0.0)
}

/// The measure as a mark out of 100. The ends come from a hand-labelled set:
/// eyes out of focus sat below 0.033, eyes in focus between 0.24 and 0.55.
/// On a log scale, because the measure runs over two factors of ten.
///
/// This is the sharpness shown for each face. The photo's own mark uses the
/// same measure on a different scale (`score::parts`, -1.2 to 0.6 in log10),
/// so a face shown as 100 sharp counts for about half the focus credit there.
/// The two were not set together, but the rankings they give have been judged
/// right as they are: change one only after checking rankings on real bursts.
pub fn score(ratio: f64) -> f64 {
    const LOW: f64 = 0.008;
    const HIGH: f64 = 0.60;
    if ratio <= 0.0 {
        return 0.0;
    }
    ((ratio.log10() - LOW.log10()) / (HIGH.log10() - LOW.log10()) * 100.0).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square picture whose brightness is `f(x, y)`.
    fn picture(side: u32, f: impl Fn(u32, u32) -> u8) -> Vec<u8> {
        let mut rgb = Vec::with_capacity((side * side * 3) as usize);
        for y in 0..side {
            for x in 0..side {
                rgb.extend([f(x, y); 3]);
            }
        }
        rgb
    }

    #[test]
    fn fine_detail_measures_sharper_than_the_same_detail_smoothed() {
        let sharp = picture(200, |x, y| if (x / 2 + y / 2) % 2 == 0 { 40 } else { 210 });
        let soft = picture(200, |x, y| (125.0 + 80.0 * ((x as f64 / 12.0).sin() + (y as f64 / 12.0).cos()) / 2.0) as u8);
        let all = (0.0, 0.0, 200.0, 200.0);
        let (a, b) = (ratio(&sharp, 200, 200, all).unwrap(), ratio(&soft, 200, 200, all).unwrap());
        assert!(a > b * 20.0, "sharp {a} against soft {b}");
        assert!(score(a) > score(b));
    }

    #[test]
    fn a_flat_patch_and_a_sliver_are_not_guessed_at() {
        let flat = picture(200, |_, _| 128);
        assert_eq!(ratio(&flat, 200, 200, (0.0, 0.0, 200.0, 200.0)), None);
        let busy = picture(200, |x, y| ((x * 37 + y * 91) % 255) as u8);
        assert_eq!(ratio(&busy, 200, 200, (10.0, 10.0, 15.0, 100.0)), None, "five pixels wide");
    }

    #[test]
    fn a_small_patch_is_widened_and_one_at_the_edge_stays_inside() {
        let busy = picture(300, |x, y| ((x * 37 + y * 91) % 255) as u8);
        assert!(ratio(&busy, 300, 300, (140.0, 140.0, 160.0, 160.0)).is_some());
        assert!(ratio(&busy, 300, 300, (-40.0, -40.0, 30.0, 30.0)).is_some());
        assert!(ratio(&busy, 300, 300, (280.0, 280.0, 400.0, 400.0)).is_some());
    }

    #[test]
    fn the_mark_runs_from_nothing_to_a_hundred() {
        assert_eq!(score(0.0), 0.0);
        assert_eq!(score(0.001), 0.0);
        assert_eq!(score(5.0), 100.0);
        assert!((score(0.008f64.sqrt() * 0.6f64.sqrt()) - 50.0).abs() < 1e-9, "half way on a log scale");
    }
}
