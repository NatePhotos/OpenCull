//! What the auto-cull does before it has learned anything: the obvious
//! rejects, and frames ranked by the app's own mark. The starting network
//! is taught to behave like this. Every number here is a starting value
//! that the measurements in the evaluation example are meant to tune.

use super::features::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault { EyesShut, LookingAway, Blurred, VeryDark, Underexposed, BlownOut, TestShot }

impl Fault {
    pub fn words(&self) -> &'static str {
        match self {
            Fault::EyesShut => "eyes shut",
            Fault::LookingAway => "looking away",
            Fault::Blurred => "motion blur or out of focus",
            Fault::VeryDark => "very dark",
            Fault::Underexposed => "underexposed",
            Fault::BlownOut => "blown out",
            Fault::TestShot => "looks like a test shot",
        }
    }

    /// The faults whose reason also names the kind of shoot, because the
    /// kind is why they count.
    pub fn by_kind(&self) -> bool {
        matches!(self, Fault::EyesShut | Fault::LookingAway | Fault::Blurred)
    }
}

/// Frames within this much mark of the burst's best are good, not repeats.
pub const RULE_BLUE_MARGIN: f32 = 0.08;

pub fn faults(r: &[f32; READINGS]) -> Vec<Fault> {
    let kind = ShootKind::of(r);
    let faces = r[HAS_FACES] > 0.5;
    let mut found = Vec::new();
    match kind {
        ShootKind::Events => {
            // Only a clear shut eye or turn-away counts: guests blink and glance in a crowd.
            if faces && (r[ANY_SHUT] > 0.5 || r[MAX_CONCERN] > 0.99) { found.push(Fault::EyesShut); }
            // ...and a turn-away only counts in a posed group shot (2+ people, most looking at
            // the camera); in a candid frame, or alone, looking away is just what people do.
            if faces && r[ANY_AWAY] > 0.5 && r[SUBJECTS] >= 0.39 && r[GAZE] >= 0.5 { found.push(Fault::LookingAway); }
        }
        ShootKind::Portraits => {
            // A posed subject should be open-eyed and facing the camera, so
            // half-shut eyes (two thirds of the way to shut) and a maybe-turned head count too.
            if faces && (r[ANY_SHUT] > 0.5 || r[MAX_CONCERN] > 0.6) { found.push(Fault::EyesShut); }
            if faces && (r[ANY_AWAY] > 0.5 || r[ANY_MAYBE_AWAY] > 0.5) { found.push(Fault::LookingAway); }
        }
        // Athletes shut their eyes and look at the ball, not the lens: neither rejects a frame.
        ShootKind::Sports => {}
    }
    // Portraits are shot slowly and sharp, so a softer face is a miss there.
    let line = if kind == ShootKind::Portraits { 0.35 } else { 0.25 };
    // Without a face the subject's own sharpness is what counts, when it is known.
    let subject_soft = r[HAS_SCORE] > 0.5 && r[SCORE] < 0.30;
    if if faces { r[MIN_SHARP] < line } else { subject_soft } { found.push(Fault::Blurred); }
    // Very dark takes priority over underexposed, so one misfire is reported once.
    if r[LIGHT_MEAN] < 0.12 && r[LIGHT_P95] < 0.35 { found.push(Fault::VeryDark); }
    // The stops reading is the stops of light between this frame and the frames around it,
    // divided by 3 (-1..1), so -0.5 is a stop and a half less light than its neighbours
    // (about a third as much): a different exposure, not a different scene.
    else if r[STOPS_VS_NEIGHBOURS] <= -0.5 { found.push(Fault::Underexposed); }
    // A stop and a half more light than its neighbours, with real clipped highlights (over a fifth of the frame).
    if r[STOPS_VS_NEIGHBOURS] >= 0.5 && r[HIGHLIGHTS] > 0.2 { found.push(Fault::BlownOut); }
    if !faces {
        // A test shot is a frame with no people that shows several of: no good
        // subject, an exposure unlike its neighbours, nothing around it, almost no detail.
        let signs = [r[HAS_SCORE] < 0.5 || r[SCORE] < 0.30, r[STOPS_VS_NEIGHBOURS].abs() >= 0.5, r[ALONE] > 0.5, r[DETAIL] < 0.12];
        if signs.iter().filter(|s| **s).count() >= 3 { found.push(Fault::TestShot); }
    }
    found
}

