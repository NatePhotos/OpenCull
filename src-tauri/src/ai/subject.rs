//! Which faces a photograph is about, and which are in the background.
//!
//! A face cut out by itself cannot say this. What can is how it stands
//! among the others in its photograph: how large beside the largest, how
//! sharp beside the sharpest, where it sits in the frame, and how sure the
//! finder and the judge were of it. Each of those is worked out here as a
//! plain figure, weighed, and the weighed sum turned into how likely the
//! face is to be a background person.
//!
//! This replaced a set of size rules carried over from the Mac app.
//! MEASURED on 1,782 faces across 79 shoots, each graded by eye and each
//! shoot answered by weights fitted without it: the size rules left out 47%
//! of the background people and 0.6% of the real subjects; this leaves out
//! 60% and 0.6%.

use crate::faces::FaceBox;

/// What is told about one face.
#[derive(Clone, Copy, Debug)]
pub struct Signals {
    pub place: FaceBox,
    /// The capture-quality number the judge gives the face.
    pub quality: f64,
    /// How sure the finder was that this is a face, 0 to 1.
    pub sure: f64,
    /// How likely the judge thinks it is a background person, from the face
    /// alone, 0 to 1.
    pub background: f64,
    /// The focus measure of its sharper eye, as a logarithm. `None` when it
    /// could not be measured.
    pub focus_log: Option<f64>,
}

