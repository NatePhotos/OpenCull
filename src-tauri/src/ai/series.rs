//! Setting a person beside themselves across the frames of a burst.
//!
//! How open someone's eyes measure says little on its own: narrow eyes wide
//! open and wide eyes half shut measure alike. Beside the same person a
//! fifth of a second earlier it says a great deal. So within a burst each
//! person is followed from frame to frame, and each frame is marked on how
//! open their eyes are as a share of their own best.
//!
//! People are followed by where they stand, not by what they look like:
//! across one press of the shutter nobody moves far. Someone who does — a
//! child running through the frame — is simply not followed, and is marked
//! the way a photo outside a burst is.
//!
//! The same goes for a pose shot again: a group lined up and photographed
//! five times, one press at a time, is five bursts of one frame by the
//! clock, and one pose to the photographer. Frames taken within a few
//! seconds of each other with the same people in the same places are one
//! pose, and are compared as the frames of a burst are. This never changes
//! what a burst is — it only decides who is set beside whom.
//!
//! An upright photo and one on its side are never the same pose, however
//! alike they are: turning the camera is making another picture. The same
//! people fill the same parts of both frames, so nothing else here would
//! tell them apart.

use serde::Serialize;

use super::score;
use super::store::{FaceRecord, Record};

/// How far a face may have moved since it was last seen, in face sizes, and
/// still be taken for the same person.
const REACH: f32 = 0.6;
/// How much larger or smaller it may have become.
const GROWTH: f32 = 1.67;
/// Eyes that never measure this open anywhere in the burst give nothing to
/// compare with: shut throughout, or not really measured.
const OPEN_AT_ALL: f32 = 0.12;
/// From this many sightings the best is taken as the second widest, not the
/// widest: the widest of many readings is as much luck as eyes.
const MANY: usize = 4;

/// Keep repeated poses within a short moment; neighbour matches must not
/// chain an entire event together. This also caps each group's total span.
pub const POSE_GAP_MS: f64 = 3_000.0;
/// How many of the subjects must be found where they were for two frames to
/// be the same pose.
const SAME_PEOPLE: f32 = 0.8;

/// One photo of a run, in the order they were shot.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub record: &'a Record,
    /// When it was taken, in milliseconds.
    pub time: Option<f64>,
    /// Which press of the shutter it came from, if it shares one.
    pub burst: Option<u32>,
    /// Taller than it is wide, the way up it is shown. `None` if its shape
    /// is not known, and then it is not held against it.
    pub upright: Option<bool>,
}

/// A photo, marked again as one frame of its burst or pose.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compared {
    /// Which of the run's poses it belongs to. Frames given the same number
    /// were compared with each other; a number held by one frame alone means
    /// it was compared with nothing.
    pub pose: usize,
    /// Out of 100. `None` for a photo with no faces.
    pub score: Option<f32>,
    /// For each face, in the order the photo lists them: how open its eyes
    /// are as a share of that person's best in the burst, 1 being the best.
    /// `None` where the person could not be followed or the eyes not read.
    pub eyes: Vec<Option<f32>>,
}

/// How far apart two faces are, in face sizes, if they could be the same
/// person a moment later.
fn near(face: &FaceRecord, last: &FaceRecord) -> Option<f32> {
    let (w, h) = ((face.w + last.w) / 2.0, (face.h + last.h) / 2.0);
    if w <= 0.0 || h <= 0.0 || face.w <= 0.0 || last.w <= 0.0 {
        return None;
    }
    let dx = (face.x + face.w / 2.0 - last.x - last.w / 2.0) / w;
    let dy = (face.y + face.h / 2.0 - last.y - last.h / 2.0) / h;
    let moved = dx.hypot(dy);
    let grown = (face.w / last.w).max(last.w / face.w);
    (moved < REACH && grown < GROWTH).then_some(moved)
}

