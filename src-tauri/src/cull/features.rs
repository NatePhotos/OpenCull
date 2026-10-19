//! The fixed list of numbers the auto-cull network sees for one photo. The
//! order never changes within a FORMAT; lessons saved under another FORMAT
//! are rebuilt or set aside.

use serde::{Deserialize, Serialize};

use super::light::Light;
use super::Frame;

// 3: stops against neighbours measured in real stops of light.
/// Bump whenever the list below changes meaning or length.
pub const FORMAT: u32 = 3;
pub const READINGS: usize = 33;
/// The MobileCLIP fingerprint, when the photographer has it switched on.
pub const EMBED: usize = 512;
pub const INPUTS: usize = READINGS + EMBED;

pub const HAS_FACES: usize = 0;
pub const SUBJECTS: usize = 1;
pub const SCORE: usize = 2;
pub const HAS_SCORE: usize = 3;
pub const FOCUS: usize = 4;
pub const GAZE: usize = 5;
pub const MIN_EYE: usize = 6;
pub const ANY_SHUT: usize = 7;
pub const ANY_AWAY: usize = 8;
pub const ANY_MAYBE_AWAY: usize = 9;
pub const MAX_CONCERN: usize = 10;
pub const MIN_SHARP: usize = 11;
pub const MEAN_SHARP: usize = 12;
pub const BIGGEST_FACE: usize = 13;
pub const ALL_OPEN: usize = 14;
pub const LIGHT_MEAN: usize = 15;
pub const LIGHT_P05: usize = 16;
pub const LIGHT_P95: usize = 17;
pub const SHADOWS: usize = 18;
pub const HIGHLIGHTS: usize = 19;
pub const ALONE: usize = 20;
pub const RANK_PLACE: usize = 21;
pub const BURST_SIZE: usize = 22;
pub const MARK: usize = 23;
pub const GAP_TO_BEST: usize = 24;
pub const MIN_EYE_SHARE: usize = 25;
pub const EXCUSED: usize = 26;
pub const HAS_EMBEDDING: usize = 27;
pub const KIND_EVENTS: usize = 28;
pub const KIND_PORTRAITS: usize = 29;
pub const KIND_SPORTS: usize = 30;
pub const STOPS_VS_NEIGHBOURS: usize = 31;
pub const DETAIL: usize = 32;

/// What sort of shoot a folder is, which changes what counts as a good frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShootKind {
    #[default]
    Events,
    Portraits,
    Sports,
}

impl ShootKind {
    pub fn label(&self) -> &'static str {
        match self {
            ShootKind::Events => "Events",
            ShootKind::Portraits => "Portraits",
            ShootKind::Sports => "Sports",
        }
    }

    /// Which reading column holds this kind's one-hot.
    pub fn column(&self) -> usize {
        match self {
            ShootKind::Events => KIND_EVENTS,
            ShootKind::Portraits => KIND_PORTRAITS,
            ShootKind::Sports => KIND_SPORTS,
        }
    }

    /// The kind a set of readings was made for; Events if none is set.
    pub fn of(r: &[f32; READINGS]) -> ShootKind {
        if r[KIND_SPORTS] > 0.5 {
            ShootKind::Sports
        } else if r[KIND_PORTRAITS] > 0.5 {
            ShootKind::Portraits
        } else {
            ShootKind::Events
        }
    }
}

/// The photo's mark among its burst, 0 to 1: the compared mark where there
/// is one, its own mark otherwise.
pub fn mark(frame: &Frame) -> f32 {
    frame.compared_score.or(frame.record.score).unwrap_or(0.0) / 100.0
}

