//! Previews already made, kept on disk so they are only ever made once.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use xxhash_rust::xxh3::xxh3_128;

/// Changing how previews are made changes this, which orphans every preview
/// made the old way instead of serving it as though it were current.
const PIPELINE_VERSION: u32 = 1;

/// How stale a file's "last used" stamp may get before it is refreshed.
/// Holding an arrow key down must not issue a metadata write per frame, and
/// eviction only ever needs to know "used lately or not in ages".
const TOUCH_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub const DEFAULT_LIMIT_BYTES: u64 = 5 * 1024 * 1024 * 1024;
/// The least and most the limit may be set to: one gigabyte, and two hundred.
pub const MIN_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_LIMIT_BYTES: u64 = 200 * 1024 * 1024 * 1024;

/// How much may be written before the cache is checked against its limit
/// again. Preparing a folder ahead writes about a megabyte a photo, so a long
/// session would otherwise run far past the limit before the next launch
/// noticed. Checking means listing every preview, which is why it is done
/// this often and no oftener.
const CHECK_EVERY_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone)]
pub struct Cache {
    dir: PathBuf,
    written_since_check: Arc<AtomicU64>,
    /// How many bytes the cache is kept under.
    limit: Arc<AtomicU64>,
    /// Set once the cache is being taken away, after which nothing more is
    /// written to it.
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl Cache {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        Self {
            dir,
            written_since_check: Arc::new(AtomicU64::new(0)),
            limit: Arc::new(AtomicU64::new(DEFAULT_LIMIT_BYTES)),
            closed: Default::default(),
        }
    }

    pub fn limit(&self) -> u64 {
        self.limit.load(Ordering::Relaxed)
    }

    /// Changes how large the cache may grow. Takes effect at the next trim.
    pub fn set_limit(&self, bytes: u64) {
        self.limit.store(bytes.clamp(MIN_LIMIT_BYTES, MAX_LIMIT_BYTES), Ordering::Relaxed);
    }

    /// Stops the cache accepting anything further. For a run that is about to
    /// delete its cache: a preview still being made when the folder goes
    /// would otherwise be written a moment later, and bring the folder back
    /// with a photograph in it.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the preview of `source` at `tier` lives. `version` is whatever
    /// the caller knows that changes when the file does.
    pub fn path_for(&self, source: &Path, version: &str, tier: &str) -> PathBuf {
        let key = format!("{PIPELINE_VERSION}\0{}\0{}\0{version}\0{tier}", crate::preview::CODEC, source.to_string_lossy());
        let hash = format!("{:032x}", xxh3_128(key.as_bytes()));
        // Spread across 256 folders: one folder holding every preview of a
        // year's shooting is slow to list on every filesystem there is.
        self.dir.join(&hash[..2]).join(format!("{}.jpg", &hash[2..]))
    }

    pub fn get(&self, path: &Path) -> Option<Vec<u8>> {
        let bytes = std::fs::read(path).ok()?;
        if bytes.is_empty() {
            return None;
        }
        // Record the use, coarsely, so eviction drops what has gone unused
        // rather than what happens to be oldest.
        if let Ok(meta) = std::fs::metadata(path) {
            let stale = meta
                .modified()
                .ok()
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .is_some_and(|age| age > TOUCH_INTERVAL);
            if stale {
                if let Ok(file) = std::fs::File::options().write(true).open(path) {
                    file.set_modified(SystemTime::now()).ok();
                }
            }
        }
        Some(bytes)
    }

    pub fn put(&self, path: &Path, bytes: &[u8]) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let Some(parent) = path.parent() else { return };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        // Written under another name and renamed into place, so a reader can
        // never open a preview that is only half there.
        crate::fsx::atomic_write(path, bytes, false).ok();

