//! How a photo is framed: which way it is turned, and what is cropped from it.
//!
//! Nothing here touches a picture. A crop is an instruction, kept in the
//! photo's sidecar in the fields Adobe's programs use, so Lightroom and Camera
//! Raw open the photo already cropped. The app shows it cropped by placing the
//! previews it already has, and never makes a preview again because of it.
//!
//! Three frames are kept apart, because mixing them is how a crop lands in
//! the wrong place:
//!
//! - SENSOR — the picture as it is stored, turned no way at all. Adobe writes
//!   its crop here. MEASURED on four sidecars Adobe wrote, one of them for a
//!   photo stored on its side: read in this frame every one comes out at
//!   exactly 3:2, and read in any other it does not.
//! - UPRIGHT — the stored picture turned the way the file itself says it
//!   should be. Every preview the app makes is this.
//! - VIEW — upright, turned or mirrored further by the photographer. What is
//!   on screen, and the only frame the interface is ever told about.
//!
//! All conversion between them is in this file.

use serde::{Deserialize, Serialize};

/// A crop in VIEW: a rectangle by its middle and its size, in pixels of the
/// full-size picture, turned `angle` degrees clockwise about its middle.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub cx: f64,
    pub cy: f64,
    pub w: f64,
    pub h: f64,
    pub angle: f64,
}

/// A photo's framing as the interface has it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Framing {
    /// How the upright picture is turned to give the view: one of the eight
    /// ways a rectangle can be turned and mirrored, numbered as EXIF numbers
    /// them. 1 is not at all.
    pub turn: u8,
    pub crop: Option<Crop>,
}

impl Default for Framing {
    fn default() -> Self {
        Framing { turn: 1, crop: None }
    }
}

impl Framing {
    pub fn is_plain(&self) -> bool {
        self.turn == 1 && self.crop.is_none()
    }
}

/// A crop as Adobe stores it: the top-left and bottom-right corners of the
/// turned rectangle, as fractions of the SENSOR frame, and the turn in
/// degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stored {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub angle: f64,
}

impl Stored {
    /// A crop that crops nothing, which some programs write out in full.
    pub fn is_whole(&self) -> bool {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        near(self.left, 0.0) && near(self.top, 0.0) && near(self.right, 1.0) && near(self.bottom, 1.0) && near(self.angle, 0.0)
    }

    /// Whether these are numbers a crop could have.
    pub fn is_sane(&self) -> bool {
        [self.left, self.top, self.right, self.bottom, self.angle].iter().all(|v| v.is_finite()) && self.angle.abs() <= 45.0001
    }
}

/// What an orientation does to a point measured from the middle of the
/// frame, y downwards. The same eight as `preview::orient`.
fn matrix(orientation: u8) -> [[f64; 2]; 2] {
    match orientation {
        2 => [[-1.0, 0.0], [0.0, 1.0]],
        3 => [[-1.0, 0.0], [0.0, -1.0]],
        4 => [[1.0, 0.0], [0.0, -1.0]],
        5 => [[0.0, 1.0], [1.0, 0.0]],
        6 => [[0.0, -1.0], [1.0, 0.0]],
        7 => [[0.0, -1.0], [-1.0, 0.0]],
        8 => [[0.0, 1.0], [-1.0, 0.0]],
        _ => [[1.0, 0.0], [0.0, 1.0]],
    }
}

/// An orientation is only ever 1 to 8; anything else is read as 1.
pub fn known(orientation: u8) -> u8 {
    if (1..=8).contains(&orientation) {
        orientation
    } else {
        1
    }
}

/// Whether an orientation lays the frame on its side.
pub fn sideways(orientation: u8) -> bool {
    orientation >= 5 && orientation <= 8
}

/// Whether an orientation mirrors the picture.
fn mirrors(orientation: u8) -> bool {
    matches!(orientation, 2 | 4 | 5 | 7)
}

/// The orientation that undoes this one.
pub fn inverse(orientation: u8) -> u8 {
    match orientation {
        6 => 8,
        8 => 6,
        other => known(other),
    }
}

