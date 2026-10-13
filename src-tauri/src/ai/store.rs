//! What the analysis found, remembered between openings of a folder.
//!
//! Analysing a photo means decoding all of it and running two models over
//! it. A folder that has been through that once should never go through it
//! again unless a photo changes — or the models do, in which case everything
//! remembered is from a different judge and is not used.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_128;

use super::{focus, Analysis};

/// Changes whenever the models or the rules applied to them do, so that
/// verdicts from an earlier version are never shown as this one's.
pub const VERSION: &str = "finder-yunet-0.6|judge-3h|junk-0.8-unsure-0.5|shut-0.9|away-0.50-glance-0.15|rules-3|eyes-1-0.7|key-1";

/// One face, as it is remembered and as the interface is told of it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FaceRecord {
    /// Left, top, width and height, as fractions of the upright photo.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub subject: bool,
    /// Why it was left out, for a face that is not a subject.
    pub reason: Option<String>,
    pub eyes_shut: bool,
    pub looking_away: bool,
    /// How sharp its eyes are, out of 100. `None` when it could not be told.
    pub sharpness: Option<f32>,
    /// How open its eyes measure: the eye ratio, about 0.3 when wide open.
    pub eye: f32,
    /// Whether that was seen well enough to be set beside the same person's
    /// eyes in another frame.
    pub eyes_read: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub faces: Vec<FaceRecord>,
    pub score: Option<f32>,
    /// The parts of the mark that are not about how open the eyes are, each
    /// 0 to 1, kept so the photo can be marked again within its burst.
    pub focus: Option<f32>,
    pub gaze: Option<f32>,
    /// With no faces: the part of the photo it is about.
    pub key: Option<KeyRecord>,
}

/// What a photo with nobody in it is about, as it is remembered.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeyRecord {
    /// Left, top, width and height, as fractions of the upright photo.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl From<&Analysis> for Record {
    fn from(a: &Analysis) -> Self {
        let faces = a
            .faces
            .iter()
            .map(|f| FaceRecord {
                x: (f.place.mid_x - f.place.w / 2.0) as f32,
                y: (1.0 - f.place.mid_y - f.place.h / 2.0) as f32,
                w: f.place.w as f32,
                h: f.place.h as f32,
                subject: f.subject,
                reason: f.reason.badge().map(str::to_string),
                eyes_shut: f.eyes_shut,
                looking_away: f.looking_away,
                sharpness: f.focus_log.map(|log| focus::score(10f64.powf(log)) as f32),
                eye: f.verdict.ear,
                eyes_read: f.eyes_read,
            })
            .collect();
        let key = a.key.map(|k| KeyRecord { x: k.x as f32, y: k.y as f32, w: k.w as f32, h: k.h as f32 });
        Record { faces, score: a.score.map(|s| s as f32), focus: a.parts.map(|p| p.focus as f32), gaze: a.parts.map(|p| p.gaze as f32), key }
    }
}

/// A photo's remembered analysis, with the version of the file it describes.
type Entry = (String, Record);

#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        Self { dir }
    }

    fn file_for(&self, folder: &Path) -> PathBuf {
        let key = format!("{VERSION}\0{}", folder.to_string_lossy());
        self.dir.join(format!("{:032x}.json", xxh3_128(key.as_bytes())))
    }

    /// What is remembered about a folder's photos, by path. Empty for a
    /// folder never analysed, or one whose record cannot be read.
    pub fn load(&self, folder: &Path) -> HashMap<String, Entry> {
        std::fs::read(self.file_for(folder)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn save(&self, folder: &Path, known: &HashMap<String, Entry>) {
        let Ok(bytes) = serde_json::to_vec(known) else { return };
        let path = self.file_for(folder);
        let temp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&temp, bytes).is_ok() && std::fs::rename(&temp, &path).is_err() {
            std::fs::remove_file(&temp).ok();
        }
    }

    /// Bytes on disk, and how many folders that is.
    pub fn usage(&self) -> (u64, usize) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return (0, 0) };
        let sizes: Vec<u64> = entries.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).collect();
        (sizes.iter().sum(), sizes.len())
    }

    /// Forgets everything. Folders are analysed again when next opened.
    pub fn clear(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return };
        for entry in entries.flatten() {
            std::fs::remove_file(entry.path()).ok();
        }
    }
}

/// The remembered analysis of a photo, if it still describes the file.
pub fn remembered(known: &HashMap<String, Entry>, path: &Path, version: &str) -> Option<Record> {
    let (was, record) = known.get(path.to_string_lossy().as_ref())?;
    (was == version).then(|| record.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(score: f32) -> Record {
        let face = FaceRecord { x: 0.1, y: 0.2, w: 0.05, h: 0.07, subject: true, reason: None, eyes_shut: false, looking_away: true, sharpness: Some(80.0), eye: 0.28, eyes_read: true };
        Record { faces: vec![face], score: Some(score), focus: Some(0.5), gaze: Some(1.0), key: None }
    }

    #[test]
    fn what_is_saved_comes_back_and_only_for_the_file_it_describes() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-store-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let folder = Path::new("/shoots/wedding");
        assert!(store.load(folder).is_empty(), "nothing is remembered of a folder never analysed");

        let mut known = HashMap::new();
        known.insert("/shoots/wedding/A.ARW".to_string(), ("v1".to_string(), record(72.5)));
        store.save(folder, &known);

        let back = store.load(folder);
        assert_eq!(remembered(&back, Path::new("/shoots/wedding/A.ARW"), "v1"), Some(record(72.5)));
        assert_eq!(remembered(&back, Path::new("/shoots/wedding/A.ARW"), "v2"), None, "the file has changed since");
        assert_eq!(remembered(&back, Path::new("/shoots/wedding/B.ARW"), "v1"), None);
        assert!(store.load(Path::new("/shoots/other")).is_empty(), "another folder's record is its own");

        assert_eq!(store.usage().1, 1);
        store.clear();
        assert!(store.load(folder).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_damaged_record_costs_a_fresh_analysis_not_an_error() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-store-bad-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let folder = Path::new("/shoots/wedding");
        std::fs::write(store.file_for(folder), b"{ not json").unwrap();
        assert!(store.load(folder).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_face_is_remembered_from_its_top_left_corner_with_its_reason() {
        use crate::ai::{Face, Reason, Verdict};
        use crate::faces::FaceBox;
        let face = Face {
            place: FaceBox { mid_x: 0.5, mid_y: 0.75, w: 0.2, h: 0.1 },
            sure: 0.9,
            verdict: Verdict::default(),
            focus_log: Some(0.6f64.log10()),
            subject: false,
            reason: Reason::Small,
            eyes_shut: false,
            looking_away: false,
            eyes_away: None,
            eyes_read: true,
        };
        let r = Record::from(&Analysis { faces: vec![face], score: Some(50.0), parts: None, key: None });
        let f = &r.faces[0];
        assert!((f.x - 0.4).abs() < 1e-6 && (f.y - 0.2).abs() < 1e-6, "its middle is three quarters of the way UP, so its top is a fifth of the way down");
        assert_eq!(f.reason.as_deref(), Some("Background — small beside the others"));
        assert!((f.sharpness.unwrap() - 100.0).abs() < 1e-3);
    }
}
