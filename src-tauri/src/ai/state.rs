//! The state a face is in: how open its eyes are and why, where it is
//! turned, and its expression — named the way a photographer would say it.
//!
//! Eyes that measure narrow are not all a fault. Someone looking down at a
//! child, laughing, or turned to the person beside them has narrow or hidden
//! eyes and nothing wrong with the frame; someone caught blinking has eyes
//! that look much the same. So the closer look at the eye pair also says how
//! far the lids are open and whether the eyes are cast down, another network
//! looks at the lower half of the face for the expression, and with the
//! judge's reading of the whole face they are put together into one state,
//! and into how likely the face is to be a blink.
//!
//! MEASURED with the whole app on 406 faces in 26 shoots none of it learned
//! from, against answers given by eye: how open the eyes are, within one
//! step of five in 97%; barely open or closed told from the rest 0.98 where
//! the eye ratio tells them 0.83 (1.00 is perfect, 0.50 a coin); looking
//! down 0.98; smiling or laughing against not, 0.96. All 12 blinks among
//! them are given a state of concern 2 or 3, and 23 of the 379 others.

use std::cell::RefCell;

use super::judge::Verdict;
use super::{eyes, onnx};
use crate::faces::{self, FaceBox, MOUTH_BAND};

static MOUTH: &[u8] = include_bytes!("../../models/mouth.onnx");
thread_local!(static MOUTH_SESSION: RefCell<Option<ort::session::Session>> = const { RefCell::new(None) });

/// What the closer look at an eye pair makes of the lids.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lids {
    /// Fully open, mostly, partly, barely, closed, cannot be seen.
    pub open: [f32; 6],
    /// How likely the eyes are cast down.
    pub down: f32,
}

impl From<&eyes::Seen> for Lids {
    fn from(seen: &eyes::Seen) -> Self {
        Lids { open: seen.open, down: seen.down }
    }
}

/// The expression of each face, from a closer look at its lower half:
/// neutral, smiling, laughing, mid-word, something else.
pub fn mouths(rgb: &[u8], width: u32, height: u32, boxes: &[FaceBox]) -> Result<Vec<[f32; 5]>, String> {
    if boxes.is_empty() {
        return Ok(Vec::new());
    }
    let (n, w, h) = (boxes.len(), MOUTH_BAND.0 as usize, MOUTH_BAND.1 as usize);
    let mut band_in = Vec::with_capacity(n * 3 * w * h);
    for b in boxes {
        let band = faces::cut(rgb, width, height, b.mouth_rect(width, height), MOUTH_BAND.0, MOUTH_BAND.1).unwrap_or_else(|_| vec![0; 3 * w * h]);
        for channel in 0..3 {
            band_in.extend((0..w * h).map(|i| band[i * 3 + channel] as f32));
        }
    }
    onnx::run(&MOUTH_SESSION, MOUTH, vec![("band", vec![n, 3, h, w], band_in)], &["expression"], |out| {
        (0..n)
            .map(|i| {
                let mut e = [0.0; 5];
                e.copy_from_slice(&out[0].1[i * 5..(i + 1) * 5]);
                e
            })
            .collect()
    })
}

/// A face's state, in a photographer's words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    EyesFullyOpen,
    EyesMostlyOpen,
    SmilingEyesOpen,
    LaughingEyesOpen,
    LookingDown,
    FacingAway,
    VeryClose,
    SmilingEyesClosed,
    LaughingEyesClosed,
    EyesPartiallyOpen,
    EyesObscured,
    EyesBarelyOpen,
    EyesClosed,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::EyesFullyOpen => "Eyes Fully Open",
            State::EyesMostlyOpen => "Eyes Mostly Open",
            State::SmilingEyesOpen => "Smiling · Eyes Open",
            State::LaughingEyesOpen => "Laughing · Eyes Open",
            State::LookingDown => "Looking Down",
            State::FacingAway => "Facing Away",
            State::VeryClose => "Very Close",
            State::SmilingEyesClosed => "Smiling · Eyes Closed",
            State::LaughingEyesClosed => "Laughing · Eyes Closed",
            State::EyesPartiallyOpen => "Eyes Partially Open",
            State::EyesObscured => "Eyes Obscured",
            State::EyesBarelyOpen => "Eyes Barely Open",
            State::EyesClosed => "Eyes Closed",
        }
    }

    /// How much there is to worry about: 0 nothing, 1 worth a look, 2
    /// probably a fault, 3 a fault.
    pub fn concern(self) -> u8 {
        match self {
            State::EyesPartiallyOpen | State::EyesObscured => 1,
            State::EyesBarelyOpen => 2,
            State::EyesClosed => 3,
            _ => 0,
        }
    }
}

