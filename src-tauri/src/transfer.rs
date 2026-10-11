//! Copying and moving photos to another folder — for the right-click menu's
//! Copy and Move, and for Ship.
//!
//! One rule holds throughout: **a file already at the destination is never
//! lost, and a file is never left there half written.** Everything is copied
//! under a hidden temporary name beside where it is going, pushed to the disk,
//! and renamed into place only once it is whole. A source is only ever
//! removed after its copy is whole and on the disk.
//!
//! That rule covers sidecars too. A sidecar at the destination may hold
//! another photo's develop settings, so a photo is only sent under a name
//! whose sidecar names are free as well.

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fsx;
use crate::xmp;

/// What to do when a photo's name is already taken at the destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// Send it alongside, renamed `NAME_1.ext`. A same-name file is more often
    /// a different photo — camera counters reset — than the same one again.
    KeepBoth,
    /// Leave the destination alone and do not send this photo.
    Skip,
}

/// One photo to send.
#[derive(Clone, Debug)]
pub struct Job {
    /// The photo itself.
    pub owner: PathBuf,
    /// Files that travel with it under the same name: a RAW's JPEG.
    pub companions: Vec<PathBuf>,
}

/// One photo that did not go as it should, and why.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    /// The photo, as the app knows it.
    pub path: PathBuf,
    /// The file's name, for saying which.
    pub name: String,
    pub reason: String,
}

impl Failure {
    fn new(photo: &Path, file: &Path, reason: impl ToString) -> Self {
        Failure { path: photo.to_path_buf(), name: fsx::file_name(file), reason: reason.to_string() }
    }
}

#[derive(Debug, Default)]
pub struct Outcome {
    /// Where each photo is now, in the order given: its new place, or where it
    /// started if it was skipped or could not be sent.
    pub placed: Vec<PathBuf>,
    /// Photos that are no longer where they started.
    pub moved: Vec<PathBuf>,
    /// Photos left out because their name was taken.
    pub skipped: Vec<PathBuf>,
    /// Photos that never reached the destination, or reached it but could
    /// not be removed from where they started.
    pub failed: Vec<Failure>,
    /// Photos that arrived without their ratings.
    pub sidecar_failed: Vec<Failure>,
}

/// Sends every job to `destination`, calling `progress` with how many are done.
pub fn run(jobs: &[Job], destination: &Path, moving: bool, conflict: Conflict, progress: impl Fn(usize)) -> Outcome {
    fsx::sweep(destination);
    let mut outcome = Outcome::default();
    for (index, job) in jobs.iter().enumerate() {
        send(job, destination, moving, conflict, &mut outcome);
        progress(index + 1);
    }
    outcome
}

/// How many of `jobs` would land on a name already taken at `destination`:
/// the photo's, its companions', or a sidecar's.
pub fn conflicts(jobs: &[Job], destination: &Path) -> usize {
    jobs.iter()
        .filter(|job| {
            let Some(stem) = stem_of(&job.owner) else { return false };
            names_of(job).iter().any(|f| destination.join(rename(f, &stem, "")).exists())
        })
        .count()
}

fn stem_of(path: &Path) -> Option<String> {
    path.file_stem().map(|s| s.to_string_lossy().into_owned())
}

/// Every name a photo takes at its destination: itself, its companions, and
/// both sidecar names — even with no sidecar, since one already there would
/// be read as its own.
fn names_of(job: &Job) -> Vec<PathBuf> {
    let mut names: Vec<PathBuf> = std::iter::once(job.owner.clone()).chain(job.companions.iter().cloned()).collect();
    names.extend(xmp::sidecar_candidates(&job.owner));
    for companion in &job.companions {
        names.push(xmp::sidecar_candidates(companion)[1].clone());
    }
    names
}