impl Signals {
    fn linear(&self) -> f64 {
        (self.place.w * self.place.h).sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    OnlyFace,
    Subject,
    /// Every face looked like background; the likeliest subject is kept.
    Rescued,
    Small,
    Soft,
    Aside,
    Indistinct,
}

impl Reason {
    /// What to say about a face left out. `None` for one kept.
    pub fn badge(self) -> Option<&'static str> {
        match self {
            Reason::Small => Some("Background — small beside the others"),
            Reason::Soft => Some("Background — out of focus beside the others"),
            Reason::Aside => Some("Background — at the side of the frame"),
            Reason::Indistinct => Some("Background — face too indistinct to be a subject"),
            Reason::OnlyFace | Reason::Subject | Reason::Rescued => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub subject: bool,
    pub reason: Reason,
}

/// The figures, in the order of the weights below.
const HEIGHT: usize = 0;
const BESIDE_LARGEST: usize = 1;
const OFF_CENTRE: usize = 2;
const JUDGE_BACKGROUND: usize = 3;
const QUALITY: usize = 4;
const SOFT: usize = 5;
const AT_EDGE: usize = 6;
const FINDER: usize = 7;

/// What each figure counts for, and what is added to their sum. Fitted to
/// the graded faces; not to be adjusted by hand, one at a time.
const WEIGHTS: [f64; 8] = [0.2395, -3.3664, 6.3309, 1.3964, -5.2919, 0.9833, 1.8031, -8.1128];
const BIAS: f64 = 5.3915;
/// What each figure usually is. Used only to say which of them counted
/// most against a face that is left out.
const USUAL: [f64; 8] = [-2.6016, -0.2016, 0.1441, 0.1993, 0.4269, 0.3667, 0.0247, 0.9107];
/// A face at least this likely to be a background person is left out.
/// Chosen where it loses no more real subjects than the size rules did;
/// at 0.75 it leaves out 68% of background people and 1.0% of subjects.
const BACKGROUND: f64 = 0.85;
/// A box this near the frame's edge is taken to be cut off by it.
const EDGE: f64 = 0.01;

fn figures(face: &Signals, largest: f64, sharpest: Option<f64>) -> [f64; 8] {
    let p = face.place;
    let edge = (p.mid_x - p.w / 2.0).min(1.0 - p.mid_x - p.w / 2.0).min(p.mid_y - p.h / 2.0).min(1.0 - p.mid_y - p.h / 2.0);
    let mut x = [0.0; 8];
    x[HEIGHT] = p.h.max(1e-3).ln();
    x[BESIDE_LARGEST] = (face.linear() / largest).max(1e-3).ln();
    x[OFF_CENTRE] = (p.mid_x - 0.5).abs();
    x[JUDGE_BACKGROUND] = face.background;
    x[QUALITY] = face.quality;
    // How far short of the sharpest face in the photograph. A face whose
    // eyes could not be measured is not counted soft for it.
    x[SOFT] = match (face.focus_log, sharpest) {
        (Some(own), Some(best)) => best - own,
        _ => 0.0,
    };
    x[AT_EDGE] = if edge < EDGE { 1.0 } else { 0.0 };
    x[FINDER] = face.sure;
    x
}

/// How likely each face is to be a background person, 0 to 1.
pub fn likelihoods(faces: &[Signals]) -> Vec<f64> {
    let largest = faces.iter().map(Signals::linear).fold(0.0, f64::max).max(1e-9);
    let sharpest = faces.iter().filter_map(|f| f.focus_log).max_by(f64::total_cmp);
    faces
        .iter()
        .map(|face| {
            let sum: f64 = figures(face, largest, sharpest).iter().zip(WEIGHTS).map(|(x, w)| x * w).sum();
            1.0 / (1.0 + (-(sum + BIAS)).exp())
        })
        .collect()
}

/// Which kind of thing counted most against a face that is left out.
fn why(face: &Signals, largest: f64, sharpest: Option<f64>) -> Reason {
    let x = figures(face, largest, sharpest);
    let against = |i: usize| WEIGHTS[i] * (x[i] - USUAL[i]);
    let kinds = [
        (Reason::Small, against(HEIGHT) + against(BESIDE_LARGEST)),
        (Reason::Soft, against(SOFT) + against(QUALITY)),
        (Reason::Aside, against(OFF_CENTRE) + against(AT_EDGE)),
        (Reason::Indistinct, against(FINDER) + against(JUDGE_BACKGROUND)),
    ];
    kinds.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).expect("four kinds").0
}

/// Decides, for the faces of one photograph together.
pub fn classify(faces: &[Signals]) -> Vec<Decision> {
    match faces.len() {
        0 => return Vec::new(),
        // A photograph's only face is its subject, whatever it looks like.
        1 => return vec![Decision { subject: true, reason: Reason::OnlyFace }],
        _ => {}
    }
    let largest = faces.iter().map(Signals::linear).fold(0.0, f64::max).max(1e-9);
    let sharpest = faces.iter().filter_map(|f| f.focus_log).max_by(f64::total_cmp);
    let likely = likelihoods(faces);
    let mut decisions: Vec<Decision> = faces
        .iter()
        .zip(&likely)
        .map(|(face, p)| if *p >= BACKGROUND { Decision { subject: false, reason: why(face, largest, sharpest) } } else { Decision { subject: true, reason: Reason::Subject } })
        .collect();
    // A photograph with faces in it is never left with no subject.
    if !decisions.iter().any(|d| d.subject) {
        let best = (0..faces.len()).min_by(|a, b| likely[*a].total_cmp(&likely[*b])).expect("at least two faces");
        decisions[best] = Decision { subject: true, reason: Reason::Rescued };
    }
    decisions
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A face as tall as 1.3 of its width, sharp and plain unless said otherwise.
    fn face(x: f64, y: f64, size: f64) -> Signals {
        Signals { place: FaceBox { mid_x: x, mid_y: y, w: size, h: size * 1.3 }, quality: 0.5, sure: 0.9, background: 0.1, focus_log: Some(-0.5) }
    }

    fn poor(x: f64, y: f64, size: f64, quality: f64, sure: f64, background: f64, focus_log: f64) -> Signals {
        Signals { quality, sure, background, focus_log: Some(focus_log), ..face(x, y, size) }
    }

    /// The same sums, done where the weights were fitted, gave these.
    fn close(got: &[f64], wanted: &[f64]) {
        assert_eq!(got.len(), wanted.len());
        for (g, w) in got.iter().zip(wanted) {
            assert!((g - w).abs() < 2e-4, "{got:?} against {wanted:?}");
        }
    }

    #[test]
    fn the_sums_agree_with_where_the_weights_were_fitted() {
        close(&likelihoods(&[face(0.42, 0.55, 0.10), face(0.58, 0.55, 0.095), poor(0.93, 0.60, 0.035, 0.25, 0.75, 0.7, -1.6)]), &[0.0121, 0.0142, 0.9962]);
        close(&likelihoods(&[face(0.40, 0.50, 0.05), face(0.46, 0.45, 0.0325)]), &[0.0117, 0.0302]);
        close(&likelihoods(&[face(0.45, 0.5, 0.09), poor(0.70, 0.62, 0.07, 0.2, 0.8, 0.6, -1.9)]), &[0.0098, 0.8314]);
        close(&likelihoods(&[poor(0.05, 0.5, 0.03, 0.1, 0.65, 0.9, -2.0), poor(0.95, 0.5, 0.028, 0.08, 0.62, 0.95, -2.1)]), &[0.9487, 0.9747]);
    }

    #[test]
    fn one_face_is_the_subject_whatever_it_looks_like() {
        let got = classify(&[poor(0.02, 0.5, 0.01, 0.01, 0.6, 0.99, -3.0)]);
        assert_eq!(got, [Decision { subject: true, reason: Reason::OnlyFace }]);
        assert!(classify(&[]).is_empty());
    }

    #[test]
    fn a_small_soft_face_at_the_side_is_background_and_the_pair_in_the_middle_are_not() {
        let got = classify(&[face(0.42, 0.55, 0.10), face(0.58, 0.55, 0.095), poor(0.93, 0.60, 0.035, 0.25, 0.75, 0.7, -1.6)]);
        assert!(got[0].subject && got[1].subject);
        assert!(!got[2].subject);
        assert!(got[2].reason.badge().is_some());
    }

    #[test]
    fn a_child_beside_a_parent_is_kept() {
        let got = classify(&[face(0.40, 0.50, 0.05), face(0.46, 0.45, 0.0325)]);
        assert!(got.iter().all(|d| d.subject));
    }

    #[test]
    fn a_face_whose_focus_could_not_be_measured_is_not_counted_soft() {
        let mut unread = face(0.60, 0.5, 0.08);
        unread.focus_log = None;
        let got = likelihoods(&[face(0.45, 0.5, 0.09), unread]);
        close(&got, &[0.0098, 0.0193]);
    }

    #[test]
    fn a_photo_is_never_left_with_no_subject() {
        let got = classify(&[poor(0.05, 0.5, 0.03, 0.1, 0.65, 0.9, -2.0), poor(0.95, 0.5, 0.028, 0.08, 0.62, 0.95, -2.1)]);
        assert_eq!(got[0], Decision { subject: true, reason: Reason::Rescued }, "the likelier subject of the two");
        assert!(!got[1].subject);
    }

    #[test]
    fn what_is_said_of_a_face_left_out_follows_what_counted_against_it() {
        // Small beside the others, and nothing else wrong with it.
        let small = Signals { background: 0.6, sure: 0.72, ..face(0.5, 0.8, 0.012) };
        let got = classify(&[face(0.4, 0.5, 0.12), face(0.6, 0.5, 0.12), small]);
        assert_eq!(got[2], Decision { subject: false, reason: Reason::Small });
        // As large as the others and in the middle, but far out of focus.
        let soft = poor(0.5, 0.6, 0.11, 0.05, 0.8, 0.5, -3.5);
        let got = classify(&[face(0.3, 0.5, 0.12), face(0.7, 0.5, 0.12), soft]);
        assert_eq!(got[2], Decision { subject: false, reason: Reason::Soft });
    }
}
