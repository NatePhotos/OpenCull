//! The network everyone starts with: taught, from made-up readings, to do
//! what the starting rules do. No photographs are involved.

use std::sync::OnceLock;

use super::features::*;
use super::net::{sigmoid, Adam, Net, Rng};
use super::rules::{faults, rule_rank_with, RULE_BLUE_MARGIN};

pub const HIDDEN: usize = 32;
const SEED: u64 = 20261018;
/// Seeds the ranker's reading's own stream of made-up values.
const RANKED_SEED: u64 = 20261019;
/// Bump whenever the rules, the readings' meaning or the made-up readings
/// change, so a starting network saved by an earlier version is rebuilt.
// 2: the ranker's reading.
pub const STARTER_VERSION: u32 = 2;

/// Readings of a plausible photo, with each fault turning up often enough to be learned.
pub fn sample(rng: &mut Rng) -> [f32; READINGS] {
    let mut r = drawn(rng);
    r[TAUGHT] = rng.next_f32();
    r
}

/// Every reading of `sample` but the ranker's, which is left average.
fn drawn(rng: &mut Rng) -> [f32; READINGS] {
    let mut r = [0.0f32; READINGS];
    let chance = |rng: &mut Rng, p: f32| if rng.next_f32() < p { 1.0 } else { 0.0 };
    let kind = [ShootKind::Events, ShootKind::Portraits, ShootKind::Sports][(rng.next_f32() * 3.0) as usize % 3];
    r[kind.column()] = 1.0;
    r[HAS_FACES] = chance(rng, 0.8);
    let faces = r[HAS_FACES] > 0.5;
    r[SUBJECTS] = if faces { (1.0 + (rng.next_f32() * 5.0).floor()) / 5.0 } else { 0.0 };
    // Without people: 40% look like test shots (3 or 4 of the four signs), 35% are plain
    // keepers (a sharp subject, normal exposure, real detail), the rest are drawn freely.
    let style = if faces { 0 } else { let u = rng.next_f32(); if u < 0.4 { 1 } else if u < 0.75 { 2 } else { 0 } };
    let signs: [bool; 4] = {
        // Which sign a test-shot-like frame leaves out (4: none).
        let left_out = if rng.next_f32() < 0.3 { 4 } else { (rng.next_f32() * 4.0) as usize % 4 };
        std::array::from_fn(|k| style == 1 && k != left_out) // no subject, odd exposure, alone, no detail
    };
    r[HAS_SCORE] = if faces { 1.0 } else { chance(rng, 0.8) };
    r[SCORE] = rng.next_f32();
    if style == 1 && signs[0] {
        if rng.next_f32() < 0.6 { r[HAS_SCORE] = 0.0; r[SCORE] = 0.0; } else { r[HAS_SCORE] = 1.0; r[SCORE] = rng.next_f32() * 0.28; }
    } else if style != 0 {
        r[HAS_SCORE] = 1.0;
        r[SCORE] = 0.4 + rng.next_f32() * 0.6;
    }
    r[FOCUS] = rng.next_f32();
    r[GAZE] = rng.next_f32();
    if faces {
        r[ANY_SHUT] = chance(rng, 0.15);
        r[MIN_EYE] = if r[ANY_SHUT] > 0.5 { rng.next_f32() * 0.3 } else { 0.4 + rng.next_f32() * 0.6 };
        r[ANY_AWAY] = chance(rng, 0.15);
        r[ANY_MAYBE_AWAY] = chance(rng, 0.1);
        r[MAX_CONCERN] = if r[ANY_SHUT] > 0.5 { [2.0, 3.0][(rng.next_f32() * 2.0) as usize % 2] / 3.0 } else { (rng.next_f32() * 3.0).floor() / 3.0 };
        r[MIN_SHARP] = rng.next_f32();
        r[MEAN_SHARP] = (r[MIN_SHARP] + rng.next_f32() * (1.0 - r[MIN_SHARP])).min(1.0);
        r[BIGGEST_FACE] = 0.02 + rng.next_f32() * 0.4;
        // Every face read as open is less common than no face read as shut: an eye
        // that could not be read is neither. The rules use neither reading.
        r[ALL_OPEN] = if r[ANY_SHUT] > 0.5 { 0.0 } else { chance(rng, 0.6) };
        r[EXCUSED] = rng.next_f32() * 0.3;
    } else {
        r[MIN_SHARP] = 1.0;
        r[MEAN_SHARP] = 1.0;
    }
    // 10% are truly dark (mean under 0.12 and nothing bright: the 95th percentile under 0.35);
    // another 5% are dark-ish but not all of it, like a night scene with a lit window.
    let dark = rng.next_f32();
    r[LIGHT_MEAN] = if dark < 0.10 { rng.next_f32() * 0.11 } else if dark < 0.15 { rng.next_f32() * 0.15 } else { 0.1 + rng.next_f32() * 0.7 };
    r[LIGHT_P05] = r[LIGHT_MEAN] * rng.next_f32();
    r[LIGHT_P95] = if dark < 0.10 {
        r[LIGHT_MEAN] + rng.next_f32() * (0.34 - r[LIGHT_MEAN])
    } else {
        (r[LIGHT_MEAN] + rng.next_f32() * (1.0 - r[LIGHT_MEAN])).min(1.0)
    };
    r[SHADOWS] = if r[LIGHT_MEAN] < 0.15 { rng.next_f32() * 0.6 } else { rng.next_f32() * 0.05 };
    // Often far from its neighbours' exposure (either way), so both exposure faults are learned.
    r[STOPS_VS_NEIGHBOURS] = if (style == 1 && signs[1]) || (style == 0 && rng.next_f32() < 0.12) {
        let low = if style == 1 { 0.5 } else { 0.4 };
        (low + rng.next_f32() * (1.0 - low)) * if rng.next_f32() < 0.5 { -1.0 } else { 1.0 }
    } else {
        (rng.next_f32() - 0.5) * 0.6
    };
    // Clipped highlights are rare, but go with a frame much brighter than its neighbours.
    let clipped = if r[STOPS_VS_NEIGHBOURS] > 0.4 { 0.7 } else { 0.05 };
    r[HIGHLIGHTS] = if rng.next_f32() < clipped { 0.2 + rng.next_f32() * 0.4 } else { rng.next_f32() * 0.05 };
    r[DETAIL] = match style {
        1 if signs[3] => rng.next_f32() * 0.11,
        1 | 2 => 0.3 + rng.next_f32() * 0.7,
        _ => if rng.next_f32() < 0.1 { rng.next_f32() * 0.1 } else { rng.next_f32() },
    };
    // Half of the frames without people are on their own, so test shots turn up often enough to be learned.
    r[ALONE] = if style == 1 { if signs[2] { 1.0 } else { 0.0 } } else { chance(rng, if faces { 0.3 } else { 0.5 }) };
    r[BURST_SIZE] = if r[ALONE] > 0.5 { 0.0 } else { rng.next_f32() * 0.8 };
    r[RANK_PLACE] = if r[ALONE] > 0.5 { 0.0 } else { rng.next_f32() };
    r[MARK] = rng.next_f32();
    r[GAP_TO_BEST] = if r[ALONE] > 0.5 { 0.0 } else { rng.next_f32() * 0.4 };
    r[MIN_EYE_SHARE] = if faces { 0.3 + rng.next_f32() * 0.7 } else { 1.0 };
    r[TAUGHT] = 0.5;
    r
}

