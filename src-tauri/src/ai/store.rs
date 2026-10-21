//! What the analysis found, remembered between openings of a folder.
//!
//! Analysing a photo means decoding all of it and running two models over
//! it. A folder that has been through that once should never go through it
//! again unless a photo changes — or the models do, in which case everything
//! remembered is from a different judge and is not used.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_128;

use super::faceprint::LEN;
use super::scene::ScenePrint;
use super::{focus, Analysis, PhotoPrint, Rules};

/// Changes whenever the models or the rules applied to them do, so that
/// verdicts from an earlier version are never shown as this one's.
const ADVANCED: &str = "finder-turning-yolo26-3-0.3-patch-160|upright-1|judge-3h|junk-0.8-unsure-0.5|shut-0.9|away-0.50-eyes-0.6-agree-0.2-turned-posed-0.4|rules-3|eyes-2|states-3|mark-4|key-1|print-1";
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
        Record { faces, score: a.score.map(|s| s as f32), focus: a.parts.map(|p| p.focus as f32), gaze: a.parts.map(|p| p.gaze as f32), key }
    }
}

/// A kind of print a `PrintStore` keeps: what its file starts with, what it
/// is called, how many numbers it is, and how they are written and read back.
pub trait Packed: Clone + Send + 'static {
    const MAGIC: &'static [u8; 8];
    const EXT: &'static str;
    const FLOATS: usize;
    fn write_floats(&self, into: &mut Vec<u8>);
    fn from_floats(f: &[f32]) -> Self;
}

impl Packed for PhotoPrint {
    const MAGIC: &'static [u8; 8] = b"OCPRINT1";
    const EXT: &'static str = "prints";
    const FLOATS: usize = 2 * LEN;
    fn write_floats(&self, into: &mut Vec<u8>) {
        for x in self.mean.iter().chain(&self.min) {
            into.extend_from_slice(&x.to_le_bytes());
        }
    }
    fn from_floats(f: &[f32]) -> Self {
        PhotoPrint { mean: f[..LEN].to_vec(), min: f[LEN..].to_vec() }
    }
}

impl Packed for ScenePrint {
    const MAGIC: &'static [u8; 8] = b"OCSCENE1";
    const EXT: &'static str = "scenes";
    const FLOATS: usize = crate::ai::scene::LEN;
    fn write_floats(&self, into: &mut Vec<u8>) {
        for x in &self.0 {
            into.extend_from_slice(&x.to_le_bytes());
        }
    }
    fn from_floats(f: &[f32]) -> Self {
        ScenePrint(f.to_vec())
    }
}

/// A photo's remembered analysis, with the version of the file it describes.
type Entry = (String, Record);

#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
    /// The face prints of the same photos, kept beside their records.
    pub prints: PrintStore,
    /// Their scene prints, for grouping photos taken of the same scene.
    pub scenes: SceneStore,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        Self { prints: PrintStore::new(dir.clone()), scenes: SceneStore::new(dir.clone()), dir }
    }

    fn file_for(&self, folder: &Path, rules: Rules) -> PathBuf {
        let key = format!("{}\0{}", version(rules), folder.to_string_lossy());
        self.dir.join(format!("{:032x}.json", xxh3_128(key.as_bytes())))
    }

    /// What is remembered about a folder's photos, by path. Empty for a
    /// folder never analysed, or one whose record cannot be read.
    pub fn load(&self, folder: &Path, rules: Rules) -> HashMap<String, Entry> {
        std::fs::read(self.file_for(folder, rules)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn save(&self, folder: &Path, rules: Rules, known: &HashMap<String, Entry>) {
        let Ok(bytes) = serde_json::to_vec(known) else { return };
        let path = self.file_for(folder, rules);
        let temp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&temp, bytes).is_ok() && std::fs::rename(&temp, &path).is_err() {
            std::fs::remove_file(&temp).ok();
        }
    }

    /// Bytes on disk, and how many folders that is. A folder's prints sit
    /// beside its record and are counted in the bytes, not as a folder.
    pub fn usage(&self) -> (u64, usize) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return (0, 0) };
        let files: Vec<(u64, bool)> = entries
            .flatten()
            .filter_map(|e| Some((e.metadata().ok().filter(|m| m.is_file())?.len(), e.path().extension().is_some_and(|x| x == PhotoPrint::EXT || x == ScenePrint::EXT))))
            .collect();
        (files.iter().map(|f| f.0).sum(), files.iter().filter(|f| !f.1).count())
    }

    /// Forgets everything. Folders are analysed again when next opened.
    pub fn clear(&self) {
        self.prints.forget();
        self.scenes.forget();
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return };
        for entry in entries.flatten() {
            std::fs::remove_file(entry.path()).ok();
        }
    }
}