/// Everything read of one face that its state is made from.
#[derive(Clone, Copy, Debug)]
pub struct Read {
    pub lids: Lids,
    /// Neutral, smiling, laughing, mid-word, something else: the judge's
    /// reading of the whole face and the closer look at the mouth, averaged.
    pub expression: [f32; 5],
    /// How likely the head is turned right away.
    pub profile: f32,
    /// How likely the person is looking down, from the set of the head.
    pub down_by_face: f32,
    pub head_yaw: f32,
    /// The face's height as a share of the frame's.
    pub height: f64,
}

impl Read {
    pub fn new(lids: Lids, mouth: [f32; 5], v: &Verdict, height: f64) -> Self {
        let mut expression = [0.0; 5];
        for (i, e) in expression.iter_mut().enumerate() {
            *e = (v.expression[i] + mouth[i]) / 2.0;
        }
        Read { lids, expression, profile: v.profile, down_by_face: v.down, head_yaw: v.head_yaw, height }
    }
}

/// What each figure counts for towards "this is a blink", and what is added
/// to their sum: partly open, barely, closed, cannot be seen, cast down by
/// the eyes, looking down by the head, turned away, neutral, smiling,
/// laughing, mid-word, how far the head is turned. Fitted, on answers each
/// face was given by networks that had not learned from its shoot; not to be
/// adjusted by hand.
const BLINK_WEIGHTS: [f64; 12] = [3.6829, 3.977, 5.8644, -2.0408, -0.9023, -0.3254, -0.8318, 0.0172, 0.5547, -2.0572, -0.3816, -0.5172];
const BLINK_BIAS: f64 = -5.6107;
/// From here up a face is taken for a blink, whatever else would explain
/// its eyes.
///
/// Chosen on 1,573 faces in 80 shoots, 53 of them blinks by eye, each read
/// by networks and weights that had not learned from its shoot: where one
/// in ten of the faces that are not blinks is taken for one.
pub const BLINK: f64 = 0.066;
/// How sure it must be that the eyes are cast down, the expression is a
/// smile or a laugh, or the head is turned away, for that to explain eyes
/// that are not open.
const EXPLAINS: f32 = 0.5;
/// A face this tall in the frame is a close-up: its eyes fill the picture
/// and are read as they are, with no state of their own to report.
const VERY_CLOSE: f64 = 0.5;

/// How likely a face is to be a blink, 0 to 1.
pub fn blink(r: &Read) -> f64 {
    let o = r.lids.open;
    let x = [o[2], o[3], o[4], o[5], r.lids.down, r.down_by_face, r.profile, r.expression[0], r.expression[1], r.expression[2], r.expression[3], r.head_yaw.abs().min(2.0)];
    let sum: f64 = x.iter().zip(BLINK_WEIGHTS).map(|(x, w)| *x as f64 * w).sum();
    1.0 / (1.0 + (-(sum + BLINK_BIAS)).exp())
}

