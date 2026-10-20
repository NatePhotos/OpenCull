//! A ranker of the frames of one burst. It reads each frame's judging
//! readings, the face print of the people it is about and its place in the
//! burst, and scores the frames against one another. Its weights ship with
//! the app and nothing here learns.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::ai::faceprint::{PhotoPrint, LEN};

/// How many of the judging's readings it reads: those before its own.
pub const READS: usize = super::features::TAUGHT;
/// The numbers it reads per frame: the readings, the print's mean and least, and the place in the burst.
const WIDTH: usize = READS + 2 * LEN + 1;

#[derive(Deserialize)]
struct Saved {
    /// The readings format it reads, as `features::FORMAT`.
    format: u32,
    inputs: usize,
    mu: Vec<f32>,
    sd: Vec<f32>,
    layers: Vec<SavedLayer>,
    act: String,
}

/// Weights as rows, one per output.
#[derive(Deserialize)]
struct SavedLayer {
    w: Vec<Vec<f32>>,
    b: Vec<f32>,
}

struct Layer {
    /// Outputs × inputs, row by row.
    w: Vec<f64>,
    b: Vec<f64>,
}

impl Layer {
    fn apply(&self, x: &[f64]) -> Vec<f64> {
        self.b.iter().enumerate().map(|(j, b)| b + dot(&self.w[j * x.len()..(j + 1) * x.len()], x)).collect()
    }
}

/// The first layer reads each frame's numbers and the same numbers less the
/// burst's average. Its two sets of weights are kept summed (`own`, on the
/// frame's numbers) and apart (`burst`, on the average, taken away), so the
/// average is weighed once per burst rather than once per frame.
pub struct Ranker {
    mu: Vec<f64>,
    sd: Vec<f64>,
    own: Vec<f64>,
    burst: Vec<f64>,
    bias: Vec<f64>,
    rest: Vec<Layer>,
}

/// The ranker that ships with the app, read once; none if its file does not
/// fit, which is said once.
fn ranker() -> Option<&'static Ranker> {
    static RANKER: OnceLock<Option<Ranker>> = OnceLock::new();
    RANKER
        .get_or_init(|| {
            let ranker = Ranker::from_json(include_str!("../../models/taught.json"));
            if ranker.is_none() {
                eprintln!("OpenCull: the burst ranker could not be read, so bursts are ranked without it");
            }
            ranker
        })
        .as_ref()
}

impl Ranker {
    /// A ranker from weights saved as `models/taught.json` is; none if they do
    /// not fit. The app uses only its own; this is for measuring others.
    pub fn from_json(text: &str) -> Option<Ranker> {
        serde_json::from_str::<Saved>(text).ok().and_then(build)
    }
}

fn build(saved: Saved) -> Option<Ranker> {
    let fits = saved.format == super::features::FORMAT
        && saved.inputs == WIDTH
        && saved.act == "gelu"
        && saved.mu.len() == WIDTH
        && saved.sd.len() == WIDTH
        && saved.sd.iter().all(|s| s.is_finite() && *s > 0.0)
        && saved.layers.len() >= 2
        && saved.layers.last().is_some_and(|l| l.b.len() == 1);
    if !fits {
        return None;
    }
    let mut layers = saved.layers.into_iter();
    let first = layers.next()?;
    if first.w.len() != first.b.len() || first.w.iter().any(|row| row.len() != 2 * WIDTH) {
        return None;
    }
    let own = first.w.iter().flat_map(|row| (0..WIDTH).map(|k| row[k] as f64 + row[WIDTH + k] as f64)).collect();
    let burst = first.w.iter().flat_map(|row| row[WIDTH..].iter().map(|&w| w as f64)).collect();
    let mut width = first.b.len();
    let mut rest = Vec::new();
    for layer in layers {
        if layer.w.len() != layer.b.len() || layer.w.iter().any(|row| row.len() != width) {
            return None;
        }
        width = layer.b.len();
        rest.push(Layer { w: layer.w.iter().flatten().map(|&w| w as f64).collect(), b: layer.b.iter().map(|&b| b as f64).collect() });
    }
    let wide = |v: Vec<f32>| v.into_iter().map(f64::from).collect();
    Some(Ranker { mu: wide(saved.mu), sd: wide(saved.sd), own, burst, bias: wide(first.b), rest })
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    // Four running sums, so the additions do not wait on one another.
    let mut sums = [0.0f64; 4];
    let (a4, b4) = (a.chunks_exact(4), b.chunks_exact(4));
    let tail: f64 = a4.remainder().iter().zip(b4.remainder()).map(|(x, y)| x * y).sum();
    for (x, y) in a4.zip(b4) {
        for k in 0..4 {
            sums[k] += x[k] * y[k];
        }
    }
    sums[0] + sums[1] + sums[2] + sums[3] + tail
}

