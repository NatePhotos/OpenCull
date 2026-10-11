//! What the analysis found, remembered between openings of a folder.
//!
//! Analysing a photo means decoding all of it and running two models over
//! it. A folder that has been through that once should never go through it
//! again unless a photo changes — or the models do, in which case everything
//! remembered is from a different judge and is not used.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::{xxh3_128, xxh3_64};

use super::{focus, Analysis};

/// Changes whenever the rules applied to the models do, so that verdicts
/// from an earlier version are never shown as this one's. Bump it when any
/// threshold or weight in `ai/` changes. The models themselves need no bump:
/// `version()` adds a fingerprint of them.
pub const VERSION: &str = "finder-yunet-0.6|judge-3|junk-0.8|shut-0.9|away-0.90-glance-0.15|rules-2|key-1";

/// `VERSION`, and a fingerprint of the two models. A model swapped without
/// `VERSION` being bumped would otherwise serve the old model's verdicts.
pub fn version() -> &'static str {
    static FULL: OnceLock<String> = OnceLock::new();
    FULL.get_or_init(|| {
        let models = [super::finder::MODEL, super::judge::MODEL].concat();
        format!("{VERSION}|models-{:016x}", xxh3_64(&models))
    })
}

/// The file in the store that says which version wrote what is in it.
const MARK: &str = "version";

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
#[serde(rename_all = "camelCase")]
pub struct Record {
    /// The photo could not be analysed. It has no faces, no score and no
    /// key, and is left out of ranking rather than counted as empty.
    #[serde(default)]
    pub failed: bool,
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
#[serde(rename_all = "camelCase")]
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
        Record { failed: false, faces, score: a.score.map(|s| s as f32), focus: a.parts.map(|p| p.focus as f32), gaze: a.parts.map(|p| p.gaze as f32), key }
    }
}

impl Record {
    /// For a photo that could not be analysed.
    pub fn failed() -> Self {
        Record { failed: true, ..Default::default() }
    }
}

/// A photo's remembered analysis, with the version of the file it describes.
pub type Entry = (String, Record);

#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// Opens the store in `dir`. What an earlier version wrote there can
    /// never be used again, so it is cleared away now rather than left to
    /// fill the disk.
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        let store = Self { dir };
        let mark = store.dir.join(MARK);
        if std::fs::read_to_string(&mark).ok().as_deref() != Some(version()) {
            store.clear();
            crate::fsx::atomic_write(&mark, version().as_bytes(), false).ok();
        }
        store
    }

    fn file_for(&self, folder: &Path) -> PathBuf {
        let key = format!("{}\0{}", version(), folder.to_string_lossy());
        self.dir.join(format!("{:032x}.json", xxh3_128(key.as_bytes())))
    }

    /// What is remembered about a folder's photos, by path. Empty for a
    /// folder never analysed, or one whose record cannot be read.
    pub fn load(&self, folder: &Path) -> HashMap<String, Entry> {
        std::fs::read(self.file_for(folder)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn save(&self, folder: &Path, known: &HashMap<String, Entry>) {
        let Ok(bytes) = serde_json::to_vec(known) else { return };
        crate::fsx::atomic_write(&self.file_for(folder), &bytes, false).ok();
    }

    /// Bytes on disk, and how many folders that is.
    pub fn usage(&self) -> (u64, usize) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return (0, 0) };
        let sizes: Vec<u64> = entries.flatten().filter(|e| e.file_name() != MARK).filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).collect();
        (sizes.iter().sum(), sizes.len())
    }

    /// Forgets everything. Folders are analysed again when next opened.
    pub fn clear(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return };
        for entry in entries.flatten().filter(|e| e.file_name() != MARK) {
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

    /// The models' fingerprints, as THIRD-PARTY-NOTICES.md publishes them.
    /// Change a model and this fails until the notice is brought up to date.
    #[test]
    fn the_models_are_the_ones_the_notices_list() {
        let notices = include_str!("../../../THIRD-PARTY-NOTICES.md");
        for (name, bytes) in [("finder.onnx", super::super::finder::MODEL), ("judge.onnx", super::super::judge::MODEL)] {
            let row = format!("| `src-tauri/models/{name}` | `");
            let line = notices.lines().find(|l| l.starts_with(&row)).unwrap_or_else(|| panic!("no fingerprint listed for {name}"));
            let listed = line.rsplit('`').nth(1).unwrap();
            assert_eq!(listed, sha256_hex(bytes), "{name} has changed: update THIRD-PARTY-NOTICES.md");
        }
    }

    /// SHA-256, written out here rather than added as a dependency for one test.
    fn sha256_hex(data: &[u8]) -> String {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];
        let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
        let mut msg = data.to_vec();
        let bits = (data.len() as u64) * 8;
        msg.push(0x80);
        while msg.len() % 64 != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bits.to_be_bytes());
        for block in msg.chunks(64) {
            let mut w = [0u32; 64];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([block[i * 4], block[i * 4 + 1], block[i * 4 + 2], block[i * 4 + 3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
            }
            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ (!e & g);
                let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let t2 = s0.wrapping_add(maj);
                (hh, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
            }
            for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
                *x = x.wrapping_add(y);
            }
        }
        h.iter().map(|v| format!("{v:08x}")).collect()
    }

    fn record(score: f32) -> Record {
        let face = FaceRecord { x: 0.1, y: 0.2, w: 0.05, h: 0.07, subject: true, reason: None, eyes_shut: false, looking_away: true, sharpness: Some(80.0), eye: 0.28, eyes_read: true };
        Record { failed: false, faces: vec![face], score: Some(score), focus: Some(0.5), gaze: Some(1.0), key: None }
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
            reason: Reason::SmallAndIsolated,
            eyes_shut: false,
            looking_away: false,
            eyes_read: true,
        };
        let r = Record::from(&Analysis { faces: vec![face], score: Some(50.0), parts: None, key: None });
        let f = &r.faces[0];
        assert!((f.x - 0.4).abs() < 1e-6 && (f.y - 0.2).abs() < 1e-6, "its middle is three quarters of the way UP, so its top is a fifth of the way down");
        assert_eq!(f.reason.as_deref(), Some("Background — small and away from the group"));
        assert!((f.sharpness.unwrap() - 100.0).abs() < 1e-3);
    }
}
