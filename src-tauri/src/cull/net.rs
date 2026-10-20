//! A small network with one hidden layer and two answers: how likely the
//! photographer is to keep a photo, and a score for ranking it among the
//! frames of its burst. Plain Rust, so it trains on any computer the app runs on.

use serde::{Deserialize, Serialize};

use super::features::{READINGS, TAUGHT};
use super::rules::TAUGHT_WEIGHT;

pub fn sigmoid(z: f32) -> f32 { 1.0 / (1.0 + (-z).exp()) }

/// A small, seeded random source, so the starting network is the same on every computer.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self { Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1) }
    pub fn next_u64(&mut self) -> u64 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; x }
    pub fn next_f32(&mut self) -> f32 { (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32 }
    pub fn normal(&mut self) -> f32 {
        let (u, v) = (self.next_f32().max(1e-7), self.next_f32());
        (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Net {
    pub inputs: usize,
    pub hidden: usize,
    /// Hidden weights (hidden × inputs), hidden biases, output weights (2 × hidden), output biases (2).
    pub params: Vec<f32>,
    /// Below this keep score a photo is red.
    pub red_line: f32,
    /// Frames whose rank score is within this of the burst's best are blue.
    pub blue_margin: f32,
    /// The rank answer is added to the starting rules' rank, so the network
    /// only learns where the photographer differs from the rules. Networks
    /// saved before this existed rank on their own.
    #[serde(default)]
    pub rule_base: bool,
}

/// `rank` orders a burst's frames; `base` is the same without the ranker's
/// reading (see `features::TAUGHT`), which says how far behind the best a frame is.
pub struct Pass { pub keep: f32, pub rank: f32, pub base: f32, pub hidden: Vec<f32> }

impl Net {
    pub fn new(inputs: usize, hidden: usize, readings: usize, seed: u64) -> Net {
        let mut rng = Rng::new(seed);
        let mut params = vec![0.0; hidden * inputs + hidden + 2 * hidden + 2];
        let scale = (1.0 / readings.max(1) as f32).sqrt();
        for j in 0..hidden {
            for i in 0..readings.min(inputs) { params[j * inputs + i] = rng.normal() * scale; }
        }
        let w2 = hidden * inputs + hidden;
        for k in 0..2 * hidden { params[w2 + k] = rng.normal() * (1.0 / hidden as f32).sqrt(); }
        Net { inputs, hidden, params, red_line: 0.0, blue_margin: 0.1, rule_base: false }
    }

    fn at(&self) -> (usize, usize, usize) {
        let b1 = self.hidden * self.inputs;
        (b1, b1 + self.hidden, b1 + 3 * self.hidden)
    }

    pub fn forward(&self, x: &[f32]) -> Pass { self.forward_sparse(&nonzero(x)) }

    /// `forward` with the ranker's reading weighed by `taught_weight`.
    pub fn forward_with(&self, x: &[f32], taught_weight: f32) -> Pass { self.forward_sparse_with(&nonzero(x), taught_weight) }

    /// `forward` for inputs already reduced to the ones that are not zero.
    pub fn forward_sparse(&self, x: &[(usize, f32)]) -> Pass { self.forward_sparse_with(x, TAUGHT_WEIGHT) }

    /// `forward_sparse` with the ranker's reading weighed by `taught_weight`.
    /// The weights never see that weight: it is added to the rank after them,
    /// so the same network ranks the same at any weight when the reading is average.
    pub fn forward_sparse_with(&self, x: &[(usize, f32)], taught_weight: f32) -> Pass {
        let (b1, w2, b2) = self.at();
        let mut hidden = vec![0.0; self.hidden];
        for (j, h) in hidden.iter_mut().enumerate() {
            let row = &self.params[j * self.inputs..(j + 1) * self.inputs];
            let z = self.params[b1 + j] + x.iter().map(|&(i, v)| row[i] * v).sum::<f32>();
            *h = z.tanh();
        }
        let keep = self.params[b2] + (0..self.hidden).map(|j| self.params[w2 + j] * hidden[j]).sum::<f32>();
        let rank = self.params[b2 + 1] + (0..self.hidden).map(|j| self.params[w2 + self.hidden + j] * hidden[j]).sum::<f32>();
        let base = rank + self.rule_rank(x);
        // Nothing is added for an average frame, so its rank is exactly the base.
        let term = self.taught_term(x, taught_weight);
        Pass { keep, rank: if term == 0.0 { base } else { base + term }, base, hidden }
    }

    /// The starting rules' rank for these inputs, without the ranker's reading,
    /// when the network ranks on top of it, else 0. It does not depend on the
    /// weights, so training is unchanged.
    fn rule_rank(&self, x: &[(usize, f32)]) -> f32 {
        if !self.rule_base || self.inputs < READINGS { return 0.0; }
        let mut r = [0.0f32; READINGS];
        for &(i, v) in x.iter().take_while(|(i, _)| *i < READINGS) { r[i] = v; }
        super::rules::rule_rank_with(&r, 0.0)
    }

    /// What the ranker's reading adds to the rank on top of the rules: the
    /// weight times the reading less its average, which is how it comes in
    /// (see `features::inputs`). Exactly 0 for an average frame, which `x` leaves out.
    fn taught_term(&self, x: &[(usize, f32)], taught_weight: f32) -> f32 {
        if !self.rule_base || self.inputs < READINGS || taught_weight == 0.0 { return 0.0; }
        x.iter().find(|(i, _)| *i == TAUGHT).map_or(0.0, |&(_, v)| taught_weight * v)
    }

    pub fn backward(&self, x: &[f32], pass: &Pass, d_keep: f32, d_rank: f32, grads: &mut [f32]) {
        self.backward_sparse(&nonzero(x), pass, d_keep, d_rank, grads)
    }

    /// `backward` for inputs already reduced to the ones that are not zero.
    pub fn backward_sparse(&self, x: &[(usize, f32)], pass: &Pass, d_keep: f32, d_rank: f32, grads: &mut [f32]) {
        let (b1, w2, b2) = self.at();
        grads[b2] += d_keep;
        grads[b2 + 1] += d_rank;
        for j in 0..self.hidden {
            let h = pass.hidden[j];
            grads[w2 + j] += d_keep * h;
            grads[w2 + self.hidden + j] += d_rank * h;
            let dz = (d_keep * self.params[w2 + j] + d_rank * self.params[w2 + self.hidden + j]) * (1.0 - h * h);
            grads[b1 + j] += dz;
            for &(i, v) in x { grads[j * self.inputs + i] += dz * v; }
        }
    }
}

/// The inputs that are not zero, with their places. A photo without a
/// fingerprint has only its few readings, so this saves most of the work;
/// leaving out a zero changes no sum, so the answers are the same.
pub fn nonzero(x: &[f32]) -> Vec<(usize, f32)> {
    x.iter().enumerate().filter(|(_, v)| **v != 0.0).map(|(i, v)| (i, *v)).collect()
}

pub struct Adam { m: Vec<f32>, v: Vec<f32>, t: i32 }
impl Adam {
    pub fn new(len: usize) -> Self { Adam { m: vec![0.0; len], v: vec![0.0; len], t: 0 } }
    pub fn step(&mut self, params: &mut [f32], grads: &[f32], lr: f32) {
        self.t += 1;
        let (b1, b2) = (0.9f32, 0.999f32);
        let (c1, c2) = (1.0 - b1.powi(self.t), 1.0 - b2.powi(self.t));
        for i in 0..params.len() {
            self.m[i] = b1 * self.m[i] + (1.0 - b1) * grads[i];
            self.v[i] = b2 * self.v[i] + (1.0 - b2) * grads[i] * grads[i];
            params[i] -= lr * (self.m[i] / c1) / ((self.v[i] / c2).sqrt() + 1e-8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seeded_generator_repeats_itself() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..10 { assert_eq!(a.next_f32(), b.next_f32()); }
    }

    #[test]
    fn a_fingerprint_changes_nothing_until_the_network_has_learned_from_one() {
        let net = Net::new(10, 4, 6, 1);
        let mut x = vec![0.3; 10];
        let plain = net.forward(&x);
        for v in &mut x[6..] { *v = 0.9; }
        let printed = net.forward(&x);
        assert_eq!(plain.keep, printed.keep);
        assert_eq!(plain.rank, printed.rank);
    }

    #[test]
    fn gradients_match_a_numerical_check() {
        let mut net = Net::new(5, 3, 5, 3);
        let x = [0.2, -0.4, 0.9, 0.1, 0.5];
        let loss = |n: &Net| { let p = n.forward(&x); 0.7 * p.keep + -1.3 * p.rank };
        let pass = net.forward(&x);
        let mut grads = vec![0.0; net.params.len()];
        net.backward(&x, &pass, 0.7, -1.3, &mut grads);
        for i in 0..net.params.len() {
            let keep = net.params[i];
            net.params[i] = keep + 1e-3; let up = loss(&net);
            net.params[i] = keep - 1e-3; let down = loss(&net);
            net.params[i] = keep;
            let numeric = (up - down) / 2e-3;
            assert!((numeric - grads[i]).abs() < 1e-2, "param {i}: {numeric} vs {}", grads[i]);
        }
    }

    #[test]
    fn a_network_on_the_rules_ranks_by_the_rules_plus_its_own_answer() {
        use crate::cull::features::{inputs, HAS_FACES, KIND_EVENTS, MARK, MIN_SHARP};
        let mut r = [0.0; READINGS];
        r[HAS_FACES] = 1.0; r[MIN_SHARP] = 0.8; r[MARK] = 0.7; r[KIND_EVENTS] = 1.0;
        let x = inputs(&r, None);
        let mut net = Net::new(x.len(), 4, READINGS, 5);
        let own = net.forward(&x).rank;
        net.rule_base = true;
        let on_rules = net.forward(&x).rank;
        // The rules' rank, plus the ranker's reading less its average by its weight.
        let rules = crate::cull::rules::rule_rank_with(&r, 0.0) + crate::cull::rules::TAUGHT_WEIGHT * (r[TAUGHT] - 0.5);
        assert!((on_rules - own - rules).abs() < 1e-5, "{on_rules} vs {own}");
    }

    #[test]
    fn the_rankers_reading_is_added_to_the_rank_after_the_network() {
        use crate::cull::features::{inputs, HAS_FACES, KIND_EVENTS, MARK, MIN_SHARP};
        let mut r = [0.0; READINGS];
        r[HAS_FACES] = 1.0; r[MIN_SHARP] = 0.8; r[MARK] = 0.7; r[KIND_EVENTS] = 1.0; r[TAUGHT] = 0.5;
        let mut net = Net::new(inputs(&r, None).len(), 4, READINGS, 5);
        net.rule_base = true;
        let even = net.forward_with(&inputs(&r, None), 0.0);
        let mut high = r;
        high[TAUGHT] = 0.9;
        for w in [0.0, 0.2, 0.3] {
            // An average frame ranks exactly the same at any weight.
            let a = net.forward_with(&inputs(&r, None), w);
            assert_eq!((a.rank.to_bits(), a.base.to_bits(), a.keep.to_bits()), (even.rank.to_bits(), even.rank.to_bits(), even.keep.to_bits()));
            // Otherwise the base is the same and the rank moves by the weight times the reading less average.
            let b = net.forward_with(&inputs(&high, None), w);
            assert_eq!(b.base.to_bits(), net.forward_with(&inputs(&high, None), 0.0).rank.to_bits());
            assert!((b.rank - b.base - w * 0.4).abs() < 1e-6, "{w}: {} {}", b.rank, b.base);
        }
        assert_eq!(net.forward(&inputs(&high, None)).rank, net.forward_with(&inputs(&high, None), crate::cull::rules::TAUGHT_WEIGHT).rank);
    }

    #[test]
    fn a_network_saved_before_rule_ranking_ranks_on_its_own() {
        let net = Net::new(5, 2, 5, 1);
        let mut json = serde_json::to_value(&net).unwrap();
        json.as_object_mut().unwrap().remove("rule_base");
        let old: Net = serde_json::from_value(json).unwrap();
        assert!(!old.rule_base);
    }

    #[test]
    fn adam_walks_downhill() {
        let mut p = vec![3.0f32];
        let mut adam = Adam::new(1);
        for _ in 0..500 { let g = vec![2.0 * p[0]]; adam.step(&mut p, &g, 0.05); }
        assert!(p[0].abs() < 0.05);
    }
}