/// The prints of one kind (face prints, scene prints) of a folder's photos,
/// remembered as the records are: by folder, under the Advanced rules'
/// version, each with the version of the file it describes. Prints are only
/// ever made by the Advanced rules.
///
/// Prints are large next to a record, so they are kept apart from the
/// records, per folder, in a file only ever added to: saving writes only
/// what is new, not the whole folder again. The two folders last asked about are held in memory, so judging
/// one folder does not put out the folder being scanned.
#[derive(Clone)]
pub struct PrintStore<P: Packed = PhotoPrint> {
    dir: PathBuf,
    open: Arc<Mutex<Held<P>>>,
    /// Woken whenever a folder's file has been read in.
    read_in: Arc<Condvar>,
    /// Counts the files of prints read, for the tests.
    #[cfg(test)]
    reads: Arc<std::sync::atomic::AtomicUsize>,
}

/// The folders whose prints are held, the one last asked about last.
struct Held<P> {
    folders: Vec<Prints<P>>,
    /// The folders whose file is being read in. Whoever else asks about one
    /// waits for that read rather than reading the file again.
    reading: Vec<PathBuf>,
    /// Counts the times a folder stopped being held, after which a file read
    /// meanwhile may miss what was written to it.
    changes: u64,
}

/// How many folders' prints are held in memory at once.
const HELD: usize = 2;

/// One folder's prints, as held in memory.
struct Prints<P> {
    folder: PathBuf,
    known: HashMap<String, (String, P)>,
    /// Paths put since the last save.
    unsaved: Vec<String>,
    /// How many entries the file holds, the stale ones included.
    on_disk: usize,
    /// The file could not be read to its end and is written afresh.
    damaged: bool,
}

impl<P> Default for Held<P> {
    fn default() -> Self {
        Self { folders: Vec::new(), reading: Vec::new(), changes: 0 }
    }
}

/// The scene prints of a folder's photos, used to group photos of the same
/// scene. They are made only while visual grouping is on.
pub type SceneStore = PrintStore<ScenePrint>;

