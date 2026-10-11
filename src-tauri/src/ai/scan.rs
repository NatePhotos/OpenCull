//! Analysing a whole folder: what is remembered goes out at once, the rest
//! is shared out among a few workers, and results go back in batches and are
//! saved as they come.
//!
//! The app's side of it — which scan is still wanted, whether the
//! photographer is busy, what is on screen, where results go — comes in
//! through `Host`, so the scan itself can be run and tested on its own.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::store::{self, Entry, Record};
use super::{queue::Queue, Analysis, Failure};
use crate::scan::Scanned;

/// One photo's analysis, as the interface is told of it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Analysed {
    pub index: usize,
    #[serde(flatten)]
    pub record: Record,
}

/// What a scan needs from whoever runs it.
pub trait Host: Sync {
    /// Whether this is still the scan anyone wants: the folder still open,
    /// and no newer scan started.
    fn wanted(&self) -> bool;
    /// Whether to wait before the next photo: the photographer is moving
    /// about, and analysis stands aside for them.
    fn hold_back(&self) -> bool;
    /// The photos being looked at now, nearest first, to analyse next.
    fn front(&self) -> Vec<usize>;
    /// Hands over results. False when nobody is listening any more.
    fn send(&self, items: Vec<Analysed>) -> bool;
    /// Keeps what has been found, for the next time the folder is opened.
    fn save(&self, known: &HashMap<String, Entry>);
}

/// How many photos are analysed between reports to the interface.
const BATCH: usize = 12;
/// How long a part-filled batch waits before it is sent anyway.
const BATCH_WAIT: Duration = Duration::from_millis(400);
/// How many freshly analysed photos between saves of what has been found.
const SAVE_EVERY: usize = 150;
/// How long a held-back worker sleeps before asking again.
const HOLD_BACK: Duration = Duration::from_millis(60);

/// How many photos are analysed at once.
///
/// A few: enough to get through a folder in a minute or two, few enough that
/// the rest of the machine does not feel it. Each worker holds a whole photo
/// decoded at full size — about 180 MB for 60 megapixels, with the models'
/// working memory on top — so four at most, which keeps the peak under a
/// gigabyte on any machine.
fn workers(todo: usize) -> usize {
    (crate::cores() / 2).clamp(1, 4).min(todo.max(1))
}

