//! Ingest: finding cards and drives, listing what is on them, and copying
//! the chosen photos off.
//!
//! This is the part of the app a photographer trusts with the only copy of a
//! shoot — the next thing they do may be to format the card. So a copy here
//! is checked and on the disk before it is called done, is never left half
//! written under a photo's name, and never replaces a file unless that was
//! asked for. Even then a sidecar already there keeps its edits: only the
//! ratings in it change.

use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::raw::{self, extension};
use crate::fsx;
use crate::scan::{self, hidden, is_image, natural_cmp, Scanned};
use crate::transfer::rename;
use crate::xmp;

// ── Sources ────────────────────────────────────────────────────────────────

/// A card or drive photos could be ingested from.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub path: PathBuf,
    pub name: String,
    /// Came and can go: a card, or a drive on a cable.
    pub removable: bool,
    /// Has a camera's folders at its top: DCIM and the like.
    pub camera_card: bool,
}

/// A folder in the source browser.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub path: PathBuf,
    pub name: String,
    /// Photos directly inside it.
    pub image_count: usize,
}

/// The folders a camera makes at the top of its card.
pub fn is_camera_media(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("dcim") || matches!(lower.as_str(), "private" | "m4root" | "misc")
}

fn has_camera_media(volume: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(volume) else { return false };
    entries.flatten().any(|e| e.file_type().is_ok_and(|k| k.is_dir()) && e.file_name().to_str().is_some_and(|n| !hidden(n) && is_camera_media(n)))
}

/// What the system says about the disk a path is on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Disk {
    internal: bool,
    ejectable: bool,
    /// `None` when it cannot be told: a network share, a disk image.
    solid_state: Option<bool>,
}

/// Reads one true-or-false entry out of `diskutil`'s report.
fn plist_bool(report: &str, key: &str) -> Option<bool> {
    let after = &report[report.find(&format!("<key>{key}</key>"))? + key.len() + 11..];
    let tag = after.trim_start();
    if tag.starts_with("<true/>") {
        Some(true)
    } else if tag.starts_with("<false/>") {
        Some(false)
    } else {
        None
    }
}

fn disk_from_report(report: &str) -> Disk {
    Disk {
        // Unknown counts as internal: something that cannot be identified is
        // not offered as a card.
        internal: plist_bool(report, "Internal").unwrap_or(true),
        ejectable: plist_bool(report, "Ejectable").unwrap_or(false) || plist_bool(report, "RemovableMedia").unwrap_or(false),
        solid_state: plist_bool(report, "SolidState"),
    }
}

#[cfg(target_os = "macos")]
fn disk_of(path: &Path) -> Option<Disk> {
    use std::os::unix::fs::MetadataExt;
    use std::sync::OnceLock;
    // Asking takes a sixth of a second, and the list of volumes is asked for
    // every couple of seconds. Remembered by where it is mounted and which
    // device that is, so a different card at the same name is asked about anew.
    static KNOWN: OnceLock<Mutex<HashMap<(PathBuf, u64), Disk>>> = OnceLock::new();
    let key = (path.to_path_buf(), std::fs::metadata(path).ok()?.dev());
    let known = KNOWN.get_or_init(Default::default);
    if let Some(disk) = known.lock().unwrap().get(&key) {
        return Some(*disk);
    }
    let out = std::process::Command::new("/usr/sbin/diskutil").arg("info").arg("-plist").arg(path).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let disk = disk_from_report(&String::from_utf8_lossy(&out.stdout));
    known.lock().unwrap().insert(key, disk);
    Some(disk)
}

/// The volume a path lives on: `/Volumes/NAME` for anything under it.
#[cfg(target_os = "macos")]
fn volume_of(path: &Path) -> PathBuf {
    let mut parts = path.components();
    match (parts.next(), parts.next(), parts.next()) {
        (Some(root), Some(volumes), Some(name)) if volumes.as_os_str() == "Volumes" => {
            [root.as_os_str(), volumes.as_os_str(), name.as_os_str()].iter().collect()
        }
        _ => PathBuf::from("/"),
    }
}

fn sorted(mut volumes: Vec<Volume>) -> Vec<Volume> {
    volumes.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    volumes
}

fn named(path: PathBuf, removable: bool) -> Volume {
    let name = fsx::file_name(&path);
    Volume { camera_card: has_camera_media(&path), name, removable, path }
}

/// Every card and outside drive attached. The computer's own disk is left out.
#[cfg(target_os = "macos")]
pub fn volumes() -> Vec<Volume> {
    let Ok(entries) = std::fs::read_dir("/Volumes") else { return Vec::new() };
    let found = entries
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| !hidden(n)))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|p| {
            let disk = disk_of(&p)?;
            (disk.ejectable || !disk.internal).then(|| named(p, true))
        })
        .collect();
    sorted(found)
}

