//! Turns the network's two answers into one judgement per photo: a colour,
//! a place in its burst, and a line saying why.

use serde::Serialize;

use super::features::*;
use super::net::{Net, Pass};
use super::rules::{faults, warnings, Fault};
use super::Prepared;

/// One photo's judgement: its colour, where it stands in its burst, and why.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Judgement {
    pub path: String,
    pub color: &'static str,
    /// Its place in its burst, 1 for the best; none for a photo on its own.
    pub place: Option<u32>,
    pub count: Option<u32>,
    /// An obvious reject: placed after every other frame of its burst.
    pub reject: bool,
    /// One line on why, for the info panel.
    pub why: String,
}

/// 1st, 2nd, 3rd, 4th ... 11th, 12th, 13th ... 21st.
pub fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// A burst's frames best first: every reject after every other frame, each
/// lot in the network's rank order. Equal ranks keep the order given.
fn ranked(passes: &[Pass], red: &[bool]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..passes.len()).collect();
    order.sort_by(|&a, &b| red[a].cmp(&red[b]).then(passes[b].rank.total_cmp(&passes[a].rank)));
    order
}

/// Groups of readings that can be named in a reason.
const NAMED: [(&str, &[usize]); 4] = [
    ("eyes", &[MIN_EYE, ANY_SHUT, MAX_CONCERN, MIN_EYE_SHARE, ALL_OPEN]),
    ("sharpness", &[MIN_SHARP, MEAN_SHARP, FOCUS]),
    ("where people look", &[ANY_AWAY, ANY_MAYBE_AWAY, GAZE]),
    ("light", &[LIGHT_MEAN, LIGHT_P05, LIGHT_P95, SHADOWS, HIGHLIGHTS]),
];

fn why_behind(net: &Net, this: &Prepared, best: &Prepared) -> String {
    let base = net.forward(&this.x).rank;
    let mut gains: Vec<(f32, &str)> = NAMED.iter().map(|(name, idx)| {
        let mut x = this.x.clone();
        for &i in *idx { x[i] = best.x[i]; }
        (net.forward(&x).rank - base, *name)
    }).filter(|(g, _)| *g > 0.0).collect();
    gains.sort_by(|a, b| b.0.total_cmp(&a.0));
    match gains.as_slice() {
        [] => "the overall mark".into(),
        [(_, a)] => (*a).into(),
        [(_, a), (_, b), ..] => format!("{a} and {b}"),
    }
}

/// How far a frame is from its neighbours' light, in words.
fn stops_words(reading: f32) -> String {
    let stops = (reading * 3.0).abs();
    if stops >= 2.95 { "3 or more stops".into() } else { format!("{stops:.1} stops") }
}

/// A fault in words, with the stops for an exposure fault.
fn phrase(f: Fault, r: &[f32; READINGS]) -> String {
    match f {
        Fault::Underexposed => format!("underexposed, {} darker than the shots around it", stops_words(r[STOPS_VS_NEIGHBOURS])),
        Fault::BlownOut => format!("blown out, {} brighter than the shots around it", stops_words(r[STOPS_VS_NEIGHBOURS])),
        _ => f.words().to_string(),
    }
}

/// The faults in words. The first one that counts because of the kind of
/// shoot also names the kind, once.
fn words_of(found: &[Fault], r: &[f32; READINGS]) -> Option<String> {
    if found.is_empty() { return None; }
    let kind = ShootKind::of(r);
    let mut named = false;
    let parts: Vec<String> = found.iter().map(|f| {
        if f.by_kind() && !named {
            named = true;
            format!("{} ({})", phrase(*f, r), kind.label())
        } else {
            phrase(*f, r)
        }
    }).collect();
    Some(parts.join(", "))
}

fn fault_words(p: &Prepared) -> Option<String> {
    words_of(&faults(&p.readings), &p.readings)
}

fn warning_words(p: &Prepared) -> Option<String> {
    words_of(&warnings(&p.readings), &p.readings)
}