/// Trained once per process; every call returns a copy of the same network.
pub fn starting_net() -> Net {
    static STARTER: OnceLock<Net> = OnceLock::new();
    STARTER.get_or_init(train).clone()
}

fn train() -> Net {
    train_at(super::rules::TAUGHT_WEIGHT)
}

/// `train` with the ranker's reading weighed by `taught_weight` wherever the rank is formed.
fn train_at(taught_weight: f32) -> Net {
    let mut rng = Rng::new(SEED);
    // The ranker's reading is drawn from a stream of its own, so every other
    // reading is drawn exactly as before it existed.
    let mut ranked = Rng::new(RANKED_SEED);
    let data: Vec<([f32; READINGS], Vec<f32>)> = (0..30_000)
        .map(|_| {
            let mut r = drawn(&mut rng);
            r[TAUGHT] = ranked.next_f32();
            (r, inputs(&r, None)[..READINGS].to_vec())
        })
        .collect();
    // Photo fingerprints are absent here, so only the readings columns can
    // learn anything: train that narrow net, then widen it. It ranks on top of
    // the rules, so its own rank answer is taught to be nothing: the rules'
    // order stands until the photographer's stars say otherwise. The ranker's
    // reading is added to the rank after the network, by its weight (see
    // `Net::forward_sparse_with`), so its column is held at exactly 0 and the
    // network is the one it was before that reading existed, at any weight.
    let mut net = with_inputs(&Net::new(TAUGHT, HIDDEN, TAUGHT, SEED), READINGS);
    net.rule_base = true;
    let mut adam = Adam::new(net.params.len());
    let mut grads = vec![0.0; net.params.len()];
    for _epoch in 0..60 {
        for batch in data.chunks(256) {
            grads.iter_mut().for_each(|g| *g = 0.0);
            for (r, x) in batch {
                let pass = net.forward_with(x, taught_weight);
                let target = if faults(r).is_empty() { 1.0 } else { 0.0 };
                let d_keep = sigmoid(pass.keep) - target;
                // Against the rules without the ranker's reading, on the rank before it is
                // added, so the weights come out the same whatever its weight.
                let d_rank = 2.0 * (pass.base - rule_rank_with(r, 0.0));
                net.backward(x, &pass, d_keep, d_rank, &mut grads);
            }
            let n = batch.len() as f32;
            grads.iter_mut().for_each(|g| *g /= n);
            (0..HIDDEN).for_each(|j| grads[j * READINGS + TAUGHT] = 0.0);
            adam.step(&mut net.params, &grads, 0.01);
        }
    }
    widen(&net)
}