#[cfg(target_os = "windows")]
pub fn volumes() -> Vec<Volume> {
    // Every drive letter but the one Windows itself is on.
    let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();
    let found = ('A'..='Z')
        .map(|letter| format!("{letter}:"))
        .filter(|drive| *drive != system)
        .map(|drive| PathBuf::from(format!("{drive}\\")))
        .filter(|p| p.is_dir())
        .map(|p| {
            let name = p.to_string_lossy().trim_end_matches('\\').to_string();
            Volume { camera_card: has_camera_media(&p), name, removable: true, path: p }
        })
        .collect();
    sorted(found)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn volumes() -> Vec<Volume> {
    // Where desktops mount what is plugged in.
    let user = std::env::var("USER").unwrap_or_default();
    let mut found = Vec::new();
    for parent in [format!("/media/{user}"), format!("/run/media/{user}"), "/media".to_string(), "/mnt".to_string()] {
        let Ok(entries) = std::fs::read_dir(&parent) else { continue };
        for path in entries.flatten().map(|e| e.path()) {
            let shown = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| !hidden(n) && n != user);
            if shown && path.is_dir() && std::fs::read_dir(&path).is_ok_and(|mut d| d.next().is_some()) {
                found.push(named(path, true));
            }
        }
    }
    sorted(found)
}

/// How many photos sit directly in a folder.
pub fn shallow_count(folder: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(folder) else { return 0 };
    entries
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| !hidden(n)))
        .filter(|e| is_image(&extension(&e.path())) && e.file_type().is_ok_and(|k| !k.is_dir()))
        .count()
}

fn subfolders(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| !hidden(n)))
        .filter(|e| e.file_type().is_ok_and(|k| k.is_dir()))
        .map(|e| e.path())
        .collect()
}

/// Whether there is a photo anywhere beneath `folder`. Stops at the first:
/// counting an archive drive's tens of thousands to answer yes would be slow
/// for nothing.
fn contains_photos(folder: &Path, depth: usize) -> bool {
    if shallow_count(folder) > 0 {
        return true;
    }
    depth < 12 && subfolders(folder).iter().any(|sub| contains_photos(sub, depth + 1))
}

/// Folders a system keeps on a drive that are never a photographer's.
const SYSTEM_FOLDERS: &[&str] = &["system volume information", "$recycle.bin", "trash"];

/// The folders directly inside `directory` that have photos somewhere in them.
pub fn photo_folders(directory: &Path) -> Vec<Folder> {
    let mut found: Vec<Folder> = subfolders(directory)
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_string();
            if SYSTEM_FOLDERS.contains(&name.to_lowercase().as_str()) || !contains_photos(&path, 0) {
                return None;
            }
            Some(Folder { image_count: shallow_count(&path), name, path })
        })
        .collect();
    found.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    found
}

/// The photos to offer from `folder`: those directly in it, or, when it holds
/// only folders — a card's DCIM — everything beneath.
pub fn scan(folder: &Path) -> Vec<Scanned> {
    let direct = scan::scan_folder(folder, false);
    if direct.images.is_empty() && direct.subfolder_count > 0 {
        scan::scan_folder(folder, true).images
    } else {
        direct.images
    }
}

/// What a photo's own file says about it: when it was taken, in milliseconds,
/// and the stars given in the camera.
pub fn details(photo: &Path) -> (Option<f64>, u8) {
    let probe = raw::probe(photo).ok();
    let taken = probe.as_ref().and_then(|p| p.exif.capture_ms);
    (taken, xmp::read(photo, probe.as_ref()).rating)
}

// ── Copying ────────────────────────────────────────────────────────────────

/// What to do with a photo whose name is already taken at the destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Policy {
    UniqueName,
    Overwrite,
    Skip,
}

/// One photo to copy.
#[derive(Clone, Debug, Deserialize)]
pub struct Item {
    pub path: PathBuf,
    /// Its JPEG, for a RAW shot with one; otherwise the same file.
    pub display: PathBuf,
}

impl Item {
    fn companion(&self) -> Option<&Path> {
        (self.display != self.path).then_some(self.display.as_path())
    }
}

/// One report from a running copy.
///
/// Bytes as well as files, because a card holds 80 MB RAWs beside 2 MB JPEGs
/// and "412 of 900" says almost nothing about how much longer this will take.
/// Two byte counts, not one: `bytes_copied` is what was actually written, and
/// is what a speed may be measured from; `bytes_settled` is what is finished
/// either way, including files skipped without being written, and is what
/// time remaining must be measured from. Both include the part-written bytes
/// of files still going across, or neither would move until a whole file landed.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub bytes_copied: u64,
    pub bytes_settled: u64,
    pub bytes_total: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Success,
    Cancelled,
    Failed(String),
}

/// What a finished copy came to, and anything that went wrong along the way
/// without stopping it: ratings that could not be carried across.
#[derive(Clone, Debug, PartialEq)]
pub struct Copied {
    pub outcome: Outcome,
    pub warnings: Vec<String>,
}