pub fn decide(net: &Net, groups: &[Vec<Prepared>]) -> Vec<Judgement> {
    let mut out = Vec::new();
    for group in groups.iter().filter(|g| !g.is_empty()) {
        let passes: Vec<Pass> = group.iter().map(|p| net.forward(&p.x)).collect();
        let red: Vec<bool> = passes.iter().map(|p| p.keep < net.red_line).collect();
        let reject_why = |i: usize| format!("Likely reject: {}", fault_words(&group[i]).unwrap_or_else(|| "unlike the photos you keep".into()));
        if group.len() == 1 {
            let (color, why) = if red[0] { ("red", reject_why(0)) } else { ("none", "Stands alone: no faults found".to_string()) };
            out.push(Judgement { path: group[0].path.clone(), color, place: None, count: None, reject: red[0], why });
            continue;
        }
        let order = ranked(&passes, &red);
        let (first, n) = (order[0], group.len());
        for (place, &i) in order.iter().enumerate() {
            let (color, why) = if place == 0 {
                let warned = !warnings(&group[i].readings).is_empty();
                if red[i] || warned {
                    let why = fault_words(&group[i]).or_else(|| warning_words(&group[i])).unwrap_or_else(|| "unlike the photos you keep".into());
                    ("purple", format!("Best of {n}, but {why}"))
                } else {
                    // What the best has that the runner-up lacks.
                    let ahead = why_behind(net, &group[order[1]], &group[i]);
                    ("green", format!("Best of {n}: ahead mostly on {ahead} ({})", ShootKind::of(&group[i].readings).label()))
                }
            } else if red[i] {
                ("red", reject_why(i))
            } else if passes[first].rank - passes[i].rank <= net.blue_margin {
                ("blue", format!("{} of {n}: close behind the best", ordinal(place + 1)))
            } else {
                ("yellow", format!("{} of {n}: behind the best mostly on {}", ordinal(place + 1), why_behind(net, &group[i], &group[first])))
            };
            out.push(Judgement { path: group[i].path.clone(), color, place: Some(place as u32 + 1), count: Some(n as u32), reject: red[i], why });
        }
    }
    out
}