        let written = self.written_since_check.fetch_add(bytes.len() as u64, Ordering::Relaxed) + bytes.len() as u64;
        if written >= CHECK_EVERY_BYTES && self.written_since_check.swap(0, Ordering::Relaxed) >= CHECK_EVERY_BYTES {
            // On a thread of its own: the caller is a preview worker, and
            // listing the whole cache is not its job to wait for.
            let cache = self.clone();
            std::thread::Builder::new()
                .name("opencull-cache-trim".into())
                .spawn(move || {
                    cache.trim(cache.limit());
                })
                .ok();
        }
    }

    /// Bytes on disk, and how many previews that is.
    pub fn usage(&self) -> (u64, usize) {
        let entries = self.entries();
        (entries.iter().map(|e| e.1).sum(), entries.len())
    }

    fn entries(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        let mut out = Vec::new();
        let Ok(shards) = std::fs::read_dir(&self.dir) else { return out };
        for shard in shards.flatten() {
            let Ok(files) = std::fs::read_dir(shard.path()) else { continue };
            for file in files.flatten() {
                if let Ok(meta) = file.metadata() {
                    if meta.is_file() {
                        out.push((file.path(), meta.len(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
                    }
                }
            }
        }
        out
    }

    /// Brings the cache under `limit`, dropping the least recently used
    /// previews first. Returns how many bytes were freed.
    pub fn trim(&self, limit: u64) -> u64 {
        let mut entries = self.entries();
        let mut total: u64 = entries.iter().map(|e| e.1).sum();
        if total <= limit {
            return 0;
        }
        entries.sort_by_key(|e| e.2);
        let mut freed = 0;
        for (path, size, _) in entries {
            if total <= limit {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= size;
                freed += size;
            }
        }
        freed
    }

    pub fn clear(&self) -> u64 {
        self.trim(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> Cache {
        let dir = std::env::temp_dir().join(format!("opencull-cache-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        Cache::new(dir)
    }

    #[test]
    fn a_preview_put_away_comes_back_the_same() {
        let cache = scratch("roundtrip");
        let path = cache.path_for(Path::new("/photos/A.ARW"), "v1", "thumb320");
        assert_eq!(cache.get(&path), None);
        cache.put(&path, b"jpeg bytes");
        assert_eq!(cache.get(&path).as_deref(), Some(&b"jpeg bytes"[..]));
        std::fs::remove_dir_all(cache.dir()).ok();
    }

    #[test]
    fn a_changed_file_or_a_different_size_is_a_different_preview() {
        let cache = scratch("keys");
        let photo = Path::new("/photos/A.ARW");
        let base = cache.path_for(photo, "v1", "thumb320");
        assert_ne!(base, cache.path_for(photo, "v2", "thumb320"), "the file was edited");
        assert_ne!(base, cache.path_for(photo, "v1", "thumb640"), "another size");
        assert_ne!(base, cache.path_for(Path::new("/photos/B.ARW"), "v1", "thumb320"));
        assert_eq!(base, cache.path_for(photo, "v1", "thumb320"), "and the same again is the same");
        std::fs::remove_dir_all(cache.dir()).ok();
    }

    #[test]
    fn a_closed_cache_writes_nothing_more() {
        let cache = scratch("closed");
        let before = cache.path_for(Path::new("/a.ARW"), "v", "t");
        let after = cache.path_for(Path::new("/b.ARW"), "v", "t");
        cache.put(&before, b"kept");
        cache.close();
        cache.put(&after, b"late");

        assert!(before.exists());
        assert!(!after.exists(), "a preview finished after closing must not be written");
        // Nor may it bring a deleted cache back.
        std::fs::remove_dir_all(cache.dir()).unwrap();
        cache.put(&after, b"later still");
        assert!(!cache.dir().exists());
    }

    #[test]
    fn the_limit_can_be_changed_but_only_within_reason() {
        let dir = std::env::temp_dir().join(format!("opencull-cache-limit-{}", std::process::id()));
        let cache = Cache::new(dir.clone());
        assert_eq!(cache.limit(), DEFAULT_LIMIT_BYTES);

        cache.set_limit(20 * 1024 * 1024 * 1024);
        assert_eq!(cache.limit(), 20 * 1024 * 1024 * 1024);
        // A copy made earlier or later answers the same: they are one cache.
        assert_eq!(cache.clone().limit(), cache.limit());

        cache.set_limit(0);
        assert_eq!(cache.limit(), MIN_LIMIT_BYTES, "nothing can set it so low the cache is useless");
        cache.set_limit(u64::MAX);
        assert_eq!(cache.limit(), MAX_LIMIT_BYTES);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn trimming_drops_the_least_recently_used_first() {
        let cache = scratch("trim");
        let old = cache.path_for(Path::new("/old.ARW"), "v", "t");
        let new = cache.path_for(Path::new("/new.ARW"), "v", "t");
        cache.put(&old, &[0u8; 1000]);
        cache.put(&new, &[0u8; 1000]);
        // Age one of them by a day.
        let yesterday = SystemTime::now() - Duration::from_secs(86_400);
        std::fs::File::options().write(true).open(&old).unwrap().set_modified(yesterday).unwrap();

        assert_eq!(cache.usage(), (2000, 2));
        assert_eq!(cache.trim(5000), 0, "already under the limit");
        assert_eq!(cache.trim(1500), 1000);
        assert!(!old.exists());
        assert!(new.exists());
        std::fs::remove_dir_all(cache.dir()).ok();
    }
}
