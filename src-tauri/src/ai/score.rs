//! One mark for a photograph, from its faces.
//!
//! The Mac app's arithmetic, carried over. A photograph is marked on its
//! WORST subject, because it is ruined by the one person who blinked and not
//! by the average: MEASURED by the Mac app, 77% against 69% for the average
//! at telling keepers from rejects.
//!
//! Looking away is the exception. Where nobody in the photograph is looking
//! at the camera, it is the photograph — a couple looking at each other, a
//! table of guests, a child with a toy — and costs nothing. It costs only
//! where someone else IS looking, and then a little: enough to put a frame
//! behind one where everyone looked, not enough to sink it.
//! MEASURED: a photographer shown 80 photos with faces flagged as looking
//! away called it a flaw in none of the 43 where everyone was flagged, and
//! in 3 of the 30 where someone else was looking. And on 734 photos that
//! were rated by hand, the mark tells the good from the bad 81% of the time
//! this way, where counting every look away in full it told 77%.

/// What the mark is made from, for one face.
#[derive(Clone, Copy, Debug)]
pub struct Reading {
    pub subject: bool,
    /// How open the eyes are: the eye ratio.
    pub eye: Option<f64>,
    /// The focus measure, as a logarithm.
    pub focus_log: Option<f64>,
    /// The looking-away score, 0 to 1.
    pub away: Option<f64>,
}

const EYE_WEIGHT: f64 = 0.45;
const FOCUS_WEIGHT: f64 = 0.15;
const GAZE_WEIGHT: f64 = 0.40;
/// A looking-away score under this is someone looking at the camera.
pub const LOOKING: f64 = 0.49;
/// How much of the gaze part one person looking away can cost, where
/// someone else in the photograph is looking at the camera.
const ODD_ONE_OUT: f64 = 0.25;

/// Where `value` falls between `low` and `high`, 0 to 1. A reading a photo
/// does not supply falls back to a neutral figure, never to nought: a
/// missing measurement must not pass for a fault.
fn between(value: Option<f64>, low: f64, high: f64, missing: f64) -> f64 {
    value.map_or(missing, |v| ((v - low) / (high - low)).clamp(0.0, 1.0))
}

/// The three things a photograph is marked on, each from 0 (ruined) to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    pub eye: f64,
    pub focus: f64,
    pub gaze: f64,
}

impl Parts {
    /// The mark out of 100.
    pub fn total(&self) -> f64 {
        (EYE_WEIGHT * self.eye + FOCUS_WEIGHT * self.focus + GAZE_WEIGHT * self.gaze) * 100.0
    }
}

/// How a face's eyes do, from how open they measure, with nothing to compare
/// them with. Narrow eyes and half-shut eyes measure alike, so this is only
/// used where the same person cannot be found in another frame.
pub fn eye_alone(ratio: Option<f64>) -> f64 {
    between(ratio, 0.05, 0.30, 0.75)
}

/// How a face's eyes do, as a share of how open the same person's eyes are
/// in the best frame of the burst.
///
/// MEASURED by eye on 200 such comparisons: under 0.60 nearly every one is a
/// blink, half a blink or eyes cast down; from 0.60 to 0.70 most are; above
/// 0.80 the two frames cannot be told apart and the difference is the
/// measurement's own wobble — so nothing is taken off there.
pub fn eye_in_burst(share_of_best: f64) -> f64 {
    between(Some(share_of_best), 0.45, 0.80, 1.0)
}

/// What a photograph is marked on. `None` for a photograph with no faces.
///
/// Marked on the subjects, or on every face if `with_background` or if none
/// was called a subject.
pub fn parts(faces: &[Reading], with_background: bool) -> Option<Parts> {
    let subjects: Vec<&Reading> = faces.iter().filter(|f| f.subject).collect();
    let marked: Vec<&Reading> = if with_background || subjects.is_empty() { faces.iter().collect() } else { subjects };
    if marked.is_empty() {
        return None;
    }
    let least = |get: fn(&Reading) -> Option<f64>| marked.iter().filter_map(|f| get(f)).min_by(f64::total_cmp);
    // Looking away counts only against someone looking: with nobody on the
    // lens there is nothing for it to be a fault beside. A face with no
    // reading is neither.
    let someone_looking = marked.iter().any(|f| f.away.is_some_and(|a| a <= LOOKING));
    let gaze = if marked.iter().all(|f| f.away.is_none()) {
        0.75
    } else if someone_looking {
        let most_away = marked.iter().filter_map(|f| f.away).max_by(f64::total_cmp);
        1.0 - ODD_ONE_OUT * between(most_away, 0.50, 0.98, 0.0)
    } else {
        1.0
    };

    Some(Parts { eye: eye_alone(least(|f| f.eye)), focus: between(least(|f| f.focus_log), -1.2, 0.6, 0.75), gaze })
}

