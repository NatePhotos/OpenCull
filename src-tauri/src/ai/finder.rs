//! Finding the faces in a photograph.
//!
//! The model is YuNet (MIT), saved so that it takes a picture of any size.
//! MEASURED on 1,236 photos against the finder in the Mac app: it finds 97.6%
//! of verified faces to the Mac app's 92.6%, and 96% to 75% of those under 40
//! pixels wide, in 22 ms a photo.

use std::cell::RefCell;

use super::onnx;
use crate::faces::FaceBox;

static MODEL: &[u8] = include_bytes!("../../models/finder.onnx");
thread_local!(static SESSION: RefCell<Option<ort::session::Session>> = const { RefCell::new(None) });

/// The longer side of the picture the finder is shown. Larger finds smaller
/// faces and takes longer; this is the size it was measured at.
pub const LONG_EDGE: u32 = 2048;
/// How sure the finder must be. Below this it starts boxing things that are
/// not faces: MEASURED, 0.19 stray boxes a photo here against 0.45 at 0.5.
const SURE: f32 = 0.6;
/// Two boxes overlapping by more than this are one face found twice.
const SAME_FACE: f32 = 0.3;
const STRIDES: [usize; 3] = [8, 16, 32];

/// A face as the finder reports it, in pixels of the picture it was given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub sure: f32,
    /// The middles of the two eyes.
    pub eyes: [(f32, f32); 2],
    /// Quarter turns anticlockwise that stand the face upright: 0 for a face
    /// the right way up, 2 for one upside down.
    pub turn: u8,
}

// How this finder's box differs from the box the judge was taught on — the
// Mac app's. MEASURED on 5,590 faces both had found: 0.92 times as wide,
// 1.14 times as tall, and its middle 7% of the height further up.
const WIDER: f64 = 1.0 / 0.92;
const SHORTER: f64 = 1.0 / 1.14;
const LOWER: f64 = 0.07;

impl Found {
    /// The same face as a box of the Mac app's proportions, in fractions of
    /// a picture `width` by `height`.
    pub fn as_face_box(&self, width: u32, height: u32) -> FaceBox {
        let (w, h) = (self.w as f64 * WIDER, self.h as f64 * SHORTER);
        let cx = self.x as f64 + self.w as f64 / 2.0;
        let cy = self.y as f64 + self.h as f64 / 2.0 + LOWER * h;
        FaceBox { mid_x: cx / width as f64, mid_y: 1.0 - cy / height as f64, w: w / width as f64, h: h / height as f64 }
    }
}

/// Finds the faces in an upright picture: three bytes a pixel, red-green-blue.
pub fn find(rgb: &[u8], width: u32, height: u32) -> Result<Vec<Found>, String> {
    find_at(rgb, width, height, SURE)
}

/// The same, keeping everything the finder is at least `sure` of. The app
/// asks for `SURE`; asking for less hands back more of what it only half
/// believes in.
pub fn find_at(rgb: &[u8], width: u32, height: u32, sure_enough: f32) -> Result<Vec<Found>, String> {
    find_with(rgb, width, height, sure_enough, true)
}

/// The same on a small cut-out, on the processor: the graphics chip prepares
/// a copy of the finder for every picture size, and cut-outs come in every size.
pub fn find_small(rgb: &[u8], width: u32, height: u32, sure_enough: f32) -> Result<Vec<Found>, String> {
    find_with(rgb, width, height, sure_enough, false)
}

fn find_with(rgb: &[u8], width: u32, height: u32, sure_enough: f32, chip: bool) -> Result<Vec<Found>, String> {
    let (w, h) = (width as usize, height as usize);
    // The model halves the picture five times over, so its sides must divide
    // by 32. The extra is left black, at the right and the bottom.
    let (pw, ph) = (w.div_ceil(32) * 32, h.div_ceil(32) * 32);
    let mut input = vec![0f32; 3 * pw * ph];
    let plane = pw * ph;
    for y in 0..h {
        let row = &rgb[y * w * 3..(y + 1) * w * 3];
        for x in 0..w {
            let at = y * pw + x;
            // It was taught on blue-green-red, 0 to 255.
            input[at] = row[x * 3 + 2] as f32;
            input[plane + at] = row[x * 3 + 1] as f32;
            input[2 * plane + at] = row[x * 3] as f32;
        }
    }

    let names = ["cls_8", "cls_16", "cls_32", "obj_8", "obj_16", "obj_32", "bbox_8", "bbox_16", "bbox_32", "kps_8", "kps_16", "kps_32"];
    let read = |out: &[(Vec<usize>, &[f32])]| {
        let mut found = Vec::new();
        for (level, stride) in STRIDES.iter().enumerate() {
            let (cls, obj, bbox, kps) = (out[level].1, out[3 + level].1, out[6 + level].1, out[9 + level].1);
            let cols = pw / stride;
            let s = *stride as f32;
            for i in 0..cls.len() {
                let sure = (cls[i].clamp(0.0, 1.0) * obj[i].clamp(0.0, 1.0)).sqrt();
                if sure < sure_enough {
                    continue;
                }
                let (row, col) = ((i / cols) as f32, (i % cols) as f32);
                let b = &bbox[i * 4..i * 4 + 4];
                let (cx, cy, bw, bh) = ((col + b[0]) * s, (row + b[1]) * s, b[2].exp() * s, b[3].exp() * s);
                let k = &kps[i * 10..i * 10 + 10];
                let eye = |n: usize| ((col + k[n * 2]) * s, (row + k[n * 2 + 1]) * s);
                found.push(Found { x: cx - bw / 2.0, y: cy - bh / 2.0, w: bw, h: bh, sure, eyes: [eye(0), eye(1)], turn: 0 });
            }
        }
        found
    };
    let inputs = vec![("input", vec![1, 3, ph, pw], input)];
    let candidates = if chip { onnx::run_finder(&SESSION, MODEL, inputs, &names, read)? } else { onnx::run(&SESSION, MODEL, inputs, &names, read)? };
    Ok(distinct(candidates))
}

