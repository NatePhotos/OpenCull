//! Putting files in place so that a crash, a pulled drive or a power cut
//! never leaves one half written, and never costs the file that was there.
//!
//! Each file is written under a hidden name beside where it is going, synced
//! to the disk, and only then given its real name. The sync matters: a write
//! the system has accepted can sit in memory for seconds, and a move that
//! deletes the original in that time can lose both.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// What every staged file's name begins with. Nothing a camera writes looks
/// like this, so a leftover is never mistaken for a photo.
pub const STAGING_PREFIX: &str = ".opencull-staging-";

static STAGED: AtomicU64 = AtomicU64::new(0);

/// A path's file name, for saying which file. The whole path for one that
/// has no file name (a drive's root).
pub fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// A fresh hidden name beside `target`, for writing it under until it is whole.
pub fn staging_path(target: &Path) -> io::Result<PathBuf> {
    let folder = target.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no destination folder"))?;
    Ok(folder.join(format!("{STAGING_PREFIX}{}-{}-{}", std::process::id(), STAGED.fetch_add(1, Ordering::Relaxed), file_name(target))))
}

/// Makes a rename or a new file in `folder` survive a power cut. Systems that
/// cannot sync a folder (Windows, some network shares) keep folder changes
/// safe their own way, so that is not an error.
pub fn sync_dir(folder: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        match File::open(folder).and_then(|f| f.sync_all()) {
            Err(e) if matches!(e.kind(), io::ErrorKind::InvalidInput | io::ErrorKind::Unsupported | io::ErrorKind::PermissionDenied) => Ok(()),
            other => other,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = folder;
        Ok(())
    }
}

/// Gives `staged` the name `target`. Unless `replace`, refuses if something
/// is already there, including a file that appears there in the meantime.
pub fn put_in_place(staged: &Path, target: &Path, replace: bool) -> io::Result<()> {
    if replace {
        return std::fs::rename(staged, target);
    }
    // A hard link refuses an existing name atomically.
    match std::fs::hard_link(staged, target) {
        Ok(()) => {
            std::fs::remove_file(staged).ok();
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(already_there()),
        // Cards and many outside drives (FAT, exFAT) have no hard links.
        // There, check and rename is the best the system offers.
        Err(_) => {
            if target.exists() {
                return Err(already_there());
            }
            std::fs::rename(staged, target)
        }
    }
}

fn already_there() -> io::Error {
    io::Error::new(io::ErrorKind::AlreadyExists, "a file of that name is already there")
}

/// Writes `bytes` to `path` whole or not at all. `durable` syncs it to the
/// disk first: wanted for the photographer's work, not for rebuildable caches.
pub fn atomic_write(path: &Path, bytes: &[u8], durable: bool) -> io::Result<()> {
    let staged = staging_path(path)?;
    let written = (|| {
        std::fs::write(&staged, bytes)?;
        if durable {
            File::open(&staged)?.sync_all()?;
        }
        std::fs::rename(&staged, path)?;
        if durable {
            sync_dir(path.parent().unwrap_or(Path::new(".")))?;
        }
        Ok(())
    })();
    if written.is_err() {
        std::fs::remove_file(&staged).ok();
    }
    written
}

/// Copies `source` to `target`, checks its size and syncs it, keeping the
/// modified date. Unless `replace`, refuses to land on an existing file.
pub fn staged_copy(source: &Path, target: &Path, replace: bool) -> io::Result<()> {
    let staged = staging_path(target)?;
    let copied = (|| {
        let expected = std::fs::metadata(source)?;
        let written = std::fs::copy(source, &staged)?;
        let file = File::options().write(true).open(&staged)?;
        if written != expected.len() || file.metadata()?.len() != expected.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the copy came out a different size from the original"));
        }
        if let Ok(modified) = expected.modified() {
            file.set_modified(modified).ok();
        }
        file.sync_all()?;
        drop(file);
        put_in_place(&staged, target, replace)?;
        sync_dir(target.parent().unwrap_or(Path::new(".")))
    })();
    if copied.is_err() {
        std::fs::remove_file(&staged).ok();
    }
    copied
}

/// How old a leftover staged file must be before it is cleared away.
const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

/// Removes staged files a crash left behind in `folder`: only another run's,
/// and only an hour after they were created. (A staged copy carries its
/// photo's modified date, so that date says nothing about its age.)
pub fn sweep(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let ours = format!("{STAGING_PREFIX}{}-", std::process::id());
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(STAGING_PREFIX) || name.starts_with(&ours) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        // Without a creation time, leave it: better a stray hidden file than
        // removing one in use.
        let Ok(made) = meta.created() else { continue };
        if meta.is_file() && now.duration_since(made).is_ok_and(|age| age >= STALE_AFTER) {
            std::fs::remove_file(entry.path()).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opencull-fsx-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_copy_arrives_whole_and_leaves_nothing_temporary() {
        let dir = scratch("copy");
        std::fs::write(dir.join("a"), b"photo").unwrap();
        staged_copy(&dir.join("a"), &dir.join("b"), false).unwrap();
        assert_eq!(std::fs::read(dir.join("b")).unwrap(), b"photo");
        assert_eq!(names(&dir), ["a", "b"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_copy_never_lands_on_a_file_unless_told_to() {
        let dir = scratch("refuse");
        std::fs::write(dir.join("a"), b"new").unwrap();
        std::fs::write(dir.join("b"), b"already here").unwrap();

        let refused = staged_copy(&dir.join("a"), &dir.join("b"), false).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(dir.join("b")).unwrap(), b"already here");
        assert_eq!(names(&dir), ["a", "b"], "the staged copy is cleared away");

        staged_copy(&dir.join("a"), &dir.join("b"), true).unwrap();
        assert_eq!(std::fs::read(dir.join("b")).unwrap(), b"new");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_atomic_write_replaces_whole() {
        let dir = scratch("write");
        let path = dir.join("a.xmp");
        atomic_write(&path, b"one", true).unwrap();
        atomic_write(&path, b"two", false).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(names(&dir), ["a.xmp"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_sweep_keeps_this_runs_files_and_new_ones() {
        let dir = scratch("sweep");
        let mine = staging_path(&dir.join("a")).unwrap();
        std::fs::write(&mine, b"in use").unwrap();
        let other = dir.join(format!("{STAGING_PREFIX}1-0-b"));
        std::fs::write(&other, b"just made by another run").unwrap();
        std::fs::write(dir.join("photo.ARW"), b"raw").unwrap();

        sweep(&dir);

        assert!(mine.exists(), "a file this run is writing is never touched");
        assert!(other.exists(), "an hour has not passed");
        assert!(dir.join("photo.ARW").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
