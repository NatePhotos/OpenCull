//! Cutting faces out of a photograph for the judge model to look at.
//!
//! The arithmetic here is the arithmetic the model's training pictures were
//! cut with, to the pixel. A model judges what it was shown; cut a face a
//! little differently when the app runs and it is being asked about a
//! picture it never learned from.

use fast_image_resize as fr;

/// The face picture: square, this many pixels a side.
pub const FACE_SIDE: u32 = 224;
/// The eye strip: this wide and this high.
pub const STRIP: (u32, u32) = (224, 64);

/// How far past the box the face picture reaches, as a share of the box's
/// longer side on each side of its middle. 0.5 would be the box itself.
const CONTEXT: f64 = 0.8;
/// The band of the box the eyes are in, from its top.
const EYES: (f64, f64) = (0.20, 0.55);
/// The band across the box where the mouth is: its lower half, and a little
/// below its foot, since a laugh drops the chin out of the box.
const MOUTH: (f64, f64) = (0.50, 1.02);
/// The picture of that band the expression network is shown.
pub const MOUTH_BAND: (u32, u32) = (128, 64);

/// A face's place in a photograph: its middle and its size, as fractions of
/// the frame, with `mid_y` measured **up from the bottom** — the Mac app's
/// convention, which the models follow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceBox {
    pub mid_x: f64,
    pub mid_y: f64,
    pub w: f64,
    pub h: f64,
}

/// A rectangle of pixels: left, top, right, bottom.
pub type Rect = (u32, u32, u32, u32);

impl FaceBox {
    fn centre(&self, width: u32, height: u32) -> (f64, f64) {
        (self.mid_x * width as f64, (1.0 - self.mid_y) * height as f64)
    }

    /// The square around the face that the judge is shown, held to the frame.
    pub fn face_rect(&self, width: u32, height: u32) -> Rect {
        let (cx, cy) = self.centre(width, height);
        let half = (self.w * width as f64).max(self.h * height as f64) * CONTEXT;
        clamp(cx - half, cy - half, cx + half, cy + half, width, height)
    }

    /// The box itself, held to the frame.
    pub fn box_rect(&self, width: u32, height: u32) -> Rect {
        let (cx, cy) = self.centre(width, height);
        let (hw, hh) = (self.w * width as f64 / 2.0, self.h * height as f64 / 2.0);
        clamp(cx - hw, cy - hh, cx + hw, cy + hh, width, height)
    }

    /// The band across the box where the eyes are. Found from the box alone
    /// and not from landmarks, so that it is still there on the small, soft
    /// and shaded faces where landmarks are not.
    pub fn eyes_rect(&self, width: u32, height: u32) -> Rect {
        let (x0, y0, x1, y1) = self.box_rect(width, height);
        let tall = (y1 - y0) as f64;
        (x0, y0 + (EYES.0 * tall) as u32, x1, y0 + (EYES.1 * tall) as u32)
    }

    /// The band across the lower half of the box, where the mouth is.
    pub fn mouth_rect(&self, width: u32, height: u32) -> Rect {
        let (x0, y0, x1, y1) = self.box_rect(width, height);
        let tall = (y1 - y0) as f64;
        (x0, y0 + (MOUTH.0 * tall) as u32, x1, (y0 + (MOUTH.1 * tall) as u32).min(height))
    }
}

/// Cut down to whole pixels inside the frame, the way the training crops were.
fn clamp(x0: f64, y0: f64, x1: f64, y1: f64, width: u32, height: u32) -> Rect {
    let low = |v: f64| v.max(0.0) as u32;
    let high = |v: f64, limit: u32| (v.min(limit as f64).max(0.0)) as u32;
    (low(x0), low(y0), high(x1, width), high(y1, height))
}

/// Cuts `rect` out of an upright picture and scales it to exactly
/// `out_w` by `out_h`. A rectangle cut short by the edge of the frame is
/// stretched to fit, as it was in training.
pub fn cut(rgb: &[u8], width: u32, height: u32, rect: Rect, out_w: u32, out_h: u32) -> Result<Vec<u8>, String> {
    let (x0, y0, x1, y1) = rect;
    if x1 <= x0 || y1 <= y0 {
        return Err("the face is outside the picture".into());
    }
    let src = fr::images::ImageRef::new(width, height, rgb, fr::PixelType::U8x3).map_err(|e| e.to_string())?;
    let mut dst = fr::images::Image::new(out_w, out_h, fr::PixelType::U8x3);
    let options = fr::ResizeOptions::new()
        .crop(x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64)
        .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::CatmullRom));
    fr::Resizer::new().resize(&src, &mut dst, &options).map_err(|e| e.to_string())?;
    Ok(dst.into_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The figures below are what the training tools' own arithmetic
    // (`crops.py`) gives for the same faces, run and copied here.
    const FACE: FaceBox = FaceBox { mid_x: 0.5522, mid_y: 0.7301, w: 0.0660, h: 0.0990 };

    #[test]
    fn the_face_square_matches_the_training_crops() {
        assert_eq!(FACE.face_rect(7008, 4672), (3499, 890, 4239, 1630));
    }

    #[test]
    fn the_eye_band_is_the_same_slice_of_the_box() {
        let (x0, y0, x1, y1) = FACE.box_rect(7008, 4672);
        assert_eq!((x0, y0, x1, y1), (3638, 1029, 4101, 1492));
        let eyes = FACE.eyes_rect(7008, 4672);
        assert_eq!(eyes, (3638, 1029 + 92, 4101, 1029 + 254));
    }

    #[test]
    fn a_face_at_the_edge_is_held_to_the_frame() {
        let corner = FaceBox { mid_x: 0.01, mid_y: 0.99, w: 0.1, h: 0.1 };
        assert_eq!(corner.face_rect(1000, 800), (0, 0, 90, 88));
    }

    #[test]
    fn a_cut_is_exactly_the_size_asked_for_and_shows_the_right_part() {
        // A picture that is black on the left half and white on the right.
        let (w, h) = (200u32, 100u32);
        let mut rgb = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 100..w {
                let i = ((y * w + x) * 3) as usize;
                rgb[i..i + 3].fill(255);
            }
        }
        let white = cut(&rgb, w, h, (120, 10, 180, 70), 224, 224).unwrap();
        assert_eq!(white.len(), 224 * 224 * 3);
        assert!(white.iter().all(|v| *v > 250));
        let black = cut(&rgb, w, h, (10, 10, 80, 90), 224, 64).unwrap();
        assert_eq!(black.len(), 224 * 64 * 3);
        assert!(black.iter().all(|v| *v < 5));
    }

    #[test]
    fn a_face_wholly_outside_the_picture_is_refused() {
        assert!(cut(&[0u8; 30], 10, 1, (5, 0, 5, 1), 8, 8).is_err());
    }
}
