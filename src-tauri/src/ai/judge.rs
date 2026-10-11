//! Judging a face: what its eyes are doing, where it is looking, and whether
//! it is someone the photograph is about.
//!
//! One small network gives every answer at once. MEASURED on 356 labelled
//! faces, against the Mac app: eyes 0.936 to 0.892, gaze 0.962 to 0.880,
//! background caught 91% to 71% at the same cost. The last two are clear
//! wins; the first is within what that many faces can tell apart.

use std::cell::RefCell;

use super::onnx;
use crate::faces::{self, FaceBox, FACE_SIDE, STRIP};

static MODEL: &[u8] = include_bytes!("../../models/judge.onnx");
thread_local!(static SESSION: RefCell<Option<ort::session::Session>> = const { RefCell::new(None) });

/// What the judge says about one face.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Verdict {
    /// How likely it is a subject, a background person, not a face at all.
    pub validity: [f32; 3],
    /// Eyes open, partly open, shut, hidden.
    pub eyes: [f32; 4],
    /// At the camera, slightly off, away, down, in profile.
    pub gaze: [f32; 5],
    /// Its copy of the eye ratio the Mac app measures from landmarks.
    pub ear: f32,
    /// Its copy of the capture-quality number Apple gives a face.
    pub quality: f32,
    /// Its copy of the Mac app's looking-away score.
    pub away: f32,
    /// How far the pupils sit from the middle of the eyes, sideways, in eye
    /// widths. Its copy of what the Mac app measures from landmarks.
    pub eye_offset: f32,
    /// How far the head is turned, by the same measure. Nought is square on.
    pub head_yaw: f32,
}

/// Lays a picture out the way the model takes it: all the red, then all the
/// green, then all the blue.
fn planes(rgb: &[u8], into: &mut Vec<f32>) {
    let pixels = rgb.len() / 3;
    for channel in 0..3 {
        into.extend((0..pixels).map(|i| rgb[i * 3 + channel] as f32));
    }
}

/// How many faces go through the model at once. A crowd can put hundreds
/// of faces in one photo, and every one costs about 800 KB of input before
/// the model's own working memory — taken all at once, across several
/// workers, that is gigabytes. Results are the same either way: each face is
/// judged on its own.
const BATCH: usize = 32;

/// Judges every face in an upright picture, in the order given.
pub fn judge(rgb: &[u8], width: u32, height: u32, boxes: &[FaceBox]) -> Result<Vec<Verdict>, String> {
    if rgb.len() < width as usize * height as usize * 3 {
        return Err("the picture is smaller than its stated size".into());
    }
    let mut verdicts = Vec::with_capacity(boxes.len());
    for batch in boxes.chunks(BATCH) {
        verdicts.extend(judge_batch(rgb, width, height, batch)?);
    }
    Ok(verdicts)
}