impl<P: Packed> PrintStore<P> {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            open: Arc::new(Mutex::new(Held::default())),
            read_in: Arc::default(),
            #[cfg(test)]
            reads: Arc::default(),
        }
    }

    fn file_for(&self, folder: &Path) -> PathBuf {
        let key = format!("{}\0{}", version(Rules::Advanced), folder.to_string_lossy());
        self.dir.join(format!("{:032x}.{}", xxh3_128(key.as_bytes()), P::EXT))
    }

    /// Holds `folder`'s prints in memory, reading them in if they were not
    /// held. The file is read with the store let go, so that photos of a
    /// folder already held are not kept waiting, and once: photos of the same
    /// folder that ask meanwhile wait for that read. Making room puts out the
    /// folder asked about longest ago, after saving what of it was not yet saved.
    fn with<T>(&self, folder: &Path, then: impl FnOnce(&mut Prints<P>) -> T) -> T {
        loop {
            let seen = {
                let mut held = self.open.lock().unwrap();
                loop {
                    if let Some(at) = held.folders.iter().position(|p| p.folder == folder) {
                        let prints = held.folders.remove(at);
                        held.folders.push(prints);
                        return then(held.folders.last_mut().expect("held"));
                    }
                    if !held.reading.iter().any(|f| f == folder) {
                        break;
                    }
                    held = self.read_in.wait(held).unwrap();
                }
                held.reading.push(folder.to_path_buf());
                held.changes
            };
            // Wakes whoever waits for this read once it is over, however it ends.
            let reading = Reading { store: self, folder };
            let fresh = self.read(folder);
            let mut held = self.open.lock().unwrap();
            // Over, in the same hold of the store as the prints are taken in,
            // so nobody finds the folder neither held nor being read meanwhile.
            held.reading.retain(|f| f != folder);
            // The file may have been written while it was read: start again.
            if held.changes != seen || held.folders.iter().any(|p| p.folder == folder) {
                continue;
            }
            if held.folders.len() >= HELD {
                let mut old = held.folders.remove(0);
                self.write(&mut old);
                held.changes += 1;
            }
            held.folders.push(fresh);
            let done = then(held.folders.last_mut().expect("held"));
            drop(held);
            drop(reading);
            return done;
        }
    }

    /// A photo's print, if one is remembered for this version of the file.
    pub fn get(&self, folder: &Path, path: &Path, version: &str) -> Option<P> {
        self.with(folder, |p| p.known.get(path.to_string_lossy().as_ref()).filter(|(was, _)| was == version).map(|(_, print)| print.clone()))
    }

    /// Remembers a photo's print, until the next `save` in memory only.
    pub fn put(&self, folder: &Path, path: &Path, version: &str, print: P) {
        let path = path.to_string_lossy().into_owned();
        self.with(folder, |p| {
            p.known.insert(path.clone(), (version.to_string(), print));
            p.unsaved.push(path);
        });
    }

    /// Writes what has been put for `folder` since the last save.
    pub fn save(&self, folder: &Path) {
        if let Some(p) = self.open.lock().unwrap().folders.iter_mut().find(|p| p.folder == folder) {
            self.write(p);
        }
    }

    /// Forgets what is held in memory; the files go with the records'.
    fn forget(&self) {
        let mut held = self.open.lock().unwrap();
        held.folders.clear();
        held.changes += 1;
    }

    fn read(&self, folder: &Path) -> Prints<P> {
        let mut prints = Prints::<P> { folder: folder.to_path_buf(), known: HashMap::new(), unsaved: Vec::new(), on_disk: 0, damaged: false };
        #[cfg(test)]
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let Ok(bytes) = std::fs::read(self.file_for(folder)) else { return prints };
        if !bytes.starts_with(P::MAGIC) {
            prints.damaged = true;
            return prints;
        }
        let mut at = P::MAGIC.len();
        while at < bytes.len() {
            let Some((path, version, print, next)) = entry::<P>(&bytes, at) else {
                // Cut short, most likely while being written: what came
                // before is whole.
                prints.damaged = true;
                break;
            };
            prints.known.insert(path, (version, print));
            prints.on_disk += 1;
            at = next;
        }
        prints
    }

    fn write(&self, prints: &mut Prints<P>) {
        if prints.unsaved.is_empty() && !prints.damaged {
            return;
        }
        let path = self.file_for(&prints.folder);
        // Written afresh when the file is damaged, new, or mostly stale.
        let afresh = prints.damaged || prints.on_disk == 0 || prints.on_disk > 2 * prints.known.len();
        let mut bytes = Vec::new();
        if afresh {
            bytes.extend_from_slice(P::MAGIC);
            for (p, (v, print)) in &prints.known {
                encode(&mut bytes, p, v, print);
            }
        } else {
            for p in &prints.unsaved {
                if let Some((v, print)) = prints.known.get(p) {
                    encode(&mut bytes, p, v, print);
                }
            }
        }
        let written = if afresh {
            let temp = path.with_extension(format!("tmp{}", std::process::id()));
            let ok = std::fs::write(&temp, &bytes).is_ok() && std::fs::rename(&temp, &path).is_ok();
            if !ok {
                std::fs::remove_file(&temp).ok();
            }
            ok.then_some(prints.known.len())
        } else {
            let ok = std::fs::OpenOptions::new().append(true).open(&path).and_then(|mut f| f.write_all(&bytes)).is_ok();
            ok.then_some(prints.on_disk + prints.unsaved.len())
        };
        match written {
            Some(on_disk) => {
                prints.on_disk = on_disk;
                prints.unsaved.clear();
                prints.damaged = false;
            }
            // Not written, or only partly added to, or the file was taken
            // away meanwhile: what it holds is not known, so the next save
            // writes it afresh.
            None => prints.damaged = true,
        }
    }
}

/// A folder whose file is being read in. Going, it makes sure the folder is
/// no longer marked as being read, and wakes whoever waits so they look again.
struct Reading<'a, P: Packed> {
    store: &'a PrintStore<P>,
    folder: &'a Path,
}

