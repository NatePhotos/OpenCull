//! Teaching the network the photographer's taste, on this computer. Every
//! retraining is held close to the network it starts from, and checked on
//! bursts it did not learn from before it replaces anything.
//!
//! Keepers and rejects count equally everywhere: in training, in where the red
//! line goes, and in the check. Most photographers keep several photos for
//! each one they reject, and counting photos would let the keepers win and
//! the red line slide until nothing is marked a reject.
//!
//! The check works on the newest shoot: its bursts are split into two halves,
//! and each half is tested by a network trained on the older shoots plus the
//! other half. On each half the new network must catch at least as many of
//! the rejects, judge keepers and rejects (counted equally) at least as well,
//! and order the frames of bursts at least as well as the current one, and do
//! better than a coin flip overall. That validates the way of training on
//! bursts it has not seen; it does not score the network that finally gets
//! installed, which is trained on everything.

use serde::{Deserialize, Serialize};

use super::lessons::{Lesson, Shoot, Taught};
use super::net::{nonzero, sigmoid, Adam, Net};

/// Fewest scored lessons (keeps and preferences) a held-out half needs before its verdict is trusted.
pub const MIN_CHECK: usize = 10;

/// Why a retrained network was not kept, though there was enough to check it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// On part of the shoot it had not seen, it judged keepers and rejects, or the frames of a burst, less well.
    Worse,
    /// On part of the shoot it had not seen, it caught fewer of the rejects.
    FewerRejects,
    /// On part of the shoot it had not seen, it did no better than a coin flip.
    Chance,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Learned {
    /// Shoots learned from, set-aside ones left out.
    pub shoots: usize,
    /// How well the current network and the retrained one did on the bursts
    /// of the newest shoot each had not seen, 0 to 1, keepers, rejects and
    /// burst order counting equally. When there was too little to check,
    /// nothing is retrained and both are the current network's figure on the whole shoot.
    pub before: f32,
    pub after: f32,
    pub accepted: bool,
    /// There were enough rated (or flagged) photos to check the result, with
    /// something to compare in each half.
    pub enough: bool,
    /// Why it was not kept, when it was checked and not kept.
    pub reason: Option<Reason>,
    /// On those same unseen bursts, for bursts with a pick (5★ or flagged): the share
    /// where the network in use ranks it first, and in the top two. None when
    /// no such burst was checked.
    pub pick_first: Option<f32>,
    pub pick_top2: Option<f32>,
}

/// Trains a copy of `start` on the lessons, pulled back toward `start` so one
/// shoot cannot throw away what it already knew. Keeps and rejects are
/// weighted to count equally; stars of only one kind say nothing about where
/// the red line belongs, so they are left out.
pub fn fit(start: &Net, lessons: &[Lesson], pull: f32, epochs: usize) -> Net {
    enum Sparse { Keep(Vec<(usize, f32)>, bool), Prefer(Vec<(usize, f32)>, Vec<(usize, f32)>) }
    let mut net = start.clone();
    let nk = lessons.iter().filter(|l| matches!(l, Lesson::Keep { keep: true, .. })).count() as f32;
    let nr = lessons.iter().filter(|l| matches!(l, Lesson::Keep { keep: false, .. })).count() as f32;
    let balanced = nk > 0.0 && nr > 0.0;
    let (w_keep, w_reject) = if balanced { ((nk + nr) / (2.0 * nk), (nk + nr) / (2.0 * nr)) } else { (0.0, 0.0) };
    // Each photo's inputs reduced once to the ones that are not zero.
    let mut trained: Vec<(Sparse, f32)> = Vec::new();
    for lesson in lessons {
        match lesson {
            Lesson::Keep { x, keep } if balanced => trained.push((Sparse::Keep(nonzero(x), *keep), if *keep { w_keep } else { w_reject })),
            Lesson::Prefer { better, worse } => trained.push((Sparse::Prefer(nonzero(better), nonzero(worse)), 1.0)),
            _ => {}
        }
    }
    if trained.is_empty() { return net; }
    let total: f32 = trained.iter().map(|t| t.1).sum();
    let mut adam = Adam::new(net.params.len());
    let mut grads = vec![0.0; net.params.len()];
    for _ in 0..epochs {
        grads.iter_mut().for_each(|g| *g = 0.0);
        for (lesson, w) in &trained {
            match lesson {
                Sparse::Keep(x, keep) => {
                    let pass = net.forward_sparse(x);
                    let d = sigmoid(pass.keep) - if *keep { 1.0 } else { 0.0 };
                    net.backward_sparse(x, &pass, w * d / total, 0.0, &mut grads);
                }
                Sparse::Prefer(better, worse) => {
                    let (pb, pw) = (net.forward_sparse(better), net.forward_sparse(worse));
                    let d = -sigmoid(-(pb.rank - pw.rank));
                    net.backward_sparse(better, &pb, 0.0, w * d / total, &mut grads);
                    net.backward_sparse(worse, &pw, 0.0, -w * d / total, &mut grads);
                }
            }
        }
        for (i, g) in grads.iter_mut().enumerate() { *g += 2.0 * pull * (net.params[i] - start.params[i]); }
        adam.step(&mut net.params, &grads, 0.005);
    }
    net
}

