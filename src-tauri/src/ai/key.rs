//! What a photograph with nobody in it is about.
//!
//! A table laid for dinner, a tray of pastries, a cufflink, a venue: there
//! is no face to judge, but there is still a part of the frame the photo is
//! OF, and whether that part is sharp is what makes the photo a keeper. The
//! Mac app finds it with Apple's map of where the eye goes. This finds it
//! with two things that need nothing of the system it runs on:
//!
//!   - what stands out — the parts of the picture that are not like the
//!     rest of it, by the spectral-residual measure (Hou & Zhang, 2007);
//!   - what is in focus — a photographer puts the subject there.
//!
//! with a lean towards the middle of the frame. Each alone goes wrong in its
//! own way: what stands out is as often a bright sky; what is sharpest is as
//! often the pavement under someone's feet. LOOKED AT on 100 photos with no
//! faces, with each one's choice drawn on the photo: the two together landed
//! on the subject in about nine of ten, and where the others missed.

use super::focus;
use crate::faces;

/// The side of the map the subject is looked for on.
const MAP: usize = 64;
/// The side of the copy of the photo that focus is measured on.
const SEEN: usize = MAP * 8;
/// How much of the map is taken as its peak.
const PEAK: f64 = 0.03;
/// The least and the most of the frame the subject may take up, each way.
/// The Mac app's own: under the first a tight subject is blown up into
/// abstraction, over the second it is simply the photo again.
const SMALLEST: f64 = 0.28;
const LARGEST: f64 = 0.80;
const BLUR: f32 = 2.5;

/// The part of a photo it is about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    /// Left, top, width and height, as fractions of the upright photo.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// The focus measure there, as a logarithm. `None` when it could not be
    /// told: nothing there but a flat wall.
    pub focus_log: Option<f64>,
}

/// Finds what an upright picture is about, and how sharp that is.
pub fn find(rgb: &[u8], width: u32, height: u32) -> Result<Key, String> {
    // A square copy, whatever the shape of the photo: everything from here
    // on is in fractions of the frame, so the squash does not matter.
    let small = faces::cut(rgb, width, height, (0, 0, width, height), SEEN as u32, SEEN as u32)?;
    let grey: Vec<f32> = small.as_chunks::<3>().0.iter().map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).collect();

    let out = standing_out(&shrunk(&grey));
    let sharp = in_focus(&grey);
    let map: Vec<f32> = (0..MAP * MAP)
        .map(|i| {
            let (x, y) = ((i % MAP) as f32, (i / MAP) as f32);
            let from_middle = |v: f32| (v - MAP as f32 / 2.0 + 0.5) / (MAP as f32 * 0.45);
            let middle = (-(from_middle(x).powi(2) + from_middle(y).powi(2)) / 2.0).exp();
            (out[i] * sharp[i] * middle).sqrt()
        })
        .collect();

    let (x, y, edge) = hottest(&map);
    let (fw, fh) = (width as f64, height as f64);
    let focus_log = focus::ratio(rgb, width, height, (x * fw, y * fh, (x + edge) * fw, (y + edge) * fh)).filter(|r| *r > 0.0).map(f64::log10);
    Ok(Key { x, y, w: edge, h: edge, focus_log })
}

/// The square around the hottest part of a map: its left, top and side, as
/// fractions. Centred on the middle of the hottest few percent, not on the
/// single hottest cell, which would jitter between near-identical frames;
/// and as large as the heat spreads, measured over everything at least half
/// as hot as the peak.
fn hottest(map: &[f32]) -> (f64, f64, f64) {
    let mut order: Vec<usize> = (0..map.len()).collect();
    order.sort_by(|a, b| map[*b].total_cmp(&map[*a]));
    let top = &order[..((map.len() as f64 * PEAK) as usize).max(1)];
    let weight: f64 = top.iter().map(|&i| map[i].max(0.0) as f64).sum();
    if weight <= 0.0 {
        // Nothing stands out anywhere: the middle of the frame.
        return ((1.0 - SMALLEST) / 2.0, (1.0 - SMALLEST) / 2.0, SMALLEST);
    }
    let cx = top.iter().map(|&i| (i % MAP) as f64 * map[i].max(0.0) as f64).sum::<f64>() / weight;
    let cy = top.iter().map(|&i| (i / MAP) as f64 * map[i].max(0.0) as f64).sum::<f64>() / weight;

    let half = map[order[0]] * 0.5;
    let (mut vx, mut vy, mut body) = (0.0, 0.0, 0.0);
    for (i, &v) in map.iter().enumerate().filter(|(_, v)| **v >= half) {
        let (dx, dy) = ((i % MAP) as f64 - cx, (i / MAP) as f64 - cy);
        vx += v as f64 * dx * dx;
        vy += v as f64 * dy * dy;
        body += v as f64;
    }
    let spread = (vx / body).sqrt().max((vy / body).sqrt()) / MAP as f64;
    // Two standard deviations either side holds about 95% of it.
    let edge = (spread * 4.0).clamp(SMALLEST, LARGEST);
    let place = |c: f64| ((c + 0.5) / MAP as f64 - edge / 2.0).clamp(0.0, 1.0 - edge);
    (place(cx), place(cy), edge)
}