/// GELU in its exact form, as the ranker was made with.
fn gelu(x: f64) -> f64 {
    0.5 * x * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

fn usable(print: &PhotoPrint) -> bool {
    print.mean.len() == LEN && print.min.len() == LEN
}

/// Raw scores for the frames of one burst, in the order given; higher is better.
/// A frame without a print reads zeros for it. `positions` are each frame's place
/// in the burst, 0 for the first to 1 for the last. All zeros if the ranker could not be read.
pub fn scores(readings: &[[f32; READS]], prints: &[Option<&PhotoPrint>], positions: &[f32]) -> Vec<f32> {
    match ranker() {
        Some(net) => net.scores(readings, prints, positions),
        None => vec![0.0; readings.len()],
    }
}

impl Ranker {
    /// As the module's `scores`, by this ranker.
    pub fn scores(&self, readings: &[[f32; READS]], prints: &[Option<&PhotoPrint>], positions: &[f32]) -> Vec<f32> {
        let net = self;
        let n = readings.len();
        let xs: Vec<Vec<f64>> = (0..n)
            .map(|i| {
                let mut x: Vec<f64> = Vec::with_capacity(WIDTH);
                x.extend(readings[i].iter().map(|&v| v as f64));
                match prints.get(i).copied().flatten().filter(|p| usable(p)) {
                    Some(p) => {
                        x.extend(p.mean.iter().map(|&v| v as f64));
                        x.extend(p.min.iter().map(|&v| v as f64));
                    }
                    None => x.resize(READS + 2 * LEN, 0.0),
                }
                x.push(positions.get(i).copied().unwrap_or(0.0) as f64);
                for (k, v) in x.iter_mut().enumerate() {
                    *v = (*v - net.mu[k]) / net.sd[k];
                }
                x
            })
            .collect();
        let mut average = vec![0.0f64; WIDTH];
        for x in &xs {
            average.iter_mut().zip(x).for_each(|(m, v)| *m += v);
        }
        average.iter_mut().for_each(|m| *m /= n.max(1) as f64);
        let shift: Vec<f64> = (0..net.bias.len()).map(|j| net.bias[j] - dot(&net.burst[j * WIDTH..(j + 1) * WIDTH], &average)).collect();
        xs.iter()
            .map(|x| {
                let mut h: Vec<f64> = shift.iter().enumerate().map(|(j, s)| gelu(s + dot(&net.own[j * WIDTH..(j + 1) * WIDTH], x))).collect();
                for (at, layer) in net.rest.iter().enumerate() {
                    h = layer.apply(&h);
                    if at + 1 < net.rest.len() {
                        h.iter_mut().for_each(|v| *v = gelu(*v));
                    }
                }
                h[0] as f32
            })
            .collect()
    }
}

/// Each frame's share of the burst: the softmax of the scores times their
/// number, so 1 is average and the shares add up to the number of frames.
pub fn shares(scores: &[f32]) -> Vec<f32> {
    let n = scores.len() as f64;
    let top = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
    let e: Vec<f64> = scores.iter().map(|&s| (s as f64 - top).exp()).collect();
    let total: f64 = e.iter().sum();
    e.iter().map(|v| (v / total * n) as f32).collect()
}

/// A share as a reading: 0 to 1, with 0.5 for an average frame.
pub fn reading(share: f32) -> f32 {
    share.clamp(0.0, 2.0) / 2.0
}

/// The ranker's reading for each frame of a burst, in the order given.
/// By design only the frames with a face print are ranked: among themselves,
/// each at its place among them, while their other readings (the burst's size,
/// their place in its marks, the gap to its best) still describe the whole
/// burst. The others, and every frame of a burst with fewer than two prints, read 0.5.
pub fn readings(readings: &[[f32; READS]], prints: &[Option<&PhotoPrint>]) -> Vec<f32> {
    readings_unless(readings, prints, off())
}

/// Whether `OPENCULL_NO_TAUGHT=1` asks for every frame to read 0.5, for
/// measuring with and without the ranker. Read once.
pub fn off() -> bool {
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| std::env::var("OPENCULL_NO_TAUGHT").is_ok_and(|v| v == "1"))
}