/// What the scan found, its light and its place in the burst, as numbers
/// mostly between 0 and 1. `best_mark` is the highest `mark` in its group;
/// `stops` is how many stops more light it caught than the frames shot around it.
pub fn readings(frame: &Frame, light: Light, best_mark: f32, kind: ShootKind, stops: f32) -> [f32; READINGS] {
    let mut r = [0.0f32; READINGS];
    let subjects: Vec<_> = frame.record.faces.iter().filter(|f| f.subject).collect();
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    r[HAS_FACES] = flag(!subjects.is_empty());
    r[SUBJECTS] = (subjects.len() as f32 / 5.0).min(1.0);
    r[SCORE] = frame.record.score.unwrap_or(0.0) / 100.0;
    r[HAS_SCORE] = flag(frame.record.score.is_some());
    r[FOCUS] = frame.record.focus.unwrap_or(1.0);
    r[GAZE] = frame.record.gaze.unwrap_or(1.0);
    r[MIN_EYE] = subjects.iter().map(|f| (f.eye / 0.3).min(1.5)).fold(1.5f32, f32::min).min(1.5) / 1.5;
    r[ANY_SHUT] = flag(subjects.iter().any(|f| f.eyes_shut));
    r[ANY_AWAY] = flag(subjects.iter().any(|f| f.looking_away));
    r[ANY_MAYBE_AWAY] = flag(subjects.iter().any(|f| f.maybe_away));
    r[MAX_CONCERN] = subjects.iter().map(|f| f.concern).max().unwrap_or(0) as f32 / 3.0;
    let sharp: Vec<f32> = subjects.iter().filter_map(|f| f.sharpness).map(|s| s / 100.0).collect();
    // Sharpness that could not be told is never read as blur.
    r[MIN_SHARP] = sharp.iter().copied().fold(1.0f32, f32::min);
    r[MEAN_SHARP] = if sharp.is_empty() { 1.0 } else { sharp.iter().sum::<f32>() / sharp.len() as f32 };
    r[BIGGEST_FACE] = subjects.iter().map(|f| f.w).fold(0.0f32, f32::max);
    r[ALL_OPEN] = flag(!subjects.is_empty() && subjects.iter().all(|f| f.open));
    r[LIGHT_MEAN] = light.mean;
    r[LIGHT_P05] = light.p05;
    r[LIGHT_P95] = light.p95;
    r[SHADOWS] = light.shadows;
    r[HIGHLIGHTS] = light.highlights;
    let count = frame.count.unwrap_or(1).max(1);
    r[ALONE] = flag(count <= 1);
    r[RANK_PLACE] = match frame.rank { Some(rank) if count > 1 => (rank.saturating_sub(1)) as f32 / (count - 1) as f32, _ => 0.0 };
    r[BURST_SIZE] = ((count as f32).log2() / 4.0).min(1.0);
    r[MARK] = mark(frame);
    r[GAP_TO_BEST] = (best_mark - mark(frame)).max(0.0);
    r[MIN_EYE_SHARE] = frame.compared_eyes.iter().flatten().copied().fold(1.0f32, f32::min);
    r[EXCUSED] = subjects.iter().map(|f| f.excused).fold(0.0f32, f32::max);
    r[HAS_EMBEDDING] = 0.0;
    r[kind.column()] = 1.0;
    r[STOPS_VS_NEIGHBOURS] = (stops / 3.0).clamp(-1.0, 1.0);
    r[DETAIL] = light.detail;
    r
}