fn send(job: &Job, destination: &Path, moving: bool, conflict: Conflict, outcome: &mut Outcome) {
    let Some(stem) = stem_of(&job.owner) else {
        outcome.placed.push(job.owner.clone());
        outcome.failed.push(Failure::new(&job.owner, &job.owner, "not a file"));
        return;
    };

    // The photo and everything that travels with it share one suffix, so a
    // RAW renamed NAME_1 is never left beside a stranger's NAME.JPG.
    let Some(suffix) = free_suffix(&stem, &names_of(job), destination, conflict) else {
        outcome.skipped.push(job.owner.clone());
        outcome.placed.push(job.owner.clone());
        return;
    };
    let renamed = |file: &Path| destination.join(rename(file, &stem, &suffix));

    // Its ratings are noted before the photo goes, since a move takes the
    // thing they are named after.
    let sidecars = xmp::sidecars_of(&job.owner);

    let target = renamed(&job.owner);
    // If the photo had to stay behind, what travels with it stays too.
    let rest_moving = match place(&job.owner, &target, moving) {
        Err(e) => {
            outcome.placed.push(job.owner.clone());
            outcome.failed.push(Failure::new(&job.owner, &job.owner, e));
            return;
        }
        Ok(Placed::SourceKept(e)) => {
            outcome.placed.push(target);
            outcome.failed.push(Failure::new(&job.owner, &job.owner, format!("copied, but the original could not be removed: {e}")));
            false
        }
        Ok(Placed::Done) => {
            outcome.placed.push(target);
            if moving {
                outcome.moved.push(job.owner.clone());
            }
            moving
        }
    };

    for companion in &job.companions {
        let result = place(companion, &renamed(companion), rest_moving);
        let problem = match result {
            Err(e) => Some(e.to_string()),
            Ok(Placed::SourceKept(e)) => Some(format!("copied, but the original could not be removed: {e}")),
            Ok(Placed::Done) => None,
        };
        if let Some(why) = problem {
            outcome.failed.push(Failure::new(&job.owner, companion, why));
        }
    }
    for sidecar in sidecars {
        if let Err(e) | Ok(Placed::SourceKept(e)) = place(&sidecar, &renamed(&sidecar), rest_moving) {
            outcome.sidecar_failed.push(Failure::new(&job.owner, &job.owner, e));
        }
    }
}

/// The first of "", "_1", "_2"… under which none of `files` is taken at
/// `destination`. `None` when the plain name is taken and the answer to that
/// is to skip.
fn free_suffix(stem: &str, files: &[PathBuf], destination: &Path, conflict: Conflict) -> Option<String> {
    for n in 0..10_000 {
        let suffix = if n == 0 { String::new() } else { format!("_{n}") };
        if files.iter().all(|f| !destination.join(rename(f, stem, &suffix)).exists()) {
            return Some(suffix);
        }
        if conflict == Conflict::Skip {
            return None;
        }
    }
    None
}

/// `file`'s name with `suffix` added to the stem it shares with its photo:
/// `DSC1.ARW` → `DSC1_1.ARW`, `DSC1.ARW.xmp` → `DSC1_1.ARW.xmp`.
pub(crate) fn rename(file: &Path, stem: &str, suffix: &str) -> String {
    let name = fsx::file_name(file);
    if suffix.is_empty() {
        return name;
    }
    match name.strip_prefix(stem) {
        Some(rest) => format!("{stem}{suffix}{rest}"),
        // A companion named differently from its photo keeps its own stem.
        None => match name.rsplit_once('.') {
            Some((own, ext)) => format!("{own}{suffix}.{ext}"),
            None => format!("{name}{suffix}"),
        },
    }
}


/// How placing a file went, when it got there.
enum Placed {
    Done,
    /// It is at the destination, whole, but the original could not be
    /// removed: a locked card, a read-only share. Both copies exist.
    SourceKept(io::Error),
}