/// What a network gets right on some lessons, each kind measured on its own,
/// so that the many keepers of a shoot cannot hide what happens to its few rejects.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Check {
    /// Share of the keepers (4–5★ or picked) at or above the red line.
    pub keeps: Option<f32>,
    /// Share of the rejects (1–2★) below it.
    pub rejects: Option<f32>,
    /// Share of burst picks ranked above a frame they were preferred to.
    pub order: Option<f32>,
}

fn mean(a: Option<f32>, b: Option<f32>) -> Option<f32> {
    match (a, b) {
        (Some(a), Some(b)) => Some((a + b) / 2.0),
        (one, None) | (None, one) => one,
    }
}

impl Check {
    /// Keepers and rejects counted equally, however many there are of each.
    pub fn stars(&self) -> Option<f32> { mean(self.keeps, self.rejects) }
    /// The stars and the order within bursts counted equally.
    pub fn combined(&self) -> Option<f32> { mean(self.stars(), self.order) }
}

pub fn check<'a>(net: &Net, lessons: impl IntoIterator<Item = &'a Lesson>) -> Check {
    let share = |right: usize, of: usize| (of > 0).then(|| right as f32 / of as f32);
    let (mut keeps, mut kept, mut rejects, mut rejected, mut pairs, mut ordered) = (0, 0, 0, 0, 0, 0);
    for lesson in lessons {
        match lesson {
            Lesson::Keep { x, keep: true } => { keeps += 1; if net.forward(x).keep >= net.red_line { kept += 1; } }
            Lesson::Keep { x, keep: false } => { rejects += 1; if net.forward(x).keep < net.red_line { rejected += 1; } }
            Lesson::Prefer { better, worse } => { pairs += 1; if net.forward(better).rank > net.forward(worse).rank { ordered += 1; } }
            Lesson::Behind { .. } => {}
        }
    }
    Check { keeps: share(kept, keeps), rejects: share(rejected, rejects), order: share(ordered, pairs) }
}

/// Whether a retrained network may replace the current one on a half it did
/// not see. Each figure the half can measure must hold, and together they
/// must beat a coin flip: two networks that both get a half wrong must not tie their way in.
pub fn verdict(candidate: &Check, current: &Check) -> Result<(), Reason> {
    if candidate.combined().map_or(true, |c| c < FLOOR) { return Err(Reason::Chance); }
    let fell = |c: Option<f32>, n: Option<f32>| matches!((c, n), (Some(c), Some(n)) if c < n);
    if fell(candidate.rejects, current.rejects) { return Err(Reason::FewerRejects); }
    if fell(candidate.stars(), current.stars()) || fell(candidate.order, current.order) { return Err(Reason::Worse); }
    Ok(())
}

/// Whether there is anything to train on: a preference between two frames,
/// or keepers and rejects both (one kind alone says nothing about where the line is).
pub fn trainable<'a>(lessons: impl IntoIterator<Item = &'a Lesson>) -> bool {
    let (mut keeps, mut rejects) = (false, false);
    for l in lessons {
        match l {
            Lesson::Prefer { .. } => return true,
            Lesson::Keep { keep: true, .. } => keeps = true,
            Lesson::Keep { keep: false, .. } => rejects = true,
            Lesson::Behind { .. } => {}
        }
    }
    keeps && rejects
}