/// Analyses `images` in `order` (outward from the photo on screen), with
/// `known` what an earlier scan remembered. Returns whether it got through
/// every photo.
pub fn run(
    images: &[Scanned],
    order: Vec<usize>,
    mut known: HashMap<String, Entry>,
    host: &impl Host,
    analyse: impl Fn(&Path) -> Result<Analysis, Failure> + Sync,
) -> bool {
    // Photos no longer in the folder are forgotten, or the record grows with
    // every photo ever deleted or renamed.
    let present: HashSet<String> = images.iter().map(|i| i.display.to_string_lossy().into_owned()).collect();
    known.retain(|path, _| present.contains(path));

    // What is already known goes out at once.
    let mut todo = Vec::new();
    let mut ready = Vec::new();
    for index in order {
        let Some(image) = images.get(index) else { continue };
        match store::remembered(&known, &image.display, &image.version) {
            Some(record) => ready.push(Analysed { index, record }),
            None => todo.push(index),
        }
    }
    for chunk in ready.chunks(256) {
        if !host.send(chunk.to_vec()) {
            return false;
        }
    }

    let known = Mutex::new(known);
    // Saved from a copy, so no worker waits on the disk.
    let save = || {
        let snapshot = known.lock().unwrap().clone();
        host.save(&snapshot);
    };

    let workers = workers(todo.len());
    let queue = Queue::new(todo);
    let (tx, rx) = mpsc::channel::<Analysed>();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let tx = tx.clone();
            let (queue, known, analyse) = (&queue, &known, &analyse);
            scope.spawn(move || loop {
                // Whoever is turning pages comes first.
                while host.wanted() && host.hold_back() {
                    std::thread::sleep(HOLD_BACK);
                }
                if !host.wanted() {
                    return;
                }
                // What is being looked at now, then the rest.
                let Some(index) = queue.take(&host.front()) else { return };
                let image = &images[index];
                // A photo that cannot be analysed is marked so, and left out
                // of ranking. Remembered when the file itself is the trouble,
                // so it is not tried on every open; not when the disk was, so
                // it is tried again next time.
                let (record, remember) = match analyse(&image.display) {
                    Ok(analysis) => (Record::from(&analysis), true),
                    Err(Failure::Unusable(_)) => (Record::failed(), true),
                    Err(Failure::Unreadable(_)) => (Record::failed(), false),
                };
                if remember {
                    known.lock().unwrap().insert(image.display.to_string_lossy().into_owned(), (image.version.clone(), record.clone()));
                }
                if tx.send(Analysed { index, record }).is_err() {
                    return;
                }
            });
        }
        drop(tx);

        let mut batch = Vec::new();
        let mut unsaved = 0;
        let mut last = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(item) => {
                    batch.push(item);
                    unsaved += 1;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if batch.len() >= BATCH || (!batch.is_empty() && last.elapsed() >= BATCH_WAIT) {
                host.send(std::mem::take(&mut batch));
                last = Instant::now();
            }
            if unsaved >= SAVE_EVERY {
                save();
                unsaved = 0;
            }
        }
        if !batch.is_empty() {
            host.send(batch);
        }
        if unsaved > 0 {
            save();
        }
    });
    queue.exhausted() && host.wanted()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct Listening {
        sent: Mutex<Vec<Analysed>>,
        saved: Mutex<Option<HashMap<String, Entry>>>,
        stopped: AtomicBool,
    }

    impl Host for Listening {
        fn wanted(&self) -> bool {
            !self.stopped.load(Ordering::SeqCst)
        }
        fn hold_back(&self) -> bool {
            false
        }
        fn front(&self) -> Vec<usize> {
            Vec::new()
        }
        fn send(&self, items: Vec<Analysed>) -> bool {
            self.sent.lock().unwrap().extend(items);
            true
        }
        fn save(&self, known: &HashMap<String, Entry>) {
            *self.saved.lock().unwrap() = Some(known.clone());
        }
    }

    fn image(name: &str) -> Scanned {
        Scanned { path: PathBuf::from(name), display: PathBuf::from(name), version: "v1".into(), ..Default::default() }
    }

    fn empty() -> Analysis {
        Analysis { faces: Vec::new(), score: Some(42.0), parts: None, key: None }
    }

    /// Analyses by name: "junk" is unusable, "gone" unreadable, the rest fine.
    fn analyse(path: &Path) -> Result<Analysis, Failure> {
        match path.to_str().unwrap() {
            "junk" => Err(Failure::Unusable("no picture".into())),
            "gone" => Err(Failure::Unreadable("card pulled".into())),
            _ => Ok(empty()),
        }
    }

    #[test]
    fn every_photo_is_analysed_once_and_failures_are_remembered_only_when_the_file_is_to_blame() {
        let images = [image("a"), image("junk"), image("gone"), image("b")];
        let host = Listening::default();

        let finished = run(&images, vec![0, 1, 2, 3, 2], HashMap::new(), &host, analyse);

        assert!(finished);
        let mut sent = host.sent.lock().unwrap().clone();
        sent.sort_by_key(|a| a.index);
        assert_eq!(sent.iter().map(|a| a.index).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert!(sent[1].record.failed && sent[2].record.failed);
        assert_eq!(sent[0].record.score, Some(42.0));

        let saved = host.saved.lock().unwrap().clone().unwrap();
        assert!(saved.contains_key("junk"), "a file that cannot be analysed is not tried again");
        assert!(!saved.contains_key("gone"), "a disk that failed is tried again next time");
    }

    #[test]
    fn what_is_remembered_goes_out_without_analysis_and_what_has_gone_is_forgotten() {
        let images = [image("a")];
        let mut known = HashMap::new();
        known.insert("a".to_string(), ("v1".to_string(), Record { score: Some(7.0), ..Default::default() }));
        known.insert("deleted".to_string(), ("v1".to_string(), Record::default()));
        let host = Listening::default();

        run(&images, vec![0], known, &host, |_| panic!("nothing should need analysing"));

        assert_eq!(host.sent.lock().unwrap()[0].record.score, Some(7.0));
        // Nothing new was found, so nothing was saved; the pruned record goes
        // out with the next save.
        assert!(host.saved.lock().unwrap().is_none());
    }

    #[test]
    fn a_scan_nobody_wants_stops_and_says_it_did_not_finish() {
        let images = [image("a"), image("b")];
        let host = Listening::default();
        host.stopped.store(true, Ordering::SeqCst);
        assert!(!run(&images, vec![0, 1], HashMap::new(), &host, analyse));
        assert!(host.sent.lock().unwrap().is_empty());
    }
}
