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

use super::{focus, Analysis, Rules};

/// Changes whenever the models or the rules applied to them do, so that
/// verdicts from an earlier version are never shown as this one's.
const ADVANCED: &str = "finder-yunet-0.6|judge-3h|junk-0.8-unsure-0.5|shut-0.9|away-0.50-eyes-0.6-agree-0.2-turned-posed-0.4|rules-3|eyes-2|states-3|mark-4|key-1";
/// The basic rules are those of release 0.3 and are remembered under its
/// name, so a folder analysed by that release is not analysed again.
const BASIC: &str = "finder-yunet-0.6|judge-3h|junk-0.8-unsure-0.5|shut-0.9|away-0.50-glance-0.15|rules-2|key-1";

/// What each set of rules is remembered under. What one found is never
/// shown as the other's: each has its own record of a folder.
pub fn version(rules: Rules) -> &'static str {
    match rules {
        Rules::Basic => BASIC,
        Rules::Advanced => ADVANCED,
    }
}

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
    /// Not called looking away, but worth a look: everyone else is looking
    /// and this one may not be.
    #[serde(default)]
    pub maybe_away: bool,
    /// How sharp its eyes are, out of 100. `None` when it could not be told.
    pub sharpness: Option<f32>,
    /// How open its eyes measure: the eye ratio, about 0.3 when wide open.
    pub eye: f32,
    /// Whether that was seen well enough to be set beside the same person's
    /// eyes in another frame.
    pub eyes_read: bool,
    /// The state it is in, in words: "Looking Down", "Smiling · Eyes Open".
    #[serde(default)]
    pub state: Option<String>,
    /// How much there is to worry about in that: 0 nothing, 1 worth a look,
    /// 2 probably a fault, 3 a fault.
    #[serde(default)]
    pub concern: u8,
    /// The least its eyes may count for in the photo's mark, 0 to 1: how
    /// far that state excuses eyes that measure narrow.
    #[serde(default)]
    pub excused: f32,
    /// Whether its state says the eyes are open, so that how open they
    /// measure is inside what is acceptable.
    #[serde(default)]
    pub open: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Record {
    #[serde(default, rename = "motionBlur")]
    pub motion_blur: Option<f32>,
    #[serde(default, rename = "motionVersion")]
    pub motion_version: u8,
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
                maybe_away: f.maybe_away,
                sharpness: f.focus_log.map(|log| focus::score(10f64.powf(log)) as f32),
                eye: f.verdict.ear,
                eyes_read: f.eyes_read,
                state: f.state.map(|s| s.name().to_string()),
                concern: f.state.map_or(0, |s| s.concern(f.facing)),
                excused: f.state.map_or(0.0, |s| s.excused(f.facing)) as f32,
                open: f.open,
            })
            .collect();
        let key = a.key.map(|k| KeyRecord { x: k.x as f32, y: k.y as f32, w: k.w as f32, h: k.h as f32 });
        Record { faces, score: a.score.map(|s| s as f32), focus: a.parts.map(|p| p.focus as f32), gaze: a.parts.map(|p| p.gaze as f32), key, motion_blur: None, motion_version: 0 }
    }
}

/// A photo's remembered analysis, with the version of the file it describes.
type Entry = (String, Record);