/// The one orientation that does what `first` and then `second` do.
pub fn then(first: u8, second: u8) -> u8 {
    let (a, b) = (matrix(second), matrix(first));
    let product = [
        [a[0][0] * b[0][0] + a[0][1] * b[1][0], a[0][0] * b[0][1] + a[0][1] * b[1][1]],
        [a[1][0] * b[0][0] + a[1][1] * b[1][0], a[1][0] * b[0][1] + a[1][1] * b[1][1]],
    ];
    (1..=8).find(|n| matrix(*n) == product).unwrap_or(1)
}

/// How the upright picture is turned to give the view, when the file says
/// `file` and the sidecar says `sidecar`. Not at all, when the sidecar says
/// nothing.
pub fn turn(file: u8, sidecar: Option<u8>) -> u8 {
    match sidecar {
        Some(sidecar) => then(inverse(known(file)), known(sidecar)),
        None => 1,
    }
}

/// The orientation a sidecar must state for the view to be the upright
/// picture turned by `turn`.
pub fn stated(file: u8, turn: u8) -> u8 {
    then(known(file), known(turn))
}

fn apply(m: [[f64; 2]; 2], p: (f64, f64)) -> (f64, f64) {
    (m[0][0] * p.0 + m[0][1] * p.1, m[1][0] * p.0 + m[1][1] * p.1)
}

/// A stored crop, in the view that `orientation` gives of a sensor frame
/// `width` by `height`. `None` for numbers that are not a crop.
pub fn to_view(stored: &Stored, orientation: u8, width: f64, height: f64) -> Option<Crop> {
    if !stored.is_sane() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let turn = stored.angle.to_radians();
    let (across, down) = ((turn.cos(), turn.sin()), (-turn.sin(), turn.cos()));
    let (tl, br) = ((stored.left * width, stored.top * height), (stored.right * width, stored.bottom * height));
    let diagonal = (br.0 - tl.0, br.1 - tl.1);
    let (w, h) = (diagonal.0 * across.0 + diagonal.1 * across.1, diagonal.0 * down.0 + diagonal.1 * down.1);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }

    let middle = apply(matrix(orientation), ((tl.0 + br.0) / 2.0 - width / 2.0, (tl.1 + br.1) / 2.0 - height / 2.0));
    let side = sideways(orientation);
    let (view_w, view_h) = if side { (height, width) } else { (width, height) };
    Some(Crop {
        cx: middle.0 + view_w / 2.0,
        cy: middle.1 + view_h / 2.0,
        w: if side { h } else { w },
        h: if side { w } else { h },
        // Seen in a mirror, a clockwise turn is an anticlockwise one.
        angle: if mirrors(orientation) { -stored.angle } else { stored.angle },
    })
}

/// The reverse: a crop in the view, as Adobe stores it for a sensor frame
/// `width` by `height` seen through `orientation`.
pub fn to_stored(crop: &Crop, orientation: u8, width: f64, height: f64) -> Stored {
    let side = sideways(orientation);
    let (view_w, view_h) = if side { (height, width) } else { (width, height) };
    let middle = apply(matrix(inverse(orientation)), (crop.cx - view_w / 2.0, crop.cy - view_h / 2.0));
    let middle = (middle.0 + width / 2.0, middle.1 + height / 2.0);
    let (w, h) = if side { (crop.h, crop.w) } else { (crop.w, crop.h) };
    let angle = if mirrors(orientation) { -crop.angle } else { crop.angle };

    let turn = angle.to_radians();
    let (across, down) = ((turn.cos(), turn.sin()), (-turn.sin(), turn.cos()));
    let reach = (w / 2.0 * across.0 + h / 2.0 * down.0, w / 2.0 * across.1 + h / 2.0 * down.1);
    let within = |v: f64| v.clamp(0.0, 1.0);
    Stored {
        left: within((middle.0 - reach.0) / width),
        top: within((middle.1 - reach.1) / height),
        right: within((middle.0 + reach.0) / width),
        bottom: within((middle.1 + reach.1) / height),
        angle,
    }
}

/// What is known of one photo's framing as its folder is opened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub framing: Framing,
    /// Whether the photo can be cropped and turned here. Not when its sidecar
    /// cannot be followed, nor when the picture shown is not the whole of
    /// the one the crop would be measured against.
    pub framable: bool,
}