fn judge_batch(rgb: &[u8], width: u32, height: u32, boxes: &[FaceBox]) -> Result<Vec<Verdict>, String> {
    if boxes.is_empty() {
        return Ok(Vec::new());
    }
    let n = boxes.len();
    let (side, (strip_w, strip_h)) = (FACE_SIDE as usize, (STRIP.0 as usize, STRIP.1 as usize));
    let mut face_in = Vec::with_capacity(n * 3 * side * side);
    let mut eyes_in = Vec::with_capacity(n * 3 * strip_w * strip_h);
    for b in boxes {
        // A face whose box has collapsed against the edge of the frame is
        // shown as a blank, and judged as one, rather than stopping the photo.
        let face = faces::cut(rgb, width, height, b.face_rect(width, height), FACE_SIDE, FACE_SIDE).unwrap_or_else(|_| vec![0; 3 * side * side]);
        let eyes = faces::cut(rgb, width, height, b.eyes_rect(width, height), STRIP.0, STRIP.1).unwrap_or_else(|_| vec![0; 3 * strip_w * strip_h]);
        planes(&face, &mut face_in);
        planes(&eyes, &mut eyes_in);
    }

    // How many numbers each output gives per face.
    const WIDTHS: [usize; 8] = [3, 4, 5, 1, 1, 1, 1, 1];
    let inputs = vec![("face", vec![n, 3, side, side], face_in), ("eyes", vec![n, 3, strip_h, strip_w], eyes_in)];
    onnx::run(&SESSION, MODEL, inputs, &["validity", "eyes_state", "gaze", "ear", "quality", "away", "eye_offset", "head_yaw"], |out| {
        // A model swapped for one that answers in another shape fails the
        // photo, rather than reading past the end of what it gave.
        if out.len() != WIDTHS.len() || out.iter().zip(WIDTHS).any(|((_, data), len)| data.len() < n * len) {
            return Err("the face model answered in an unexpected shape".to_string());
        }
        Ok((0..n)
            .map(|i| {
                let take = |which: usize, len: usize| &out[which].1[i * len..(i + 1) * len];
                let mut v = Verdict::default();
                v.validity.copy_from_slice(take(0, 3));
                v.eyes.copy_from_slice(take(1, 4));
                v.gaze.copy_from_slice(take(2, 5));
                v.ear = take(3, 1)[0];
                v.quality = take(4, 1)[0];
                v.away = take(5, 1)[0];
                v.eye_offset = take(6, 1)[0];
                v.head_yaw = take(7, 1)[0];
                v
            })
            .collect())
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_face_gets_a_verdict_and_each_set_of_answers_adds_to_one() {
        let (w, h) = (640u32, 480u32);
        let rgb: Vec<u8> = (0..w * h * 3).map(|i| (i % 251) as u8).collect();
        let boxes = [FaceBox { mid_x: 0.3, mid_y: 0.5, w: 0.2, h: 0.3 }, FaceBox { mid_x: 0.7, mid_y: 0.6, w: 0.1, h: 0.15 }, FaceBox { mid_x: 0.999, mid_y: 0.001, w: 0.05, h: 0.05 }];
        let verdicts = judge(&rgb, w, h, &boxes).unwrap();
        assert_eq!(verdicts.len(), 3);
        for v in verdicts {
            assert!((v.validity.iter().sum::<f32>() - 1.0).abs() < 1e-3);
            assert!((v.eyes.iter().sum::<f32>() - 1.0).abs() < 1e-3);
            assert!((v.gaze.iter().sum::<f32>() - 1.0).abs() < 1e-3);
            assert!((0.0..=0.41).contains(&v.ear) && (0.0..=1.0).contains(&v.quality) && (0.0..=1.0).contains(&v.away));
        }
    }

    #[test]
    fn a_crowd_is_judged_in_batches_with_the_same_answers() {
        let (w, h) = (640u32, 480u32);
        let rgb: Vec<u8> = (0..w * h * 3).map(|i| ((i * 13) % 251) as u8).collect();
        let crowd: Vec<FaceBox> = (0..BATCH + 5).map(|i| FaceBox { mid_x: 0.1 + 0.02 * i as f64, mid_y: 0.5, w: 0.05, h: 0.08 }).collect();
        let all = judge(&rgb, w, h, &crowd).unwrap();
        assert_eq!(all.len(), crowd.len());
        let last = judge(&rgb, w, h, &crowd[BATCH + 4..]).unwrap()[0];
        assert!((all[BATCH + 4].quality - last.quality).abs() < 1e-4, "the face in the second batch is judged as it is alone");
    }

    #[test]
    fn a_picture_shorter_than_its_size_is_refused_not_read_past() {
        assert!(judge(&[0; 12], 640, 480, &[FaceBox { mid_x: 0.5, mid_y: 0.5, w: 0.2, h: 0.2 }]).is_err());
    }

    #[test]
    fn no_faces_is_no_work() {
        assert_eq!(judge(&[0; 12], 2, 2, &[]).unwrap(), Vec::new());
    }

    #[test]
    fn a_face_is_judged_the_same_alone_as_in_company() {
        let (w, h) = (640u32, 480u32);
        let rgb: Vec<u8> = (0..w * h * 3).map(|i| ((i * 7) % 253) as u8).collect();
        let a = FaceBox { mid_x: 0.3, mid_y: 0.5, w: 0.2, h: 0.3 };
        let b = FaceBox { mid_x: 0.7, mid_y: 0.6, w: 0.1, h: 0.15 };
        let alone = judge(&rgb, w, h, &[a]).unwrap()[0];
        let together = judge(&rgb, w, h, &[b, a]).unwrap()[1];
        assert!((alone.quality - together.quality).abs() < 1e-4 && (alone.ear - together.ear).abs() < 1e-4);
    }
}