/// Puts `source` at `target`, refusing if something is already there.
fn place(source: &Path, target: &Path, moving: bool) -> io::Result<Placed> {
    if target.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "a file of that name is already there"));
    }
    // A move within one drive is a rename: instant, and whole or not at all.
    if moving && std::fs::rename(source, target).is_ok() {
        fsx::sync_dir(target.parent().unwrap_or(Path::new(".")))?;
        return Ok(Placed::Done);
    }
    // Otherwise a copy, on the disk before the original is touched.
    fsx::staged_copy(source, target, false)?;
    if moving {
        if let Err(e) = std::fs::remove_file(source) {
            return Ok(Placed::SourceKept(e));
        }
    }
    Ok(Placed::Done)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source folder and a destination folder, removed when dropped.
    struct Folders {
        root: PathBuf,
        from: PathBuf,
        to: PathBuf,
    }

    impl Folders {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("opencull-transfer-{name}-{}", std::process::id()));
            std::fs::remove_dir_all(&root).ok();
            let (from, to) = (root.join("from"), root.join("to"));
            std::fs::create_dir_all(&from).unwrap();
            std::fs::create_dir_all(&to).unwrap();
            Self { root, from, to }
        }

        fn source(&self, name: &str, contents: &str) -> PathBuf {
            let path = self.from.join(name);
            std::fs::write(&path, contents).unwrap();
            path
        }

        fn there(&self, name: &str) -> Option<String> {
            std::fs::read_to_string(self.to.join(name)).ok()
        }

        fn names_there(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(&self.to).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            names
        }
    }

    impl Drop for Folders {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).ok();
        }
    }

    fn job(owner: &Path) -> Job {
        Job { owner: owner.to_path_buf(), companions: vec![] }
    }

    #[test]
    fn a_copy_takes_the_photo_and_its_ratings_and_leaves_the_original() {
        let f = Folders::new("copy");
        let raw = f.source("A.ARW", "raw");
        f.source("A.xmp", "stars");

        let out = run(&[job(&raw)], &f.to, false, Conflict::KeepBoth, |_| {});

        assert!(out.failed.is_empty() && out.sidecar_failed.is_empty() && out.skipped.is_empty());
        assert_eq!(out.placed, vec![f.to.join("A.ARW")]);
        assert!(out.moved.is_empty());
        assert_eq!(f.names_there(), ["A.ARW", "A.xmp"]);
        assert_eq!(f.there("A.xmp").as_deref(), Some("stars"));
        assert!(raw.exists() && f.from.join("A.xmp").exists(), "a copy must not take the original");
    }

    #[test]
    fn a_move_takes_everything_and_leaves_nothing_behind() {
        let f = Folders::new("move");
        let raw = f.source("A.ARW", "raw");
        let jpg = f.source("A.JPG", "jpg");
        f.source("A.xmp", "stars");

        let out = run(&[Job { owner: raw.clone(), companions: vec![jpg.clone()] }], &f.to, true, Conflict::KeepBoth, |_| {});

        assert!(out.failed.is_empty());
        assert_eq!(out.moved, vec![raw.clone()]);
        assert_eq!(f.names_there(), ["A.ARW", "A.JPG", "A.xmp"]);
        assert_eq!(std::fs::read_dir(&f.from).unwrap().count(), 0, "a move leaves the folder it came from empty");
    }

    #[test]
    fn keeping_both_renames_the_newcomer_and_never_touches_what_was_there() {
        let f = Folders::new("keepboth");
        std::fs::write(f.to.join("A.ARW"), "already here").unwrap();
        let raw = f.source("A.ARW", "new");
        let jpg = f.source("A.JPG", "new jpg");
        f.source("A.xmp", "new stars");

        let out = run(&[Job { owner: raw, companions: vec![jpg] }], &f.to, false, Conflict::KeepBoth, |_| {});

        assert!(out.failed.is_empty());
        assert_eq!(out.placed, vec![f.to.join("A_1.ARW")]);
        assert_eq!(f.there("A.ARW").as_deref(), Some("already here"));
        // The JPEG and the ratings follow the photo's new name, so neither
        // attaches itself to the photo that was already there.
        assert_eq!(f.names_there(), ["A.ARW", "A_1.ARW", "A_1.JPG", "A_1.xmp"]);
    }

    #[test]
    fn a_taken_name_among_the_companions_moves_the_whole_photo_along() {
        let f = Folders::new("companion");
        std::fs::write(f.to.join("A.JPG"), "someone else's").unwrap();
        let raw = f.source("A.ARW", "raw");
        let jpg = f.source("A.JPG", "mine");

        run(&[Job { owner: raw, companions: vec![jpg] }], &f.to, false, Conflict::KeepBoth, |_| {});

        assert_eq!(f.there("A.JPG").as_deref(), Some("someone else's"));
        assert_eq!(f.names_there(), ["A.JPG", "A_1.ARW", "A_1.JPG"]);
    }

    #[test]
    fn skipping_leaves_both_sides_exactly_as_they_were() {
        let f = Folders::new("skip");
        std::fs::write(f.to.join("A.ARW"), "already here").unwrap();
        let taken = f.source("A.ARW", "new");
        f.source("A.xmp", "new stars");
        let free = f.source("B.ARW", "b");

        let out = run(&[job(&taken), job(&free)], &f.to, true, Conflict::Skip, |_| {});

        assert_eq!(out.skipped, vec![taken.clone()]);
        // Skipped, so it is still where it started — which is where anything
        // opening it afterwards should look.
        assert_eq!(out.placed, vec![taken.clone(), f.to.join("B.ARW")]);
        assert_eq!(out.moved, vec![free]);
        assert_eq!(f.there("A.ARW").as_deref(), Some("already here"));
        assert!(taken.exists() && f.from.join("A.xmp").exists(), "a skipped photo and its ratings stay put");
        assert_eq!(f.names_there(), ["A.ARW", "B.ARW"], "its ratings must not land beside the photo already there");
    }

    #[test]
    fn two_photos_of_one_name_in_the_same_run_do_not_collide() {
        let f = Folders::new("twins");
        let first = f.source("A.ARW", "first");
        let sub = f.from.join("101");
        std::fs::create_dir_all(&sub).unwrap();
        let second = sub.join("A.ARW");
        std::fs::write(&second, "second").unwrap();

        let out = run(&[job(&first), job(&second)], &f.to, false, Conflict::KeepBoth, |_| {});

        assert!(out.failed.is_empty());
        assert_eq!(f.there("A.ARW").as_deref(), Some("first"));
        assert_eq!(f.there("A_1.ARW").as_deref(), Some("second"));
    }

    #[test]
    fn a_sidecar_already_at_the_destination_is_never_replaced() {
        let f = Folders::new("stale");
        std::fs::write(f.to.join("A.xmp"), "someone's edits").unwrap();
        let raw = f.source("A.ARW", "raw");
        f.source("A.xmp", "new stars");

        let out = run(&[job(&raw)], &f.to, false, Conflict::KeepBoth, |_| {});

        assert!(out.sidecar_failed.is_empty() && out.failed.is_empty());
        assert_eq!(f.there("A.xmp").as_deref(), Some("someone's edits"));
        assert_eq!(f.names_there(), ["A.xmp", "A_1.ARW", "A_1.xmp"]);
        assert_eq!(f.there("A_1.xmp").as_deref(), Some("new stars"));
    }

    #[test]
    fn a_photo_never_lands_beside_another_photos_sidecar() {
        // A.NEF's edits are in A.xmp. An A.ARW sent in beside it, even one
        // with no sidecar of its own, would be read as having those edits.
        let f = Folders::new("foreign");
        std::fs::write(f.to.join("A.NEF"), "nikon").unwrap();
        std::fs::write(f.to.join("A.xmp"), "the NEF's edits").unwrap();
        let raw = f.source("A.ARW", "sony");

        let out = run(&[job(&raw)], &f.to, false, Conflict::KeepBoth, |_| {});

        assert!(out.failed.is_empty());
        assert_eq!(out.placed, vec![f.to.join("A_1.ARW")]);
        assert_eq!(f.there("A.xmp").as_deref(), Some("the NEF's edits"));
        assert_eq!(conflicts(&[job(&raw)], &f.to), 1, "and it is counted as a conflict before anything is written");
    }

    #[cfg(unix)]
    #[test]
    fn a_move_whose_original_cannot_be_removed_says_so_and_splits_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let f = Folders::new("locked");
        let raw = f.source("A.ARW", "raw");
        let jpg = f.source("A.JPG", "jpg");
        f.source("A.xmp", "stars");
        // A folder nothing can be removed from: a locked card, a read-only share.
        std::fs::set_permissions(&f.from, std::fs::Permissions::from_mode(0o555)).unwrap();

        let out = run(&[Job { owner: raw.clone(), companions: vec![jpg.clone()] }], &f.to, true, Conflict::KeepBoth, |_| {});

        std::fs::set_permissions(&f.from, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(out.moved.is_empty(), "it did not move: the original is still there");
        assert_eq!(out.failed.len(), 1);
        assert!(out.failed[0].reason.contains("could not be removed"), "{:?}", out.failed);
        assert_eq!(out.failed[0].path, raw);
        assert_eq!(f.names_there(), ["A.ARW", "A.JPG", "A.xmp"]);
        assert!(raw.exists() && jpg.exists() && f.from.join("A.xmp").exists());
    }

    #[test]
    fn both_ways_of_naming_a_sidecar_travel() {
        let f = Folders::new("double");
        std::fs::write(f.to.join("A.ARW"), "already here").unwrap();
        let raw = f.source("A.ARW", "raw");
        f.source("A.ARW.xmp", "stars");

        run(&[job(&raw)], &f.to, false, Conflict::KeepBoth, |_| {});

        assert_eq!(f.there("A_1.ARW.xmp").as_deref(), Some("stars"));
    }

    #[test]
    fn a_photo_that_cannot_be_sent_is_reported_and_left_alone() {
        let f = Folders::new("missing");
        let raw = f.source("A.ARW", "raw");
        let nowhere = f.root.join("not-a-folder");

        let out = run(&[job(&raw)], &nowhere, true, Conflict::KeepBoth, |_| {});

        assert_eq!(out.failed.len(), 1);
        assert!(out.moved.is_empty());
        assert_eq!(out.placed, vec![raw.clone()], "it is still where it started");
        assert_eq!(std::fs::read_to_string(&raw).unwrap(), "raw");
    }

    #[test]
    fn nothing_temporary_is_left_at_the_destination() {
        let f = Folders::new("clean");
        let raws: Vec<PathBuf> = (0..5).map(|i| f.source(&format!("P{i}.ARW"), "raw")).collect();
        let jobs: Vec<Job> = raws.iter().map(|r| job(r)).collect();

        run(&jobs, &f.to, false, Conflict::KeepBoth, |_| {});

        assert!(f.names_there().iter().all(|n| !n.starts_with('.')), "{:?}", f.names_there());
        assert_eq!(f.names_there().len(), 5);
    }

    #[test]
    fn a_copy_keeps_the_date_the_photo_was_last_changed() {
        let f = Folders::new("dates");
        let raw = f.source("A.ARW", "raw");
        let then = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
        std::fs::File::options().write(true).open(&raw).unwrap().set_modified(then).unwrap();

        run(&[job(&raw)], &f.to, false, Conflict::KeepBoth, |_| {});

        assert_eq!(std::fs::metadata(f.to.join("A.ARW")).unwrap().modified().unwrap(), then);
    }

    #[test]
    fn conflicts_are_counted_before_anything_is_written() {
        let f = Folders::new("count");
        std::fs::write(f.to.join("A.ARW"), "x").unwrap();
        let a = f.source("A.ARW", "a");
        let b = f.source("B.ARW", "b");

        assert_eq!(conflicts(&[job(&a), job(&b)], &f.to), 1);
        assert_eq!(f.names_there(), ["A.ARW"]);
    }

    #[test]
    fn progress_counts_every_photo_whatever_became_of_it() {
        let f = Folders::new("progress");
        std::fs::write(f.to.join("A.ARW"), "x").unwrap();
        let a = f.source("A.ARW", "a");
        let b = f.source("B.ARW", "b");
        let seen = std::sync::Mutex::new(Vec::new());

        run(&[job(&a), job(&b)], &f.to, false, Conflict::Skip, |n| seen.lock().unwrap().push(n));

        assert_eq!(*seen.lock().unwrap(), [1, 2]);
    }
}