/// The picture at the map's size: each cell the average of the block it covers.
fn shrunk(grey: &[f32]) -> Vec<f32> {
    let block = SEEN / MAP;
    (0..MAP * MAP)
        .map(|i| {
            let (x0, y0) = ((i % MAP) * block, (i / MAP) * block);
            let sum: f32 = (0..block).flat_map(|dy| (0..block).map(move |dx| (dx, dy))).map(|(dx, dy)| grey[(y0 + dy) * SEEN + x0 + dx]).sum();
            sum / (block * block) as f32
        })
        .collect()
}

/// A neighbour's place, off the edge of a row: the row is mirrored there.
fn mirrored(at: isize, len: usize) -> usize {
    let last = len as isize - 1;
    let at = if at < 0 { -at } else if at > last { 2 * last - at } else { at };
    at.clamp(0, last) as usize
}

/// A map smoothed, so that a peak is a region and not a speck. Scaled so its
/// highest point is 1.
fn smoothed(map: &[f32], side: usize) -> Vec<f32> {
    let reach = (BLUR * 4.0).ceil() as isize;
    let kernel: Vec<f32> = (-reach..=reach).map(|d| (-(d * d) as f32 / (2.0 * BLUR * BLUR)).exp()).collect();
    let total: f32 = kernel.iter().sum();
    let pass = |from: &[f32], along_x: bool| -> Vec<f32> {
        (0..side * side)
            .map(|i| {
                let (x, y) = ((i % side) as isize, (i / side) as isize);
                kernel
                    .iter()
                    .zip(-reach..=reach)
                    .map(|(k, d)| {
                        let (sx, sy) = if along_x { (mirrored(x + d, side), y as usize) } else { (x as usize, mirrored(y + d, side)) };
                        k * from[sy * side + sx]
                    })
                    .sum::<f32>()
                    / total
            })
            .collect()
    };
    let out = pass(&pass(map, true), false);
    let most = out.iter().cloned().fold(0.0f32, f32::max).max(1e-9);
    out.into_iter().map(|v| v / most).collect()
}

/// Where the picture is in focus: how sharply each pixel differs from its
/// neighbours, gathered to the map's size.
fn in_focus(grey: &[f32]) -> Vec<f32> {
    let at = |x: isize, y: isize| grey[mirrored(y, SEEN) * SEEN + mirrored(x, SEEN)];
    let block = SEEN / MAP;
    let mut energy = vec![0f32; MAP * MAP];
    for y in 0..SEEN as isize {
        for x in 0..SEEN as isize {
            let edge = 2.0 * (at(x - 1, y - 1) + at(x + 1, y - 1) + at(x - 1, y + 1) + at(x + 1, y + 1)) - 8.0 * at(x, y);
            energy[(y as usize / block) * MAP + x as usize / block] += edge * edge;
        }
    }
    smoothed(&energy, MAP)
}

/// Where the picture is unlike the rest of itself: the spectral residual.
/// What is ordinary about a picture shows as a smooth spectrum; take the
/// smooth part away, turn what is left back into a picture, and what is left
/// is what was not ordinary.
fn standing_out(small: &[f32]) -> Vec<f32> {
    let mut spectrum: Vec<(f32, f32)> = small.iter().map(|v| (*v, 0.0)).collect();
    transform(&mut spectrum, false);

    let log: Vec<f32> = spectrum.iter().map(|(re, im)| (re.hypot(*im) + 1e-6).ln()).collect();
    // The smooth part: each point averaged with the eight around it.
    let residual: Vec<f32> = (0..MAP * MAP)
        .map(|i| {
            let (x, y) = ((i % MAP) as isize, (i / MAP) as isize);
            let around: f32 = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy))).map(|(dx, dy)| log[mirrored(y + dy, MAP) * MAP + mirrored(x + dx, MAP)]).sum();
            log[i] - around / 9.0
        })
        .collect();
    // The same phases, with only the residual for their size.
    for (point, r) in spectrum.iter_mut().zip(&residual) {
        let size = point.0.hypot(point.1).max(1e-12);
        let keep = r.exp() / size;
        *point = (point.0 * keep, point.1 * keep);
    }
    transform(&mut spectrum, true);
    smoothed(&spectrum.iter().map(|(re, im)| re * re + im * im).collect::<Vec<f32>>(), MAP)
}