/// What is needed to place a crop: the photo's own file and the picture
/// shown for it.
pub struct Shown {
    /// Which way up the photo's own file says it is stored.
    pub file: u8,
    /// The same for the file that is drawn — a RAW's JPEG, or the photo.
    pub drawn: u8,
    /// The size of the upright picture that is drawn.
    pub upright: (u32, u32),
    /// The size the photo's own file states for itself, as stored.
    pub stated: Option<(u32, u32)>,
}

impl Shown {
    /// Whether a crop measured on what is drawn lands in the same place on
    /// the photo itself.
    fn agrees(&self) -> bool {
        let (w, h) = self.upright;
        if w == 0 || h == 0 || known(self.file) != known(self.drawn) {
            return false;
        }
        // A camera set to shoot 16:9 or square writes a JPEG and a preview of
        // that shape beside a RAW of the whole sensor. A crop drawn on one
        // would not be the same crop on the other.
        let (sw, sh) = sensor_size(self.file, w as f64, h as f64);
        self.stated.is_none_or(|(tw, th)| tw == 0 || th == 0 || ((sw / sh) / (tw as f64 / th as f64) - 1.0).abs() <= 0.01)
    }
}

/// A photo's framing, from what its sidecar says. `packet` is the XMP inside
/// the file itself, read for a crop only when there is no sidecar: a turn is
/// never taken from there, because the file already says which way up it is.
pub fn seen(sidecar: Option<&str>, packet: Option<&str>, shown: &Shown) -> Seen {
    let plain = Framing::default();
    if !shown.agrees() {
        return Seen { framing: plain, framable: false };
    }
    let (text, from_sidecar) = match (sidecar, packet) {
        (Some(text), _) => (text, true),
        (None, Some(text)) => (text, false),
        (None, None) => return Seen { framing: plain, framable: true },
    };
    if !crate::sidecar::may_frame(text) {
        return Seen { framing: plain, framable: true };
    }
    let found = match crate::sidecar::read(text) {
        Ok(found) => found,
        // A sidecar that cannot be followed cannot be added to. A packet that
        // cannot is no obstacle: nothing is ever written there unasked.
        Err(_) => return Seen { framing: plain, framable: !from_sidecar },
    };
    if found.already_applied {
        return Seen { framing: plain, framable: !from_sidecar };
    }

    let said = if from_sidecar { found.orientation } else { None };
    let through = said.unwrap_or(known(shown.file));
    let (sw, sh) = sensor_size(shown.file, shown.upright.0 as f64, shown.upright.1 as f64);
    let crop = found.crop.and_then(|stored| to_view(&stored, through, sw, sh));
    Seen { framing: Framing { turn: turn(shown.file, said), crop }, framable: true }
}