fn scored(lessons: &[&Taught]) -> usize {
    lessons.iter().filter(|t| !matches!(t.lesson, Lesson::Behind { .. })).count()
}

/// The cut that does best, trying one below and one above every score too.
/// `right` says how well a cut does, as a whole number so ties are exact.
/// Among equally good cuts, the middle of the first longest run of them.
fn best_cut(scores: &[f32], right: impl Fn(f32) -> u64) -> Option<f32> {
    let mut sorted = scores.to_vec();
    sorted.sort_by(f32::total_cmp);
    let (lo, hi) = (*sorted.first()?, *sorted.last()?);
    let mut cuts = vec![lo - 1e-3];
    cuts.extend(sorted.windows(2).map(|w| (w[0] + w[1]) / 2.0));
    cuts.push(hi + 1e-3);
    let counts: Vec<u64> = cuts.iter().map(|c| right(*c)).collect();
    let top = *counts.iter().max()?;
    let (mut best_start, mut best_len, mut i) = (0, 0, 0);
    while i < counts.len() {
        if counts[i] == top {
            let start = i;
            while i < counts.len() && counts[i] == top { i += 1; }
            if i - start > best_len { best_start = start; best_len = i - start; }
        } else {
            i += 1;
        }
    }
    Some(cuts[best_start + best_len / 2])
}

/// How well a cut sorts scored photos into their two kinds, each kind counting
/// equally however many there are of it: the share of each kind put on its
/// right side, added, scaled to a whole number (right_a × n_b + right_b × n_a).
fn balanced(scored: &[(f32, bool)], on_true_side: impl Fn(f32) -> bool) -> u64 {
    let n_true = scored.iter().filter(|s| s.1).count() as u64;
    let n_false = scored.len() as u64 - n_true;
    let right_true = scored.iter().filter(|s| s.1 && on_true_side(s.0)).count() as u64;
    let right_false = scored.iter().filter(|s| !s.1 && !on_true_side(s.0)).count() as u64;
    right_true * n_false + right_false * n_true
}

/// Moves the red line and the blue margin to where they best match the stars,
/// keepers and rejects (and blue and yellow frames) counting equally.
pub fn refit(net: &mut Net, lessons: &[Lesson]) {
    let keeps: Vec<(f32, bool)> = lessons.iter().filter_map(|l| match l { Lesson::Keep { x, keep } => Some((net.forward(x).keep, *keep)), _ => None }).collect();
    if keeps.iter().any(|k| k.1) && keeps.iter().any(|k| !k.1) {
        let scores: Vec<f32> = keeps.iter().map(|k| k.0).collect();
        if let Some(cut) = best_cut(&scores, |cut| balanced(&keeps, |z| z >= cut)) {
            net.red_line = cut;
        }
    }
    let gaps: Vec<(f32, bool)> = lessons.iter().filter_map(|l| match l { Lesson::Behind { best, this, blue } => Some((net.forward(best).rank - net.forward(this).rank, *blue)), _ => None }).collect();
    if gaps.iter().any(|g| g.1) && gaps.iter().any(|g| !g.1) {
        let scores: Vec<f32> = gaps.iter().map(|g| g.0).collect();
        if let Some(cut) = best_cut(&scores, |cut| balanced(&gaps, |gap| gap <= cut)) {
            net.blue_margin = cut.max(0.0);
        }
    }
}

/// Better than a coin flip.
const FLOOR: f32 = 0.5;
/// How strongly a retraining is held to the network it starts from, and how long it trains.
pub const PULL: f32 = 1e-3;
pub const EPOCHS: usize = 300;

/// Bursts holding a pick (5★ or flagged), and in how many the network ranks it first, and in the top two.
#[derive(Clone, Copy, Debug, Default)]
struct Picks { bursts: usize, first: usize, top2: usize }

impl Picks {
    fn add(self, o: Picks) -> Picks { Picks { bursts: self.bursts + o.bursts, first: self.first + o.first, top2: self.top2 + o.top2 } }
    fn shares(self) -> (Option<f32>, Option<f32>) {
        if self.bursts == 0 { return (None, None); }
        let n = self.bursts as f32;
        (Some(self.first as f32 / n), Some(self.top2 as f32 / n))
    }
}