/// Whether two frames show the same people in the same places.
fn same_pose(a: &Record, b: &Record) -> bool {
    let subjects = |r: &'_ Record| -> Vec<FaceRecord> { r.faces.iter().filter(|f| f.subject).cloned().collect() };
    let (a, b) = (subjects(a), subjects(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
    for (i, face) in a.iter().enumerate() {
        for (j, other) in b.iter().enumerate() {
            if let Some(moved) = near(face, other) {
                pairs.push((moved, i, j));
            }
        }
    }
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
    let (mut used_a, mut used_b) = (vec![false; a.len()], vec![false; b.len()]);
    let mut found = 0;
    for (_, i, j) in pairs {
        if !used_a[i] && !used_b[j] {
            used_a[i] = true;
            used_b[j] = true;
            found += 1;
        }
    }
    found as f32 >= SAME_PEOPLE * a.len().max(b.len()) as f32
}

/// Marks a run of photos, given in the order they were shot: splits it into
/// poses — the frames of one burst, and frames taken soon after one another
/// with the same people in the same places — and marks the frames of each
/// pose against each other. Without `across_presses` a pose is a burst and
/// nothing more.
pub fn compare_run(frames: &[Frame], across_presses: bool) -> Vec<Compared> {
    compare_run_visual(frames, across_presses, None, super::mobileclip::Grouping::Normal)
}

/// Optional visual embeddings replace face geometry, while retaining time and orientation limits.
pub fn compare_run_visual(frames: &[Frame], across_presses: bool, visual: Option<&[Vec<f32>]>, grouping: super::mobileclip::Grouping) -> Vec<Compared> {
    let mut out: Vec<Compared> = Vec::with_capacity(frames.len());
    let mut start = 0;
    let mut pose = 0;
    for at in 1..=frames.len() {
        let joined = at < frames.len() && {
            let (a, b) = (&frames[at - 1], &frames[at]);
            let one_press = a.burst.is_some() && a.burst == b.burst;
            let soon = matches!((a.time, b.time), (Some(t0), Some(t1)) if (t1 - t0).abs() <= POSE_GAP_MS);
            // The camera turned between them — even, rarely, within a press.
            let turned = matches!((a.upright, b.upright), (Some(a), Some(b)) if a != b);
            let within_span = matches!((frames[start].time, b.time), (Some(t0), Some(t1)) if (t1 - t0).abs() <= POSE_GAP_MS);
            let alike = if let Some(v) = visual {
                super::mobileclip::matching(v.get(at-1).map(Vec::as_slice), v.get(at).map(Vec::as_slice), grouping)
                    && super::mobileclip::matching(v.get(start).map(Vec::as_slice), v.get(at).map(Vec::as_slice), grouping)
            } else { same_pose(a.record, b.record) && same_pose(frames[start].record, b.record) };
            !turned && within_span && if visual.is_some() { alike && (one_press || (across_presses && soon)) } else { one_press || (across_presses && soon && alike) }
        };
        if !joined {
            let records: Vec<&Record> = frames[start..at].iter().map(|f| f.record).collect();
            out.extend(compare(&records).into_iter().map(|c| Compared { pose, ..c }));
            start = at;
            pose += 1;
        }
    }
    out
}

/// Who each face is, for every frame: faces given the same number are the
/// same person. Numbers mean nothing beyond that.
fn follow(frames: &[&Record]) -> Vec<Vec<usize>> {
    // Where each person was last seen.
    let mut people: Vec<&FaceRecord> = Vec::new();
    let mut who = Vec::with_capacity(frames.len());
    for frame in frames {
        let mut pairs = Vec::new();
        for (f, face) in frame.faces.iter().enumerate() {
            for (p, last) in people.iter().enumerate() {
                if let Some(moved) = near(face, last) {
                    pairs.push((moved, f, p));
                }
            }
        }
        // The nearest pairs are made first, so that of two people standing
        // close each keeps their own.
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut mine: Vec<Option<usize>> = vec![None; frame.faces.len()];
        let mut taken = vec![false; people.len()];
        for (_, f, p) in pairs {
            if mine[f].is_none() && !taken[p] {
                mine[f] = Some(p);
                taken[p] = true;
            }
        }
        let ids: Vec<usize> = frame
            .faces
            .iter()
            .zip(mine)
            .map(|(face, person)| match person {
                Some(p) => {
                    people[p] = face;
                    p
                }
                None => {
                    people.push(face);
                    people.len() - 1
                }
            })
            .collect();
        who.push(ids);
    }
    who
}

/// Marks the frames of one burst or pose against each other. Frames are
/// given in the order they were shot; the answer is in the same order.
pub fn compare(frames: &[&Record]) -> Vec<Compared> {
    let who = follow(frames);
    let people = who.iter().flatten().max().map_or(0, |m| m + 1);

    // Each person's best: how open their eyes get in this burst.
    let mut readings: Vec<Vec<f32>> = vec![Vec::new(); people];
    for (frame, ids) in frames.iter().zip(&who) {
        for (face, &id) in frame.faces.iter().zip(ids) {
            if face.eyes_read {
                readings[id].push(face.eye);
            }
        }
    }
    let best: Vec<Option<f32>> = readings
        .into_iter()
        .map(|mut seen| {
            if seen.len() < 2 {
                return None;
            }
            seen.sort_by(|a, b| b.total_cmp(a));
            let best = if seen.len() >= MANY { seen[1] } else { seen[0] };
            (best >= OPEN_AT_ALL).then_some(best)
        })
        .collect();

    frames
        .iter()
        .zip(&who)
        .map(|(frame, ids)| {
            let eyes: Vec<Option<f32>> = frame
                .faces
                .iter()
                .zip(ids)
                .map(|(face, &id)| best[id].filter(|_| face.eyes_read).map(|best| (face.eye / best).min(1.0)))
                .collect();

            let (Some(focus), Some(gaze)) = (frame.focus, frame.gaze) else {
                return Compared { pose: 0, score: frame.score, eyes };
            };
            // The same faces a photo on its own is marked on: its subjects,
            // or everyone if nobody was called one.
            let any_subject = frame.faces.iter().any(|f| f.subject);
            let eye = frame
                .faces
                .iter()
                .zip(&eyes)
                .filter(|(face, _)| face.subject || !any_subject)
                .map(|(face, share)| {
                    let eye = match share {
                        Some(_) if face.eyes_shut => 0.0,
                        // Eyes the face's state calls open are inside what
                        // is acceptable: a little narrower than their best
                        // is not a blink, and costs little.
                        Some(share) if face.open => score::eye_open_in_burst(*share as f64),
                        Some(share) => score::eye_in_burst(*share as f64),
                        None if face.open => 1.0,
                        None => score::eye_alone(Some(face.eye as f64)),
                    };
                    // Eyes lowered or narrowed for a reason the face's
                    // state gives are not a blink, however far short of
                    // the same person's best they fall.
                    eye.max(face.excused as f64)
                })
                .min_by(f64::total_cmp);
            let score = eye.map(|eye| score::Parts { eye, focus: focus as f64, gaze: gaze as f64 }.total() as f32);
            Compared { pose: 0, score, eyes }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(x: f32, eye: f32) -> FaceRecord {
        FaceRecord { x, y: 0.3, w: 0.1, h: 0.15, subject: true, reason: None, eyes_shut: false, looking_away: false, maybe_away: false, sharpness: Some(80.0), eye, eyes_read: true, state: None, concern: 0, excused: 0.0, open: false }
    }

    fn frame(faces: Vec<FaceRecord>) -> Record {
        Record { faces, score: Some(50.0), focus: Some(1.0), gaze: Some(1.0), key: None, ..Default::default() }
    }

    fn marks(frames: &[Record]) -> Vec<Compared> {
        compare(&frames.iter().collect::<Vec<_>>())
    }

    #[test]
    fn the_frame_where_someone_blinks_is_marked_down_and_says_who() {
        // Two people. The one on the right half-blinks in the second frame.
        let got = marks(&[frame(vec![face(0.2, 0.30), face(0.6, 0.28)]), frame(vec![face(0.21, 0.29), face(0.6, 0.15)]), frame(vec![face(0.2, 0.30), face(0.61, 0.27)])]);
        assert_eq!(got[0].score, Some(100.0));
        assert_eq!(got[2].score, Some(100.0), "a hair under their best is no fault");
        assert!(got[1].score.unwrap() < 75.0, "half a blink costs most of the eye marks: {:?}", got[1].score);
        assert_eq!(got[1].eyes[0].map(|e| e > 0.9), Some(true));
        assert!((got[1].eyes[1].unwrap() - 0.15 / 0.28).abs() < 1e-6);
        assert_eq!(got[0].eyes, [Some(1.0), Some(1.0)]);
    }

    #[test]
    fn narrow_eyes_are_not_a_fault_and_do_not_hide_someone_elses_blink() {
        // On the left, eyes that never measure wide. A photo marked on its
        // own would call that its worst face in every frame, and the blink
        // on the right in the second would cost almost nothing.
        let got = marks(&[frame(vec![face(0.2, 0.17), face(0.6, 0.30)]), frame(vec![face(0.2, 0.17), face(0.6, 0.16)])]);
        assert_eq!(got[0].score, Some(100.0), "narrow eyes as open as they get are open");
        assert!(got[0].score.unwrap() - got[1].score.unwrap() > 25.0);
    }

    #[test]
    fn people_are_followed_by_where_they_stand_even_listed_in_another_order() {
        let got = marks(&[frame(vec![face(0.2, 0.30), face(0.6, 0.20)]), frame(vec![face(0.6, 0.20), face(0.2, 0.15)])]);
        assert_eq!(got[1].eyes[0], Some(1.0), "the one on the right is as open as before");
        assert!((got[1].eyes[1].unwrap() - 0.5).abs() < 1e-6, "the one on the left is half as open");
    }

    #[test]
    fn someone_missing_from_a_frame_is_picked_up_again_after_it() {
        let got = marks(&[frame(vec![face(0.2, 0.30)]), frame(vec![]), frame(vec![face(0.2, 0.15)])]);
        assert_eq!(got[1], Compared { pose: 0, score: None, eyes: vec![] });
        assert!((got[2].eyes[0].unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn someone_who_cannot_be_followed_is_marked_as_a_photo_on_its_own_would_be() {
        // Seen once, or moved right across the frame: nothing to compare with.
        let got = marks(&[frame(vec![face(0.1, 0.30)]), frame(vec![face(0.7, 0.15)])]);
        assert_eq!(got[1].eyes, [None]);
        let alone = score::Parts { eye: score::eye_alone(Some(0.15)), focus: 1.0, gaze: 1.0 }.total() as f32;
        assert_eq!(got[1].score, Some(alone));
    }

    #[test]
    fn a_face_whose_eyes_could_not_be_read_takes_no_part() {
        // A profile measures wide. It must not become the "best" that the
        // square-on frames are then found wanting beside.
        let mut turned = face(0.2, 0.38);
        turned.eyes_read = false;
        let got = marks(&[frame(vec![turned]), frame(vec![face(0.2, 0.25)]), frame(vec![face(0.2, 0.24)])]);
        assert_eq!(got[0].eyes, [None]);
        assert_eq!(got[1].eyes, [Some(1.0)]);
        assert_eq!(got[1].score, Some(100.0));
    }

    #[test]
    fn in_a_long_burst_the_best_is_the_second_widest() {
        let frames: Vec<Record> = [0.36, 0.30, 0.29, 0.30, 0.15].iter().map(|&eye| frame(vec![face(0.2, eye)])).collect();
        let got = marks(&frames);
        assert_eq!(got[0].eyes, [Some(1.0)], "the one lucky reading is not the yardstick");
        assert_eq!(got[1].eyes, [Some(1.0)]);
        assert!((got[4].eyes[0].unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn eyes_shut_in_every_frame_are_not_called_their_best() {
        let mut shut = face(0.2, 0.07);
        shut.eyes_shut = true;
        let got = marks(&[frame(vec![shut.clone()]), frame(vec![shut])]);
        assert_eq!(got[0].eyes, [None]);
        assert!(got[0].score.unwrap() < 60.0, "marked as a photo on its own would be: {:?}", got[0].score);
    }

    #[test]
    fn eyes_lowered_for_a_reason_are_not_marked_as_a_blink_beside_the_same_person_looking_up() {
        let mut looking_down = face(0.2, 0.12);
        looking_down.excused = 0.85;
        let got = marks(&[frame(vec![face(0.2, 0.30)]), frame(vec![looking_down]), frame(vec![face(0.2, 0.12)])]);
        assert!((got[1].eyes[0].unwrap() - 0.4).abs() < 1e-6, "still seen to be far less open than their best");
        assert!((got[1].score.unwrap() - 93.25).abs() < 1e-4, "and nearly nothing taken off for it: {:?}", got[1].score);
        assert_eq!(got[2].score, Some(55.0), "where the same eyes with no reason given cost all the eye marks");
    }

    #[test]
    fn open_eyes_a_little_short_of_their_best_cost_little_and_a_blink_still_costs_everything() {
        let mut narrower = face(0.2, 0.20);
        narrower.open = true;
        let mut wide = face(0.2, 0.30);
        wide.open = true;
        let got = marks(&[frame(vec![wide]), frame(vec![narrower]), frame(vec![face(0.2, 0.20)])]);
        // Two thirds of their best: 0.63 of the way up from 0.45 to 0.80, on the gentle scale.
        let eye = 0.65 + 0.35 * ((0.20 / 0.30 - 0.45) / 0.35);
        assert!((got[1].score.unwrap() as f64 - (45.0 * eye + 55.0)).abs() < 1e-3, "{:?}", got[1].score);
        assert!(got[2].score.unwrap() < got[1].score.unwrap() - 10.0, "the same eyes not called open are marked as before");
        let mut shut = face(0.2, 0.20);
        shut.open = true;
        shut.eyes_shut = true;
        assert_eq!(marks(&[frame(vec![wide_open()]), frame(vec![shut])])[1].score, Some(55.0));
    }

    fn wide_open() -> FaceRecord {
        face(0.2, 0.30)
    }

    #[test]
    fn eyes_the_judge_calls_shut_cost_all_the_eye_marks_whatever_they_measure() {
        let mut shut = face(0.2, 0.22);
        shut.eyes_shut = true;
        let got = marks(&[frame(vec![face(0.2, 0.30)]), frame(vec![shut])]);
        assert_eq!(got[1].score, Some(55.0));
    }

    fn run(frames: &[(Record, f64, Option<u32>)]) -> Vec<Compared> {
        compare_run(&frames.iter().map(|(record, time, burst)| Frame { record, time: Some(*time), burst: *burst, upright: None }).collect::<Vec<_>>(), true)
    }

    #[test]
    fn visual_grouping_vetoes_unrelated_burst_frames_and_can_join_without_faces() {
        let r = frame(vec![]);
        let frames = [0.,200.,400.].map(|time| Frame {record:&r,time:Some(time),burst:Some(1),upright:Some(true)});
        let feature = |x: usize| { let mut v=vec![0.;512]; v[x]=1.;v };
        let visual = vec![feature(0),feature(0),feature(1)];
        assert_eq!(compare_run(&frames,true).iter().map(|f| f.pose).collect::<Vec<_>>(),[0,0,0]);
        assert_eq!(compare_run_visual(&frames,true,Some(&visual),super::super::mobileclip::Grouping::Tight).iter().map(|f| f.pose).collect::<Vec<_>>(),[0,0,1]);
        let no_burst = frames.map(|f| Frame {burst:None,..f});
        assert_eq!(compare_run_visual(&no_burst,true,Some(&visual),super::super::mobileclip::Grouping::Tight).iter().map(|f| f.pose).collect::<Vec<_>>(),[0,0,1]);
        assert_eq!(compare_run_visual(&no_burst,false,Some(&visual),super::super::mobileclip::Grouping::Tight).iter().map(|f| f.pose).collect::<Vec<_>>(),[0,1,2]);
        assert_eq!(compare_run_visual(&frames,true,Some(&[]),super::super::mobileclip::Grouping::Tight).iter().map(|f| f.pose).collect::<Vec<_>>(),[0,1,2]);
    }
    #[test]
    fn visual_groups_cannot_drift_from_anchor_or_cross_time_and_orientation_limits() {
        let r=frame(vec![]);
        let feature = |angle:f32| {let mut v=vec![0.;512];v[0]=angle.cos();v[1]=angle.sin();v};
        let visual=vec![feature(0.),feature(0.25),feature(0.5)];
        let frames=[0.,200.,400.].map(|time| Frame {record:&r,time:Some(time),burst:Some(1),upright:Some(true)});
        assert_eq!(compare_run_visual(&frames,true,Some(&visual),super::super::mobileclip::Grouping::Tight).iter().map(|f| f.pose).collect::<Vec<_>>(),[0,0,1]);
        let same=vec![feature(0.);3];
        let mut changed=frames;changed[2].time=Some(4000.);
        assert_eq!(compare_run_visual(&changed,true,Some(&same),super::super::mobileclip::Grouping::Tight)[2].pose,1);
        changed=frames;changed[2].upright=Some(false);
        assert_eq!(compare_run_visual(&changed,true,Some(&same),super::super::mobileclip::Grouping::Tight)[2].pose,1);
    }

    #[test]
    fn an_upright_photo_and_one_on_its_side_are_never_the_same_pose() {
        // The same two people in the same parts of the frame, seconds apart:
        // one pose by everything else here. The camera was turned for the
        // third and fourth, and turned back for the fifth.
        let pair = || frame(vec![face(0.2, 0.30), face(0.6, 0.28)]);
        let shot = |at: f64, upright: Option<bool>, burst: Option<u32>| (pair(), at, upright, burst);
        let poses = |frames: &[(Record, f64, Option<bool>, Option<u32>)], across: bool| {
            let frames: Vec<Frame> = frames.iter().map(|(record, time, upright, burst)| Frame { record, time: Some(*time), burst: *burst, upright: *upright }).collect();
            compare_run(&frames, across).iter().map(|c| c.pose).collect::<Vec<_>>()
        };
        let wide = Some(false);
        let tall = Some(true);
        let frames = [shot(0.0, wide, None), shot(2_000.0, wide, None), shot(4_000.0, tall, None), shot(6_000.0, tall, None), shot(8_000.0, wide, None)];
        assert_eq!(poses(&frames, true), [0, 0, 1, 1, 2]);
        // A shape not known is not held against a photo.
        assert_eq!(poses(&[shot(0.0, wide, None), shot(1_000.0, None, None), shot(2_000.0, tall, None)], true), [0, 0, 0]);
        // Nor does one press of the shutter keep them together.
        assert_eq!(poses(&[shot(0.0, wide, Some(3)), shot(200.0, tall, Some(3))], false), [0, 1]);
    }

    #[test]
    fn asked_not_to_look_across_presses_only_a_burst_is_a_pose() {
        let pair = || frame(vec![face(0.2, 0.30), face(0.6, 0.28)]);
        let frames = [(pair(), 0.0, Some(1)), (pair(), 200.0, Some(1)), (pair(), 3_000.0, None)];
        let frames: Vec<Frame> = frames.iter().map(|(record, time, burst)| Frame { record, time: Some(*time), burst: *burst, upright: None }).collect();
        assert_eq!(compare_run(&frames, false).iter().map(|c| c.pose).collect::<Vec<_>>(), [0, 0, 1]);
        assert_eq!(compare_run(&frames, true).iter().map(|c| c.pose).collect::<Vec<_>>(), [0, 0, 0]);
    }

    #[test]
    fn nearby_repeated_poses_compare_but_a_later_press_stands_alone() {
        // Two people, three separate presses of the shutter, seconds apart.
        // The second catches the one on the right half way through a blink.
        let got = run(&[
            (frame(vec![face(0.2, 0.30), face(0.6, 0.28)]), 0.0, None),
            (frame(vec![face(0.2, 0.30), face(0.6, 0.14)]), 3_000.0, None),
            (frame(vec![face(0.21, 0.29), face(0.61, 0.28)]), 7_000.0, None),
        ]);
        assert_eq!(got.iter().map(|c| c.pose).collect::<Vec<_>>(), [0, 0, 1]);
        assert!((got[1].eyes[1].unwrap() - 0.5).abs() < 1e-6);
        assert!(got[1].score.unwrap() < got[0].score.unwrap() - 25.0);
    }

    #[test]
    fn thirteen_second_gaps_and_neighbor_chains_do_not_merge_scenes() {
        let pair = || frame(vec![face(0.2, 0.30), face(0.6, 0.28)]);
        let got = run(&[(pair(), 0.0, None), (pair(), 2_000.0, None), (pair(), 4_000.0, None), (pair(), 17_000.0, None)]);
        assert_eq!(got.iter().map(|c| c.pose).collect::<Vec<_>>(), [0, 0, 1, 2]);
    }

    #[test]
    fn different_people_or_a_long_wait_is_another_pose() {
        let pair = || frame(vec![face(0.2, 0.30), face(0.6, 0.28)]);
        let got = run(&[
            (pair(), 0.0, None),
            (pair(), 2_000.0, None),
            // The same two, but half a minute on.
            (pair(), 32_000.0, None),
            // Straight after, but someone else, somewhere else.
            (frame(vec![face(0.4, 0.30)]), 33_000.0, None),
            // Nobody in it at all.
            (frame(vec![]), 34_000.0, None),
        ]);
        assert_eq!(got.iter().map(|c| c.pose).collect::<Vec<_>>(), [0, 0, 1, 2, 3]);
        assert_eq!(got[2].eyes, [None, None], "alone in its pose, there is nothing to compare with");
    }

    #[test]
    fn one_of_five_people_stepping_out_is_still_the_same_pose_and_three_is_not() {
        let line = |n: usize| frame((0..n).map(|i| face(0.1 + 0.15 * i as f32, 0.30)).collect());
        let poses = |got: Vec<Compared>| got.iter().map(|c| c.pose).collect::<Vec<_>>();
        assert_eq!(poses(run(&[(line(5), 0.0, None), (line(4), 2_000.0, None)])), [0, 0]);
        assert_eq!(poses(run(&[(line(5), 0.0, None), (line(2), 2_000.0, None)])), [0, 1]);
    }

    #[test]
    fn the_frames_of_one_press_stay_together_whatever_is_in_them() {
        // A child running: nobody is where they were. It is still one burst,
        // and still ranked as one; it joins the pose before it only if that
        // pose's last frame matches its first.
        let got = run(&[(frame(vec![face(0.1, 0.30)]), 0.0, Some(7)), (frame(vec![face(0.7, 0.30)]), 200.0, Some(7)), (frame(vec![]), 400.0, Some(7))]);
        assert_eq!(got.iter().map(|c| c.pose).collect::<Vec<_>>(), [0, 0, 0]);
        assert_eq!(got[1].eyes, [None], "but the child is not taken for the same person");
    }

    #[test]
    fn a_background_face_blinking_does_not_mark_the_photo_down() {
        let mut passer_by = face(0.6, 0.10);
        passer_by.subject = false;
        let mut open = face(0.6, 0.30);
        open.subject = false;
        let got = marks(&[frame(vec![face(0.2, 0.30), open]), frame(vec![face(0.2, 0.30), passer_by])]);
        assert_eq!(got[1].score, Some(100.0));
        assert!(got[1].eyes[1].unwrap() < 0.4, "but it is still told how its eyes compare");
    }
}