fn stem_of(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Every name a photo takes at the destination: its own, its JPEG's, and both
/// sidecar names (even with no sidecar, since one already there would be read
/// as this photo's).
fn names_of(item: &Item) -> Vec<PathBuf> {
    let mut names = vec![item.path.clone()];
    if let Some(jpeg) = item.companion() {
        names.push(jpeg.to_path_buf());
    }
    names.extend(xmp::sidecar_candidates(&item.path));
    names
}

/// How many of these photos want a name that is already taken.
///
/// Two ways to collide, not one. The obvious way is a file already in the
/// destination: the photo, its JPEG or a sidecar. The other is the selection
/// colliding with itself: a camera restarts numbering in every card folder,
/// so a card carrying three shoots holds the same file name three times
/// over. Miss that, and a whole card copied into an empty folder counts no
/// conflicts, never asks, and quietly loses a photograph for every repeat.
pub fn conflict_count(items: &[Item], destination: &Path) -> usize {
    let mut claimed = HashSet::new();
    items
        .iter()
        .filter(|item| {
            let stem = stem_of(&item.path);
            let names: Vec<String> = names_of(item).iter().map(|f| rename(f, &stem, "")).collect();
            // Lowercased: the destination very likely ignores case, so
            // `IMG_1.ARW` and `img_1.arw` are one file there.
            let repeat = names.iter().any(|n| !claimed.insert(n.to_lowercase()));
            repeat || names.iter().any(|n| destination.join(n).exists())
        })
        .count()
}

/// Where one photo is to go.
enum Reserved {
    /// Under this suffix to its stem ("" for its own name), every name of
    /// which is free.
    Fresh(String),
    /// Under its own name, over what is there, as was asked for.
    Overwrite,
    Skip,
}

/// Hands out destination names so two copies running at once can never
/// choose the same one.
///
/// Asking the disk whether a name is free is not enough: with four copies
/// going, two can be told the same name is free before either has written a
/// byte, and the loser's photo is then overwritten by the winner's.
#[derive(Default)]
struct Names {
    claimed: Mutex<HashSet<String>>,
}

impl Names {
    fn key(path: &Path) -> String {
        path.to_string_lossy().to_lowercase()
    }

    /// Finds and claims one name for a photo and everything that goes with it,
    /// so a RAW and its JPEG always share a stem.
    fn reserve(&self, item: &Item, destination: &Path, policy: Policy) -> Option<Reserved> {
        let stem = stem_of(&item.path);
        let files = names_of(item);
        let mut claimed = self.claimed.lock().unwrap();
        let targets = |suffix: &str| -> Vec<PathBuf> { files.iter().map(|f| destination.join(rename(f, &stem, suffix))).collect() };
        let in_run = |claimed: &HashSet<String>, paths: &[PathBuf]| paths.iter().any(|p| claimed.contains(&Self::key(p)));
        let on_disk = |paths: &[PathBuf]| paths.iter().any(|p| p.exists());

        let plain = targets("");
        let claim = |claimed: &mut HashSet<String>, paths: &[PathBuf]| {
            for p in paths {
                claimed.insert(Self::key(p));
            }
        };
        if !in_run(&claimed, &plain) && !on_disk(&plain) {
            claim(&mut claimed, &plain);
            return Some(Reserved::Fresh(String::new()));
        }
        match policy {
            Policy::Skip => return Some(Reserved::Skip),
            // Overwriting is for what was there before, never another photo of
            // the same name from this card.
            Policy::Overwrite if !in_run(&claimed, &plain) => {
                claim(&mut claimed, &plain);
                return Some(Reserved::Overwrite);
            }
            _ => {}
        }
        for n in 1..=10_000 {
            let suffix = format!("_{n}");
            let candidate = targets(&suffix);
            if !in_run(&claimed, &candidate) && !on_disk(&candidate) {
                claim(&mut claimed, &candidate);
                return Some(Reserved::Fresh(suffix));
            }
        }
        None
    }
}

/// How much is read from the card at a time.
const BLOCK: usize = 4 * 1024 * 1024;

/// Copies `source` to `target`, telling `on_bytes` as it goes, and checks
/// the result before giving it its name.
///
/// Written under a hidden temporary name and renamed only once whole and on
/// the disk, so a card pulled part way leaves no half photo that could later
/// be taken for a good one, and a copy called done survives the destination
/// being unplugged straight after. The check is of size, not a checksum: a
/// checksum means reading every byte a second time, minutes on a full card,
/// and what actually happens to copies is that they come up short.
///
/// Unless `replace`, refuses to land on a file that is already there.
fn verified_copy(source: &Path, target: &Path, replace: bool, on_bytes: &mut dyn FnMut(u64)) -> io::Result<()> {
    let staged = fsx::staging_path(target)?;

    let copied = (|| {
        let mut from = std::fs::File::open(source)?;
        let expected = from.metadata()?;
        let mut to = std::fs::File::create(&staged)?;
        let mut block = vec![0u8; BLOCK.min(expected.len().max(1) as usize)];
        let mut written = 0u64;
        loop {
            let n = from.read(&mut block)?;
            if n == 0 {
                break;
            }
            to.write_all(&block[..n])?;
            written += n as u64;
            on_bytes(n as u64);
        }
        to.flush()?;
        // The date it was last changed goes with it: it is what most
        // software sorts photographs by.
        if let Ok(modified) = expected.modified() {
            to.set_modified(modified).ok();
        }
        // Synced before it is called copied: the card may be formatted next.
        to.sync_all()?;
        drop(to);

        let landed = std::fs::metadata(&staged)?.len();
        if written != expected.len() || landed != expected.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("copied {landed} of {} bytes — the copy was removed, keep the card until this one succeeds", expected.len()),
            ));
        }
        fsx::put_in_place(&staged, target, replace).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                io::Error::new(io::ErrorKind::AlreadyExists, "a file of that name appeared at the destination")
            } else {
                e
            }
        })?;
        fsx::sync_dir(target.parent().unwrap_or(Path::new(".")))
    })();

    if copied.is_err() {
        std::fs::remove_file(&staged).ok();
    }
    copied
}