fn readings_unless(readings: &[[f32; READS]], prints: &[Option<&PhotoPrint>], off: bool) -> Vec<f32> {
    match ranker() {
        Some(net) if !off => net.readings(readings, prints),
        _ => vec![0.5; readings.len()],
    }
}

impl Ranker {
    /// As the module's `readings`, by this ranker, whatever `OPENCULL_NO_TAUGHT` says.
    pub fn readings(&self, readings: &[[f32; READS]], prints: &[Option<&PhotoPrint>]) -> Vec<f32> {
        let mut out = vec![0.5; readings.len()];
        let with: Vec<usize> = (0..readings.len()).filter(|&i| prints.get(i).copied().flatten().is_some_and(usable)).collect();
        // It ranks bursts of two or more frames, each with a print.
        if with.len() < 2 {
            return out;
        }
        let last = (with.len() - 1) as f32;
        let positions: Vec<f32> = (0..with.len()).map(|k| k as f32 / last).collect();
        let r: Vec<[f32; READS]> = with.iter().map(|&i| readings[i]).collect();
        let p: Vec<Option<&PhotoPrint>> = with.iter().map(|&i| prints[i]).collect();
        let raw = self.scores(&r, &p, &positions);
        if raw.iter().any(|s| !s.is_finite()) {
            return out;
        }
        for (&i, share) in with.iter().zip(shares(&raw)) {
            out[i] = reading(share);
        }
        out
    }
}

