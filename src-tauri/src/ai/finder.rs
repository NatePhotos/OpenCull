//! Finding the faces in a photograph.
//!
//! The model is YuNet (MIT), saved so that it takes a picture of any size.
//! MEASURED on 1,236 photos against the finder in the Mac app: it finds 97.6%
//! of verified faces to the Mac app's 92.6%, and 96% to 75% of those under 40
//! pixels wide, in 22 ms a photo.

use std::cell::RefCell;

use super::onnx;
use crate::faces::FaceBox;

pub(crate) static MODEL: &[u8] = include_bytes!("../../models/finder.onnx");
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
fn find_at(rgb: &[u8], width: u32, height: u32, sure_enough: f32) -> Result<Vec<Found>, String> {
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
    let candidates = onnx::run(&SESSION, MODEL, vec![("input", vec![1, 3, ph, pw], input)], &names, |out| {
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
                found.push(Found { x: cx - bw / 2.0, y: cy - bh / 2.0, w: bw, h: bh, sure, eyes: [eye(0), eye(1)] });
            }
        }
        found
    })?;
    Ok(distinct(candidates))
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

    fn face(x: f32, y: f32, size: f32, sure: f32) -> Found {
        Found { x, y, w: size, h: size, sure, eyes: [(0.0, 0.0); 2] }
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
        let f = Found { x: 954.0, y: 440.0, w: 92.0, h: 114.0, sure: 0.9, eyes: [(0.0, 0.0); 2] };
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