enum Placed {
    /// Copied, with any problems carrying its ratings across.
    Copied(Vec<String>),
    Skipped,
    Failed(String),
}

/// Puts one photo into the destination: itself, its JPEG, its ratings.
fn place(item: &Item, destination: &Path, policy: Policy, names: &Names, on_bytes: &mut dyn FnMut(u64)) -> Placed {
    let name = fsx::file_name(&item.path);
    let stem = stem_of(&item.path);
    let (suffix, overwrite) = match names.reserve(item, destination, policy) {
        Some(Reserved::Skip) => return Placed::Skipped,
        Some(Reserved::Overwrite) => (String::new(), true),
        Some(Reserved::Fresh(suffix)) => (suffix, false),
        None => return Placed::Failed(format!("{name}: could not find a free name")),
    };
    let renamed = |file: &Path| destination.join(rename(file, &stem, &suffix));
    let failed = |e: io::Error| Placed::Failed(format!("Failed to copy {name}: {e}"));

    let target = renamed(&item.path);
    if let Err(e) = verified_copy(&item.path, &target, overwrite, on_bytes) {
        return failed(e);
    }
    // Its JPEG takes the name its RAW landed under.
    if let Some(jpeg) = item.companion() {
        if let Err(e) = verified_copy(jpeg, &renamed(jpeg), overwrite, on_bytes) {
            return failed(e);
        }
    }

    // A sidecar beside the source travels too. Not counted in the progress:
    // a few hundred bytes against a RAW's tens of millions. One already at
    // the destination is never replaced, as it may hold develop settings;
    // only its ratings are updated, below.
    let mut warnings = Vec::new();
    let mut carried = false;
    for sidecar in xmp::sidecars_of(&item.path) {
        let beside = renamed(&sidecar);
        if beside.exists() {
            continue;
        }
        match verified_copy(&sidecar, &beside, false, &mut |_| {}) {
            Ok(()) => carried = true,
            Err(e) => warnings.push(format!("{name}: its ratings did not come across ({e})")),
        }
    }

    // Stars given in the camera are written beside the copy, but only when
    // there are some: a reader stops at a sidecar, so an empty one would hide
    // the rating inside the file. Read from the copy, not the card.
    if !carried {
        let marks = xmp::read(&item.path, raw::probe(&target).ok().as_ref());
        if !marks.is_empty() {
            if let Err(e) = xmp::write(&target, marks) {
                warnings.push(format!("{name}: its ratings could not be written ({e})"));
            }
        }
    }
    Placed::Copied(warnings)
}

/// The running count, shared by every copy going.
struct Tally<'a> {
    state: Mutex<TallyState>,
    total: usize,
    bytes_total: u64,
    report: &'a (dyn Fn(Progress) + Sync),
}

#[derive(Default)]
struct TallyState {
    done: usize,
    bytes_copied: u64,
    bytes_settled: u64,
    /// Part-written bytes of the files still going across.
    in_flight: u64,
    last_report: Option<Instant>,
    failure: Option<String>,
    warnings: Vec<String>,
}

/// The shortest gap between reports raised by part-written bytes. Blocks land
/// hundreds of times a second; the eye needs about twenty-five.
const REPORT_EVERY: Duration = Duration::from_millis(40);

impl Tally<'_> {
    fn snapshot(&self, s: &TallyState) -> Progress {
        // Held to the total: a file can turn out larger than it was when
        // sized, and a bar past its end reads as a fault.
        Progress {
            done: s.done,
            total: self.total,
            bytes_copied: (s.bytes_copied + s.in_flight).min(self.bytes_total),
            bytes_settled: (s.bytes_settled + s.in_flight).min(self.bytes_total),
            bytes_total: self.bytes_total,
        }
    }

    fn failed(&self) -> bool {
        self.state.lock().unwrap().failure.is_some()
    }

    /// Bytes from a copy still running. Throttled, being per block.
    fn advance(&self, bytes: u64) {
        let mut s = self.state.lock().unwrap();
        s.in_flight += bytes;
        if s.last_report.is_some_and(|at| at.elapsed() < REPORT_EVERY) {
            return;
        }
        s.last_report = Some(Instant::now());
        let snapshot = self.snapshot(&s);
        // Reported with the lock still held, so two reports can never
        // arrive out of order and make the bar step backwards.
        (self.report)(snapshot);
    }

    /// A photo has finished. `credited` is what it already reported through
    /// `advance`, taken back out now that the whole of it is being added.
    fn record(&self, placed: Placed, bytes: u64, credited: u64) {
        let mut s = self.state.lock().unwrap();
        s.in_flight = s.in_flight.saturating_sub(credited);
        match placed {
            Placed::Failed(why) => {
                s.failure.get_or_insert(why);
            }
            Placed::Skipped => {}
            Placed::Copied(warnings) => {
                s.bytes_copied += bytes;
                s.warnings.extend(warnings);
            }
        }
        s.done += 1;
        s.bytes_settled += bytes;
        s.last_report = Some(Instant::now());
        let snapshot = self.snapshot(&s);
        (self.report)(snapshot);
    }
}

