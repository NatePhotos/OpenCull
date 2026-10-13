//! A closer look at the eyes: whether they are on the camera, how far the
//! lids are open, and whether they are cast down.
//!
//! The judge is shown one band across a face and has to find the eyes in it
//! before it can say where the pupils sit; what it ends up reading is mostly
//! the turn of the head, and a head held square with the eyes turned aside
//! gets past it. This network is shown only the two eyes, each cut from the
//! full-size photograph and centred on the finder's own eye point, and is
//! told how far the judge finds the head turned — eyes swung back to the
//! lens from a turned head sit to one side of their sockets and are looking
//! at the camera all the same.
//!
//! One network gives all three answers from one look. It was taught to give
//! what six gave between them — three for where the eyes point, three for
//! the lids — and keeps to them closely for a sixth of the work.

use std::cell::RefCell;

use super::onnx;
use crate::faces;

static MODEL: &[u8] = include_bytes!("../../models/eyes.onnx");
thread_local!(static SESSION: RefCell<Option<ort::session::Session>> = const { RefCell::new(None) });

/// One eye's picture. The network takes two side by side: the eye on the
/// left of the photograph, then the other.
pub const EYE: (u32, u32) = (96, 64);
/// How wide a stretch of the photograph one eye's picture takes in, as a
/// share of the distance between the eyes. An eye is about half that
/// distance wide.
const WIDE: f64 = 0.8;
/// On a turned head the eyes draw together in the picture; the stretch is
/// then taken from the finder's box instead, as this share of its width.
const LEAST: f64 = 0.35;

/// What is needed to look at one face's eyes.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    /// The middles of the two eyes, in pixels of the photograph.
    pub eyes: [(f64, f64); 2],
    /// The width of the finder's box, in pixels of the photograph.
    pub box_wide: f64,
    /// How far the judge finds the head turned.
    pub head_yaw: f32,
}

/// The two eyes of a face, side by side in one picture `2 * EYE.0` wide.
/// `None` where an eye is too near the edge of the frame to be centred.
pub fn pair(rgb: &[u8], width: u32, height: u32, look: &Look) -> Option<Vec<u8>> {
    let mut eyes = look.eyes;
    eyes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let apart = ((eyes[1].0 - eyes[0].0).powi(2) + (eyes[1].1 - eyes[0].1).powi(2)).sqrt();
    let wide = WIDE * apart.max(LEAST * look.box_wide);
    let tall = wide * EYE.1 as f64 / EYE.0 as f64;
    let (one, row) = (EYE.0 as usize * 3, EYE.0 as usize * 2 * 3);
    let mut out = vec![0u8; row * EYE.1 as usize];
    for (side, (cx, cy)) in eyes.iter().enumerate() {
        let (x0, y0, x1, y1) = (cx - wide / 2.0, cy - tall / 2.0, cx + wide / 2.0, cy + tall / 2.0);
        if x0 < 0.0 || y0 < 0.0 || x1 > width as f64 || y1 > height as f64 || x1 - x0 < 2.0 || y1 - y0 < 2.0 {
            return None;
        }
        let cut = faces::cut(rgb, width, height, (x0 as u32, y0 as u32, x1 as u32, y1 as u32), EYE.0, EYE.1).ok()?;
        for line in 0..EYE.1 as usize {
            out[line * row + side * one..line * row + side * one + one].copy_from_slice(&cut[line * one..line * one + one]);
        }
    }
    Some(out)
}

/// What the look at one face's eyes makes of them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    /// How likely the eyes are to be off the camera, 0 to 1.
    pub away: f32,
    /// How far the lids are open: fully, mostly, partly, barely, closed,
    /// cannot be seen.
    pub open: [f32; 6],
    /// How likely the eyes are cast down.
    pub down: f32,
}

/// Looks at each face's eyes, in the order given. `None` for a face whose
/// eyes could not be cut out.
pub fn read(rgb: &[u8], width: u32, height: u32, looks: &[Look]) -> Result<Vec<Option<Seen>>, String> {
    read_of(&pairs(rgb, width, height, looks), looks)
}

/// Every face's eye pair, cut once: more than one network looks at them.
pub fn pairs(rgb: &[u8], width: u32, height: u32, looks: &[Look]) -> Vec<Option<Vec<u8>>> {
    looks.iter().map(|look| pair(rgb, width, height, look)).collect()
}