impl<P: Packed> Drop for Reading<'_, P> {
    fn drop(&mut self) {
        let mut held = self.store.open.lock().unwrap_or_else(|e| e.into_inner());
        held.reading.retain(|f| f != self.folder);
        drop(held);
        self.store.read_in.notify_all();
    }
}

/// One photo's print as it is written: the path and the file's version,
/// each with its length before it, then the print's numbers.
fn encode<P: Packed>(into: &mut Vec<u8>, path: &str, version: &str, print: &P) {
    for text in [path, version] {
        into.extend_from_slice(&(text.len() as u32).to_le_bytes());
        into.extend_from_slice(text.as_bytes());
    }
    print.write_floats(into);
}

/// The entry starting at `at`, and where the next starts. `None` if it is
/// not all there.
fn entry<P: Packed>(bytes: &[u8], at: usize) -> Option<(String, String, P, usize)> {
    let mut at = at;
    let mut text = || -> Option<String> {
        let len = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
        let s = String::from_utf8(bytes.get(at + 4..at + 4 + len)?.to_vec()).ok()?;
        at += 4 + len;
        Some(s)
    };
    let (path, version) = (text()?, text()?);
    let numbers: Vec<f32> = bytes.get(at..at + P::FLOATS * 4)?.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
    Some((path, version, P::from_floats(&numbers), at + P::FLOATS * 4))
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
        let face = FaceRecord { x: 0.1, y: 0.2, w: 0.05, h: 0.07, subject: true, reason: None, eyes_shut: false, looking_away: true, maybe_away: false, sharpness: Some(80.0), eye: 0.28, eyes_read: true, state: Some("Looking Down".into()), concern: 0, excused: 0.85, open: false };
        Record { faces: vec![face], score: Some(score), focus: Some(0.5), gaze: Some(1.0), key: None }
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

    fn print(seed: f32) -> PhotoPrint {
        PhotoPrint { mean: (0..512).map(|i| seed + i as f32 / 1000.0).collect(), min: (0..512).map(|i| seed - i as f32 / 1000.0).collect() }
    }

    #[test]
    fn a_print_comes_back_by_path_and_version_and_outlives_the_session() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-prints-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let (folder, a, b) = (Path::new("/shoots/wedding"), Path::new("/shoots/wedding/A.ARW"), Path::new("/shoots/wedding/B.ARW"));
        assert_eq!(store.prints.get(folder, a, "v1"), None, "nothing is remembered of a folder never analysed");

        store.prints.put(folder, a, "v1", print(0.25));
        assert_eq!(store.prints.get(folder, a, "v1"), Some(print(0.25)));
        assert_eq!(store.prints.get(folder, a, "v2"), None, "the file has changed since");
        assert_eq!(store.prints.get(folder, b, "v1"), None);
        store.prints.save(folder);
        store.prints.put(folder, b, "v1", print(0.5));
        store.prints.save(folder);

        let again = PrintStore::new(dir.clone());
        assert_eq!(again.get(folder, a, "v1"), Some(print(0.25)), "read back from disk");
        assert_eq!(again.get(folder, b, "v1"), Some(print(0.5)));
        assert_eq!(again.get(Path::new("/shoots/other"), Path::new("/shoots/other/A.ARW"), "v1"), None, "another folder's prints are its own");
        assert_eq!(again.get(folder, a, "v2"), None);

        // A photo analysed again, after it changed, is remembered as it is now.
        again.put(folder, a, "v2", print(0.75));
        again.save(folder);
        let third = PrintStore::new(dir.clone());
        assert_eq!(third.get(folder, a, "v2"), Some(print(0.75)));
        assert_eq!(third.get(folder, a, "v1"), None);

        assert_eq!(store.usage().1, 0, "prints alone are not a folder's record");
        assert!(store.usage().0 > 2 * 4096);
        store.clear();
        assert_eq!(store.prints.get(folder, b, "v1"), None, "forgotten with the records");
        assert_eq!(PrintStore::<PhotoPrint>::new(dir.clone()).get(folder, b, "v1"), None);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn judging_another_folder_keeps_the_scanned_one_held_and_a_third_saves_it() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-prints-held-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let store = PrintStore::new(dir.clone());
        let (a, b, c) = (Path::new("/shoots/a"), Path::new("/shoots/b"), Path::new("/shoots/c"));
        let photo = Path::new("/shoots/a/1.ARW");
        store.put(a, photo, "v1", print(0.25));
        assert_eq!(store.get(b, Path::new("/shoots/b/1.ARW"), "v1"), None);
        assert_eq!(PrintStore::<PhotoPrint>::new(dir.clone()).get(a, photo, "v1"), None, "a is still held, not yet saved");
        assert_eq!(store.get(a, photo, "v1"), Some(print(0.25)));
        // b was asked about before a last was, so c puts out b, not a.
        store.get(c, Path::new("/shoots/c/1.ARW"), "v1");
        assert_eq!(PrintStore::<PhotoPrint>::new(dir.clone()).get(a, photo, "v1"), None, "a is still held");
        store.get(b, Path::new("/shoots/b/1.ARW"), "v1");
        assert_eq!(PrintStore::new(dir.clone()).get(a, photo, "v1"), Some(print(0.25)), "put out, and saved as it went");
        assert_eq!(store.get(a, photo, "v1"), Some(print(0.25)));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn prints_put_while_other_folders_are_judged_are_all_kept() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-prints-busy-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let store = PrintStore::new(dir.clone());
        let a = Path::new("/shoots/a");
        let photo = |i: usize| PathBuf::from(format!("/shoots/a/{i}.ARW"));
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for i in 0..200 {
                    store.put(a, &photo(i), "v1", print(i as f32));
                    if i % 50 == 49 {
                        store.save(a);
                    }
                }
            });
            for folder in ["/shoots/b", "/shoots/c"] {
                let store = &store;
                scope.spawn(move || {
                    for i in 0..100 {
                        store.get(Path::new(folder), Path::new(&format!("{folder}/{i}.ARW")), "v1");
                    }
                });
            }
        });
        assert!((0..200).all(|i| store.get(a, &photo(i), "v1") == Some(print(i as f32))), "in memory or read back");
        store.get(Path::new("/shoots/b"), Path::new("/shoots/b/0.ARW"), "v1");
        store.get(Path::new("/shoots/c"), Path::new("/shoots/c/0.ARW"), "v1");
        let back = PrintStore::new(dir.clone());
        assert!((0..200).all(|i| back.get(a, &photo(i), "v1") == Some(print(i as f32))), "and on disk once put out");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn photos_that_miss_a_folder_together_read_its_file_once() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-prints-once-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let a = Path::new("/shoots/a");
        let photo = |i: usize| PathBuf::from(format!("/shoots/a/{i}.ARW"));
        let first = PrintStore::new(dir.clone());
        (0..500).for_each(|i| first.put(a, &photo(i), "v1", print(i as f32)));
        first.save(a);
        for _ in 0..5 {
            let store = PrintStore::new(dir.clone());
            let start = std::sync::Barrier::new(6);
            std::thread::scope(|scope| {
                for t in 0..6 {
                    let (store, start) = (&store, &start);
                    scope.spawn(move || {
                        start.wait();
                        assert_eq!(store.get(a, &photo(t), "v1"), Some(print(t as f32)));
                    });
                }
            });
            assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), 1, "one read, the others wait for it");
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_save_that_could_not_add_to_the_file_writes_it_afresh_next_time() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-prints-gone-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let store = PrintStore::new(dir.clone());
        let (folder, a, b) = (Path::new("/shoots/wedding"), Path::new("/shoots/wedding/A.ARW"), Path::new("/shoots/wedding/B.ARW"));
        store.put(folder, a, "v1", print(0.25));
        store.save(folder);
        store.put(folder, b, "v1", print(0.5));
        // The file goes, as when everything is forgotten meanwhile: there is nothing to add to.
        std::fs::remove_file(store.file_for(folder)).unwrap();
        store.save(folder);
        assert!(!store.file_for(folder).exists(), "an addition does not make a file");
        store.save(folder);
        let back = PrintStore::new(dir.clone());
        assert!(back.get(folder, a, "v1") == Some(print(0.25)) && back.get(folder, b, "v1") == Some(print(0.5)), "written whole");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn prints_cut_short_keep_what_was_whole() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-prints-cut-{}", std::process::id()));
        let store = PrintStore::new(dir.clone());
        std::fs::create_dir_all(&dir).unwrap();
        let folder = Path::new("/shoots/wedding");
        store.put(folder, Path::new("/shoots/wedding/A.ARW"), "v1", print(0.25));
        store.put(folder, Path::new("/shoots/wedding/B.ARW"), "v1", print(0.5));
        store.save(folder);
        let file = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "prints")).unwrap();
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, &bytes[..bytes.len() - 100]).unwrap();
        let back = PrintStore::<PhotoPrint>::new(dir.clone());
        let whole = [Path::new("/shoots/wedding/A.ARW"), Path::new("/shoots/wedding/B.ARW")].iter().filter(|p| back.get(folder, p, "v1").is_some()).count();
        assert_eq!(whole, 1, "the photo written in full is kept, the one cut short is not");
        std::fs::write(&file, b"not prints at all").unwrap();
        assert_eq!(PrintStore::<PhotoPrint>::new(dir.clone()).get(folder, Path::new("/shoots/wedding/A.ARW"), "v1"), None);
        std::fs::remove_dir_all(dir).ok();
    }

    fn scene(seed: f32) -> ScenePrint {
        ScenePrint((0..512).map(|i| seed + i as f32 / 1000.0).collect())
    }

    #[test]
    fn scene_prints_are_kept_and_read_back() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-scenes-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let store = SceneStore::new(dir.clone());
        let folder = Path::new("/shoots/wedding");
        store.put(folder, Path::new("/shoots/wedding/A.ARW"), "v1", scene(0.25));
        store.save(folder);
        let back = SceneStore::new(dir.clone());
        assert_eq!(back.get(folder, Path::new("/shoots/wedding/A.ARW"), "v1"), Some(scene(0.25)));
        assert_eq!(back.get(folder, Path::new("/shoots/wedding/A.ARW"), "v2"), None, "another version of the file");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn scene_prints_and_face_prints_live_in_different_files() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-both-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        let folder = Path::new("/shoots/wedding");
        store.prints.put(folder, Path::new("/shoots/wedding/A.ARW"), "v1", print(0.25));
        store.scenes.put(folder, Path::new("/shoots/wedding/A.ARW"), "v1", scene(0.5));
        store.prints.save(folder);
        store.scenes.save(folder);
        let back = Store::new(dir.clone());
        assert_eq!(back.prints.get(folder, Path::new("/shoots/wedding/A.ARW"), "v1"), Some(print(0.25)));
        assert_eq!(back.scenes.get(folder, Path::new("/shoots/wedding/A.ARW"), "v1"), Some(scene(0.5)));
        assert_eq!(back.usage().1, 0, "neither kind of print is counted as a folder");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_damaged_scene_file_reads_what_is_whole() {
        let dir = std::env::temp_dir().join(format!("opencull-ai-scenes-cut-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let store = SceneStore::new(dir.clone());
        let folder = Path::new("/shoots/wedding");
        store.put(folder, Path::new("/shoots/wedding/A.ARW"), "v1", scene(0.25));
        store.put(folder, Path::new("/shoots/wedding/B.ARW"), "v1", scene(0.5));
        store.save(folder);
        let file = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "scenes")).unwrap();
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, &bytes[..bytes.len() - 100]).unwrap();
        let back = SceneStore::new(dir.clone());
        let whole = [Path::new("/shoots/wedding/A.ARW"), Path::new("/shoots/wedding/B.ARW")].iter().filter(|p| back.get(folder, p, "v1").is_some()).count();
        assert_eq!(whole, 1, "the photo written in full is kept, the one cut short is not");
        std::fs::write(&file, b"not scenes at all").unwrap();
        assert_eq!(SceneStore::new(dir.clone()).get(folder, Path::new("/shoots/wedding/A.ARW"), "v1"), None);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_advanced_version_does_not_name_scene_prints() {
        assert!(version(Rules::Advanced).ends_with("|print-1") && !version(Rules::Advanced).contains("scene"));
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
        let r = Record::from(&Analysis { faces: vec![face], score: Some(50.0), parts: None, key: None, prints: None });
        let f = &r.faces[0];
        assert!((f.x - 0.4).abs() < 1e-6 && (f.y - 0.2).abs() < 1e-6, "its middle is three quarters of the way UP, so its top is a fifth of the way down");
        assert_eq!(f.reason.as_deref(), Some("Background — small beside the others"));
        assert!((f.sharpness.unwrap() - 100.0).abs() < 1e-3);
    }
}
