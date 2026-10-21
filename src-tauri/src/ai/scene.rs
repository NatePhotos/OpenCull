//! A scene print: 512 numbers that describe a whole photograph, from a scene
//! network OpenCull owns. Photos of the same scene shot again give prints
//! close together, which is what grouping reads.
//!
//! It is shown the middle square of the small picture an Advanced scan makes
//! anyway for brightness, so nothing is decoded twice.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use fast_image_resize as fr;

use super::onnx;

/// Weights kept at half precision; the network works in full precision.
static MODEL: &[u8] = include_bytes!("../../models/scene.onnx");
thread_local!(static SESSION: RefCell<Option<ort::session::Session>> = const { RefCell::new(None) });
/// Set once the network could not be opened: it is not tried again this run.
static UNOPENED: AtomicBool = AtomicBool::new(false);

/// How many numbers a scene print is.
pub const LEN: usize = 512;
/// The side of the square the network is shown.
pub const SIDE: u32 = 224;

/// A photograph's scene print, of length one.
#[derive(Clone, Debug, PartialEq)]
pub struct ScenePrint(pub Vec<f32>);

/// The middle square of a picture, resized to SIDE x SIDE (bilinear).
pub fn square(rgb: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgb.len() < (width * height * 3) as usize {
        return None;
    }
    let side = width.min(height);
    let (left, top) = ((width - side) / 2, (height - side) / 2);
    let mut sq = Vec::with_capacity((side * side * 3) as usize);
    for y in top..top + side {
        let start = ((y * width + left) * 3) as usize;
        sq.extend_from_slice(rgb.get(start..start + side as usize * 3)?);
    }
    let src = fr::images::ImageRef::new(side, side, &sq, fr::PixelType::U8x3).ok()?;
    let mut dst = fr::images::Image::new(SIDE, SIDE, fr::PixelType::U8x3);
    fr::Resizer::new().resize(&src, &mut dst, &fr::ResizeOptions::new().resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Bilinear))).ok()?;
    Some(dst.into_vec())
}

/// The scene print of a picture: its middle square, through the network.
pub fn print(rgb: &[u8], width: u32, height: u32) -> Result<ScenePrint, String> {
    let sq = square(rgb, width, height).ok_or("no picture to print")?;
    embed(&sq)
}

/// The print of a SIDE x SIDE square of RGB bytes.
fn embed(square: &[u8]) -> Result<ScenePrint, String> {
    let n = (SIDE * SIDE) as usize;
    if square.len() != 3 * n {
        return Err(format!("{} bytes given for a {SIDE}-pixel square", square.len()));
    }
    let mut planes = vec![0f32; 3 * n];
    for (i, px) in square.chunks_exact(3).enumerate() {
        for c in 0..3 {
            planes[c * n + i] = px[c] as f32;
        }
    }
    let inputs = vec![("picture", vec![1, 3, SIDE as usize, SIDE as usize], planes)];
    let raw = unless_unopened(&UNOPENED, || onnx::run(&SESSION, MODEL, inputs, &["embedding"], |out| out[0].1[..LEN].to_vec()))?;
    let norm = raw.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !norm.is_finite() || norm < 1e-8 {
        return Err("empty scene print".into());
    }
    Ok(ScenePrint(raw.iter().map(|x| x / norm).collect()))
}

/// How alike two prints are: 1 the same, lower less alike.
pub fn similarity(a: &ScenePrint, b: &ScenePrint) -> f32 {
    a.0.iter().zip(&b.0).map(|(x, y)| x * y).sum::<f32>().clamp(-1.0, 1.0)
}

/// Runs the network by `run`, unless it could not be opened before. Said once.
fn unless_unopened<T>(unopened: &AtomicBool, run: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    if unopened.load(Ordering::Relaxed) {
        return Err("the scene network could not be opened".into());
    }
    let done = run();
    if let Err(e) = &done {
        if onnx::not_opened(e) && !unopened.swap(true, Ordering::Relaxed) {
            eprintln!("OpenCull: the scene network could not be opened, so photos are grouped as before ({e})");
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic() -> Vec<u8> { (0..(SIDE * SIDE * 3) as usize).map(|i| ((i * 7 + 13) % 256) as u8).collect() }

    #[test]
    fn the_network_gives_the_print_it_gave_when_made() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../models/scene-fixture.json")).unwrap();
        let want: Vec<f32> = fixture["embedding"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
        let got = embed(&synthetic()).unwrap();
        assert_eq!(got.0.len(), LEN);
        assert!(got.0.iter().zip(&want).all(|(a, b)| (a - b).abs() < 1e-4));
    }

    #[test]
    fn a_print_has_length_one_and_two_pictures_differ() {
        let a = print(&vec![200u8; 320 * 213 * 3], 320, 213).unwrap();
        let b = print(&synthetic(), SIDE, SIDE).unwrap();
        let norm = |p: &ScenePrint| p.0.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm(&a) - 1.0).abs() < 1e-3 && (norm(&b) - 1.0).abs() < 1e-3);
        assert!(similarity(&a, &b) < 0.999);
        assert!((similarity(&a, &a) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_square_is_the_middle_of_the_picture() {
        // A 320x213 picture, red on the left third, blue on the right third, green between.
        let (w, h) = (320u32, 213u32);
        let rgb: Vec<u8> = (0..w * h).flat_map(|i| { let x = i % w; if x < 107 { [255, 0, 0] } else if x >= 213 { [0, 0, 255] } else { [0, 255, 0] } }).collect();
        let sq = square(&rgb, w, h).unwrap();
        assert_eq!(sq.len(), (SIDE * SIDE * 3) as usize);
        let centre = ((SIDE / 2 * SIDE + SIDE / 2) * 3) as usize;
        assert_eq!(&sq[centre..centre + 3], &[0, 255, 0]);
        assert!(square(&[], 0, 0).is_none());
    }

    #[test]
    fn a_network_that_cannot_be_opened_is_tried_once_a_run() {
        static BROKEN: &[u8] = b"not a model";
        thread_local!(static SLOT: std::cell::RefCell<Option<ort::session::Session>> = const { std::cell::RefCell::new(None) });
        let unopened = AtomicBool::new(false);
        let tries = std::cell::Cell::new(0);
        let attempt = || unless_unopened(&unopened, || { tries.set(tries.get() + 1); onnx::run(&SLOT, BROKEN, vec![("picture", vec![1], vec![0.0])], &["embedding"], |_| ()) });
        assert!(attempt().is_err());
        assert!(attempt().is_err());
        assert_eq!(tries.get(), 1);
    }

    #[test]
    #[ignore]
    fn timing_of_a_print() {
        let (w, h) = (320u32, 213u32);
        let rgb: Vec<u8> = (0..(w * h * 3) as usize).map(|i| ((i * 7 + 13) % 256) as u8).collect();
        print(&rgb, w, h).unwrap();
        let start = std::time::Instant::now();
        for _ in 0..200 {
            print(&rgb, w, h).unwrap();
        }
        println!("scene print: {:.2} ms per picture", start.elapsed().as_secs_f64() * 1000.0 / 200.0);
    }
}