/// Where each burst's first pick (5★ or flagged) stands against the frames it was preferred to.
fn picks_in(net: &Net, half: &[&Taught]) -> Picks {
    let mut seen: Vec<u32> = Vec::new();
    let mut out = Picks::default();
    for t in half {
        let Lesson::Prefer { better, .. } = &t.lesson else { continue };
        if seen.contains(&t.group) { continue; }
        seen.push(t.group);
        let pick = net.forward(better).rank;
        let ahead = half.iter()
            .filter(|o| o.group == t.group)
            .filter(|o| matches!(&o.lesson, Lesson::Prefer { better: b, worse } if b == better && net.forward(worse).rank > pick))
            .count();
        out.bursts += 1;
        if ahead == 0 { out.first += 1; }
        if ahead <= 1 { out.top2 += 1; }
    }
    out
}

/// One fold: train on the older lessons plus one half of the newest shoot, test on the other half.
struct Fold { candidate: Check, current: Check, tested: usize, picks: [Picks; 2] }

fn fold(current: &Net, older: &[Lesson], train_half: &[&Taught], test_half: &[&Taught]) -> Fold {
    let mut on: Vec<Lesson> = older.to_vec();
    on.extend(train_half.iter().map(|t| t.lesson.clone()));
    let mut candidate = fit(current, &on, PULL, EPOCHS);
    refit(&mut candidate, &on);
    let tested = test_half.iter().map(|t| &t.lesson);
    Fold {
        candidate: check(&candidate, tested.clone()),
        current: check(current, tested),
        tested: scored(test_half),
        picks: [picks_in(&candidate, test_half), picks_in(current, test_half)],
    }
}

