//! A face print: 512 numbers that describe a face, from a face network
//! OpenCull owns. Two frames of the same pose give their faces prints close
//! together; how they differ is what a ranker of a burst reads.
//!
//! It is shown the very picture of the face the judge was shown, so nothing
//! is cut twice, and only the faces a photograph is about are printed.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use super::onnx;
use crate::faces::FACE_SIDE;

/// The weights are kept at half precision, which halves the file; the
/// network still works in full precision. MEASURED on 4,417 faces it never
/// learned from: its prints agree with those of the full-precision weights
/// at cosine 0.99999 on average and 0.9998 at worst, and it is as good at
/// its job to the fourth decimal place. Working in half precision too was
/// tried, and overflows.
static MODEL: &[u8] = include_bytes!("../../models/faceprint.onnx");
thread_local!(static SESSION: RefCell<Option<ort::session::Session>> = const { RefCell::new(None) });
/// Set once the network could not be opened: it is not tried again this run.
static UNOPENED: AtomicBool = AtomicBool::new(false);

/// How many numbers a face print is.
pub const LEN: usize = 512;

/// What a photograph's subjects' prints come to: their mean and their
/// least, number by number.
#[derive(Clone, Debug, PartialEq)]
pub struct PhotoPrint {
    pub mean: Vec<f32>,
    pub min: Vec<f32>,
}

/// The print of each of `n` faces, given as the judge's own face pictures
/// laid out for it (`n` × 3 × 224 × 224, 0 to 255). Each print has length one.
pub fn prints(face_planes: &[f32], n: usize) -> Result<Vec<[f32; LEN]>, String> {
    if n == 0 {
        return Ok(Vec::new());
    }
    let side = FACE_SIDE as usize;
    if face_planes.len() != n * 3 * side * side {
        return Err(format!("{} numbers given for {n} face pictures", face_planes.len()));
    }
    // The runner takes its numbers by value: a copy, at a small fraction of
    // what the network itself costs.
    let inputs = vec![("face", vec![n, 3, side, side], face_planes.to_vec())];
    unless_unopened(&UNOPENED, || {
        onnx::run(&SESSION, MODEL, inputs, &["embedding"], |out| {
            out[0]
                .1
                .chunks_exact(LEN)
                .take(n)
                .map(|p| {
                    let mut print = [0.0; LEN];
                    print.copy_from_slice(p);
                    print
                })
                .collect()
        })
    })
}

/// Runs the network by `run`, unless it could not be opened before, as
/// `unopened` remembers: then not again, since every photo would only pay to
/// fail the same way. That is said once, the first time.
fn unless_unopened<T>(unopened: &AtomicBool, run: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    if unopened.load(Ordering::Relaxed) {
        return Err("the face network could not be opened".into());
    }
    let done = run();
    if let Err(e) = &done {
        if onnx::not_opened(e) && !unopened.swap(true, Ordering::Relaxed) {
            eprintln!("OpenCull: the face network could not be opened, so photos are judged without face prints ({e})");
        }
    }
    done
}

/// The mean and the least of some prints, number by number. `None` for none.
pub fn summarise(prints: &[[f32; LEN]]) -> Option<PhotoPrint> {
    let first = prints.first()?;
    let mut mean = vec![0.0f32; LEN];
    let mut min = first.to_vec();
    for p in prints {
        for (k, x) in p.iter().enumerate() {
            mean[k] += x;
            min[k] = min[k].min(*x);
        }
    }
    let n = prints.len() as f32;
    mean.iter_mut().for_each(|m| *m /= n);
    Some(PhotoPrint { mean, min })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FACE: usize = 3 * FACE_SIDE as usize * FACE_SIDE as usize;

    fn faces(seeds: &[usize]) -> Vec<f32> {
        seeds.iter().flat_map(|s| (0..FACE).map(move |i| ((i * s + s * 31) % 256) as f32)).collect()
    }

    fn norm(p: &[f32]) -> f32 {
        p.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    #[test]
    fn every_face_gets_512_numbers_of_length_one() {
        let out = prints(&faces(&[7, 13]), 2).unwrap();
        assert_eq!(out.len(), 2);
        for p in &out {
            assert!(p.iter().all(|x| x.is_finite()));
            assert!((norm(p) - 1.0).abs() < 1e-3, "length {}", norm(p));
        }
        assert!(out[0].iter().zip(&out[1]).any(|(a, b)| (a - b).abs() > 1e-3), "two different faces, two different prints");
    }

    #[test]
    fn a_face_is_printed_the_same_alone_as_in_company() {
        let alone = prints(&faces(&[13]), 1).unwrap()[0];
        let together = prints(&faces(&[7, 13]), 2).unwrap()[1];
        assert!(alone.iter().zip(&together).all(|(a, b)| (a - b).abs() < 1e-4));
    }

    #[test]
    fn a_network_that_cannot_be_opened_is_tried_once_a_run() {
        static BROKEN: &[u8] = b"not a model";
        thread_local!(static SLOT: RefCell<Option<ort::session::Session>> = const { RefCell::new(None) });
        let unopened = AtomicBool::new(false);
        let tries = std::cell::Cell::new(0);
        let attempt = || {
            unless_unopened(&unopened, || {
                tries.set(tries.get() + 1);
                onnx::run(&SLOT, BROKEN, vec![("face", vec![1], vec![0.0])], &["embedding"], |_| ())
            })
        };
        assert!(onnx::not_opened(&attempt().unwrap_err()));
        assert!(unopened.load(Ordering::Relaxed), "remembered");
        assert!(attempt().is_err());
        assert!(attempt().is_err());
        assert_eq!(tries.get(), 1, "not opened again on every photo");

        // A network that opened but failed on one photo is tried on the next.
        let ran = AtomicBool::new(false);
        assert!(unless_unopened::<()>(&ran, || Err("a model failed to run: no".into())).is_err());
        assert!(!ran.load(Ordering::Relaxed));
    }

    #[test]
    fn no_faces_is_no_work() {
        assert!(prints(&[], 0).unwrap().is_empty());
    }

    #[test]
    fn the_summary_is_the_mean_and_the_least_number_by_number() {
        let mut a = [0.0f32; LEN];
        let mut b = [0.0f32; LEN];
        a[0] = 1.0;
        b[0] = 0.5;
        a[1] = -0.2;
        b[1] = 0.4;
        let s = summarise(&[a, b]).unwrap();
        assert_eq!((s.mean.len(), s.min.len()), (LEN, LEN));
        assert!((s.mean[0] - 0.75).abs() < 1e-6 && (s.min[0] - 0.5).abs() < 1e-6);
        assert!((s.mean[1] - 0.1).abs() < 1e-6 && (s.min[1] + 0.2).abs() < 1e-6);
        assert_eq!(summarise(&[]), None);
    }
}