/// Faults that make a burst's best frame purple without making any frame red.
pub fn warnings(r: &[f32; READINGS]) -> Vec<Fault> {
    // Shut eyes are tolerated in sports, but a burst of nothing else is worth a look.
    if ShootKind::of(r) == ShootKind::Sports && r[HAS_FACES] > 0.5 && (r[ANY_SHUT] > 0.5 || r[MAX_CONCERN] > 0.99) {
        vec![Fault::EyesShut]
    } else {
        Vec::new()
    }
}

pub fn rule_rank(r: &[f32; READINGS]) -> f32 {
    let base = match ShootKind::of(r) {
        ShootKind::Events => r[MARK],
        // Portraits: how open and sharp the eyes are matters beside the mark.
        ShootKind::Portraits => 0.7 * r[MARK] + 0.3 * r[MIN_EYE_SHARE],
        // Sports: the frame where the athlete is crisp wins.
        ShootKind::Sports => 0.6 * r[MARK] + 0.4 * if r[HAS_FACES] > 0.5 { r[MIN_SHARP] } else { r[SCORE] },
    };
    base - if faults(r).is_empty() { 0.0 } else { 0.5 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(kind: ShootKind) -> [f32; READINGS] {
        let mut r = [0.0; READINGS];
        r[HAS_FACES] = 1.0; r[MIN_SHARP] = 0.8; r[MEAN_SHARP] = 0.8; r[MARK] = 0.7; r[SCORE] = 0.7; r[HAS_SCORE] = 1.0;
        r[LIGHT_MEAN] = 0.45; r[LIGHT_P95] = 0.9; r[DETAIL] = 0.5; r[STOPS_VS_NEIGHBOURS] = 0.0;
        r[kind.column()] = 1.0;
        r
    }

    #[test]
    fn a_clean_photo_has_no_faults() {
        for k in [ShootKind::Events, ShootKind::Portraits, ShootKind::Sports] { assert!(faults(&clean(k)).is_empty()); }
    }

    #[test]
    fn each_fault_is_named() {
        let e = ShootKind::Events;
        let mut r = clean(e); r[ANY_SHUT] = 1.0; assert_eq!(faults(&r), vec![Fault::EyesShut]);
        let mut r = clean(e); r[ANY_AWAY] = 1.0; r[SUBJECTS] = 0.4; r[GAZE] = 0.8; assert_eq!(faults(&r), vec![Fault::LookingAway]);
        let mut r = clean(e); r[MIN_SHARP] = 0.1; assert_eq!(faults(&r), vec![Fault::Blurred]);
        let mut r = clean(e); r[LIGHT_MEAN] = 0.05; r[LIGHT_P95] = 0.2; assert_eq!(faults(&r), vec![Fault::VeryDark]);
    }

    #[test]
    fn a_soft_photo_without_people_is_blurred_only_when_its_sharpness_is_known() {
        let mut r = clean(ShootKind::Events); r[HAS_FACES] = 0.0; r[SCORE] = 0.1;
        assert_eq!(faults(&r), vec![Fault::Blurred]);
        r[HAS_SCORE] = 0.0;
        assert!(faults(&r).is_empty());
    }

    #[test]
    fn a_fault_ranks_a_frame_below_a_clean_one_of_lower_mark() {
        let mut bad = clean(ShootKind::Events); bad[MARK] = 0.9; bad[ANY_SHUT] = 1.0;
        let mut good = clean(ShootKind::Events); good[MARK] = 0.6;
        assert!(rule_rank(&good) > rule_rank(&bad));
    }

    #[test]
    fn portraits_are_strict_about_looking_and_half_shut_eyes() {
        let mut r = clean(ShootKind::Portraits); r[ANY_MAYBE_AWAY] = 1.0; assert_eq!(faults(&r), vec![Fault::LookingAway]);
        let mut r = clean(ShootKind::Portraits); r[MAX_CONCERN] = 2.0 / 3.0; assert_eq!(faults(&r), vec![Fault::EyesShut]);
        let mut r = clean(ShootKind::Events); r[MAX_CONCERN] = 2.0 / 3.0; assert!(faults(&r).is_empty());
        let mut r = clean(ShootKind::Portraits); r[MIN_SHARP] = 0.3; assert_eq!(faults(&r), vec![Fault::Blurred]);
        let mut r = clean(ShootKind::Events); r[MIN_SHARP] = 0.3; assert!(faults(&r).is_empty());
    }

    #[test]
    fn at_an_event_looking_away_only_counts_in_a_posed_group() {
        let e = ShootKind::Events;
        // One person looking off: just a candid.
        let mut r = clean(e); r[ANY_AWAY] = 1.0; r[SUBJECTS] = 0.2; r[GAZE] = 0.9; assert!(faults(&r).is_empty());
        // A group where most are looking at the camera and one is not: a miss.
        let mut r = clean(e); r[ANY_AWAY] = 1.0; r[SUBJECTS] = 0.6; r[GAZE] = 0.8; assert_eq!(faults(&r), vec![Fault::LookingAway]);
        // A group where hardly anyone is looking at the camera: a candid.
        let mut r = clean(e); r[ANY_AWAY] = 1.0; r[SUBJECTS] = 0.6; r[GAZE] = 0.2; assert!(faults(&r).is_empty());
        // Portraits are strict about it whatever the group.
        let mut r = clean(ShootKind::Portraits); r[ANY_AWAY] = 1.0; r[SUBJECTS] = 0.2; r[GAZE] = 0.0; assert_eq!(faults(&r), vec![Fault::LookingAway]);
    }

    #[test]
    fn sports_does_not_reject_shut_eyes_or_looking_away_but_warns() {
        let mut r = clean(ShootKind::Sports); r[ANY_SHUT] = 1.0; r[ANY_AWAY] = 1.0;
        assert!(faults(&r).is_empty());
        assert_eq!(warnings(&r), vec![Fault::EyesShut]);
    }

    #[test]
    fn exposure_is_judged_against_the_frames_around_it_in_every_kind() {
        for k in [ShootKind::Events, ShootKind::Portraits, ShootKind::Sports] {
            let mut r = clean(k); r[STOPS_VS_NEIGHBOURS] = -2.0 / 3.0; assert_eq!(faults(&r), vec![Fault::Underexposed]);
            let mut r = clean(k); r[STOPS_VS_NEIGHBOURS] = 2.0 / 3.0; r[HIGHLIGHTS] = 0.3; assert_eq!(faults(&r), vec![Fault::BlownOut]);
            let mut r = clean(k); r[STOPS_VS_NEIGHBOURS] = -0.3; assert!(faults(&r).is_empty()); // under 1.5 stops: fine
        }
    }

    #[test]
    fn a_test_shot_needs_several_signs_and_no_people() {
        let mut r = clean(ShootKind::Events); r[HAS_FACES] = 0.0; r[HAS_SCORE] = 0.0; r[ALONE] = 1.0; r[DETAIL] = 0.05;
        assert!(faults(&r).contains(&Fault::TestShot));
        let mut r = clean(ShootKind::Events); r[HAS_FACES] = 0.0; r[ALONE] = 1.0; // a sharp detail shot on its own
        assert!(!faults(&r).contains(&Fault::TestShot));
        let mut r = clean(ShootKind::Events); r[HAS_SCORE] = 0.0; r[ALONE] = 1.0; r[DETAIL] = 0.05; // has people
        assert!(!faults(&r).contains(&Fault::TestShot));
    }

    #[test]
    fn only_the_kind_dependent_faults_carry_the_label() {
        assert!(Fault::EyesShut.by_kind() && Fault::LookingAway.by_kind() && Fault::Blurred.by_kind());
        assert!(!Fault::VeryDark.by_kind() && !Fault::Underexposed.by_kind() && !Fault::BlownOut.by_kind() && !Fault::TestShot.by_kind());
    }
}