/// A picture turned a quarter anticlockwise `quarters` times, and its new size.
pub fn turned(rgb: &[u8], width: u32, height: u32, quarters: u8) -> (Vec<u8>, u32, u32) {
    let (mut pic, mut w, mut h) = (rgb.to_vec(), width as usize, height as usize);
    for _ in 0..quarters % 4 {
        // (x, y) in a w x h picture goes to (y, w - 1 - x) in the h x w one.
        let mut out = vec![0u8; pic.len()];
        for y in 0..h {
            for x in 0..w {
                let (from, to) = ((y * w + x) * 3, ((w - 1 - x) * h + y) * 3);
                out[to..to + 3].copy_from_slice(&pic[from..from + 3]);
            }
        }
        pic = out;
        std::mem::swap(&mut w, &mut h);
    }
    (pic, w as u32, h as u32)
}

/// Where a point of a picture turned `quarters` times (see `turned`) was, in
/// the picture as it was, `width` by `height`.
pub fn unturned(p: (f32, f32), width: u32, height: u32, quarters: u8) -> (f32, f32) {
    // Each turn took (x, y) to (y, w - x), w the width before it; undone last first.
    let mut widths = Vec::new();
    let (mut w, mut h) = (width as f32, height as f32);
    for _ in 0..quarters % 4 {
        widths.push(w);
        std::mem::swap(&mut w, &mut h);
    }
    widths.iter().rev().fold(p, |q, &w| (w - q.1, q.0))
}

/// The finder Advanced AI uses: YOLO26n, taught on photographs at every
/// quarter turn, which also says which way each face stands. YuNet, shown
/// a small patch round each face it finds, then places the eyes.
///
/// MEASURED, against the referee (RetinaFace R34) and the hand-graded faces:
/// on 775 photos it never saw, 96.9% of faces found to YuNet's 96.0%, and
/// non-faces taken for subjects 0.20% to 0.29%; on the 1,236 August previews
/// turned a quarter or a half, 92-93% found to YuNet's 37-65%, and which way
/// up read right 99.9% of the time; eyes as close to the referee's as
/// YuNet's own. 34 ms a photo to YuNet's 55, on one processor core.
pub mod turning {
    use super::*;

    static MODEL: &[u8] = include_bytes!("../../models/faces-turning.onnx");
    thread_local!(static SESSION: RefCell<Option<ort::session::Session>> = const { RefCell::new(None) });
    /// The longer side of the picture it is shown.
    const SIDE: u32 = 640;
    /// How sure it must be. MEASURED: 0.3 keeps nearly every face at no more stray boxes than YuNet's.
    const KEEP: f32 = 0.3;
    /// Its scores put on YuNet's scale, which the rest of the app was tuned on: pairs of scores at
    /// which each is right equally often, MEASURED on the August previews.
    const SCALE: &str = "0.25:0.600,0.30:0.640,0.35:0.660,0.40:0.680,0.45:0.700,0.50:0.720,0.55:0.740,0.60:0.760,0.65:0.800,0.70:0.820,0.75:0.840,0.80:0.860,0.85:0.860,0.90:0.880,0.95:0.880,1.00:1.000";
    /// The patch YuNet is shown: twice the face, at most this many pixels across.
    /// MEASURED: eyes as close at 160 as at 320, for 7 ms a photo instead of 12.
    const PATCH: f32 = 2.0;
    const PATCH_MOST: f32 = 160.0;
    /// How sure YuNet must be of the face in its patch to take it the way YOLO says it stands.
    const PLAINLY: f32 = 0.75;
    /// Where the eyes are taken to be in the box when YuNet cannot find the face in its patch.
    const GUESSED_EYES: [(f32, f32); 2] = [(0.32, 0.40), (0.68, 0.40)];