/// The Fourier transform of a square map, in place: every row, then every
/// column.
fn transform(map: &mut [(f32, f32)], back: bool) {
    let mut line = vec![(0f32, 0f32); MAP];
    for y in 0..MAP {
        line.copy_from_slice(&map[y * MAP..(y + 1) * MAP]);
        fourier(&mut line, back);
        map[y * MAP..(y + 1) * MAP].copy_from_slice(&line);
    }
    for x in 0..MAP {
        for y in 0..MAP {
            line[y] = map[y * MAP + x];
        }
        fourier(&mut line, back);
        for y in 0..MAP {
            map[y * MAP + x] = line[y];
        }
    }
}

/// The Fourier transform of a row whose length is a power of two.
fn fourier(row: &mut [(f32, f32)], back: bool) {
    let n = row.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            row.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let turn = if back { 2.0 } else { -2.0 } * std::f32::consts::PI / len as f32;
        let (step_re, step_im) = (turn.cos(), turn.sin());
        for start in (0..n).step_by(len) {
            let (mut w_re, mut w_im) = (1f32, 0f32);
            for k in 0..len / 2 {
                let (a, b) = (row[start + k], row[start + k + len / 2]);
                let t = (b.0 * w_re - b.1 * w_im, b.0 * w_im + b.1 * w_re);
                row[start + k] = (a.0 + t.0, a.1 + t.1);
                row[start + k + len / 2] = (a.0 - t.0, a.1 - t.1);
                (w_re, w_im) = (w_re * step_re - w_im * step_im, w_re * step_im + w_im * step_re);
            }
        }
        len <<= 1;
    }
    if back {
        for v in row.iter_mut() {
            *v = (v.0 / n as f32, v.1 / n as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grey frame with one busy, sharp patch: a checkerboard.
    fn frame(width: u32, height: u32, patch: (u32, u32, u32, u32)) -> Vec<u8> {
        let mut rgb = vec![110u8; (width * height * 3) as usize];
        for y in patch.1..patch.3 {
            for x in patch.0..patch.2 {
                let v = if (x / 6 + y / 6) % 2 == 0 { 235 } else { 25 };
                rgb[((y * width + x) * 3) as usize..][..3].copy_from_slice(&[v, v, v]);
            }
        }
        rgb
    }

    #[test]
    fn the_one_sharp_busy_thing_in_a_plain_frame_is_what_it_is_about() {
        let (w, h) = (1200u32, 800u32);
        // Up and to the right of the middle.
        let key = find(&frame(w, h, (760, 180, 980, 360)), w, h).unwrap();
        let (cx, cy) = (key.x + key.w / 2.0, key.y + key.h / 2.0);
        assert!((cx - 870.0 / 1200.0).abs() < 0.08 && (cy - 270.0 / 800.0).abs() < 0.08, "centred on the patch: {cx:.2}, {cy:.2}");
        assert!(key.w < 0.45, "a small subject is not given half the frame: {}", key.w);
        assert!(key.focus_log.is_some());
    }

    #[test]
    fn it_stays_inside_the_frame_when_the_subject_is_in_a_corner() {
        let (w, h) = (1200u32, 800u32);
        let key = find(&frame(w, h, (0, 0, 200, 150)), w, h).unwrap();
        assert!(key.x >= 0.0 && key.y >= 0.0 && key.x + key.w <= 1.0 && key.y + key.h <= 1.0);
        assert!(key.x < 0.1 && key.y < 0.1, "slid into the corner, not cut off by it");
    }

    #[test]
    fn a_frame_with_nothing_in_it_gets_the_middle_and_no_sharpness() {
        let (w, h) = (900u32, 600u32);
        let key = find(&vec![128u8; (w * h * 3) as usize], w, h).unwrap();
        assert!((key.x + key.w / 2.0 - 0.5).abs() < 0.2 && (key.y + key.h / 2.0 - 0.5).abs() < 0.2);
        assert_eq!(key.focus_log, None, "a flat wall cannot be in or out of focus");
    }

    #[test]
    fn the_transform_there_and_back_is_where_it_started() {
        let mut map: Vec<(f32, f32)> = (0..MAP * MAP).map(|i| (((i * 37) % 101) as f32, 0.0)).collect();
        let was = map.clone();
        transform(&mut map, false);
        // One steady tone is one point in the spectrum, and it is the first.
        assert!(map[0].0 > 1000.0);
        transform(&mut map, true);
        assert!(map.iter().zip(&was).all(|(a, b)| (a.0 - b.0).abs() < 1e-2 && a.1.abs() < 1e-2));
    }
}
