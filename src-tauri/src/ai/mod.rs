//! Finding faces in a photograph and judging them.
//!
//! No part of this comes from the system it runs on: the same two model
//! files give the same answers on a Mac, on Windows and on Linux.

pub mod eyes;
pub mod finder;
pub mod focus;
pub mod judge;
pub mod key;
pub mod onnx;
pub mod queue;
pub mod score;
pub mod series;
pub mod state;
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
    /// What the closer look at the eyes makes of them: how likely they are
    /// to be off the camera. `None` for a face too small to be accused, or
    /// with an eye at the edge of the frame.
    pub eyes_away: Option<f32>,
    /// The state it is in — eyes, expression, where it is turned — and how
    /// likely it is to be a blink. `None` for a face too small to say, or
    /// with an eye at the edge of the frame.
    pub state: Option<state::State>,
    pub blink: Option<f32>,
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

// Faces narrower than this, in the finder's picture, are not accused of
// anything: there are too few pixels to tell. The Mac app's own two gates.
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
/// The score is the judge's own answer to "is this person looking somewhere
/// other than the camera".
/// MEASURED on 185 faces graded by hand, from photographs it never learned
/// from (112 looking away, 73 at the camera), each counted for as many faces
/// as its kind is common: at 0.5, with the glance test below, it catches 62% of
/// those looking away and wrongly accuses 1.3% of those looking at the
/// camera. The score it replaced, cut at 0.90, caught 53% and accused none.
/// On 658 faces in posed groups, held back and all but a handful of them
/// looking at the camera, it accuses 14 where the old one accused 10.
const AWAY: f32 = 0.5;
/// What a face found to be looking away counts as when the photo is marked:
/// well up the scale, however narrowly it was found so.
const AWAY_MARK: f32 = 0.90;
/// How sure the closer look at the eyes must be that they are off the
/// camera for that alone to call a face looking away.
///
/// MEASURED with the whole app, before and after it was added, on faces
/// nothing here learned from: of 101 looking away in 25 shoots held out
/// whole, 83% caught where 74% were; on 185 graded by hand, each counted for
/// as many as its kind is common, 75% where 60% were.
const EYES_AWAY: f32 = 0.7;
/// The looking-away score reads mostly the turn of the head, and by itself
/// it accuses people who are looking at the camera. Where the eyes could be
/// looked at, it is believed only if that look goes at least this far along
/// with it.
///
/// MEASURED against the score believed outright: on the 185 graded by hand,
/// 2 of 73 looking at the camera accused where 5 were, for 70% of those
/// looking away caught where 75% were; in the shoots held out whole, 7 of
/// 190 accused where 9 were and 81% caught where 83% were; on 646 faces in
/// posed groups, 4 accused where 5 were. And of five faces a photographer
/// picked out of 80 flagged photos as plainly looking at the camera, four
/// were accused by the score alone and are let go by this.
const EYES_AGREE: f32 = 0.2;
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
/// Where the finder itself was this unsure of a box, less is asked of the
/// judge: half sure it is not a face is enough. A balloon the finder was
/// 0.62 sure of and the judge 0.77 sure was not a face was being shown as
/// one. MEASURED on the same 1,420 pictures: 14 more of 409 wrongly boxed
/// things thrown out, and 20 more of 7,842 faces. Of ten such boxes graded
/// by hand, one was not a face, four were people in the background, five
/// could not be told, and none was a subject.
const FINDER_UNSURE: f32 = 0.70;
const NOT_A_FACE_IF_UNSURE: f32 = 0.5;
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

    let signals: Vec<subject::Signals> = (0..found.len())
        .map(|i| subject::Signals {
            place: places[i],
            quality: verdicts[i].quality as f64,
            sure: found[i].sure as f64,
            background: verdicts[i].validity[1] as f64,
            focus_log: focus_logs[i],
        })
        .collect();
    let decisions = subject::classify(&signals);

    // The closer look at the eyes, for faces large enough to be accused.
    let looked: Vec<usize> = (0..found.len()).filter(|&i| places[i].w * sw as f64 >= AWAY_MIN_WIDTH).collect();
    let looks: Vec<eyes::Look> = looked
        .iter()
        .map(|&i| {
            let point = |e: (f32, f32)| (e.0 as f64 * up, e.1 as f64 * up);
            eyes::Look { eyes: [point(found[i].eyes[0]), point(found[i].eyes[1])], box_wide: found[i].w as f64 * up, head_yaw: verdicts[i].head_yaw }
        })
        .collect();
    // One look at each eye pair says where the eyes point and how the lids
    // are; the lower half of the face gives the expression; together with
    // the judge's reading they make the state the face is in.
    let seen = eyes::read(rgb, width, height, &looks)?;
    let mouths = state::mouths(rgb, width, height, &looked.iter().map(|&i| places[i]).collect::<Vec<_>>())?;
    let mut eyes_away: Vec<Option<f32>> = vec![None; found.len()];
    let mut states: Vec<Option<(state::State, f32)>> = vec![None; found.len()];
    for ((i, seen), mouth) in looked.iter().zip(&seen).zip(mouths) {
        if let Some(seen) = seen {
            eyes_away[*i] = Some(seen.away);
            let read = state::Read::new(seen.into(), mouth, &verdicts[*i], places[*i].h);
            states[*i] = Some((state::of(&read), state::blink(&read) as f32));
        }
    }

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
                looking_away: wide >= AWAY_MIN_WIDTH && is_away(&v, eyes_away[i]),
                eyes_away: eyes_away[i],
                state: states[i].map(|s| s.0),
                blink: states[i].map(|s| s.1),
                eyes_read: wide >= SHUT_MIN_WIDTH && v.eyes[3] < HIDDEN && v.head_yaw.abs() < READ_MAX_TURN,
            }
        })
        .collect();

    let readings: Vec<score::Reading> = faces
        .iter()
        .map(|f| {
            // The mark follows what was decided of the face: one found to be
            // looking away counts as well up the scale however narrowly it
            // was found so, and one let go counts as looking, whatever the
            // score alone made of it.
            let away = if f.looking_away { f.verdict.away.max(AWAY_MARK) } else { f.verdict.away.min(score::LOOKING as f32) };
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
    (sure < FINDER_CERTAIN && v.validity[2] >= NOT_A_FACE) || (sure < FINDER_UNSURE && v.validity[2] >= NOT_A_FACE_IF_UNSURE)
}

/// Whether a face is looking somewhere other than the camera. `eyes` is
/// what the closer look at its eyes made of them, where they could be
/// looked at.
fn is_away(v: &Verdict, eyes: Option<f32>) -> bool {
    match eyes {
        Some(eyes) => eyes >= EYES_AWAY || (v.away >= AWAY && eyes >= EYES_AGREE),
        None => v.away >= AWAY,
    }
}

/// Finds and judges the faces in a photo file.
pub fn analyze(path: &Path) -> Result<Analysis, String> {
    let picture = preview::decode_upright(path, u32::MAX)?;
    analyze_pixels(&picture.rgb, picture.width, picture.height)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn where_the_finder_was_unsure_less_is_asked_of_the_judge() {
        assert!(is_junk(0.62, &verdict(0.77, 0.0, 0.0, 0.0)), "the balloon: neither was sure it was a face");
        assert!(!is_junk(0.75, &verdict(0.77, 0.0, 0.0, 0.0)), "the finder was surer, and the judge short of certain");
        assert!(!is_junk(0.62, &verdict(0.45, 0.0, 0.0, 0.0)), "the judge leans towards a face");
    }

    #[test]
    fn the_closer_look_at_the_eyes_can_call_it_alone() {
        assert!(is_away(&verdict(0.0, 0.01, 0.0, 0.0), Some(0.85)), "head square, eyes turned aside");
        assert!(!is_away(&verdict(0.0, 0.01, 0.0, 0.0), Some(0.60)), "not sure enough of it");
    }

    #[test]
    fn the_looking_away_score_is_believed_only_if_the_eyes_go_some_way_with_it() {
        assert!(is_away(&verdict(0.0, 0.97, 0.0, 0.0), Some(0.30)), "the score says away and the eyes half agree");
        assert!(!is_away(&verdict(0.0, 0.97, 0.0, 0.0), Some(0.05)), "a turned head with the eyes plainly on the lens");
        assert!(!is_away(&verdict(0.0, 0.45, 0.0, 0.0), Some(0.30)), "neither says so");
    }

    #[test]
    fn where_the_eyes_could_not_be_looked_at_the_score_decides() {
        assert!(is_away(&verdict(0.0, 0.55, 0.0, 0.0), None));
        assert!(!is_away(&verdict(0.0, 0.45, 0.0, 0.0), None));
    }
}