    fn on_yunet_scale(conf: f32) -> f32 {
        let table: Vec<(f32, f32)> = SCALE.split(',').filter_map(|p| p.split_once(':')).filter_map(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))).collect();
        if conf <= table[0].0 {
            return table[0].1;
        }
        for w in table.windows(2) {
            let ((a0, b0), (a1, b1)) = (w[0], w[1]);
            if conf <= a1 {
                return b0 + (b1 - b0) * (conf - a0) / (a1 - a0).max(1e-6);
            }
        }
        table[table.len() - 1].1
    }

    /// Finds the faces in a picture, whichever way up they stand.
    pub fn find(rgb: &[u8], width: u32, height: u32) -> Result<Vec<Found>, String> {
        let k = (SIDE as f32 / width.max(height) as f32).min(1.0);
        let (w, h) = (((width as f32 * k).round() as u32).max(1), ((height as f32 * k).round() as u32).max(1));
        let small = if k < 1.0 { crate::faces::cut(rgb, width, height, (0, 0, width, height), w, h)? } else { rgb.to_vec() };
        let (w, h) = (w as usize, h as usize);
        let (pw, ph) = (w.div_ceil(32) * 32, h.div_ceil(32) * 32);
        let plane = pw * ph;
        // Red-green-blue, 0 to 1; the extra past a side that does not divide by 32 is left black.
        let mut input = vec![0f32; 3 * plane];
        for y in 0..h {
            for x in 0..w {
                let (i, at) = ((y * w + x) * 3, y * pw + x);
                input[at] = small[i] as f32 / 255.0;
                input[plane + at] = small[i + 1] as f32 / 255.0;
                input[2 * plane + at] = small[i + 2] as f32 / 255.0;
            }
        }
        let boxes = onnx::run_finder(&SESSION, MODEL, vec![("images", vec![1, 3, ph, pw], input)], &["output0"], |out| {
            let (shape, d) = (&out[0].0, out[0].1);
            let n = shape[2];
            // Four scores a box, one for each way a face may stand; the best says both how sure and which way.
            let ways = shape[1] - 4;
            let mut found = Vec::new();
            for i in 0..n {
                let (turn, conf) = (0..ways).map(|c| (c as u8, d[(4 + c) * n + i])).fold((0u8, f32::MIN), |b, x| if x.1 > b.1 { x } else { b });
                if conf < KEEP {
                    continue;
                }
                let (cx, cy, bw, bh) = (d[i] / k, d[n + i] / k, d[2 * n + i] / k, d[3 * n + i] / k);
                found.push(Found { x: cx - bw / 2.0, y: cy - bh / 2.0, w: bw, h: bh, sure: on_yunet_scale(conf), eyes: [(0.0, 0.0); 2], turn });
            }
            found
        })?;
        let mut found = distinct(boxes);
        for face in &mut found {
            match eyes(rgb, width, height, face) {
                Some((eyes, turn)) => {
                    face.eyes = eyes;
                    face.turn = turn;
                }
                None => face.eyes = GUESSED_EYES.map(|(ex, ey)| (face.x + ex * face.w, face.y + ey * face.h)),
            }
        }
        Ok(found)
    }

    /// YuNet's eyes for a face, from a patch round it turned the way YOLO says
    /// the face stands; tried every other way if YuNet is not plainly sure.
    fn eyes(rgb: &[u8], width: u32, height: u32, face: &Found) -> Option<([(f32, f32); 2], u8)> {
        let side = face.w.max(face.h) * PATCH;
        let (cx, cy) = (face.x + face.w / 2.0, face.y + face.h / 2.0);
        let (x0, y0) = ((cx - side / 2.0).max(0.0) as u32, (cy - side / 2.0).max(0.0) as u32);
        let (x1, y1) = (((cx + side / 2.0) as u32).min(width), ((cy + side / 2.0) as u32).min(height));
        if x1 <= x0 + 8 || y1 <= y0 + 8 {
            return None;
        }
        let shrink = (PATCH_MOST / (x1 - x0).max(y1 - y0) as f32).min(1.0);
        let (cw, ch) = ((((x1 - x0) as f32 * shrink).round() as u32).max(1), (((y1 - y0) as f32 * shrink).round() as u32).max(1));
        let patch = crate::faces::cut(rgb, width, height, (x0, y0, x1, y1), cw, ch).ok()?;
        let at = |q: u8| -> Option<Found> {
            let (pic, w, h) = turned(&patch, cw, ch, q);
            nearest_middle(find_small(&pic, w, h, 0.3).ok()?, w, h)
        };
        let first = face.turn % 4;
        let mut best = at(first).map(|f| (f, first));
        if !best.is_some_and(|(f, _)| f.sure >= PLAINLY) {
            for q in (0..4u8).filter(|&q| q != first) {
                if let Some(f) = at(q) {
                    if best.is_none_or(|(b, _)| f.sure > b.sure + 0.05) {
                        best = Some((f, q));
                    }
                }
            }
        }
        let (found, q) = best?;
        Some((found.eyes.map(|e| {
            let (px, py) = unturned(e, cw, ch, q);
            (x0 as f32 + px / shrink, y0 as f32 + py / shrink)
        }), q))
    }
}

