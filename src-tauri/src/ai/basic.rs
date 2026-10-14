//! The rules of release 0.3, kept as they were: who a photograph is about,
//! who is looking away, and what looking away costs it. They are what the
//! app judges by with Advanced AI turned off — the judgement people already
//! know, beside the one still being worked on.
//!
//! Nothing here is to be improved. A better rule belongs with the advanced
//! ones; this file changes only if what it stands beside does.

use super::judge::Verdict;
use super::score::{self, Parts, Reading};
use super::subject::{Decision, Reason};
use crate::faces::FaceBox;

/// What the rules are told about one face.
#[derive(Clone, Copy, Debug)]
pub struct Signals {
    pub place: FaceBox,
    /// The capture-quality number, if there is one.
    pub quality: Option<f64>,
    /// How likely the judge thinks it is a subject, 0 to 1.
    ///
    /// Stands in for something the Mac app had and this app does not: where
    /// Apple's saliency said the eye is drawn. The Mac app would not call the
    /// largest face a poster if the eye was drawn to it. Here it is not
    /// called one if the judge thinks it a subject.
    pub likely_subject: f64,
}

impl Signals {
    fn linear(&self) -> f64 {
        (self.place.w * self.place.h).sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    OnlyFace,
    LargeInFrame,
    SameScale,
    BesideASubject,
    SmallAndIsolated,
    Poster,
    RescuedLargest,
    LowQuality,
}

impl Why {
    /// The same, in the reasons the rest of the app knows.
    fn reason(self) -> Reason {
        match self {
            Why::OnlyFace => Reason::OnlyFace,
            Why::LargeInFrame | Why::SameScale | Why::BesideASubject => Reason::Subject,
            Why::RescuedLargest => Reason::Rescued,
            Why::SmallAndIsolated => Reason::Apart,
            Why::Poster => Reason::Poster,
            Why::LowQuality => Reason::Indistinct,
        }
    }
}

/// A face this indistinct is not who the photograph is about. MEASURED by
/// the Mac app against 188 hand-judged faces: at 0.15, 58% of background
/// removed for 1.3% of real subjects lost. Not to be raised above 0.20.
const MIN_QUALITY: f64 = 0.15;
/// Kept if at least this tall, as a share of the frame.
const KEEP_HEIGHT: f64 = 0.06;
/// Kept if at least this size against the largest face — LINEAR size. By
/// area, a child beside a parent is 0.42 and falls out; linearly, 0.65.
const KEEP_RELATIVE: f64 = 0.48;
/// Kept if within this many of its own widths of a face already kept.
const BESIDE: f64 = 3.0;
const POSTER_MIN_FACES: usize = 4;
const POSTER_SIZE: f64 = 2.0;
const POSTER_QUALITY: f64 = 0.6;

/// Decides, for the faces of one photograph together.
pub fn classify(faces: &[Signals]) -> Vec<Decision> {
    sort(faces).into_iter().map(|(subject, why)| Decision { subject, reason: why.reason() }).collect()
}

fn sort(faces: &[Signals]) -> Vec<(bool, Why)> {
    let keep = |why| (true, why);
    let drop = |why| (false, why);
    match faces.len() {
        0 => return Vec::new(),
        1 => return vec![keep(Why::OnlyFace)],
        _ => {}
    }

    let linear: Vec<f64> = faces.iter().map(Signals::linear).collect();
    let mut sorted = linear.clone();
    sorted.sort_by(f64::total_cmp);
    let (largest, second) = (sorted[sorted.len() - 1], sorted[sorted.len() - 2]);
    let mut qualities: Vec<f64> = faces.iter().filter_map(|f| f.quality).collect();
    qualities.sort_by(f64::total_cmp);
    let median_quality = (qualities.len() >= 2).then(|| qualities[qualities.len() / 2]);
    let poster_possible = faces.len() >= POSTER_MIN_FACES && largest > second * POSTER_SIZE;

    let mut decisions: Vec<(bool, Why)> = faces
        .iter()
        .zip(&linear)
        .map(|(face, size)| {
            // One face far larger than the rest and far less distinct is a
            // print or a sign: a banner behind a table of guests.
            if poster_possible && *size >= largest * 0.98 {
                let reads_as_print = match (face.quality, median_quality) {
                    (Some(q), Some(median)) if median > 0.0 => q < median * POSTER_QUALITY,
                    (None, Some(_)) => true,
                    _ => false,
                };
                if reads_as_print && face.likely_subject < 0.5 {
                    return drop(Why::Poster);
                }
            }
            if face.place.h >= KEEP_HEIGHT {
                keep(Why::LargeInFrame)
            } else if largest > 0.0 && size / largest >= KEEP_RELATIVE {
                keep(Why::SameScale)
            } else {
                drop(Why::SmallAndIsolated)
            }
        })
        .collect();

    // A small face right beside a kept one belongs with it: the child at a
    // parent's shoulder. One step only, from the faces kept above.
    let seeds: Vec<usize> = (0..faces.len()).filter(|i| decisions[*i].0).collect();
    for i in 0..faces.len() {
        if decisions[i].0 || decisions[i].1 == Why::Poster {
            continue;
        }
        let own = faces[i].place.w.max(0.001);
        let beside = seeds.iter().any(|s| {
            let (dx, dy) = (faces[*s].place.mid_x - faces[i].place.mid_x, faces[*s].place.mid_y - faces[i].place.mid_y);
            (dx * dx + dy * dy).sqrt() / own <= BESIDE
        });
        if beside {
            decisions[i] = keep(Why::BesideASubject);
        }
    }

    // After the step above, which was reaching past a real subject into the
    // crowd. A missing reading is not a low one, and is let through.
    for (decision, face) in decisions.iter_mut().zip(faces) {
        if decision.0 && face.quality.is_some_and(|q| q < MIN_QUALITY) {
            *decision = drop(Why::LowQuality);
        }
    }

    // A photograph with faces in it is never left with no subject.
    if !decisions.iter().any(|d| d.0) {
        let biggest = (0..faces.len()).max_by(|a, b| linear[*a].total_cmp(&linear[*b])).expect("at least two faces");
        decisions[biggest] = keep(Why::RescuedLargest);
    }
    decisions
}

/// How high the looking-away score must be.
const AWAY: f32 = 0.5;
/// How far the pupils may sit to one side, once the turn of the head is
/// allowed for, before the face is taken to be looking away. Someone whose
/// head is turned and whose eyes are on the lens has pupils well off centre
/// BECAUSE they are looking at the camera; adding a share of the head's turn
/// cancels that.
const GLANCE: f32 = 0.15;
const HEAD_SHARE: f32 = 0.3;
/// Pupils cannot be placed on a face narrower than this in the finder's
/// picture.
const GLANCE_MIN_WIDTH: f64 = 90.0;

/// Whether a face is looking somewhere other than the camera. `wide` is its
/// width in the finder's picture.
pub fn is_away(v: &Verdict, wide: f64) -> bool {
    v.away >= AWAY || (wide >= GLANCE_MIN_WIDTH && (v.eye_offset + HEAD_SHARE * v.head_yaw).abs() >= GLANCE)
}

/// What a photograph is marked on: its worst subject in each part, with
/// looking away counted in full whoever else is in the frame.
pub fn parts(faces: &[Reading], with_background: bool) -> Option<Parts> {
    let subjects: Vec<&Reading> = faces.iter().filter(|f| f.subject).collect();
    let marked: Vec<&Reading> = if with_background || subjects.is_empty() { faces.iter().collect() } else { subjects };
    if marked.is_empty() {
        return None;
    }
    let least = |get: fn(&Reading) -> Option<f64>| marked.iter().filter_map(|f| get(f)).min_by(f64::total_cmp);
    let most_away = marked.iter().filter_map(|f| f.away).max_by(f64::total_cmp);
    Some(Parts { eye: score::eye_alone(least(|f| f.eye)), focus: score::between(least(|f| f.focus_log), -1.2, 0.6, 0.75), gaze: 1.0 - score::between(most_away, 0.50, 0.98, 0.25) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(x: f64, y: f64, size: f64, quality: f64) -> Signals {
        Signals { place: FaceBox { mid_x: x, mid_y: y, w: size, h: size }, quality: Some(quality), likely_subject: 0.9 }
    }

    fn verdicts(faces: &[Signals]) -> Vec<(bool, Why)> {
        sort(faces)
    }

    #[test]
    fn one_face_is_the_subject_whatever_it_looks_like() {
        assert_eq!(verdicts(&[face(0.5, 0.5, 0.01, 0.01)]), [(true, Why::OnlyFace)]);
        assert!(classify(&[]).is_empty());
    }

    #[test]
    fn a_child_beside_a_parent_is_kept_by_linear_size() {
        // 0.65 of the parent linearly; 0.42 by area, which would have lost them.
        let got = verdicts(&[face(0.4, 0.5, 0.05, 0.5), face(0.46, 0.45, 0.0325, 0.5)]);
        assert_eq!(got[1], (true, Why::SameScale));
    }

    #[test]
    fn a_small_face_far_from_anyone_is_background_and_one_at_a_shoulder_is_not() {
        let big = face(0.3, 0.5, 0.05, 0.5);
        let far = face(0.9, 0.5, 0.01, 0.5);
        let near = face(0.325, 0.5, 0.01, 0.5);
        let got = verdicts(&[big, far, near]);
        assert_eq!(got[1], (false, Why::SmallAndIsolated));
        assert_eq!(got[2], (true, Why::BesideASubject));
    }

    #[test]
    fn an_indistinct_face_is_background_even_beside_a_subject() {
        let got = verdicts(&[face(0.3, 0.5, 0.05, 0.5), face(0.325, 0.5, 0.01, 0.05), face(0.6, 0.5, 0.08, 0.10)]);
        assert_eq!(got[1], (false, Why::LowQuality), "reached by the shoulder rule, then put back");
        assert_eq!(got[2], (false, Why::LowQuality), "large in the frame is not enough either");
    }

    #[test]
    fn a_face_with_no_quality_reading_is_not_treated_as_a_low_one() {
        let mut unread = face(0.6, 0.5, 0.08, 0.0);
        unread.quality = None;
        assert_eq!(verdicts(&[face(0.3, 0.5, 0.05, 0.5), unread])[1], (true, Why::LargeInFrame));
    }

    #[test]
    fn a_photo_is_never_left_with_no_subject() {
        let got = verdicts(&[face(0.3, 0.5, 0.05, 0.05), face(0.6, 0.5, 0.07, 0.05)]);
        assert_eq!(got, [(false, Why::LowQuality), (true, Why::RescuedLargest)]);
    }

    #[test]
    fn a_huge_indistinct_face_behind_a_group_is_a_poster_unless_the_judge_says_subject() {
        let guests = [face(0.2, 0.3, 0.07, 0.5), face(0.4, 0.3, 0.07, 0.5), face(0.6, 0.3, 0.07, 0.5)];
        let mut banner = face(0.5, 0.7, 0.2, 0.2);
        banner.likely_subject = 0.1;
        let mut faces = guests.to_vec();
        faces.push(banner);
        assert_eq!(verdicts(&faces)[3], (false, Why::Poster));
        assert!(verdicts(&faces)[..3].iter().all(|v| v.0));

        // The same picture, but the judge takes the large face for a person
        // the photo is about: a close-up in front of guests, slightly soft.
        faces[3].likely_subject = 0.9;
        assert_eq!(verdicts(&faces)[3], (true, Why::LargeInFrame));
    }

    fn verdict(away: f32, eye_offset: f32, head_yaw: f32) -> Verdict {
        Verdict { away, eye_offset, head_yaw, ..Verdict::default() }
    }

    #[test]
    fn a_sideways_glance_from_a_square_on_face_is_looking_away() {
        assert!(is_away(&verdict(0.01, 0.20, 0.0), 130.0), "pupils well to one side, head straight");
        assert!(is_away(&verdict(0.01, -0.20, 0.0), 130.0), "and to the other side");
        assert!(is_away(&verdict(0.97, 0.0, 0.0), 130.0), "the looking-away score still counts alone");
        assert!(is_away(&verdict(0.55, 0.0, 0.0), 130.0), "and from half way up");
        assert!(!is_away(&verdict(0.45, 0.0, 0.0), 130.0), "but not below it");
        assert!(!is_away(&verdict(0.01, 0.05, 0.0), 130.0), "pupils near the middle");
    }

    #[test]
    fn eyes_turned_back_to_the_lens_from_a_turned_head_is_not_looking_away() {
        assert!(!is_away(&verdict(0.01, -0.22, 0.40), 130.0));
    }

    #[test]
    fn a_small_face_is_not_accused_on_its_pupils() {
        assert!(!is_away(&verdict(0.01, 0.30, 0.0), 60.0), "too few pixels to place a pupil");
        assert!(is_away(&verdict(0.97, 0.30, 0.0), 60.0), "the looking-away score needs no pupils");
    }

    #[test]
    fn looking_away_is_counted_in_full_whoever_else_is_in_the_frame() {
        let reading = |eye: f64, focus: f64, away: f64| Reading { subject: true, eye: Some(eye), focus_log: Some(focus), away: Some(away), excused: 0.0, open: false };
        assert!((parts(&[reading(0.32, 0.7, 0.1)], false).unwrap().total() - 100.0).abs() < 1e-9);
        assert!(parts(&[reading(0.02, -2.0, 1.0)], false).unwrap().total().abs() < 1e-9, "eyes shut, out of focus and looking away is nought");
        assert!((parts(&[reading(0.32, 0.7, 0.98), reading(0.32, 0.7, 0.98)], false).unwrap().total() - 60.0).abs() < 1e-9, "a couple looking at each other loses all the gaze part");
        assert_eq!(parts(&[], false), None);
    }
}
