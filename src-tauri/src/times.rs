//! Capture times, remembered between openings of a folder.
//!
//! Reading when each photo was taken is quick — a few bytes from the front of
//! each file — but it is still a read per photo, off a drive that may be a
//! card. A folder opened a second time has not changed, in the usual case,
//! and should not be read again to learn what was learned the first time.
//!
//! One small file per folder. A photo's remembered time is used only while
//! its size and its modified date are what they were; anything else and it is
//! read afresh.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use xxhash_rust::xxh3::xxh3_128;

use crate::scan::Scanned;

/// Size, modified date, and the capture time — which may be none, for a file
/// that was read and turned out not to carry one.
type Entry = (u64, u64, Option<f64>);

#[derive(Clone)]
pub struct Times {
    dir: PathBuf,
}

impl Times {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        Self { dir }
    }

    fn file_for(&self, folder: &Path) -> PathBuf {
        self.dir.join(format!("{:032x}.json", xxh3_128(folder.to_string_lossy().as_bytes())))
    }

    /// What is remembered about a folder's photos, by path. Empty for a
    /// folder never opened, or one whose record cannot be read — a damaged
    /// record costs one re-read, not an error.
    pub fn load(&self, folder: &Path) -> HashMap<String, Entry> {
        std::fs::read(self.file_for(folder))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Remembers the times read for a folder. `times[i]` belongs to
    /// `images[i]`; an outer `None` means that photo was not read, and it is
    /// left out rather than recorded as having no time.
    pub fn save(&self, folder: &Path, images: &[Scanned], times: &[Option<Option<f64>>]) {
        let record: HashMap<String, Entry> = images
            .iter()
            .zip(times)
            .filter_map(|(image, time)| {
                time.map(|t| (image.path.to_string_lossy().into_owned(), (image.size, image.modified, t)))
            })
            .collect();
        let Ok(bytes) = serde_json::to_vec(&record) else { return };
        crate::fsx::atomic_write(&self.file_for(folder), &bytes, false).ok();
    }
}

/// The remembered time for a photo, if what is remembered still describes
/// the file on disk.
pub fn remembered(known: &HashMap<String, Entry>, image: &Scanned) -> Option<Option<f64>> {
    let (size, modified, time) = known.get(image.path.to_string_lossy().as_ref())?;
    (*size == image.size && *modified == image.modified).then_some(*time)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(name: &str, size: u64, modified: u64) -> Scanned {
        Scanned {
            path: PathBuf::from(format!("/shoot/{name}")),
            display: PathBuf::from(format!("/shoot/{name}")),
            name: name.into(),
            subpath: None,
            version: String::new(),
            size,
            modified,
        }
    }

    fn scratch(name: &str) -> Times {
        let dir = std::env::temp_dir().join(format!("opencull-times-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        Times::new(dir)
    }

    #[test]
    fn what_is_remembered_comes_back_for_an_unchanged_file() {
        let times = scratch("roundtrip");
        let folder = Path::new("/shoot");
        let images = vec![photo("A.ARW", 100, 5), photo("B.ARW", 200, 6), photo("C.ARW", 300, 7)];
        // A has a time, B was read and has none, C was never got to.
        times.save(folder, &images, &[Some(Some(1234.5)), Some(None), None]);

        let known = times.load(folder);
        assert_eq!(remembered(&known, &images[0]), Some(Some(1234.5)));
        assert_eq!(remembered(&known, &images[1]), Some(None), "read, and known to have no time");
        assert_eq!(remembered(&known, &images[2]), None, "never read, so nothing is claimed");
        std::fs::remove_dir_all(&times.dir).ok();
    }

    #[test]
    fn a_file_that_has_changed_is_not_trusted() {
        let times = scratch("changed");
        let folder = Path::new("/shoot");
        times.save(folder, &[photo("A.ARW", 100, 5)], &[Some(Some(1.0))]);
        let known = times.load(folder);

        assert_eq!(remembered(&known, &photo("A.ARW", 101, 5)), None, "a different size");
        assert_eq!(remembered(&known, &photo("A.ARW", 100, 9)), None, "saved again since");
        assert_eq!(remembered(&known, &photo("Z.ARW", 100, 5)), None, "a different file");
        std::fs::remove_dir_all(&times.dir).ok();
    }

    #[test]
    fn folders_are_kept_apart_and_an_unknown_one_is_empty() {
        let times = scratch("apart");
        times.save(Path::new("/shoot-one"), &[photo("A.ARW", 1, 1)], &[Some(Some(1.0))]);
        assert_eq!(times.load(Path::new("/shoot-one")).len(), 1);
        assert!(times.load(Path::new("/shoot-two")).is_empty());

        // A record that is not what it should be is ignored, not fatal.
        std::fs::write(times.file_for(Path::new("/shoot-one")), b"not json").unwrap();
        assert!(times.load(Path::new("/shoot-one")).is_empty());
        std::fs::remove_dir_all(&times.dir).ok();
    }
}