/// The photograph's mark out of 100. `None` for a photograph with no faces.
pub fn photo(faces: &[Reading], with_background: bool) -> Option<f64> {
    parts(faces, with_background).map(|p| p.total())
}

/// Each photo's place in its burst, 1 for the best. Photos with no mark come
/// last, in the order given. Equal marks keep the order given.
pub fn ranks(marks: &[Option<f64>]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..marks.len()).collect();
    order.sort_by(|a, b| marks[*b].unwrap_or(f64::NEG_INFINITY).total_cmp(&marks[*a].unwrap_or(f64::NEG_INFINITY)));
    let mut rank = vec![0; marks.len()];
    for (place, index) in order.into_iter().enumerate() {
        rank[index] = place + 1;
    }
    rank
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(eye: f64, focus: f64, away: f64) -> Reading {
        Reading { subject: true, eye: Some(eye), focus_log: Some(focus), away: Some(away) }
    }

    #[test]
    fn eyes_wide_in_focus_and_on_the_lens_is_full_marks() {
        assert!((photo(&[face(0.32, 0.7, 0.1)], false).unwrap() - 100.0).abs() < 1e-9);
        // Eyes shut and out of focus is nought of those two parts; looking
        // away, with nobody else in the frame looking, takes nothing more.
        assert!((photo(&[face(0.02, -2.0, 1.0)], false).unwrap() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn one_blink_marks_the_whole_photo_down() {
        let good = photo(&[face(0.3, 0.5, 0.1), face(0.3, 0.5, 0.1)], false).unwrap();
        let blink = photo(&[face(0.3, 0.5, 0.1), face(0.05, 0.5, 0.1)], false).unwrap();
        assert!((good - blink - 45.0).abs() < 1e-6, "the eye term is 45 of the 100");
    }

    #[test]
    fn a_background_face_does_not_count_unless_asked_or_there_is_no_one_else() {
        let mut passer_by = face(0.05, 0.5, 1.0);
        passer_by.subject = false;
        let subject = face(0.3, 0.5, 0.1);
        let just_subject = photo(&[subject], false).unwrap();
        assert_eq!(photo(&[subject, passer_by], false).unwrap(), just_subject);
        assert!(photo(&[subject, passer_by], true).unwrap() < just_subject);
        assert!(photo(&[passer_by], false).is_some(), "with no subject, every face counts");
        assert_eq!(photo(&[], false), None);
    }

    #[test]
    fn looking_away_costs_nothing_where_nobody_is_looking_at_the_camera() {
        let together = photo(&[face(0.3, 0.7, 0.95), face(0.3, 0.7, 0.92)], false).unwrap();
        assert!((together - 100.0).abs() < 1e-9, "a couple looking at each other");
        assert!((photo(&[face(0.3, 0.7, 0.95)], false).unwrap() - 100.0).abs() < 1e-9, "or one person looking down");
    }

    #[test]
    fn it_costs_a_little_where_someone_else_is_looking() {
        let all = photo(&[face(0.3, 0.7, 0.1), face(0.3, 0.7, 0.1), face(0.3, 0.7, 0.1)], false).unwrap();
        let one_off = photo(&[face(0.3, 0.7, 0.1), face(0.3, 0.7, 0.1), face(0.3, 0.7, 0.98)], false).unwrap();
        assert!((all - one_off - 10.0).abs() < 1e-6, "a quarter of the gaze part's 40");
        let blink = photo(&[face(0.3, 0.7, 0.1), face(0.3, 0.7, 0.1), face(0.05, 0.7, 0.1)], false).unwrap();
        assert!(blink < one_off, "and a blink still costs far more");
    }

    #[test]
    fn a_missing_reading_is_neutral_not_a_fault() {
        let unread = Reading { subject: true, eye: None, focus_log: None, away: None };
        let got = photo(&[unread], false).unwrap();
        assert!((got - (0.45 * 0.75 + 0.15 * 0.75 + 0.40 * 0.75) * 100.0).abs() < 1e-9);
    }

    #[test]
    fn against_their_own_best_a_small_difference_costs_nothing_and_a_blink_costs_everything() {
        assert_eq!(eye_in_burst(1.0), 1.0);
        assert_eq!(eye_in_burst(0.85), 1.0, "within the measurement's own wobble");
        assert!(eye_in_burst(0.62) < 0.5 && eye_in_burst(0.62) > 0.4);
        assert_eq!(eye_in_burst(0.40), 0.0);
    }

    #[test]
    fn a_burst_is_ranked_best_first_with_unmarked_photos_last() {
        assert_eq!(ranks(&[Some(60.0), Some(90.0), None, Some(75.0)]), [3, 1, 4, 2]);
        assert_eq!(ranks(&[Some(50.0), Some(50.0)]), [1, 2], "a tie keeps shooting order");
        assert!(ranks(&[]).is_empty());
    }
}
