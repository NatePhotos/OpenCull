//! Finding faces in a photograph and judging them.
//!
//! No part of this comes from the system it runs on: the same two model
//! files run on a Mac, on Windows and on Linux. Answers should be very close
//! everywhere but are not promised identical: ONNX Runtime uses different CPU
//! instructions on each. Only the Mac has been checked.

pub mod finder;
pub mod focus;
pub mod judge;
pub mod key;
pub mod onnx;
pub mod queue;
pub mod scan;
pub mod score;
pub mod series;
pub mod store;
pub mod subject;

use std::path::Path;

use crate::faces::{self, FaceBox};
use crate::preview;

pub use judge::Verdict;
pub use subject::Reason;

/// One face in a photograph, and everything decided about it.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    /// Where it is, in the proportions the Mac app boxes a face.
    pub place: FaceBox,
    /// How sure the finder was that it is a face.
    pub sure: f32,
    pub verdict: Verdict,
    /// The focus measure of its sharper eye, as a logarithm. `None` when it
    /// could not be measured.
    pub focus_log: Option<f64>,
    /// Someone the photograph is about, as against someone in the background.
    pub subject: bool,
    pub reason: Reason,
    pub eyes_shut: bool,
    pub looking_away: bool,
    /// Whether its eyes were seen well enough for how open they are to be
    /// set beside the same person's eyes in another frame.
    pub eyes_read: bool,
}

/// A photograph's faces and its mark.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub faces: Vec<Face>,
    /// Out of 100, on its subjects. `None` when it has no faces.
    pub score: Option<f64>,
    /// What that mark is made of.
    pub parts: Option<score::Parts>,
    /// With no faces: the part of the photo it is about, which it is then
    /// marked on.
    pub key: Option<key::Key>,
}

// Faces narrower than this are not accused of anything: there are too few
// pixels to tell. The Mac app's own two gates, applied to the box as reshaped
// to its proportions (finder.rs), in pixels of the finder's picture.
const SHUT_MIN_WIDTH: f64 = 48.0;
const AWAY_MIN_WIDTH: f64 = 36.0;
/// How sure the judge must be that the eyes are shut.
///
/// MEASURED on hand-labelled faces it never learned from: at 0.9 it catches
/// 5 of 15 shut on one set and 11 of 24 on another, and accuses one open
/// eye in 130. At 0.5 it catches nine more and accuses five — and a wrong
/// "eyes shut" quietly marks down a keeper, which is the dear mistake.
const SHUT: f32 = 0.9;
/// A head turned further than this has eyes that measure wider than they
/// are — MEASURED: every false "less open than their best" found in a test
/// of 2,328 photos had a profile as its "best". Such a face is left out of
/// comparisons between frames, as is one whose eyes are covered.
const READ_MAX_TURN: f32 = 0.45;
const HIDDEN: f32 = 0.5;
/// How high the looking-away score must be.
///
/// MEASURED on 615 faces labelled by hand that the judge never learned from
/// (339 looking away, 276 at the camera): at 0.90, with the glance test
/// below, it catches 79% of those looking away and wrongly accuses 5.4% of
/// those looking at the camera; the Mac app's own test, on the 95 of them it
/// had a reading for, caught 64% and accused 8.8%. On 1,360 faces known to
/// be looking at the camera, it accuses 13.
const AWAY: f32 = 0.90;
/// How far the pupils may sit to one side, once the turn of the head is
/// allowed for, before the face is taken to be looking away. Someone whose
/// head is turned and whose eyes are on the lens has pupils well off centre
/// BECAUSE they are looking at the camera; adding a share of the head's turn
/// cancels that. The looking-away score reads mostly the head and misses a
/// sideways glance from a face held square to the camera; this catches it.
const GLANCE: f32 = 0.15;
const HEAD_SHARE: f32 = 0.3;
/// Pupils cannot be placed on a face narrower than this in the finder's
/// picture. MEASURED: on wider faces the judge's reading tracks the Mac
/// app's landmarks at r = 0.84; over all faces, 0.24.
const GLANCE_MIN_WIDTH: f64 = 90.0;
/// How sure the judge must be that a box is not a face for it to be thrown
/// out — a hand, a plate of food, the back of a head.
///
/// MEASURED on 1,420 pictures it never learned from, where a large,
/// slow finder gave the answer: of what the finder wrongly boxes, 88% is
/// thrown out, with 1.2% of real faces — nearly all of them heads seen from
/// behind. Of 192 faces labelled a subject by hand, none is thrown out.
const NOT_A_FACE: f32 = 0.8;
/// A box the finder itself is this sure of is kept whatever the judge says.
/// MEASURED: none of the junk seen on 1,236 photos reached 0.83.
const FINDER_CERTAIN: f32 = 0.88;
/// The patch around each eye that focus is measured on, as a share of the
/// face's box. About what the Mac app took around each eye's landmarks.
const EYE_PATCH: (f64, f64) = (0.28, 0.18);