/// Lays eye pairs out the way a network takes them — all the red, then all
/// the green, then all the blue — with the turn of each head beside it.
/// Faces without a pair are left out.
pub fn laid_out(pairs: &[Option<Vec<u8>>], looks: &[Look]) -> (usize, Vec<f32>, Vec<f32>) {
    let (w, h) = (EYE.0 as usize * 2, EYE.1 as usize);
    let n = pairs.iter().flatten().count();
    let mut pair_in = Vec::with_capacity(n * 3 * w * h);
    let mut turn_in = Vec::with_capacity(n);
    for (picture, look) in pairs.iter().zip(looks) {
        if let Some(picture) = picture {
            for channel in 0..3 {
                pair_in.extend((0..w * h).map(|i| picture[i * 3 + channel] as f32));
            }
            turn_in.push(look.head_yaw);
        }
    }
    (n, pair_in, turn_in)
}

/// As `read`, on pairs already cut.
pub fn read_of(pairs: &[Option<Vec<u8>>], looks: &[Look]) -> Result<Vec<Option<Seen>>, String> {
    let (n, pair_in, turn_in) = laid_out(pairs, looks);
    if n == 0 {
        return Ok(vec![None; looks.len()]);
    }
    let (w, h) = (EYE.0 as usize * 2, EYE.1 as usize);
    let inputs = vec![("pair", vec![n, 3, h, w], pair_in), ("turn", vec![n, 1], turn_in)];
    let said: Vec<Seen> = onnx::run(&SESSION, MODEL, inputs, &["away", "open", "down"], |out| {
        (0..n)
            .map(|i| {
                let mut open = [0.0; 6];
                open.copy_from_slice(&out[1].1[i * 6..(i + 1) * 6]);
                Seen { away: out[0].1[i], open, down: out[2].1[i] }
            })
            .collect()
    })?;
    let mut said = said.into_iter();
    Ok(pairs.iter().map(|p| p.as_ref().and_then(|_| said.next())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: u32, h: u32) -> Vec<u8> {
        (0..w * h * 3).map(|i| ((i * 7) % 253) as u8).collect()
    }

    #[test]
    fn each_eye_is_centred_in_its_own_half_whichever_is_named_first() {
        // A white square at one eye and a black one at the other, on grey.
        let (w, h) = (400u32, 300u32);
        let mut rgb = vec![128u8; (w * h * 3) as usize];
        let mut paint = |cx: u32, cy: u32, value: u8| {
            for y in cy - 4..cy + 4 {
                for x in cx - 4..cx + 4 {
                    let at = ((y * w + x) * 3) as usize;
                    rgb[at..at + 3].fill(value);
                }
            }
        };
        paint(150, 150, 255);
        paint(250, 150, 0);
        let look = Look { eyes: [(250.0, 150.0), (150.0, 150.0)], box_wide: 200.0, head_yaw: 0.0 };
        let got = pair(&rgb, w, h, &look).unwrap();
        let at = |x: usize, y: usize| got[(y * EYE.0 as usize * 2 + x) * 3];
        assert!(at(48, 32) > 200, "the eye on the left of the photo, in the middle of the left half");
        assert!(at(96 + 48, 32) < 50, "the other, in the middle of the right half");
        assert!((100..=156).contains(&at(4, 4)), "and plain skin at the corner");
    }

    #[test]
    fn an_eye_at_the_edge_of_the_frame_is_not_looked_at() {
        let rgb = picture(400, 300);
        let look = Look { eyes: [(5.0, 150.0), (105.0, 150.0)], box_wide: 200.0, head_yaw: 0.0 };
        assert!(pair(&rgb, 400, 300, &look).is_none());
        assert_eq!(read(&rgb, 400, 300, &[look]).unwrap(), vec![None]);
    }

    #[test]
    fn every_face_gets_its_own_answer_and_the_same_one_alone_as_in_company() {
        let rgb = picture(640, 480);
        let a = Look { eyes: [(200.0, 200.0), (280.0, 204.0)], box_wide: 180.0, head_yaw: 0.1 };
        let b = Look { eyes: [(420.0, 300.0), (470.0, 298.0)], box_wide: 110.0, head_yaw: -0.4 };
        let edge = Look { eyes: [(2.0, 2.0), (60.0, 2.0)], box_wide: 110.0, head_yaw: 0.0 };
        let together = read(&rgb, 640, 480, &[a, edge, b]).unwrap();
        assert_eq!(together.len(), 3);
        assert!(together[1].is_none());
        let alone = read(&rgb, 640, 480, &[b]).unwrap()[0].unwrap();
        assert!((together[2].unwrap().away - alone.away).abs() < 1e-4 && (together[2].unwrap().down - alone.down).abs() < 1e-4);
        assert!(together.iter().flatten().all(|v| (0.0..=1.0).contains(&v.away) && (v.open.iter().sum::<f32>() - 1.0).abs() < 1e-3));
    }
}