#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
    writer: std::sync::Arc<std::sync::Mutex<()>>,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        if let Err(error) = std::fs::create_dir_all(&dir) { eprintln!("[ai-cache] cannot create {}: {error}", dir.display()); }
        Self { dir, writer: Default::default() }
    }

    fn file_for(&self, folder: &Path, rules: Rules) -> PathBuf {
        let key = format!("{}\0{}", version(rules), folder.to_string_lossy());
        self.dir.join(format!("{:032x}.json", xxh3_128(key.as_bytes())))
    }

    /// What is remembered about a folder's photos, by path. Empty for a
    /// folder never analysed, or one whose record cannot be read.
    pub fn load(&self, folder: &Path, rules: Rules) -> HashMap<String, Entry> {
        let Some(values) = std::fs::read(self.file_for(folder, rules)).ok().and_then(|bytes| serde_json::from_slice::<HashMap<String, serde_json::Value>>(&bytes).ok()) else { return HashMap::new(); };
        // One damaged entry must not discard a whole shoot's good analysis.
        values.into_iter().filter_map(|(path, value)| serde_json::from_value(value).ok().map(|entry| (path, entry))).collect()
    }

    /// A session's root can change (card, DCIM, camera subfolder, multi-card
    /// review). Reuse the same files' analysis through their containing folders.
    pub fn load_images<'a>(&self, folder: &Path, rules: Rules, paths: impl IntoIterator<Item = (&'a Path, &'a str)>) -> HashMap<String, Entry> {
        let requested: HashMap<_, _> = paths.into_iter().map(|(path, version)| (path.to_string_lossy().into_owned(), version)).collect();
        let wanted = |path: &String, entry: &Entry| requested.get(path).is_some_and(|version| *version == entry.0);
        let mut known = self.load(folder, rules);
        known.retain(|path, entry| wanted(path, entry));
        // Read older parent-root caches too, so the existing cache migrates
        // without running the face models again after this update.
        let parents: std::collections::HashSet<_> = requested.keys().flat_map(|p| Path::new(p).parent().into_iter().flat_map(|p| p.ancestors().take(4))).filter(|p| *p != folder).collect();
        for parent in parents {
            for (path, entry) in self.load(parent, rules).into_iter().filter(|(path, entry)| wanted(path, entry)) {
                if known.get(&path).is_some_and(|old| old.1.motion_version > entry.1.motion_version) { continue; }
                known.insert(path, entry);
            }
        }
        known
    }

    pub fn save(&self, folder: &Path, rules: Rules, known: &HashMap<String, Entry>) {
        let _guard = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        let mut folders: HashMap<PathBuf, HashMap<String, Entry>> = HashMap::new();
        folders.insert(folder.to_path_buf(), known.clone());
        for (path, entry) in known {
            if let Some(parent) = Path::new(path).parent() { folders.entry(parent.to_path_buf()).or_default().insert(path.clone(), entry.clone()); }
        }
        for (parent, entries) in folders {
            // Keep completed photos if a newer session contains only a subset.
            let mut merged = self.load(&parent, rules);
            for (path, entry) in entries {
                // A cancelled older scan must not erase newer blur enrichment.
                if merged.get(&path).is_some_and(|old| old.0 == entry.0 && old.1.motion_version > entry.1.motion_version) { continue; }
                merged.insert(path, entry);
            }
            self.write(&parent, rules, &merged);
        }
    }

    fn write(&self, folder: &Path, rules: Rules, known: &HashMap<String, Entry>) {
        let Ok(bytes) = serde_json::to_vec(known) else { return };
        let path = self.file_for(folder, rules);
        let temp = path.with_extension(format!("tmp{}", std::process::id()));
        if let Err(error) = std::fs::write(&temp, bytes).and_then(|_| std::fs::rename(&temp, &path)) {
            eprintln!("[ai-cache] could not save {}: {error}", path.display());
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
        let _guard = self.writer.lock().unwrap_or_else(|e| e.into_inner());
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

    #[test]
    fn cached_photos_survive_new_stores_and_different_session_roots() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-shared-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let photo = Path::new("/card/DCIM/100CAM/A.RAW");
        let first = Store::new(dir.clone());
        first.save(Path::new("/card"), Rules::Advanced, &HashMap::from([(photo.to_string_lossy().into_owned(), ("v1".into(), record(80.0)))]));
        let reopened = Store::new(dir.clone());
        let known = reopened.load_images(Path::new("/card/DCIM/100CAM"), Rules::Advanced, [(photo, "v1")]);
        assert_eq!(remembered(&known, photo, "v1"), Some(record(80.0)));
        assert_eq!(remembered(&known, photo, "v2"), None);
        assert!(reopened.load_images(Path::new("/card"), Rules::Basic, [(photo, "v1")]).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn legacy_root_cache_and_good_entries_are_reused_without_reanalysis() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-legacy-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let root = Path::new("/card/DCIM");
        let photo = Path::new("/card/DCIM/100CAM/A.RAW");
        let known = HashMap::from([(photo.to_string_lossy().into_owned(), ("v2".into(), record(80.0)))]);
        store.write(root, Rules::Advanced, &known);
        let mut raw: serde_json::Value = serde_json::from_slice(&std::fs::read(store.file_for(root, Rules::Advanced)).unwrap()).unwrap();
        raw["/card/DCIM/100CAM/broken.RAW"] = serde_json::json!(["v2", { "faces": "bad" }]);
        std::fs::write(store.file_for(root, Rules::Advanced), serde_json::to_vec(&raw).unwrap()).unwrap();
        let back = store.load_images(photo.parent().unwrap(), Rules::Advanced, [(photo, "v2")]);
        assert_eq!(remembered(&back, photo, "v2"), Some(record(80.0)));
        store.save(root, Rules::Advanced, &back);
        let empty: HashMap<String, Entry> = HashMap::new();
        store.save(root, Rules::Advanced, &empty);
        assert_eq!(remembered(&store.load(root, Rules::Advanced), photo, "v2"), Some(record(80.0)), "an incomplete scan must not erase completed work");
        std::fs::remove_dir_all(dir).ok();
    }

    fn record(score: f32) -> Record {
        let face = FaceRecord { x: 0.1, y: 0.2, w: 0.05, h: 0.07, subject: true, reason: None, eyes_shut: false, looking_away: true, maybe_away: false, sharpness: Some(80.0), eye: 0.28, eyes_read: true, state: Some("Looking Down".into()), concern: 0, excused: 0.85, open: false };
        Record { faces: vec![face], score: Some(score), focus: Some(0.5), gaze: Some(1.0), key: None, ..Default::default() }
    }

    #[test]
    fn what_is_saved_comes_back_and_only_for_the_file_it_describes() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-store-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let folder = Path::new("/shoots/wedding");
        assert!(store.load(folder, Rules::Advanced).is_empty(), "nothing is remembered of a folder never analysed");

        let mut known = HashMap::new();
        known.insert("/shoots/wedding/A.ARW".to_string(), ("v1".to_string(), record(72.5)));
        store.save(folder, Rules::Advanced, &known);

        let back = store.load(folder, Rules::Advanced);
        assert_eq!(remembered(&back, Path::new("/shoots/wedding/A.ARW"), "v1"), Some(record(72.5)));
        assert_eq!(remembered(&back, Path::new("/shoots/wedding/A.ARW"), "v2"), None, "the file has changed since");
        assert_eq!(remembered(&back, Path::new("/shoots/wedding/B.ARW"), "v1"), None);
        assert!(store.load(Path::new("/shoots/other"), Rules::Advanced).is_empty(), "another folder's record is its own");
        assert!(store.load(folder, Rules::Basic).is_empty(), "and so is what the other rules found in this one");

        assert_eq!(store.usage().1, 1);
        store.clear();
        assert!(store.load(folder, Rules::Advanced).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_damaged_record_costs_a_fresh_analysis_not_an_error() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-store-bad-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let folder = Path::new("/shoots/wedding");
        std::fs::write(store.file_for(folder, Rules::Advanced), b"{ not json").unwrap();
        assert!(store.load(folder, Rules::Advanced).is_empty());
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
            maybe_away: false,
            facing: false,
            eyes_away: None,
            state: Some(crate::ai::state::State::LookingDown),
            blink: Some(0.01),
            open: false,
            eyes_read: true,
        };
        let r = Record::from(&Analysis { faces: vec![face], score: Some(50.0), parts: None, key: None });
        let f = &r.faces[0];
        assert!((f.x - 0.4).abs() < 1e-6 && (f.y - 0.2).abs() < 1e-6, "its middle is three quarters of the way UP, so its top is a fifth of the way down");
        assert_eq!(f.reason.as_deref(), Some("Background — small beside the others"));
        assert!((f.sharpness.unwrap() - 100.0).abs() < 1e-3);
    }
}