/// Finds and judges the faces in an upright picture at full size.
pub fn analyze_pixels(rgb: &[u8], width: u32, height: u32) -> Result<Analysis, String> {
    // The finder is shown a smaller copy; everything after it looks at the
    // photograph itself, where the detail is.
    let k = (finder::LONG_EDGE as f64 / width.max(height) as f64).min(1.0);
    let (sw, sh) = (((width as f64 * k).round() as u32).max(1), ((height as f64 * k).round() as u32).max(1));
    let found = if k < 1.0 {
        finder::find(&faces::cut(rgb, width, height, (0, 0, width, height), sw, sh)?, sw, sh)?
    } else {
        finder::find(rgb, width, height)?
    };
    if found.is_empty() {
        return about(rgb, width, height);
    }

    let places: Vec<FaceBox> = found.iter().map(|f| f.as_face_box(sw, sh)).collect();
    let verdicts = judge::judge(rgb, width, height, &places)?;

    // What is not a face goes before anything is compared with anything: a
    // hand the size of a face must not make the faces around it look small.
    let real: Vec<usize> = (0..found.len()).filter(|&i| !is_junk(found[i].sure, &verdicts[i])).collect();
    if real.is_empty() {
        return about(rgb, width, height);
    }
    let found: Vec<finder::Found> = real.iter().map(|&i| found[i]).collect();
    let places: Vec<FaceBox> = real.iter().map(|&i| places[i]).collect();
    let verdicts: Vec<Verdict> = real.iter().map(|&i| verdicts[i]).collect();

    let up = 1.0 / k;
    let focus_logs: Vec<Option<f64>> = found
        .iter()
        .zip(&places)
        .map(|(f, place)| {
            let (pw, ph) = (place.w * width as f64 * EYE_PATCH.0, place.h * height as f64 * EYE_PATCH.1);
            f.eyes
                .iter()
                .filter_map(|(ex, ey)| {
                    let (cx, cy) = (*ex as f64 * up, *ey as f64 * up);
                    focus::ratio(rgb, width, height, (cx - pw / 2.0, cy - ph / 2.0, cx + pw / 2.0, cy + ph / 2.0))
                })
                .max_by(f64::total_cmp)
                .filter(|r| *r > 0.0)
                .map(f64::log10)
        })
        .collect();

    let signals: Vec<subject::Signals> = places
        .iter()
        .zip(&verdicts)
        .map(|(place, v)| subject::Signals { place: *place, quality: Some(v.quality as f64), likely_subject: v.validity[0] as f64 })
        .collect();
    let decisions = subject::classify(&signals);

    let faces: Vec<Face> = (0..found.len())
        .map(|i| {
            let v = verdicts[i];
            let wide = places[i].w * sw as f64;
            Face {
                place: places[i],
                sure: found[i].sure,
                verdict: v,
                focus_log: focus_logs[i],
                subject: decisions[i].subject,
                reason: decisions[i].reason,
                eyes_shut: wide >= SHUT_MIN_WIDTH && v.eyes[2] >= SHUT,
                looking_away: wide >= AWAY_MIN_WIDTH && is_away(&v, wide),
                eyes_read: wide >= SHUT_MIN_WIDTH && v.eyes[3] < HIDDEN && v.head_yaw.abs() < READ_MAX_TURN,
            }
        })
        .collect();

    let readings: Vec<score::Reading> = faces
        .iter()
        .map(|f| {
            // A face caught glancing aside is marked as looking away, even
            // where the looking-away score alone would have let it pass.
            let away = if f.looking_away { f.verdict.away.max(AWAY) } else { f.verdict.away };
            score::Reading { subject: f.subject, eye: Some(f.verdict.ear as f64), focus_log: f.focus_log, away: Some(away as f64) }
        })
        .collect();
    let parts = score::parts(&readings, false);
    Ok(Analysis { faces, score: parts.map(|p| p.total()), parts, key: None })
}

/// A photograph with nobody in it: found what it is about, and marked on
/// how sharp that is — a table, a detail, a venue is a keeper or not on
/// whether its subject is in focus.
fn about(rgb: &[u8], width: u32, height: u32) -> Result<Analysis, String> {
    let key = key::find(rgb, width, height)?;
    let score = key.focus_log.map(|log| focus::score(10f64.powf(log)));
    Ok(Analysis { faces: Vec::new(), score, parts: None, key: Some(key) })
}