/// The readings followed by the fingerprint, or by zeros where there is none.
pub fn inputs(readings: &[f32; READINGS], embedding: Option<&[f32]>) -> Vec<f32> {
    let mut x = Vec::with_capacity(INPUTS);
    x.extend_from_slice(readings);
    x[HAS_EMBEDDING] = 0.0;
    match embedding.filter(|e| e.len() == EMBED) {
        Some(e) => {
            x[HAS_EMBEDDING] = 1.0;
            x.extend_from_slice(e);
        }
        None => x.resize(INPUTS, 0.0),
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::store::{FaceRecord, Record};
    use crate::cull::light::Light;
    use crate::cull::Frame;

    fn face(shut: bool, away: bool, sharp: Option<f32>, eye: f32, w: f32) -> FaceRecord {
        FaceRecord {
            x: 0.4, y: 0.3, w, h: w * 1.3, subject: true, reason: None, eyes_shut: shut, looking_away: away,
            maybe_away: false, sharpness: sharp, eye, eyes_read: true, state: None, concern: if shut { 3 } else { 0 },
            excused: 0.0, open: !shut,
        }
    }

    fn frame(faces: Vec<FaceRecord>, score: Option<f32>, rank: Option<u32>, count: Option<u32>) -> Frame {
        Frame {
            path: "/p/a.ARW".into(), version: "v".into(),
            record: Record { faces, score, focus: Some(0.8), gaze: Some(0.9), key: None },
            compared_score: score, compared_eyes: vec![Some(0.9)], rank, count, pose: Some(1), rating: 0, time: None, flag: "none".into(),
        }
    }

    const LIGHT: Light = Light { mean: 0.45, linear: 0.17, p05: 0.05, p95: 0.9, shadows: 0.01, highlights: 0.0, detail: 0.3 };

    #[test]
    fn a_shut_eyed_subject_is_read_as_shut_and_concerning() {
        let r = readings(&frame(vec![face(true, false, Some(80.0), 0.1, 0.1)], Some(40.0), Some(2), Some(3)), LIGHT, 0.7, ShootKind::Events, 0.0);
        assert_eq!(r[HAS_FACES], 1.0);
        assert_eq!(r[ANY_SHUT], 1.0);
        assert_eq!(r[MAX_CONCERN], 1.0);
        assert!((r[GAP_TO_BEST] - 0.3).abs() < 1e-6);
        assert!((r[RANK_PLACE] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn unknown_sharpness_is_not_read_as_blur() {
        let r = readings(&frame(vec![face(false, false, None, 0.3, 0.1)], Some(70.0), None, None), LIGHT, 0.7, ShootKind::Events, 0.0);
        assert_eq!(r[MIN_SHARP], 1.0);
        assert_eq!(r[ALONE], 1.0);
    }

    #[test]
    fn a_photo_without_people_says_so_and_carries_its_light() {
        let r = readings(&frame(vec![], Some(30.0), None, None), LIGHT, 0.3, ShootKind::Events, 0.0);
        assert_eq!(r[HAS_FACES], 0.0);
        assert_eq!(r[HAS_SCORE], 1.0);
        assert!((r[LIGHT_MEAN] - 0.45).abs() < 1e-6);
    }

    #[test]
    fn the_shoot_kind_is_one_hot_and_reads_back() {
        let r = readings(&frame(vec![], Some(50.0), None, None), LIGHT, 0.5, ShootKind::Sports, 0.0);
        assert_eq!((r[KIND_EVENTS], r[KIND_PORTRAITS], r[KIND_SPORTS]), (0.0, 0.0, 1.0));
        assert_eq!(ShootKind::of(&r), ShootKind::Sports);
    }

    #[test]
    fn neighbour_stops_and_detail_are_carried_and_stops_are_limited() {
        let r = readings(&frame(vec![], Some(50.0), None, None), LIGHT, 0.5, ShootKind::Events, -9.0);
        assert_eq!(r[STOPS_VS_NEIGHBOURS], -1.0);
        assert!((r[DETAIL] - 0.3).abs() < 1e-6);
        let r = readings(&frame(vec![], Some(50.0), None, None), LIGHT, 0.5, ShootKind::Events, 1.5);
        assert!((r[STOPS_VS_NEIGHBOURS] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn inputs_are_always_the_same_length_and_flag_the_fingerprint() {
        let r = [0.5; READINGS];
        let without = inputs(&r, None);
        assert_eq!(without.len(), INPUTS);
        assert_eq!(without[HAS_EMBEDDING], 0.0);
        let print = vec![0.1; EMBED];
        let with = inputs(&r, Some(&print));
        assert_eq!(with.len(), INPUTS);
        assert_eq!(with[HAS_EMBEDDING], 1.0);
        assert_eq!(with[READINGS], 0.1);
    }
}