/// For groups that hold a 5-star frame: the share where it is placed first,
/// and the share where it is in the top two, in the order `decide` places them.
pub fn picks(net: &Net, groups: &[Vec<Prepared>]) -> (f32, f32) {
    let (mut groups_with_pick, mut first, mut top2) = (0, 0, 0);
    for group in groups.iter().filter(|g| g.len() > 1) {
        let Some(pick) = group.iter().position(|p| p.rating == 5) else { continue };
        groups_with_pick += 1;
        let passes: Vec<Pass> = group.iter().map(|p| net.forward(&p.x)).collect();
        let red: Vec<bool> = passes.iter().map(|p| p.keep < net.red_line).collect();
        let place = ranked(&passes, &red).iter().position(|&i| i == pick).unwrap();
        if place == 0 { first += 1; }
        if place <= 1 { top2 += 1; }
    }
    if groups_with_pick == 0 { return (0.0, 0.0); }
    (first as f32 / groups_with_pick as f32, top2 as f32 / groups_with_pick as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cull::starter::starting_net;
    use crate::cull::Prepared;

    fn frame_with(name: &str, mark: f32, shut: bool, kind: ShootKind) -> Prepared {
        let mut r = [0.0; READINGS];
        r[HAS_FACES] = 1.0; r[MIN_SHARP] = 0.8; r[MEAN_SHARP] = 0.8; r[MARK] = mark; r[SCORE] = mark; r[HAS_SCORE] = 1.0;
        r[LIGHT_MEAN] = 0.45; r[LIGHT_P95] = 0.9; r[kind.column()] = 1.0; r[DETAIL] = 0.5; r[ANY_SHUT] = if shut { 1.0 } else { 0.0 }; r[MAX_CONCERN] = if shut { 1.0 } else { 0.0 };
        Prepared { path: name.into(), version: "v".into(), readings: r, x: inputs(&r, None), rating: 0, flag: 0 }
    }

    fn frame(name: &str, mark: f32, shut: bool) -> Prepared { frame_with(name, mark, shut, ShootKind::Events) }

    fn frame_kind(name: &str, mark: f32, kind: ShootKind) -> Prepared { frame_with(name, mark, false, kind) }

    fn find<'a>(d: &'a [Judgement], path: &str) -> &'a Judgement { d.iter().find(|d| d.path == path).unwrap() }
    fn why<'a>(d: &'a [Judgement], path: &str) -> &'a str { &find(d, path).why }
    fn colour<'a>(d: &'a [Judgement], path: &str) -> &'a str { find(d, path).color }

    #[test]
    fn a_burst_gets_a_pick_close_seconds_and_repeats_in_order() {
        let net = starting_net();
        let d = decide(&net, &[vec![frame("a", 0.80, false), frame("b", 0.77, false), frame("c", 0.50, false)]]);
        assert_eq!((colour(&d, "a"), colour(&d, "b"), colour(&d, "c")), ("green", "blue", "yellow"));
        assert_eq!((find(&d, "a").place, find(&d, "b").place, find(&d, "c").place), (Some(1), Some(2), Some(3)));
        assert!(d.iter().all(|j| j.count == Some(3) && !j.reject));
        assert!(why(&d, "a").starts_with("Best of 3: ahead mostly on ") && why(&d, "a").ends_with(" (Events)"), "{}", why(&d, "a"));
        assert_eq!(why(&d, "b"), "2nd of 3: close behind the best");
        assert!(why(&d, "c").starts_with("3rd of 3: behind the best mostly on "), "{}", why(&d, "c"));
    }

    #[test]
    fn rejects_come_after_every_other_frame_whatever_their_rank() {
        let pass = |keep: f32, rank: f32| crate::cull::net::Pass { keep, rank, hidden: Vec::new() };
        // b ranks highest but is a reject; it goes last, after a and c in rank order.
        let passes = [pass(1.0, 0.5), pass(-5.0, 0.9), pass(1.0, 0.7)];
        let red = [false, true, false];
        assert_eq!(ranked(&passes, &red), vec![2, 0, 1]);
        // Every frame a reject: rank order alone.
        assert_eq!(ranked(&passes, &[true, true, true]), vec![1, 2, 0]);
    }

    #[test]
    fn a_reject_is_red_last_and_says_why() {
        let net = starting_net();
        let d = decide(&net, &[vec![frame("a", 0.8, false), frame("b", 0.7, true), frame("c", 0.6, false)]]);
        let b = find(&d, "b");
        assert_eq!((b.color, b.place, b.reject), ("red", Some(3), true));
        assert_eq!(b.why, "Likely reject: eyes shut (Events)");
    }

    #[test]
    fn a_burst_where_every_frame_fails_gets_a_purple_best_that_is_still_a_reject() {
        let net = starting_net();
        let d = decide(&net, &[vec![frame("a", 0.8, true), frame("b", 0.6, true)]]);
        assert_eq!((colour(&d, "a"), find(&d, "a").reject, find(&d, "a").place), ("purple", true, Some(1)));
        assert_eq!(why(&d, "a"), "Best of 2, but eyes shut (Events)");
        assert_eq!(colour(&d, "b"), "red");
    }

    #[test]
    fn a_frame_on_its_own_has_no_place_and_is_marked_only_if_it_fails() {
        let net = starting_net();
        let d = decide(&net, &[vec![frame("a", 0.6, false)], vec![frame("b", 0.6, true)]]);
        let (a, b) = (find(&d, "a"), find(&d, "b"));
        assert_eq!((a.color, a.place, a.count, a.reject, a.why.as_str()), ("none", None, None, false, "Stands alone: no faults found"));
        assert_eq!((b.color, b.place, b.reject, b.why.as_str()), ("red", None, true, "Likely reject: eyes shut (Events)"));
    }

    #[test]
    fn pick_agreement_counts_the_five_star_frame() {
        let net = starting_net();
        let mut best = frame("a", 0.8, false); best.rating = 5;
        let (first, top2) = picks(&net, &[vec![best, frame("b", 0.5, false)]]);
        assert_eq!((first, top2), (1.0, 1.0));
    }

    #[test]
    fn in_sports_a_best_frame_with_shut_eyes_is_purple_but_not_a_reject() {
        let net = starting_net();
        let mut a = frame_kind("a", 0.85, ShootKind::Sports); a.readings[ANY_SHUT] = 1.0; a.x = inputs(&a.readings, None);
        let b = frame_kind("b", 0.60, ShootKind::Sports);
        let d = decide(&net, &[vec![a, b]]);
        assert_eq!((colour(&d, "a"), find(&d, "a").reject), ("purple", false));
        assert_eq!(why(&d, "a"), "Best of 2, but eyes shut (Sports)");
        assert_ne!(colour(&d, "b"), "red");
    }

    #[test]
    fn a_reason_from_a_type_rule_names_the_type() {
        let net = starting_net();
        let mut b = frame_kind("b", 0.7, ShootKind::Portraits); b.readings[ANY_MAYBE_AWAY] = 1.0; b.x = inputs(&b.readings, None);
        let d = decide(&net, &[vec![frame_kind("a", 0.8, ShootKind::Portraits), b]]);
        assert_eq!(why(&d, "b"), "Likely reject: looking away (Portraits)");
    }

    #[test]
    fn exposure_reasons_give_the_stops_and_no_type() {
        let net = starting_net();
        let mut a = frame_kind("a", 0.6, ShootKind::Events); a.readings[STOPS_VS_NEIGHBOURS] = -0.6; a.x = inputs(&a.readings, None);
        let mut b = frame_kind("b", 0.6, ShootKind::Events); b.readings[STOPS_VS_NEIGHBOURS] = -1.0; b.x = inputs(&b.readings, None);
        let mut c = frame_kind("c", 0.6, ShootKind::Events); c.readings[STOPS_VS_NEIGHBOURS] = 0.6; c.readings[HIGHLIGHTS] = 0.4; c.x = inputs(&c.readings, None);
        let d = decide(&net, &[vec![a], vec![b], vec![c]]);
        assert_eq!(why(&d, "a"), "Likely reject: underexposed, 1.8 stops darker than the shots around it");
        assert_eq!(why(&d, "b"), "Likely reject: underexposed, 3 or more stops darker than the shots around it");
        assert_eq!(why(&d, "c"), "Likely reject: blown out, 1.8 stops brighter than the shots around it");
    }

    #[test]
    fn an_empty_group_is_passed_over() {
        assert!(decide(&starting_net(), &[Vec::new()]).is_empty());
    }

    #[test]
    fn ordinals_read_as_english() {
        let got: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 111, 112].iter().map(|&n| ordinal(n)).collect();
        assert_eq!(got, ["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "23rd", "111th", "112th"]);
    }
}