/// How many photos are copied at once to `destination`.
///
/// Four onto solid state, one onto anything that spins or cannot be
/// identified. Copying several at once wins by hiding a wait of about ten
/// milliseconds per file, and on flash that wait is pure waiting — MEASURED
/// by the Mac app, card to USB SSD over 800 RAWs: one at a time 392 MB/s,
/// two 738, four 742, eight no better. On a spinning drive four writes make
/// the head seek between four places, which is slower and leaves the files
/// in pieces. Photographers archive to cheap spinning drives, so the fast
/// way must not become their slow way.
pub fn parallel_copies(destination: &Path) -> usize {
    match solid_state(destination) {
        Some(true) => 4,
        _ => 1,
    }
}

#[cfg(target_os = "macos")]
fn solid_state(path: &Path) -> Option<bool> {
    disk_of(&volume_of(path))?.solid_state
}

#[cfg(not(target_os = "macos"))]
fn solid_state(_path: &Path) -> Option<bool> {
    None
}

/// Copies `items` into `destination`, `workers` at a time.
///
/// Stops taking new photos once `cancel` is set or one has failed. A photo
/// already going across is finished, not cut off: one photo more is better
/// than half of one.
pub fn copy(items: &[Item], destination: &Path, policy: Policy, workers: usize, cancel: &AtomicBool, report: &(dyn Fn(Progress) + Sync)) -> Copied {
    fsx::sweep(destination);
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    // Sized here, once Copy is pressed, and not while the card is browsed:
    // the photographer is waiting now, and was not then.
    let planned: Vec<u64> = items.iter().map(|i| size(&i.path) + i.companion().map_or(0, size)).collect();
    let tally = Tally { state: Default::default(), total: items.len(), bytes_total: planned.iter().sum(), report };
    (tally.report)(tally.snapshot(&tally.state.lock().unwrap()));

    let names = Names::default();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, items.len().max(1)) {
            scope.spawn(|| loop {
                if cancel.load(Ordering::SeqCst) || tally.failed() {
                    return;
                }
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(item) = items.get(index) else { return };
                let mut credited = 0u64;
                let placed = place(item, destination, policy, &names, &mut |bytes| {
                    credited += bytes;
                    tally.advance(bytes);
                });
                tally.record(placed, planned[index], credited);
            });
        }
    });

    let state = tally.state.into_inner().unwrap();
    let outcome = if let Some(why) = state.failure {
        Outcome::Failed(why)
    } else if cancel.load(Ordering::SeqCst) && state.done < items.len() {
        Outcome::Cancelled
    } else {
        Outcome::Success
    };
    Copied { outcome, warnings: state.warnings }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A card and a destination, removed when dropped.
    struct World {
        root: PathBuf,
        card: PathBuf,
        dest: PathBuf,
    }

    impl World {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("opencull-ingest-{name}-{}", std::process::id()));
            std::fs::remove_dir_all(&root).ok();
            let (card, dest) = (root.join("card"), root.join("dest"));
            std::fs::create_dir_all(&card).unwrap();
            std::fs::create_dir_all(&dest).unwrap();
            Self { root, card, dest }
        }

        fn file(&self, relative: &str, contents: &[u8]) -> PathBuf {
            let path = self.card.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, contents).unwrap();
            path
        }

        fn landed(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(&self.dest).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            names
        }

        fn read(&self, name: &str) -> Vec<u8> {
            std::fs::read(self.dest.join(name)).unwrap()
        }

        fn run(&self, items: &[Item], policy: Policy, workers: usize) -> (Outcome, Vec<Progress>) {
            let reports = Mutex::new(Vec::new());
            let copied = copy(items, &self.dest, policy, workers, &AtomicBool::new(false), &|p| reports.lock().unwrap().push(p));
            assert!(copied.warnings.is_empty(), "{:?}", copied.warnings);
            (copied.outcome, reports.into_inner().unwrap())
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).ok();
        }
    }

    fn alone(path: &Path) -> Item {
        Item { path: path.to_path_buf(), display: path.to_path_buf() }
    }

    const SIDECAR: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="4"/></rdf:RDF></x:xmpmeta>"#;

    #[test]
    fn a_photo_arrives_whole_with_its_jpeg_and_its_ratings() {
        let w = World::new("whole");
        let raw = w.file("DCIM/100/A.ARW", &vec![7u8; 300_000]);
        let jpg = w.file("DCIM/100/A.JPG", b"jpeg");
        w.file("DCIM/100/A.xmp", SIDECAR.as_bytes());

        let (outcome, reports) = w.run(&[Item { path: raw.clone(), display: jpg }], Policy::UniqueName, 4);

        assert_eq!(outcome, Outcome::Success);
        assert_eq!(w.landed(), ["A.ARW", "A.JPG", "A.xmp"]);
        assert_eq!(w.read("A.ARW"), vec![7u8; 300_000]);
        assert_eq!(xmp::read(&w.dest.join("A.ARW"), None).rating, 4, "the stars came across");
        assert!(raw.exists(), "ingest copies; the card keeps its photo");

        let last = reports.last().unwrap();
        assert_eq!((last.done, last.total), (1, 1));
        assert_eq!(last.bytes_total, 300_004, "the RAW and its JPEG");
        assert_eq!(last.bytes_copied, last.bytes_total);
        assert_eq!(last.bytes_settled, last.bytes_total);
    }

    #[test]
    fn progress_only_ever_moves_forward_and_never_past_the_end() {
        let w = World::new("forward");
        let items: Vec<Item> = (0..12).map(|i| alone(&w.file(&format!("P{i:02}.ARW"), &vec![i as u8; 200_000 + i * 1000]))).collect();

        let (outcome, reports) = w.run(&items, Policy::UniqueName, 4);

        assert_eq!(outcome, Outcome::Success);
        assert_eq!(reports.first().unwrap().done, 0, "a report before anything has moved, so the bar starts at nothing");
        for pair in reports.windows(2) {
            assert!(pair[1].done >= pair[0].done);
            assert!(pair[1].bytes_settled >= pair[0].bytes_settled, "{pair:?}");
        }
        assert!(reports.iter().all(|p| p.bytes_settled <= p.bytes_total && p.bytes_copied <= p.bytes_total));
        assert_eq!(reports.last().unwrap().done, 12);
        assert_eq!(w.landed().len(), 12);
    }

    #[test]
    fn nothing_without_a_photos_name_is_left_behind() {
        let w = World::new("clean");
        let items: Vec<Item> = (0..6).map(|i| alone(&w.file(&format!("P{i}.ARW"), b"raw"))).collect();
        w.run(&items, Policy::UniqueName, 4);
        assert!(w.landed().iter().all(|n| !n.starts_with('.')), "{:?}", w.landed());
    }

    #[test]
    fn the_same_name_from_two_card_folders_is_counted_and_both_are_kept() {
        let w = World::new("twins");
        let first = w.file("DCIM/100/A.ARW", b"first shoot");
        let second = w.file("DCIM/101/A.ARW", b"second shoot");
        let third = w.file("DCIM/102/a.arw", b"third shoot");
        let items = [alone(&first), alone(&second), alone(&third)];

        // Into an empty folder, and still two conflicts: with each other.
        assert_eq!(conflict_count(&items, &w.dest), 2);

        // All at once, which is when two copies could be handed one name.
        let (outcome, _) = w.run(&items, Policy::UniqueName, 4);
        assert_eq!(outcome, Outcome::Success);
        assert_eq!(w.landed().len(), 3, "{:?}", w.landed());
        let mut contents: Vec<Vec<u8>> = w.landed().iter().map(|n| w.read(n)).collect();
        contents.sort();
        assert_eq!(contents, [b"first shoot".to_vec(), b"second shoot".to_vec(), b"third shoot".to_vec()]);
    }

    #[test]
    fn a_unique_name_never_touches_what_was_there() {
        let w = World::new("unique");
        std::fs::write(w.dest.join("A.ARW"), b"already here").unwrap();
        std::fs::write(w.dest.join("A.JPG"), b"its jpeg").unwrap();
        let raw = w.file("A.ARW", b"new");
        let jpg = w.file("A.JPG", b"new jpeg");
        let items = [Item { path: raw, display: jpg }];

        assert_eq!(conflict_count(&items, &w.dest), 1);
        let (outcome, _) = w.run(&items, Policy::UniqueName, 1);

        assert_eq!(outcome, Outcome::Success);
        assert_eq!(w.read("A.ARW"), b"already here");
        assert_eq!(w.read("A.JPG"), b"its jpeg");
        assert_eq!(w.read("A_1.ARW"), b"new");
        assert_eq!(w.read("A_1.JPG"), b"new jpeg", "the JPEG follows its RAW's new name");
    }

    #[test]
    fn skipping_leaves_the_destination_alone_and_still_counts_as_settled() {
        let w = World::new("skip");
        std::fs::write(w.dest.join("A.ARW"), b"already here").unwrap();
        let taken = w.file("A.ARW", b"new and longer");
        let free = w.file("B.ARW", b"bee");

        let (outcome, reports) = w.run(&[alone(&taken), alone(&free)], Policy::Skip, 1);

        assert_eq!(outcome, Outcome::Success);
        assert_eq!(w.read("A.ARW"), b"already here");
        assert_eq!(w.landed(), ["A.ARW", "B.ARW"]);
        let last = reports.last().unwrap();
        assert_eq!(last.done, 2);
        assert_eq!(last.bytes_settled, last.bytes_total, "a skipped photo is finished with, so the time left must shrink");
        assert_eq!(last.bytes_copied, 3, "but it was not written, so it must not count towards the speed");
    }

    #[test]
    fn overwriting_replaces_and_is_the_only_thing_that_does() {
        let w = World::new("overwrite");
        std::fs::write(w.dest.join("A.ARW"), b"old").unwrap();
        let raw = w.file("A.ARW", b"new");

        let (outcome, _) = w.run(&[alone(&raw)], Policy::Overwrite, 1);

        assert_eq!(outcome, Outcome::Success);
        assert_eq!(w.landed(), ["A.ARW"]);
        assert_eq!(w.read("A.ARW"), b"new");
    }

    #[test]
    fn a_raw_and_its_jpeg_keep_one_name_when_only_the_jpegs_is_taken() {
        let w = World::new("pair");
        std::fs::write(w.dest.join("A.JPG"), b"a stranger's jpeg").unwrap();
        let raw = w.file("A.ARW", b"raw");
        let jpg = w.file("A.JPG", b"mine");
        let items = [Item { path: raw, display: jpg }];

        assert_eq!(conflict_count(&items, &w.dest), 1, "a taken JPEG name is a conflict too");
        let (outcome, _) = w.run(&items, Policy::UniqueName, 1);

        assert_eq!(outcome, Outcome::Success);
        assert_eq!(w.landed(), ["A.JPG", "A_1.ARW", "A_1.JPG"], "never A.ARW beside a stranger's A.JPG");
        assert_eq!(w.read("A.JPG"), b"a stranger's jpeg");
    }

    #[test]
    fn skipping_a_taken_jpeg_skips_the_whole_photo() {
        let w = World::new("skippair");
        std::fs::write(w.dest.join("A.JPG"), b"a stranger's jpeg").unwrap();
        let raw = w.file("A.ARW", b"raw");
        let jpg = w.file("A.JPG", b"mine");

        let (outcome, _) = w.run(&[Item { path: raw, display: jpg }], Policy::Skip, 1);

        assert_eq!(outcome, Outcome::Success);
        assert_eq!(w.landed(), ["A.JPG"], "not a RAW that arrived without its JPEG");
    }

    #[test]
    fn a_sidecar_already_at_the_destination_is_never_edited_when_skipping_or_renaming() {
        let w = World::new("strangerxmp");
        std::fs::write(w.dest.join("A.xmp"), b"another photo's edits").unwrap();
        let raw = w.file("A.ARW", b"raw");
        w.file("A.xmp", SIDECAR.as_bytes());

        let (outcome, _) = w.run(&[alone(&raw)], Policy::Skip, 1);
        assert_eq!(outcome, Outcome::Success);
        assert_eq!(w.landed(), ["A.xmp"], "skipped whole");

        let (outcome, _) = w.run(&[alone(&raw)], Policy::UniqueName, 1);
        assert_eq!(outcome, Outcome::Success);
        assert_eq!(w.read("A.xmp"), b"another photo's edits");
        assert_eq!(w.landed(), ["A.xmp", "A_1.ARW", "A_1.xmp"]);
        assert_eq!(xmp::read(&w.dest.join("A_1.ARW"), None).rating, 4, "its own ratings went with it");
    }

    #[test]
    fn overwriting_keeps_the_edits_in_a_sidecar_and_changes_only_its_ratings() {
        let w = World::new("overwritexmp");
        std::fs::write(w.dest.join("A.ARW"), b"old").unwrap();
        let edits = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" xmp:Rating="1" crs:Exposure2012="+0.35"/></rdf:RDF></x:xmpmeta>"#;
        std::fs::write(w.dest.join("A.xmp"), edits).unwrap();
        let raw = w.file("A.ARW", b"new");
        w.file("A.xmp", SIDECAR.as_bytes());

        let (outcome, _) = w.run(&[alone(&raw)], Policy::Overwrite, 1);

        assert_eq!(outcome, Outcome::Success);
        assert_eq!(w.read("A.ARW"), b"new");
        let text = String::from_utf8(w.read("A.xmp")).unwrap();
        assert!(text.contains(r#"crs:Exposure2012="+0.35""#), "the develop settings stay: {text}");
        assert_eq!(xmp::parse(&text).rating, 4, "the card's rating is brought across");
    }

    #[test]
    fn stars_given_in_the_camera_go_beside_the_copy_under_its_new_name() {
        let w = World::new("camerastars");
        std::fs::write(w.dest.join("A.ARW"), b"already here").unwrap();
        std::fs::write(w.dest.join("A.xmp"), b"its edits").unwrap();
        // A sidecar on the card stands in for stars the camera wrote.
        let raw = w.file("A.ARW", b"new");
        w.file("A.xmp", SIDECAR.as_bytes());

        w.run(&[alone(&raw)], Policy::UniqueName, 1);

        assert_eq!(w.read("A.xmp"), b"its edits");
        assert_eq!(xmp::read(&w.dest.join("A_1.ARW"), None).rating, 4);
    }

    #[test]
    fn a_photo_that_cannot_be_read_fails_the_ingest_and_says_which() {
        let w = World::new("fail");
        let good = w.file("A.ARW", b"good");
        let gone = w.card.join("MISSING.ARW");

        let (outcome, _) = w.run(&[alone(&gone), alone(&good)], Policy::UniqueName, 1);

        match outcome {
            Outcome::Failed(why) => assert!(why.contains("MISSING.ARW"), "{why}"),
            other => panic!("expected a failure, got {other:?}"),
        }
        // Nothing more is started once one has failed: the photographer
        // needs to look at the card, not at a bar still running.
        assert!(w.landed().is_empty(), "{:?}", w.landed());
    }

    #[test]
    fn cancelling_stops_before_the_next_photo_and_says_so() {
        let w = World::new("cancel");
        let items: Vec<Item> = (0..5).map(|i| alone(&w.file(&format!("P{i}.ARW"), b"raw"))).collect();
        let cancel = AtomicBool::new(false);

        // Cancelled from inside the first report that shows a photo landed.
        let outcome = copy(&items, &w.dest, Policy::UniqueName, 1, &cancel, &|p| {
            if p.done >= 1 {
                cancel.store(true, Ordering::SeqCst);
            }
        })
        .outcome;

        assert_eq!(outcome, Outcome::Cancelled);
        assert_eq!(w.landed(), ["P0.ARW"], "the photo in hand is finished; no other is begun");
        assert_eq!(w.read("P0.ARW"), b"raw", "and it is whole");
    }

    #[test]
    fn cancelling_as_the_last_photo_lands_is_still_a_finished_ingest() {
        let w = World::new("late");
        let raw = w.file("A.ARW", b"raw");
        let cancel = AtomicBool::new(false);
        let outcome = copy(&[alone(&raw)], &w.dest, Policy::UniqueName, 1, &cancel, &|p| {
            if p.done == 1 {
                cancel.store(true, Ordering::SeqCst);
            }
        })
        .outcome;
        assert_eq!(outcome, Outcome::Success);
    }

    #[test]
    fn a_copy_keeps_the_date_the_photo_was_last_changed() {
        let w = World::new("dates");
        let raw = w.file("A.ARW", b"raw");
        let then = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        std::fs::File::options().write(true).open(&raw).unwrap().set_modified(then).unwrap();

        w.run(&[alone(&raw)], Policy::UniqueName, 1);

        assert_eq!(std::fs::metadata(w.dest.join("A.ARW")).unwrap().modified().unwrap(), then);
    }

    #[test]
    fn a_photo_with_nothing_marked_gets_no_sidecar() {
        let w = World::new("unrated");
        let raw = w.file("A.ARW", b"raw");
        w.run(&[alone(&raw)], Policy::UniqueName, 1);
        assert_eq!(w.landed(), ["A.ARW"]);
    }

    #[test]
    fn a_card_is_scanned_through_its_folders_and_a_plain_folder_is_not() {
        let w = World::new("scan");
        w.file("DCIM/100MSDCF/A.ARW", b"a");
        w.file("DCIM/100MSDCF/A.JPG", b"a");
        w.file("DCIM/101MSDCF/A.ARW", b"b");
        w.file("PRIVATE/notes.txt", b"x");

        // DCIM holds only folders, so everything beneath it is offered.
        let found = scan(&w.card.join("DCIM"));
        assert_eq!(found.len(), 2, "two photos; the JPEG belongs to its RAW");
        assert!(found[0].display.ends_with("100MSDCF/A.JPG"));

        // A folder with photos of its own offers those, and not its subfolders'.
        w.file("DCIM/top.ARW", b"t");
        assert_eq!(scan(&w.card.join("DCIM")).len(), 1);
    }

    #[test]
    fn only_folders_with_photos_somewhere_inside_are_listed() {
        let w = World::new("browse");
        w.file("Weddings/2026/June/A.ARW", b"a");
        w.file("Empty/readme.txt", b"x");
        w.file("Loose/B.ARW", b"b");
        w.file("Loose/C.ARW", b"c");
        w.file(".hidden/D.ARW", b"d");
        w.file("$RECYCLE.BIN/E.ARW", b"e");

        let found = photo_folders(&w.card);

        let listed: Vec<(&str, usize)> = found.iter().map(|f| (f.name.as_str(), f.image_count)).collect();
        assert_eq!(listed, [("Loose", 2), ("Weddings", 0)], "by name; the count is of photos directly inside");
    }

    #[test]
    fn a_cameras_folders_are_known_by_name() {
        for name in ["DCIM", "dcim", "DCIM2", "PRIVATE", "M4ROOT", "MISC"] {
            assert!(is_camera_media(name), "{name}");
        }
        for name in ["Photos", "Weddings", "private stuff"] {
            assert!(!is_camera_media(name), "{name}");
        }
        let w = World::new("card");
        assert!(!has_camera_media(&w.card));
        w.file("DCIM/100/A.ARW", b"a");
        assert!(has_camera_media(&w.card));
    }

    #[test]
    fn a_disks_report_is_read_for_what_it_says() {
        let report = "<dict>\n\t<key>Ejectable</key>\n\t<true/>\n\t<key>Internal</key>\n\t<false/>\n\t<key>RemovableMedia</key>\n\t<false/>\n\t<key>SolidState</key>\n\t<true/>\n</dict>";
        assert_eq!(disk_from_report(report), Disk { internal: false, ejectable: true, solid_state: Some(true) });

        let inside = "<dict>\n\t<key>Ejectable</key>\n\t<false/>\n\t<key>Internal</key>\n\t<true/>\n</dict>";
        assert_eq!(disk_from_report(inside), Disk { internal: true, ejectable: false, solid_state: None });

        // Said nothing at all: not offered as a card, and copied to one at a time.
        assert_eq!(disk_from_report(""), Disk { internal: true, ejectable: false, solid_state: None });
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_volume_of_a_path_is_where_it_is_mounted() {
        assert_eq!(volume_of(Path::new("/Volumes/Photo Drive/2026/June")), PathBuf::from("/Volumes/Photo Drive"));
        assert_eq!(volume_of(Path::new("/Volumes/CARD")), PathBuf::from("/Volumes/CARD"));
        assert_eq!(volume_of(Path::new("/Users/someone/Pictures")), PathBuf::from("/"));
    }
}