/// Learns from the shoots, oldest first, and says whether to keep the result.
/// Shoots that were set aside are ignored. The newest shoot's bursts are split
/// in two; each half is predicted by a network that never saw it, and the new
/// network is kept only if it passes `verdict` on both halves, with enough
/// rated photos in each half for that to mean anything. Without enough,
/// nothing is trained.
pub fn learn(current: &Net, shoots: &[Shoot]) -> (Net, Learned) {
    let usable: Vec<&Shoot> = shoots.iter().filter(|s| !s.set_aside).collect();
    let Some((newest, rest)) = usable.split_last() else {
        return (current.clone(), Learned { shoots: 0, before: 0.5, after: 0.5, accepted: false, enough: false, reason: None, pick_first: None, pick_top2: None });
    };
    let mut groups: Vec<u32> = newest.lessons.iter().map(|t| t.group).collect();
    groups.sort_unstable();
    groups.dedup();
    let (mut a, mut b): (Vec<&Taught>, Vec<&Taught>) = (Vec::new(), Vec::new());
    for t in &newest.lessons {
        let first = groups.len() < 2 || groups.iter().position(|g| *g == t.group).unwrap_or(0) % 2 == 0;
        if first { a.push(t) } else { b.push(t) }
    }
    let enough = scored(&a) >= MIN_CHECK && scored(&b) >= MIN_CHECK && trainable(a.iter().map(|t| &t.lesson)) && trainable(b.iter().map(|t| &t.lesson));
    if !enough {
        let before = check(current, newest.lessons.iter().map(|t| &t.lesson)).combined().unwrap_or(0.5);
        return (current.clone(), Learned { shoots: usable.len(), before, after: before, accepted: false, enough, reason: None, pick_first: None, pick_top2: None });
    }

    let older: Vec<Lesson> = rest.iter().flat_map(|s| s.lessons.iter().map(|t| t.lesson.clone())).collect();
    let folds = [fold(current, &older, &a, &b), fold(current, &older, &b, &a)];
    let reason = folds.iter().filter_map(|f| verdict(&f.candidate, &f.current).err()).max();
    let accepted = reason.is_none();

    let weight: f32 = folds.iter().map(|f| f.tested as f32).sum();
    let weighted = |of: fn(&Fold) -> Check| folds.iter().map(|f| of(f).combined().unwrap_or(0.5) * f.tested as f32).sum::<f32>() / weight;
    let (before, after) = (weighted(|f| f.current), weighted(|f| f.candidate));
    let in_use = if accepted { 0 } else { 1 };
    let (pick_first, pick_top2) = folds.iter().fold(Picks::default(), |sum, f| sum.add(f.picks[in_use])).shares();

    let net = if accepted {
        let mut on = older;
        on.extend(newest.lessons.iter().map(|t| t.lesson.clone()));
        let mut n = fit(current, &on, PULL, EPOCHS);
        refit(&mut n, &on);
        n
    } else {
        current.clone()
    };
    (net, Learned { shoots: usable.len(), before, after, accepted, enough, reason, pick_first, pick_top2 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cull::features::{inputs, FORMAT, HAS_FACES, SHADOWS, HIGHLIGHTS, KIND_EVENTS, KIND_PORTRAITS, KIND_SPORTS, LIGHT_MEAN, READINGS, MARK, ANY_SHUT, STOPS_VS_NEIGHBOURS};
    use crate::cull::lessons::{Lesson, Shoot, Taught};
    use crate::cull::net::Rng;
    use crate::cull::rules::faults;
    use crate::cull::starter::{sample, starting_net};

    fn x(mark: f32, shut: f32, light: f32) -> Vec<f32> {
        let mut r = [0.5; READINGS];
        r[KIND_PORTRAITS] = 0.0; r[KIND_SPORTS] = 0.0; r[KIND_EVENTS] = 1.0; r[STOPS_VS_NEIGHBOURS] = 0.0; r[HIGHLIGHTS] = 0.0;
        // A photo of people with little crushed to black, as real photos are, so
        // the starting network is judged where it was taught.
        r[HAS_FACES] = 1.0; r[SHADOWS] = 0.02;
        r[MARK] = mark; r[ANY_SHUT] = shut; r[LIGHT_MEAN] = light;
        inputs(&r, None)
    }

    // A photographer who rejects every photo darker than 0.35, which the starting rules allow.
    fn taste(n: usize, seed: u32, dark_is_bad: bool) -> Vec<Lesson> {
        (0..n).map(|i| {
            let light = ((i as u32 * 37 + seed) % 100) as f32 / 100.0;
            Lesson::Keep { x: x(0.7, 0.0, light), keep: (light >= 0.35) == dark_is_bad }
        }).collect()
    }
    fn dark_hater(n: usize, seed: u32) -> Vec<Lesson> { taste(n, seed, true) }

    // Several bursts per shoot, so a shoot can be tested on bursts it was not trained on.
    fn shoot(folder: &str, at: u64, lessons: Vec<Lesson>) -> Shoot {
        let lessons = lessons.into_iter().enumerate().map(|(i, lesson)| Taught { group: (i % 8) as u32, lesson }).collect();
        Shoot { folder: folder.into(), format: FORMAT, taught_at: at, lessons, set_aside: false, accepted: false }
    }

    #[test]
    fn training_moves_toward_the_lessons() {
        let start = starting_net();
        let lessons = dark_hater(200, 1);
        let before = check(&start, &lessons).stars().unwrap();
        let after = check(&fit(&start, &lessons, 1e-3, 300), &lessons).stars().unwrap();
        assert!(after > before + 0.1, "{before} -> {after}");
    }

    #[test]
    fn the_pull_keeps_a_strong_pull_close_to_the_start() {
        let start = starting_net();
        let lessons = dark_hater(200, 1);
        let loose = fit(&start, &lessons, 0.0, 300);
        let tight = fit(&start, &lessons, 10.0, 300);
        let dist = |n: &Net| n.params.iter().zip(&start.params).map(|(a, b)| (a - b).powi(2)).sum::<f32>();
        assert!(dist(&tight) < dist(&loose) * 0.5);
    }

    #[test]
    fn a_bad_newest_shoot_is_rejected_even_when_the_network_has_already_learned() {
        let start = starting_net();
        let a = shoot("/a", 1, dark_hater(400, 1));
        let (current, first) = learn(&start, &[a.clone()]);
        assert!(first.accepted && first.enough, "{first:?}");
        let opposite = shoot("/b", 2, taste(400, 3, false));
        let (kept, learned) = learn(&current, &[a, opposite]);
        assert!(!learned.accepted && learned.reason.is_some(), "{learned:?}");
        assert_eq!(kept, current);
    }

    #[test]
    fn a_consistent_photographer_is_learned_and_kept() {
        let start = starting_net();
        let a = shoot("/a", 1, dark_hater(200, 1));
        let b = shoot("/b", 2, dark_hater(200, 5));
        let (net, learned) = learn(&start, &[a, b]);
        assert!(learned.accepted && learned.enough && learned.after >= learned.before, "{learned:?}");
        assert_ne!(net, start);
    }

    #[test]
    fn a_shoot_of_keepers_alone_has_nothing_to_compare_and_is_not_enough() {
        let start = starting_net();
        let keepers: Vec<Lesson> = (0..60).map(|i| Lesson::Keep { x: x(0.7, 0.0, (i % 100) as f32 / 100.0), keep: true }).collect();
        let (kept, learned) = learn(&start, &[shoot("/a", 1, keepers)]);
        assert!(!learned.enough && !learned.accepted, "{learned:?}");
        assert_eq!(kept, start);
    }

    #[test]
    fn too_few_rated_photos_is_not_accepted() {
        let start = starting_net();
        let (kept, learned) = learn(&start, &[shoot("/a", 1, dark_hater(200, 1)), shoot("/b", 2, dark_hater(6, 5))]);
        assert!(!learned.enough && !learned.accepted && learned.reason.is_none(), "{learned:?}");
        assert_eq!(kept, start);
    }

    #[test]
    fn a_set_aside_shoot_is_ignored() {
        let start = starting_net();
        let a = shoot("/a", 1, dark_hater(200, 1));
        let mut bad = shoot("/b", 2, taste(200, 3, false));
        bad.set_aside = true;
        let c = shoot("/c", 3, dark_hater(200, 5));
        let (with, _) = learn(&start, &[a.clone(), bad, c.clone()]);
        let (without, _) = learn(&start, &[a, c]);
        assert_eq!(with, without);
    }

    #[test]
    fn keep_only_stars_cannot_switch_off_rejects() {
        let start = starting_net();
        let sample = x(0.2, 1.0, 0.0);
        assert!(start.forward(&sample).keep < start.red_line, "the sample should start out red");
        let mut lessons: Vec<Lesson> = (0..100).map(|i| Lesson::Keep { x: x(0.7, 0.0, (i % 100) as f32 / 100.0), keep: true }).collect();
        lessons.extend((0..100).map(|i| {
            let light = (i % 50) as f32 / 100.0 + 0.3;
            Lesson::Prefer { better: x(0.8, 0.0, light), worse: x(0.4, 0.0, light) }
        }));
        let (net, learned) = learn(&start, &[shoot("/a", 1, lessons.clone()), shoot("/b", 2, lessons)]);
        assert!(learned.enough && learned.pick_first.is_some(), "{learned:?}");
        assert!(net.forward(&sample).keep < net.red_line, "{} vs {}", net.forward(&sample).keep, net.red_line);
    }

    // A photographer who keeps far more than they reject, as most do (about
    // 3.4 to 1). What the starting rules flag is a little more common among
    // their rejects (60%) than among their keeps (40%), so a red line placed
    // to get the most photos right would wave every flagged photo through.
    fn skewed(seed: u64, keeps: usize, rejects: usize) -> Vec<Lesson> {
        let mut rng = Rng::new(seed);
        let quota = |n: usize, flagged: f32| ((n as f32 * flagged).round() as usize, n - (n as f32 * flagged).round() as usize);
        let (mut keep_quota, mut reject_quota) = (quota(keeps, 0.4), quota(rejects, 0.6));
        let (mut k, mut r) = (Vec::new(), Vec::new());
        while keep_quota != (0, 0) || reject_quota != (0, 0) {
            let s = sample(&mut rng);
            let flagged = !faults(&s).is_empty();
            let x = inputs(&s, None);
            let take = |q: &mut (usize, usize)| { let slot = if flagged { &mut q.0 } else { &mut q.1 }; if *slot > 0 { *slot -= 1; true } else { false } };
            if rng.next_f32() < 0.5 { if take(&mut keep_quota) { k.push(Lesson::Keep { x, keep: true }) } } else if take(&mut reject_quota) { r.push(Lesson::Keep { x, keep: false }) }
        }
        // Mixed through the shoot, as they would be.
        let mut out = Vec::new();
        let mut r = r.into_iter();
        for (i, l) in k.into_iter().enumerate() {
            out.push(l);
            if i % 3 == 2 { if let Some(l) = r.next() { out.push(l) } }
        }
        out.extend(r);
        out
    }

    fn reject_recall(net: &Net, lessons: &[Lesson]) -> f32 {
        let rejects: Vec<&Vec<f32>> = lessons.iter().filter_map(|l| match l { Lesson::Keep { x, keep: false } => Some(x), _ => None }).collect();
        rejects.iter().filter(|x| net.forward(x).keep < net.red_line).count() as f32 / rejects.len() as f32
    }

    #[test]
    fn many_keeps_and_few_rejects_do_not_make_rejects_worse() {
        let start = starting_net();
        let shoots = [shoot("/a", 1, skewed(1, 340, 100)), shoot("/b", 2, skewed(2, 340, 100))];
        let (net, learned) = learn(&start, &shoots);
        let unseen = skewed(999, 340, 100);
        let (before, after) = (reject_recall(&start, &unseen), reject_recall(&net, &unseen));
        assert!(after >= before, "rejects caught on unseen photos: {before} -> {after} ({learned:?})");
    }

    #[test]
    fn the_red_line_weighs_keeps_and_rejects_equally() {
        // Ten keepers scored 1 to 10 and two rejects scored 1.5 and 2.5. Keeping
        // everything gets the most photos right (10 of 12) but catches no reject;
        // counting each kind equally puts the line just above the rejects.
        let scored: Vec<(f32, bool)> = (1..=10).map(|i| (i as f32, true)).chain([(1.5, false), (2.5, false)]).collect();
        let scores: Vec<f32> = scored.iter().map(|s| s.0).collect();
        let cut = best_cut(&scores, |cut| balanced(&scored, |z| z >= cut)).unwrap();
        assert!(cut > 2.5 && cut < 3.0, "{cut}");
    }

    #[test]
    fn a_half_where_fewer_rejects_are_caught_fails_even_if_more_is_right_overall() {
        let current = Check { keeps: Some(0.5), rejects: Some(0.9), order: None };
        let candidate = Check { keeps: Some(1.0), rejects: Some(0.8), order: None };
        assert_eq!(verdict(&candidate, &current), Err(Reason::FewerRejects));
        let better = Check { keeps: Some(0.6), rejects: Some(0.9), order: Some(0.7) };
        assert_eq!(verdict(&better, &Check { order: Some(0.6), ..current }), Ok(()));
        let worse_order = Check { order: Some(0.5), ..better };
        assert_eq!(verdict(&worse_order, &Check { order: Some(0.6), ..current }), Err(Reason::Worse));
        let chance = Check { keeps: Some(0.4), rejects: Some(0.5), order: None };
        assert_eq!(verdict(&chance, &Check { keeps: Some(0.3), rejects: Some(0.5), order: None }), Err(Reason::Chance));
    }

    #[test]
    fn picks_on_unseen_bursts_count_where_the_five_star_frame_stands() {
        let net = starting_net();
        let t = |group: u32, better: f32, worse: f32| Taught { group, lesson: Lesson::Prefer { better: x(better, 0.0, 0.5), worse: x(worse, 0.0, 0.5) } };
        // Burst 0: the 5★ frame has the best mark. Burst 1: one frame has a better mark than the 5★ one.
        let half = [t(0, 0.9, 0.5), t(0, 0.9, 0.4), t(1, 0.6, 0.8), t(1, 0.6, 0.3)];
        let p = picks_in(&net, &half.iter().collect::<Vec<_>>());
        assert_eq!((p.bursts, p.first, p.top2), (2, 1, 2));
    }

    #[test]
    fn a_single_burst_shoot_with_nothing_older_is_not_enough() {
        let start = starting_net();
        let lessons = dark_hater(200, 1).into_iter().map(|lesson| Taught { group: 0, lesson }).collect();
        let only = Shoot { folder: "/a".into(), format: FORMAT, taught_at: 1, lessons, set_aside: false, accepted: false };
        let (kept, learned) = learn(&start, &[only]);
        assert!(!learned.enough && !learned.accepted, "{learned:?}");
        assert_eq!(kept, start);
    }
}