/// Whether a box the finder drew is something other than a face.
fn is_junk(sure: f32, v: &Verdict) -> bool {
    sure < FINDER_CERTAIN && v.validity[2] >= NOT_A_FACE
}

/// Whether a face is looking somewhere other than the camera.
/// `wide` is the width of its box in the finder's picture, in pixels.
fn is_away(v: &Verdict, wide: f64) -> bool {
    v.away >= AWAY || (wide >= GLANCE_MIN_WIDTH && (v.eye_offset + HEAD_SHARE * v.head_yaw).abs() >= GLANCE)
}

/// Why a photo could not be analysed.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// The file could not be got at: a card pulled, a drive gone to sleep.
    /// Worth trying again next time.
    Unreadable(String),
    /// The file holds no picture that decodes. Trying again would not help.
    Unusable(String),
}

/// Finds and judges the faces in a photo file.
pub fn analyze(path: &Path) -> Result<Analysis, Failure> {
    let reachable = || std::fs::File::open(path).and_then(|f| f.metadata()).is_ok();
    if !reachable() {
        return Err(Failure::Unreadable(format!("{} could not be opened", path.display())));
    }
    let picture = preview::decode_upright(path, u32::MAX).map_err(|why| {
        // A read that failed part way looks like a file cut short. If the
        // file can no longer be opened at all, it was the disk, not the file.
        if reachable() {
            Failure::Unusable(why)
        } else {
            Failure::Unreadable(why)
        }
    })?;
    analyze_pixels(&picture.rgb, picture.width, picture.height).map_err(Failure::Unusable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_worth_trying_again_and_a_junk_one_is_not() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-failures-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(matches!(analyze(&dir.join("gone.ARW")), Err(Failure::Unreadable(_))));
        let junk = dir.join("junk.ARW");
        std::fs::write(&junk, b"not a photograph at all").unwrap();
        assert!(matches!(analyze(&junk), Err(Failure::Unusable(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_png_is_decoded_for_analysis() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-png-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shot.png");
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 64, 48);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&vec![128u8; 64 * 48 * 3]).unwrap();
        }
        let picture = preview::decode_upright(&path, u32::MAX).unwrap();
        assert_eq!((picture.width, picture.height), (64, 48));
        std::fs::remove_dir_all(&dir).ok();
    }

    fn verdict(not_a_face: f32, away: f32, eye_offset: f32, head_yaw: f32) -> Verdict {
        Verdict { validity: [1.0 - not_a_face, 0.0, not_a_face], away, eye_offset, head_yaw, ..Verdict::default() }
    }

    #[test]
    fn what_the_judge_calls_not_a_face_is_thrown_out_unless_the_finder_is_certain() {
        assert!(is_junk(0.62, &verdict(0.95, 0.0, 0.0, 0.0)), "a hand the finder half believed in");
        assert!(!is_junk(0.62, &verdict(0.4, 0.0, 0.0, 0.0)), "the judge is not sure enough");
        assert!(!is_junk(0.93, &verdict(0.95, 0.0, 0.0, 0.0)), "the finder is certain it is a face");
    }

    #[test]
    fn a_sideways_glance_from_a_square_on_face_is_looking_away() {
        assert!(is_away(&verdict(0.0, 0.01, 0.20, 0.0), 130.0), "pupils well to one side, head straight");
        assert!(is_away(&verdict(0.0, 0.01, -0.20, 0.0), 130.0), "and to the other side");
        assert!(is_away(&verdict(0.0, 0.97, 0.0, 0.0), 130.0), "the looking-away score still counts alone");
        assert!(!is_away(&verdict(0.0, 0.01, 0.05, 0.0), 130.0), "pupils near the middle");
    }

    #[test]
    fn eyes_turned_back_to_the_lens_from_a_turned_head_is_not_looking_away() {
        // The pupils are off centre one way and the head the other: the eyes
        // are making up for the head, which is someone looking at the camera.
        assert!(!is_away(&verdict(0.0, 0.01, -0.22, 0.40), 130.0));
    }

    #[test]
    fn a_small_face_is_not_accused_on_its_pupils() {
        assert!(!is_away(&verdict(0.0, 0.01, 0.30, 0.0), 60.0), "too few pixels to place a pupil");
        assert!(is_away(&verdict(0.0, 0.97, 0.30, 0.0), 60.0), "the looking-away score needs no pupils");
    }
}