/// The state of one face.
pub fn of(r: &Read) -> State {
    let level = (0..6).max_by(|a, b| r.lids.open[*a].total_cmp(&r.lids.open[*b])).expect("six answers");
    let (smiling, laughing) = (r.expression[1], r.expression[2]);
    let happy = smiling + laughing >= EXPLAINS;
    let shut = [State::EyesPartiallyOpen, State::EyesBarelyOpen, State::EyesClosed];
    // A blink is a blink whatever else is true of the face: a smile does
    // not explain eyes caught half way down.
    if (2..=4).contains(&level) && blink(r) >= BLINK {
        return shut[level - 2];
    }
    if r.profile >= EXPLAINS {
        return State::FacingAway;
    }
    match level {
        5 => State::EyesObscured,
        0 | 1 => {
            if laughing >= EXPLAINS {
                State::LaughingEyesOpen
            } else if happy {
                State::SmilingEyesOpen
            } else if r.height >= VERY_CLOSE {
                State::VeryClose
            } else if level == 0 {
                State::EyesFullyOpen
            } else {
                State::EyesMostlyOpen
            }
        }
        _ => {
            if (r.lids.down + r.down_by_face) / 2.0 >= EXPLAINS {
                State::LookingDown
            } else if happy && laughing > smiling {
                State::LaughingEyesClosed
            } else if happy {
                State::SmilingEyesClosed
            } else {
                shut[level - 2]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(open: [f32; 6], down: f32, expression: [f32; 5], profile: f32) -> Read {
        Read { lids: Lids { open, down }, expression, profile, down_by_face: down, head_yaw: 0.0, height: 0.1 }
    }

    const NEUTRAL: [f32; 5] = [0.9, 0.05, 0.0, 0.05, 0.0];
    const SMILING: [f32; 5] = [0.05, 0.85, 0.05, 0.05, 0.0];
    const LAUGHING: [f32; 5] = [0.02, 0.2, 0.75, 0.03, 0.0];
    const OPEN: [f32; 6] = [0.8, 0.15, 0.05, 0.0, 0.0, 0.0];
    const PARTLY: [f32; 6] = [0.02, 0.18, 0.6, 0.15, 0.05, 0.0];
    const CLOSED: [f32; 6] = [0.0, 0.0, 0.02, 0.08, 0.9, 0.0];

    #[test]
    fn open_eyes_are_named_by_the_expression_with_them() {
        assert_eq!(of(&read(OPEN, 0.05, NEUTRAL, 0.0)), State::EyesFullyOpen);
        assert_eq!(of(&read(OPEN, 0.05, SMILING, 0.0)), State::SmilingEyesOpen);
        assert_eq!(of(&read(OPEN, 0.05, LAUGHING, 0.0)), State::LaughingEyesOpen);
        assert_eq!(of(&read(OPEN, 0.05, SMILING, 0.0)).concern(), 0);
    }

    #[test]
    fn eyes_cast_down_are_looking_down_and_nothing_to_worry_about() {
        let got = of(&read(PARTLY, 0.95, NEUTRAL, 0.0));
        assert_eq!(got, State::LookingDown);
        assert_eq!(got.concern(), 0);
    }

    #[test]
    fn lids_part_way_down_in_a_laugh_are_the_laugh() {
        assert_eq!(of(&read(PARTLY, 0.05, LAUGHING, 0.0)), State::LaughingEyesClosed);
    }

    #[test]
    fn eyes_shut_with_nothing_to_explain_them_are_a_fault() {
        let got = of(&read(CLOSED, 0.02, NEUTRAL, 0.0));
        assert_eq!(got, State::EyesClosed);
        assert_eq!(got.concern(), 3);
    }

    #[test]
    fn a_smile_does_not_explain_eyes_shut_tight_at_the_camera() {
        // Posed, smiling, and caught blinking: the frame-spoiler.
        let face = read(CLOSED, 0.02, SMILING, 0.0);
        assert!(blink(&face) >= BLINK);
        assert_eq!(of(&face), State::EyesClosed);
    }

    #[test]
    fn a_head_turned_right_away_is_facing_away_whatever_its_eyes() {
        assert_eq!(of(&read(OPEN, 0.1, NEUTRAL, 0.9)), State::FacingAway);
    }

    #[test]
    fn a_close_up_with_open_eyes_and_no_smile_is_very_close() {
        let mut face = read(OPEN, 0.05, NEUTRAL, 0.0);
        face.height = 0.7;
        assert_eq!(of(&face), State::VeryClose);
    }

    #[test]
    fn eyes_that_cannot_be_seen_are_obscured_and_worth_a_look() {
        let got = of(&read([0.0, 0.05, 0.05, 0.0, 0.0, 0.9], 0.1, NEUTRAL, 0.0));
        assert_eq!(got, State::EyesObscured);
        assert_eq!(got.concern(), 1);
    }

    #[test]
    fn mouths_are_read_for_every_face_and_each_reading_adds_to_one() {
        let (w, h) = (640u32, 480u32);
        let rgb: Vec<u8> = (0..w * h * 3).map(|i| ((i * 7) % 253) as u8).collect();
        let boxes = [FaceBox { mid_x: 0.3, mid_y: 0.5, w: 0.2, h: 0.3 }, FaceBox { mid_x: 0.999, mid_y: 0.001, w: 0.05, h: 0.05 }];
        let got = mouths(&rgb, w, h, &boxes).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|e| (e.iter().sum::<f32>() - 1.0).abs() < 1e-3));
        assert!(mouths(&rgb, w, h, &[]).unwrap().is_empty());
    }
}