/// The error function, to about the last digit of an f64.
pub fn erf(x: f64) -> f64 {
    let a = x.abs();
    let value = if a.is_nan() {
        return x;
    } else if a < 2.0 {
        // 2/√π · e^(−a²) · Σ 2ⁿ a^(2n+1) / (1·3·…·(2n+1)): every term is positive,
        // so nothing cancels. Under 2 it takes at most about 30 terms.
        let a2 = a * a;
        let (mut term, mut sum) = (a, a);
        for n in 1..200 {
            term *= 2.0 * a2 / (2 * n + 1) as f64;
            sum += term;
            if term <= sum * 1e-17 {
                break;
            }
        }
        std::f64::consts::FRAC_2_SQRT_PI * (-a2).exp() * sum
    } else if a < 6.0 {
        // 1 − erfc, with erfc(a) = e^(−a²)/√π · 1/(a + ½/(a + 1/(a + 3⁄2/(a + …)))),
        // which 60 steps settle to the last digit from 2 up.
        let mut t = a;
        for k in (1..=60).rev() {
            t = a + (k as f64 / 2.0) / t;
        }
        1.0 - (-a * a).exp() / (std::f64::consts::PI.sqrt() * t)
    } else {
        // erfc(6) is 2e-17, under an f64's last digit near 1.
        1.0
    };
    value.copysign(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct FixtureFrame { readings: Vec<f32>, mean: Option<Vec<f32>>, min: Option<Vec<f32>>, position: f32 }
    #[derive(Deserialize)]
    struct Fixture { frames: Vec<FixtureFrame>, scores: Vec<f64> }

    /// Five made-up frames of one burst and the scores the ranker's own export gave them.
    fn fixture() -> (Vec<[f32; READS]>, Vec<Option<PhotoPrint>>, Vec<f32>, Vec<f64>) {
        let f: Fixture = serde_json::from_str(include_str!("../../models/taught-fixture.json")).unwrap();
        let readings = f.frames.iter().map(|fr| fr.readings.as_slice().try_into().unwrap()).collect();
        let prints = f.frames.iter().map(|fr| Some(PhotoPrint { mean: fr.mean.clone()?, min: fr.min.clone()? })).collect();
        let positions = f.frames.iter().map(|fr| fr.position).collect();
        (readings, prints, positions, f.scores)
    }

    fn refs(prints: &[Option<PhotoPrint>]) -> Vec<Option<&PhotoPrint>> { prints.iter().map(Option::as_ref).collect() }

    #[test]
    fn the_forward_pass_gives_the_exported_scores() {
        let (readings, prints, positions, want) = fixture();
        assert_eq!(want.len(), 5);
        let got = scores(&readings, &refs(&prints), &positions);
        for (g, w) in got.iter().zip(&want) {
            assert!((*g as f64 - w).abs() < 1e-6, "got {got:?}, want {want:?}");
        }
    }

    #[test]
    fn the_shoot_kind_and_whether_there_is_a_mark_change_nothing() {
        use crate::cull::features::{HAS_SCORE, KIND_EVENTS, KIND_PORTRAITS, KIND_SPORTS};
        let (readings, prints, positions, _) = fixture();
        let before = scores(&readings, &refs(&prints), &positions);
        for kind in [KIND_EVENTS, KIND_PORTRAITS, KIND_SPORTS] {
            let changed: Vec<[f32; READS]> = readings.iter().map(|r| {
                let mut r = *r;
                r[KIND_EVENTS] = 0.0; r[KIND_PORTRAITS] = 0.0; r[KIND_SPORTS] = 0.0; r[kind] = 1.0;
                r[HAS_SCORE] = 1.0 - r[HAS_SCORE];
                r
            }).collect();
            assert_eq!(scores(&changed, &refs(&prints), &positions), before);
        }
    }

    #[test]
    fn erf_is_accurate() {
        // Reference values to 16 digits.
        for (x, want) in [
            (0.0, 0.0), (1e-3, 0.0011283787909692365), (0.1, 0.1124629160182849), (0.5, 0.5204998778130465), (1.0, 0.8427007929497149),
            (1.5, 0.9661051464753108), (2.0, 0.9953222650189527), (2.5, 0.9995930479825550), (3.0, 0.9999779095030014),
            (3.5, 0.9999992569016276), (4.0, 0.9999999845827421), (6.0, 1.0), (40.0, 1.0),
        ] {
            assert!((erf(x) - want).abs() < 1e-14, "erf({x}) = {}, want {want}", erf(x));
            assert!((erf(-x) + want).abs() < 1e-14, "erf(-{x})");
        }
        // No step at 2, where the method changes.
        for (x, want) in [(1.999_999_9, 0.9953222629522538), (2.000_000_1, 0.9953222670856509)] {
            assert!((erf(x) - want).abs() < 1e-14, "erf({x}) = {}, want {want}", erf(x));
        }
        assert!(erf(f64::NAN).is_nan());
    }

    #[test]
    fn shares_average_one_and_follow_the_scores() {
        let s = shares(&[0.2, 1.4, -0.3, 0.9]);
        assert!((s.iter().sum::<f32>() / 4.0 - 1.0).abs() < 1e-6, "{s:?}");
        assert!(s[1] > s[3] && s[3] > s[0] && s[0] > s[2]);
        let r: Vec<f32> = s.iter().map(|&x| reading(x)).collect();
        assert!((r.iter().sum::<f32>() / 4.0 - 0.5).abs() < 1e-6, "no share is clamped here: {r:?}");
        assert!(r[1] > r[3] && r[3] > r[0] && r[0] > r[2]);
        // Clamped to 0..1, with 0.5 for an average frame.
        assert_eq!((reading(5.0), reading(-1.0), reading(1.0)), (1.0, 0.0, 0.5));
        assert_eq!(shares(&[3.7]), vec![1.0]);
    }

    #[test]
    fn a_burst_is_read_in_its_order_and_the_better_frame_reads_higher() {
        let (readings, prints, _, _) = fixture();
        // Frames 0, 1 and 4 have prints; ranked among themselves at positions 0, 0.5 and 1.
        let with: Vec<usize> = vec![0, 1, 4];
        let r: Vec<[f32; READS]> = with.iter().map(|&i| readings[i]).collect();
        let p: Vec<Option<&PhotoPrint>> = with.iter().map(|&i| prints[i].as_ref()).collect();
        let raw = scores(&r, &p, &[0.0, 0.5, 1.0]);
        let want: Vec<f32> = shares(&raw).into_iter().map(reading).collect();
        let got = readings_unless(&r, &p, false);
        assert_eq!(got, want);
        let best = (0..3).max_by(|&a, &b| raw[a].total_cmp(&raw[b])).unwrap();
        let worst = (0..3).min_by(|&a, &b| raw[a].total_cmp(&raw[b])).unwrap();
        assert!(got[best] > got[worst], "{raw:?} {got:?}");
        // The place in the burst counts: the same frames in the other order score differently.
        let back: Vec<[f32; READS]> = r.iter().rev().copied().collect();
        let backp: Vec<Option<&PhotoPrint>> = p.iter().rev().copied().collect();
        let reversed: Vec<f32> = scores(&back, &backp, &[0.0, 0.5, 1.0]).into_iter().rev().collect();
        assert_ne!(reversed, raw);
    }

    #[test]
    fn a_frame_without_a_print_reads_average_and_the_rest_are_ranked_without_it() {
        let (readings, prints, _, _) = fixture();
        // Frame 3 has no print; frames 0, 1, 2 and 4 are ranked among themselves.
        let got = readings_unless(&readings, &refs(&prints), false);
        assert_eq!(got[3], 0.5);
        let with = [0, 1, 2, 4];
        let r: Vec<[f32; READS]> = with.iter().map(|&i| readings[i]).collect();
        let p: Vec<Option<&PhotoPrint>> = with.iter().map(|&i| prints[i].as_ref()).collect();
        let alone = readings_unless(&r, &p, false);
        assert_eq!([got[0], got[1], got[2], got[4]].to_vec(), alone);
    }

    #[test]
    fn a_ranker_saved_for_other_readings_is_not_used() {
        let saved = || serde_json::from_str::<Saved>(include_str!("../../models/taught.json")).unwrap();
        assert!(build(saved()).is_some());
        let mut other = saved();
        other.format = crate::cull::features::FORMAT - 1;
        assert!(build(other).is_none());
    }

    #[test]
    fn a_ranker_read_from_its_file_ranks_as_the_shipped_one() {
        let (readings, prints, positions, _) = fixture();
        let own = Ranker::from_json(include_str!("../../models/taught.json")).unwrap();
        assert_eq!(own.scores(&readings, &refs(&prints), &positions), scores(&readings, &refs(&prints), &positions));
        assert_eq!(own.readings(&readings, &refs(&prints)), readings_unless(&readings, &refs(&prints), false));
        assert!(Ranker::from_json("{}").is_none());
    }

    #[test]
    fn a_lone_frame_no_prints_one_print_or_switched_off_reads_average() {
        let (readings, prints, _, _) = fixture();
        assert_eq!(readings_unless(&readings[..1], &refs(&prints[..1]), false), vec![0.5]);
        assert_eq!(readings_unless(&readings, &vec![None; 5], false), vec![0.5; 5]);
        let one = [prints[0].as_ref(), None, None, None, None];
        assert_eq!(readings_unless(&readings, &one, false), vec![0.5; 5]);
        assert_eq!(readings_unless(&readings, &refs(&prints), true), vec![0.5; 5]);
        assert!(readings_unless(&[], &[], false).is_empty());
    }
}