/// Of the faces found in a cut-out, the one in its middle: the face the cut-out was made round.
pub fn nearest_middle(found: Vec<Found>, w: u32, h: u32) -> Option<Found> {
    let (mx, my) = (w as f32 / 2.0, h as f32 / 2.0);
    let best = found.into_iter().min_by(|a, b| {
        let d = |f: &Found| (f.x + f.w / 2.0 - mx).powi(2) + (f.y + f.h / 2.0 - my).powi(2);
        d(a).total_cmp(&d(b))
    })?;
    let (bx, by) = (best.x + best.w / 2.0, best.y + best.h / 2.0);
    ((bx - mx).abs() <= w as f32 / 4.0 && (by - my).abs() <= h as f32 / 4.0).then_some(best)
}

fn overlap(a: &Found, b: &Found) -> f32 {
    let ix = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let iy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    if ix <= 0.0 || iy <= 0.0 {
        return 0.0;
    }
    let inter = ix * iy;
    inter / (a.w * a.h + b.w * b.h - inter)
}

/// Keeps one box for each face: the surest, and nothing that overlaps it.
fn distinct(mut found: Vec<Found>) -> Vec<Found> {
    found.sort_by(|a, b| b.sure.total_cmp(&a.sure));
    let mut kept: Vec<Found> = Vec::new();
    for f in found {
        if kept.iter().all(|k| overlap(k, &f) <= SAME_FACE) {
            kept.push(f);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_turned_with_its_picture_comes_back_where_it_was() {
        // A 5 x 3 picture with one red pixel at (3, 1).
        let mut pic = vec![0u8; 5 * 3 * 3];
        pic[(5 + 3) * 3] = 255;
        for q in 0..4u8 {
            let (t, w, h) = turned(&pic, 5, 3, q);
            let at = (0..(w * h) as usize).find(|i| t[i * 3] == 255).unwrap();
            let (x, y) = ((at % w as usize) as f32 + 0.5, (at / w as usize) as f32 + 0.5);
            let back = unturned((x, y), 5, 3, q);
            assert!((back.0 - 3.5).abs() < 1e-4 && (back.1 - 1.5).abs() < 1e-4, "turn {q}: {back:?}");
        }
    }

    fn face(x: f32, y: f32, size: f32, sure: f32) -> Found {
        Found { x, y, w: size, h: size, sure, eyes: [(0.0, 0.0); 2], turn: 0 }
    }

    #[test]
    fn a_face_found_twice_is_kept_once_and_it_is_the_surer_one() {
        let kept = distinct(vec![face(100.0, 100.0, 50.0, 0.7), face(104.0, 102.0, 50.0, 0.9), face(400.0, 100.0, 50.0, 0.8)]);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].sure, 0.9);
        assert_eq!(kept[1].x, 400.0);
    }

    #[test]
    fn neighbours_who_only_touch_are_both_kept() {
        let kept = distinct(vec![face(100.0, 100.0, 50.0, 0.9), face(140.0, 100.0, 50.0, 0.8)]);
        assert_eq!(kept.len(), 2, "cheek to cheek is two people");
    }

    #[test]
    fn the_box_is_reshaped_to_the_proportions_the_judge_learned() {
        let f = Found { x: 954.0, y: 440.0, w: 92.0, h: 114.0, sure: 0.9, eyes: [(0.0, 0.0); 2], turn: 0 };
        let b = f.as_face_box(2000, 1000);
        assert!((b.w - 0.05).abs() < 1e-9, "92 px is 0.92 of a Mac-style 100");
        assert!((b.h - 0.1).abs() < 1e-9, "114 px is 1.14 of a Mac-style 100");
        assert!((b.mid_x - 0.5).abs() < 1e-9);
        // Its middle was at 497 down; the Mac-style middle sits 7 lower, at 504.
        assert!((b.mid_y - (1.0 - 0.504)).abs() < 1e-9);
    }

    #[test]
    fn a_blank_picture_has_no_faces_and_an_odd_size_is_no_trouble() {
        let (w, h) = (333u32, 201u32);
        let grey = vec![128u8; (w * h * 3) as usize];
        assert_eq!(find(&grey, w, h).unwrap(), Vec::new());
    }
}