/// The sensor frame behind an upright picture `width` by `height`, for a file
/// that says it is stored `file` way up.
pub fn sensor_size(file: u8, width: f64, height: f64) -> (f64, f64) {
    if sideways(file) {
        (height, width)
    } else {
        (width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a further turn does to a crop already in the view: the same
    /// thing `src/lib/framing.ts` does when Rotate or Flip is pressed.
    fn turned(crop: &Crop, by: u8, view_w: f64, view_h: f64) -> Crop {
        let middle = apply(matrix(by), (crop.cx - view_w / 2.0, crop.cy - view_h / 2.0));
        let side = sideways(by);
        let (w, h) = if side { (view_h, view_w) } else { (view_w, view_h) };
        Crop {
            cx: middle.0 + w / 2.0,
            cy: middle.1 + h / 2.0,
            w: if side { crop.h } else { crop.w },
            h: if side { crop.w } else { crop.h },
            angle: if mirrors(by) { -crop.angle } else { crop.angle },
        }
    }

    fn close(a: f64, b: f64, by: f64) -> bool {
        (a - b).abs() <= by
    }

    // Four crops as Adobe's own programs wrote them, for a 7008 × 4672
    // sensor. Each was made with the shape locked at 3:2.
    const ADOBE: [(u8, Stored); 4] = [
        (1, Stored { top: 0.005146, left: 0.030776, bottom: 0.936057, right: 0.969119, angle: -0.21 }),
        (8, Stored { top: 0.066883, left: 0.010957, bottom: 0.968034, right: 0.941733, angle: -0.85 }),
        (1, Stored { top: 0.006244, left: 0.001734, bottom: 0.993756, right: 0.998266, angle: -0.24 }),
        (1, Stored { top: 0.000883, left: 0.001739, bottom: 0.968907, right: 0.965386, angle: 0.12 }),
    ];

    #[test]
    fn adobes_own_crops_come_out_the_shape_they_were_made() {
        for (orientation, stored) in ADOBE {
            let crop = to_view(&stored, orientation, 7008.0, 4672.0).unwrap();
            let shape = if sideways(orientation) { crop.h / crop.w } else { crop.w / crop.h };
            assert!(close(shape, 1.5, 0.0005), "orientation {orientation}: {shape}");
            if sideways(orientation) {
                assert!(crop.h > crop.w, "a photo stored on its side is cropped tall");
                assert!(crop.cx < 4672.0 && crop.cy < 7008.0);
            }
        }
    }

    #[test]
    fn a_crop_survives_the_trip_to_the_view_and_back_every_way_up() {
        let stored = Stored { left: 0.12, top: 0.2, right: 0.71, bottom: 0.83, angle: 7.5 };
        for orientation in 1..=8 {
            let crop = to_view(&stored, orientation, 6000.0, 4000.0).unwrap();
            let back = to_stored(&crop, orientation, 6000.0, 4000.0);
            for (a, b) in [(back.left, stored.left), (back.top, stored.top), (back.right, stored.right), (back.bottom, stored.bottom), (back.angle, stored.angle)] {
                assert!(close(a, b, 1e-9), "orientation {orientation}: {back:?}");
            }
        }
    }

    #[test]
    fn a_level_crop_is_where_the_picture_under_it_goes() {
        // The top-left eighth of the sensor.
        let stored = Stored { left: 0.0, top: 0.0, right: 0.25, bottom: 0.5, angle: 0.0 };
        // Turned a quarter clockwise, the top-left of the sensor is the top-right of the view.
        let crop = to_view(&stored, 6, 6000.0, 4000.0).unwrap();
        assert!(close(crop.cx, 4000.0 - 1000.0, 1e-9) && close(crop.cy, 750.0, 1e-9), "{crop:?}");
        assert!(close(crop.w, 2000.0, 1e-9) && close(crop.h, 1500.0, 1e-9));
        // Mirrored left to right, it is the top-right too, and no taller.
        let crop = to_view(&stored, 2, 6000.0, 4000.0).unwrap();
        assert!(close(crop.cx, 6000.0 - 750.0, 1e-9) && close(crop.cy, 1000.0, 1e-9), "{crop:?}");
    }

    #[test]
    fn a_mirror_turns_a_clockwise_tilt_the_other_way() {
        let stored = Stored { left: 0.2, top: 0.2, right: 0.8, bottom: 0.8, angle: 10.0 };
        for (orientation, sign) in [(1, 1.0), (3, 1.0), (6, 1.0), (8, 1.0), (2, -1.0), (4, -1.0), (5, -1.0), (7, -1.0)] {
            assert_eq!(to_view(&stored, orientation, 6000.0, 4000.0).unwrap().angle, sign * 10.0, "orientation {orientation}");
        }
    }

    #[test]
    fn turning_the_photo_leaves_what_is_stored_of_its_crop_alone() {
        // Rotate and Flip change which way up the sidecar says the photo is,
        // and move the crop on screen to match. The crop Adobe reads, which
        // is against the sensor, must not move.
        let stored = Stored { left: 0.1, top: 0.15, right: 0.62, bottom: 0.7, angle: -6.0 };
        for file in 1..=8u8 {
            for by in 1..=8u8 {
                let before = stated(file, 1);
                let (sw, sh) = (6000.0, 4000.0);
                let crop = to_view(&stored, before, sw, sh).unwrap();
                let (vw, vh) = if sideways(before) { (sh, sw) } else { (sw, sh) };
                let after = then(before, by);
                let back = to_stored(&turned(&crop, by, vw, vh), after, sw, sh);
                for (a, b) in [(back.left, stored.left), (back.top, stored.top), (back.right, stored.right), (back.bottom, stored.bottom), (back.angle, stored.angle)] {
                    assert!(close(a, b, 1e-9), "file {file}, turned by {by}: {back:?}");
                }
            }
        }
    }

    #[test]
    fn orientations_combine_and_undo_as_turns_do() {
        for n in 1..=8 {
            assert_eq!(then(n, inverse(n)), 1, "{n}");
            assert_eq!(then(inverse(n), n), 1, "{n}");
            assert_eq!(then(1, n), n);
        }
        assert_eq!(then(6, 6), 3, "two quarter turns are a half");
        assert_eq!(then(6, 8), 1);
        assert_eq!(then(2, 6), 7, "mirrored, then a quarter clockwise");
        // The turn and what the sidecar must say are each other's reverse.
        for file in 1..=8 {
            for said in 1..=8 {
                assert_eq!(stated(file, turn(file, Some(said))), said, "file {file}, sidecar {said}");
            }
            assert_eq!(turn(file, None), 1);
            assert_eq!(turn(file, Some(file)), 1, "a sidecar that agrees with the file turns nothing");
        }
    }

    const WITH: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:tiff="http://ns.adobe.com/tiff/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" tiff:Orientation="6" crs:CropTop="0" crs:CropLeft="0" crs:CropBottom="0.5" crs:CropRight="0.25" crs:CropAngle="0" crs:HasCrop="True"/></rdf:RDF></x:xmpmeta>"#;

    fn level(upright: (u32, u32)) -> Shown {
        Shown { file: 1, drawn: 1, upright, stated: Some(upright) }
    }

    #[test]
    fn a_folder_is_told_each_photos_framing_in_the_view() {
        // The file says it is level and the sidecar says a quarter turn.
        let seen = seen(Some(WITH), None, &level((6000, 4000)));
        assert!(seen.framable);
        assert_eq!(seen.framing.turn, 6);
        let crop = seen.framing.crop.unwrap();
        assert!(close(crop.cx, 3000.0, 1e-9) && close(crop.cy, 750.0, 1e-9) && close(crop.w, 2000.0, 1e-9) && close(crop.h, 1500.0, 1e-9), "{crop:?}");

        // A sidecar that agrees with the file turns nothing.
        let agrees = Shown { file: 6, drawn: 6, upright: (4000, 6000), stated: Some((6000, 4000)) };
        assert_eq!(super::seen(Some(WITH), None, &agrees).framing.turn, 1);

        // No sidecar, nothing to say, and nothing in the way of saying it.
        assert_eq!(super::seen(None, None, &level((6000, 4000))), Seen { framing: Framing::default(), framable: true });
    }

    #[test]
    fn a_crop_inside_the_file_is_read_but_a_turn_there_is_not() {
        let seen = seen(None, Some(WITH), &level((6000, 4000)));
        assert_eq!(seen.framing.turn, 1, "the file already says which way up it is");
        let crop = seen.framing.crop.unwrap();
        assert!(close(crop.cx, 750.0, 1e-9) && close(crop.cy, 1000.0, 1e-9), "{crop:?}");
    }

    #[test]
    fn a_photo_is_not_cropped_where_the_crop_could_land_wrongly() {
        // A sidecar that cannot be followed.
        assert_eq!(seen(Some("<x:xmpmeta Crop"), None, &level((6000, 4000))), Seen { framing: Framing::default(), framable: false });
        // A RAW of the whole sensor, drawn through a 16:9 JPEG.
        let wide = Shown { file: 1, drawn: 1, upright: (6000, 3376), stated: Some((6000, 4000)) };
        assert!(!seen(Some(WITH), None, &wide).framable);
        // A pair whose two files disagree about which way up they are.
        let split = Shown { file: 1, drawn: 6, upright: (4000, 6000), stated: None };
        assert!(!seen(None, None, &split).framable);
        // Nothing yet known of its size.
        assert!(!seen(None, None, &level((0, 0))).framable);
    }

    #[test]
    fn numbers_that_are_not_a_crop_are_not_one() {
        let inside_out = Stored { left: 0.8, top: 0.8, right: 0.2, bottom: 0.2, angle: 0.0 };
        assert!(to_view(&inside_out, 1, 6000.0, 4000.0).is_none());
        let nonsense = Stored { left: f64::NAN, top: 0.0, right: 1.0, bottom: 1.0, angle: 0.0 };
        assert!(to_view(&nonsense, 1, 6000.0, 4000.0).is_none());
        assert!(Stored { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0, angle: 0.0 }.is_whole());
    }
}