/// The same network reading `inputs` numbers: the columns it had are copied
/// and the new ones start at exactly 0.
fn with_inputs(narrow: &Net, inputs: usize) -> Net {
    let had = narrow.inputs;
    let mut wide = Net::new(inputs, narrow.hidden, READINGS, SEED);
    wide.params.iter_mut().for_each(|p| *p = 0.0);
    for j in 0..narrow.hidden {
        wide.params[j * inputs..j * inputs + had].copy_from_slice(&narrow.params[j * had..(j + 1) * had]);
    }
    let (from, to) = (narrow.hidden * had, narrow.hidden * inputs);
    wide.params[to..].copy_from_slice(&narrow.params[from..]);
    wide.red_line = narrow.red_line;
    wide.blue_margin = narrow.blue_margin;
    wide.rule_base = narrow.rule_base;
    wide
}

/// The same network with room for a fingerprint: those columns start at exactly 0.
fn widen(narrow: &Net) -> Net {
    let mut wide = with_inputs(narrow, INPUTS);
    wide.red_line = 0.0;
    // The rules' margin is in mark points; the network's ranking score is
    // the rule rank plus an answer taught to be nothing, so the same margin carries over.
    wide.blue_margin = RULE_BLUE_MARGIN;
    wide
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cull::features::inputs;
    use crate::cull::rules::{faults, rule_rank, rule_rank_with, Fault};

    #[test]
    fn the_starting_network_agrees_with_the_rules() {
        let net = starting_net();
        let mut rng = Rng::new(99);
        let mut keep_right = 0;
        for _ in 0..2000 {
            let a = sample(&mut rng);
            if (net.forward(&inputs(&a, None)).keep >= net.red_line) == faults(&a).is_empty() { keep_right += 1; }
        }
        assert!(keep_right >= 1900, "keep agreement {keep_right}/2000");
    }

    /// Another frame of the same burst as `r`: the same scene and people, a
    /// slightly different moment, so the marks and faces differ a little.
    fn sibling(rng: &mut Rng, r: &[f32; READINGS]) -> [f32; READINGS] {
        let mut s = *r;
        let jitter = |rng: &mut Rng, v: f32, by: f32| (v + (rng.next_f32() - 0.5) * 2.0 * by).clamp(0.0, 1.0);
        s[MARK] = jitter(rng, r[MARK], 0.08);
        s[SCORE] = jitter(rng, r[SCORE], 0.08);
        if r[HAS_FACES] > 0.5 {
            s[MIN_SHARP] = jitter(rng, r[MIN_SHARP], 0.1);
            s[MEAN_SHARP] = s[MIN_SHARP].max(jitter(rng, r[MEAN_SHARP], 0.1));
            s[MIN_EYE_SHARE] = jitter(rng, r[MIN_EYE_SHARE], 0.1).max(0.3);
            s[MIN_EYE] = jitter(rng, r[MIN_EYE], 0.1);
            s[FOCUS] = jitter(rng, r[FOCUS], 0.1);
            s[GAZE] = jitter(rng, r[GAZE], 0.1);
            if rng.next_f32() < 0.2 { s[ANY_SHUT] = 1.0 - r[ANY_SHUT]; }
            s[ALL_OPEN] = if s[ANY_SHUT] > 0.5 { 0.0 } else { r[ALL_OPEN] };
            if rng.next_f32() < 0.1 { s[ANY_AWAY] = 1.0 - r[ANY_AWAY]; }
        }
        s[RANK_PLACE] = rng.next_f32();
        s[GAP_TO_BEST] = jitter(rng, r[GAP_TO_BEST], 0.05);
        s[LIGHT_MEAN] = jitter(rng, r[LIGHT_MEAN], 0.02);
        s[DETAIL] = jitter(rng, r[DETAIL], 0.03);
        s
    }

    #[test]
    fn it_orders_the_frames_of_a_burst_as_the_rules_do() {
        // Frames of one burst differ only a little, which is where the order matters.
        let net = starting_net();
        let mut rng = Rng::new(99);
        let (mut pairs, mut right) = (0, 0);
        while pairs < 2000 {
            let a = sample(&mut rng);
            if a[ALONE] > 0.5 { continue; }
            let b = sibling(&mut rng, &a);
            // Ties, and differences too small for anyone to see, prove nothing.
            if (rule_rank(&a) - rule_rank(&b)).abs() < 0.01 { continue; }
            pairs += 1;
            let (pa, pb) = (net.forward(&inputs(&a, None)), net.forward(&inputs(&b, None)));
            if (pa.rank > pb.rank) == (rule_rank(&a) > rule_rank(&b)) { right += 1; }
        }
        assert!(right >= 1960, "same-burst order agreement {right}/2000");
    }

    fn forced(rng: &mut Rng, kind: ShootKind) -> [f32; READINGS] {
        let mut r = sample(rng);
        r[KIND_EVENTS] = 0.0; r[KIND_PORTRAITS] = 0.0; r[KIND_SPORTS] = 0.0;
        r[kind.column()] = 1.0;
        r
    }

    #[test]
    fn it_agrees_with_the_rules_in_every_kind_of_shoot() {
        let net = starting_net();
        let mut rng = Rng::new(7);
        for kind in [ShootKind::Events, ShootKind::Portraits, ShootKind::Sports] {
            let right = (0..600).filter(|_| {
                let r = forced(&mut rng, kind);
                (net.forward(&inputs(&r, None)).keep >= net.red_line) == faults(&r).is_empty()
            }).count();
            assert!(right >= 558, "{} keep agreement {right}/600", kind.label());
        }
    }

    fn keeps(net: &Net, r: &[f32; READINGS]) -> bool { net.forward(&inputs(r, None)).keep >= net.red_line }

    #[test]
    fn the_new_readings_alone_change_the_decision_the_way_the_rules_do() {
        let net = starting_net();
        let base = |kind: usize| {
            let mut r = [0.0; READINGS];
            r[HAS_FACES] = 1.0; r[SUBJECTS] = 0.2; r[HAS_SCORE] = 1.0; r[SCORE] = 0.7; r[MIN_SHARP] = 0.8; r[MEAN_SHARP] = 0.8;
            r[MIN_EYE] = 0.8; r[ALL_OPEN] = 1.0; r[BIGGEST_FACE] = 0.2; r[MARK] = 0.7; r[MIN_EYE_SHARE] = 0.8;
            r[LIGHT_MEAN] = 0.45; r[LIGHT_P05] = 0.1; r[LIGHT_P95] = 0.9; r[ALONE] = 1.0; r[DETAIL] = 0.5;
            r[kind] = 1.0;
            r
        };
        // The same frame, a maybe-turned face: wrong for a posed portrait, fine at an event.
        let mut maybe = base(KIND_PORTRAITS); maybe[ANY_MAYBE_AWAY] = 1.0;
        assert!(!keeps(&net, &maybe), "portrait with a maybe-turned face should be red");
        let mut maybe = base(KIND_EVENTS); maybe[ANY_MAYBE_AWAY] = 1.0;
        assert!(keeps(&net, &maybe), "event with a maybe-turned face should be kept");
        // At an event, one person looking away in a group that is facing the camera is a miss; alone, it is not.
        let mut group = base(KIND_EVENTS); group[ANY_AWAY] = 1.0; group[SUBJECTS] = 0.6; group[GAZE] = 0.8;
        assert!(!keeps(&net, &group), "event group shot with one person looking away should be red");
        let mut lone = base(KIND_EVENTS); lone[ANY_AWAY] = 1.0; lone[SUBJECTS] = 0.2; lone[GAZE] = 0.8;
        assert!(keeps(&net, &lone), "a lone person looking away at an event should be kept");
        // Shut eyes and a turned face do not reject a sports frame.
        let mut sport = base(KIND_SPORTS); sport[ANY_SHUT] = 1.0; sport[MAX_CONCERN] = 1.0; sport[ANY_AWAY] = 1.0;
        assert!(keeps(&net, &sport), "sports frame with shut eyes should be kept");
        let mut event = base(KIND_EVENTS); event[ANY_SHUT] = 1.0; event[MAX_CONCERN] = 1.0; event[MIN_EYE] = 0.1; event[ALL_OPEN] = 0.0;
        assert!(!keeps(&net, &event), "event frame with shut eyes should be red");
        // Exposure against the neighbours, in every kind.
        for kind in [KIND_EVENTS, KIND_PORTRAITS, KIND_SPORTS] {
            let mut dark = base(kind); dark[STOPS_VS_NEIGHBOURS] = -0.8;
            assert!(!keeps(&net, &dark), "frame 2.4 stops under its neighbours should be red");
            let mut same = base(kind); same[STOPS_VS_NEIGHBOURS] = 0.0;
            assert!(keeps(&net, &same), "the same frame matching its neighbours should be kept");
            let mut bright = base(kind); bright[STOPS_VS_NEIGHBOURS] = 0.8; bright[HIGHLIGHTS] = 0.4;
            assert!(!keeps(&net, &bright), "frame 2.4 stops over its neighbours with clipped highlights should be red");
        }
    }

    /// Of fresh samples whose only fault is `fault`: (how many were found, how many the network sends red).
    fn red_share_when_only(fault: Fault, wanted: usize) -> (usize, usize) {
        let net = starting_net();
        let mut rng = Rng::new(31);
        let (mut found, mut red) = (0, 0);
        for _ in 0..400_000 {
            let r = sample(&mut rng);
            if faults(&r) != vec![fault] { continue; }
            found += 1;
            if !keeps(&net, &r) { red += 1; }
            if found == wanted { break; }
        }
        (found, red)
    }

    #[test]
    fn a_frame_that_only_looks_like_a_test_shot_is_red_in_every_kind() {
        let net = starting_net();
        for kind in [KIND_EVENTS, KIND_PORTRAITS, KIND_SPORTS] {
            let mut r = [0.0; READINGS];
            r[kind] = 1.0; r[LIGHT_MEAN] = 0.45; r[LIGHT_P05] = 0.1; r[LIGHT_P95] = 0.9; r[MIN_SHARP] = 1.0; r[MEAN_SHARP] = 1.0; r[MARK] = 0.6;
            // Alone, no subject, almost no detail.
            let mut test = r; test[ALONE] = 1.0; test[DETAIL] = 0.05;
            assert_eq!(faults(&test), vec![Fault::TestShot]);
            assert!(!keeps(&net, &test), "an empty, featureless frame on its own should be red");
            // A sharp subject with detail, alone or not, is kept.
            let mut subject = r; subject[ALONE] = 1.0; subject[DETAIL] = 0.5; subject[HAS_SCORE] = 1.0; subject[SCORE] = 0.7;
            assert!(faults(&subject).is_empty());
            assert!(keeps(&net, &subject), "a sharp subject with detail should be kept");
            subject[ALONE] = 0.0; subject[BURST_SIZE] = 0.4; subject[RANK_PLACE] = 0.3;
            assert!(keeps(&net, &subject), "the same in a burst should be kept");
        }
    }

    #[test]
    fn each_new_fault_on_its_own_is_found_by_the_network() {
        for (fault, percent) in [(Fault::TestShot, 85), (Fault::LookingAway, 85), (Fault::VeryDark, 90), (Fault::BlownOut, 85)] {
            let (found, red) = red_share_when_only(fault, 300);
            assert!(found >= 300, "only {found} samples with {fault:?} as the only fault");
            assert!(red * 100 >= found * percent, "{fault:?} on its own: network red on {red}/{found} (needs {percent}%)");
        }
    }

    #[test]
    fn it_is_the_same_every_time() { assert_eq!(train(), train()); }

    #[test]
    fn the_rankers_reading_counts_only_through_the_rules() {
        let net = starting_net();
        assert!((0..HIDDEN).all(|j| net.params[j * INPUTS + TAUGHT] == 0.0), "its column is exactly 0");
        let mut rng = Rng::new(5);
        for _ in 0..200 {
            let mut low = sample(&mut rng);
            low[TAUGHT] = 0.1;
            let mut high = low;
            high[TAUGHT] = 0.9;
            let (a, b) = (net.forward(&inputs(&low, None)), net.forward(&inputs(&high, None)));
            assert_eq!(a.keep, b.keep);
            // What the network adds to the rules' rank is the same whatever the reading...
            assert!(((b.rank - rule_rank(&high)) - (a.rank - rule_rank(&low))).abs() < 1e-6);
            // ...so at any weight the rank moves by exactly the weight times the reading.
            assert!(((rule_rank_with(&high, 0.3) - rule_rank_with(&low, 0.3)) - 0.3 * 0.8).abs() < 1e-6);
        }
    }

    #[test]
    fn the_starting_network_is_the_same_whatever_the_rankers_weight() {
        let at_zero = train_at(0.0);
        assert_eq!(train_at(0.3), at_zero);
        assert_eq!(train(), at_zero);
    }

    #[test]
    fn samples_carry_the_rankers_reading() {
        let mut rng = Rng::new(3);
        let taught: Vec<f32> = (0..500).map(|_| sample(&mut rng)[TAUGHT]).collect();
        assert!(taught.iter().all(|t| (0.0..1.0).contains(t)));
        assert!(taught.iter().any(|&t| t < 0.2) && taught.iter().any(|&t| t > 0.8));
    }
}
