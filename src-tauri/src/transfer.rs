//! Copying and moving photos to another folder — for the right-click menu's
//! Copy and Move, and for Ship.
//!
//! One rule holds throughout: **a file already at the destination is never
//! lost, and a file is never left there half written.** Everything is copied
//! under a hidden temporary name beside where it is going and renamed into
//! place only once it is whole, and a source is only ever removed after its
//! copy has been checked.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

#[derive(Debug, Default)]
pub struct Outcome {
    /// Where each photo is now, in the order given: its new place, or where it
    /// started if it was skipped or could not be sent.
    pub placed: Vec<PathBuf>,
    /// Photos that are no longer where they started.
    pub moved: Vec<PathBuf>,
    /// Photos left out because their name was taken.
    pub skipped: Vec<PathBuf>,
    /// Photos that never reached the destination, each with why.
    pub failed: Vec<String>,
    /// Photos that arrived without their ratings, each with why.
    pub sidecar_failed: Vec<String>,
}

/// Sends every job to `destination`, calling `progress` with how many are done.
pub fn run(jobs: &[Job], destination: &Path, moving: bool, conflict: Conflict, progress: impl Fn(usize)) -> Outcome {
    let mut outcome = Outcome::default();
    for (index, job) in jobs.iter().enumerate() {
        send(job, destination, moving, conflict, &mut outcome);
        progress(index + 1);
    }
    outcome
}

/// How many of `jobs` would land on a file already at `destination`: the
/// photo's own name taken, or that of anything travelling with it.
pub fn conflicts(jobs: &[Job], destination: &Path) -> usize {
    let taken = |file: &PathBuf| file.file_name().is_some_and(|n| destination.join(n).exists());
    jobs.iter().filter(|job| taken(&job.owner) || job.companions.iter().any(taken)).count()
}

fn send(job: &Job, destination: &Path, moving: bool, conflict: Conflict, outcome: &mut Outcome) {
    let name = display_name(&job.owner);
    let Some(stem) = job.owner.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
        outcome.placed.push(job.owner.clone());
        outcome.failed.push(format!("{name}: not a file"));
        return;
    };

    // The photo and what travels with it take one name between them. Renaming
    // each on its own could leave a RAW called NAME_1 beside a JPEG still
    // called NAME, and that JPEG paired with the stranger already there.
    let travelling: Vec<&PathBuf> = std::iter::once(&job.owner).chain(job.companions.iter()).collect();
    let Some(suffix) = free_suffix(&stem, &travelling, destination, conflict) else {
        outcome.skipped.push(job.owner.clone());
        outcome.placed.push(job.owner.clone());
        return;
    };
    let renamed = |file: &Path| destination.join(rename(file, &stem, &suffix));

    // Its ratings are noted before the photo goes, since a move takes the
    // thing they are named after.
    let sidecars: Vec<PathBuf> = sidecars_of(&job.owner);

    let target = renamed(&job.owner);
    if let Err(e) = place(&job.owner, &target, moving, false) {
        outcome.placed.push(job.owner.clone());
        outcome.failed.push(format!("{name}: {e}"));
        return;
    }
    outcome.placed.push(target);
    if moving {
        outcome.moved.push(job.owner.clone());
    }

    for companion in &job.companions {
        if let Err(e) = place(companion, &renamed(companion), moving, false) {
            outcome.failed.push(format!("{}: {e}", display_name(companion)));
        }
    }
    for sidecar in sidecars {
        // Always replaced: it belongs to the photo just placed, and a stale
        // one beside that photo would report the wrong rating.
        if let Err(e) = place(&sidecar, &renamed(&sidecar), moving, true) {
            outcome.sidecar_failed.push(format!("{name}: {e}"));
        }
    }
}

/// The sidecars beside a photo, under either naming: `NAME.xmp`, `NAME.ext.xmp`.
fn sidecars_of(owner: &Path) -> Vec<PathBuf> {
    let mut double = owner.as_os_str().to_owned();
    double.push(".xmp");
    let mut found: Vec<PathBuf> = [owner.with_extension("xmp"), PathBuf::from(double)].into_iter().filter(|p| p.is_file()).collect();
    found.dedup();
    found
}

/// The first of "", "_1", "_2"… under which none of `files` is taken at
/// `destination`. `None` when the plain name is taken and the answer to that
/// is to skip.
fn free_suffix(stem: &str, files: &[&PathBuf], destination: &Path, conflict: Conflict) -> Option<String> {
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
fn rename(file: &Path, stem: &str, suffix: &str) -> String {
    let name = display_name(file);
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

fn display_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned())
}

static STAGED: AtomicU64 = AtomicU64::new(0);

/// Puts `source` at `target`. Unless `replace`, refuses if something is there.
fn place(source: &Path, target: &Path, moving: bool, replace: bool) -> io::Result<()> {
    if !replace && target.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "a file of that name is already there"));
    }
    // A move within one drive is a rename: instant, and whole or not at all.
    if moving && std::fs::rename(source, target).is_ok() {
        return Ok(());
    }

    // Otherwise a copy, made under a name nothing will mistake for a photo
    // and given its real name only when complete.
    let folder = target.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no destination folder"))?;
    let staged = folder.join(format!(".opencull-staging-{}-{}-{}", std::process::id(), STAGED.fetch_add(1, Ordering::Relaxed), display_name(target)));
    let copied = copy_whole(source, &staged).and_then(|()| std::fs::rename(&staged, target));
    if let Err(e) = copied {
        std::fs::remove_file(&staged).ok();
        return Err(e);
    }
    if moving {
        std::fs::remove_file(source)?;
    }
    Ok(())
}

/// Copies `source` to `to` and checks that all of it arrived, keeping the
/// date it was last changed — which is what most software sorts photos by.
fn copy_whole(source: &Path, to: &Path) -> io::Result<()> {
    let expected = std::fs::metadata(source)?;
    let written = std::fs::copy(source, to)?;
    if written != expected.len() || std::fs::metadata(to)?.len() != expected.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the copy came out a different size from the original"));
    }
    if let Ok(modified) = expected.modified() {
        if let Ok(file) = std::fs::File::options().write(true).open(to) {
            file.set_modified(modified).ok();
        }
    }
    Ok(())
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
    fn ratings_replace_stale_ones_left_at_the_destination() {
        let f = Folders::new("stale");
        std::fs::write(f.to.join("A.xmp"), "old stars").unwrap();
        let raw = f.source("A.ARW", "raw");
        f.source("A.xmp", "new stars");

        let out = run(&[job(&raw)], &f.to, false, Conflict::KeepBoth, |_| {});

        assert!(out.sidecar_failed.is_empty());
        assert_eq!(f.there("A.xmp").as_deref(), Some("new stars"));
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
    fn a_taken_jpeg_name_counts_against_its_raw() {
        let f = Folders::new("count-pair");
        std::fs::write(f.to.join("A.JPG"), "a stranger's").unwrap();
        let raw = f.source("A.ARW", "raw");
        let jpg = f.source("A.JPG", "jpeg");

        assert_eq!(conflicts(&[Job { owner: raw.clone(), companions: vec![jpg] }], &f.to), 1);
        assert_eq!(conflicts(&[job(&raw)], &f.to), 0, "on its own the RAW's name is free");
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
