//! Handing previews to the web view.
//!
//! The view asks for pictures by address, the same way it would ask a web
//! server, and this answers. An address names a file, a size, and the file's
//! version:
//!
//!     opencull://localhost/thumb?s=320&v=18f3a-2a5b01&p=/Volumes/Card/A7V00001.ARW
//!     opencull://localhost/fit?s=2400&v=…&p=…
//!     opencull://localhost/screen?v=…&p=…
//!     opencull://localhost/full?v=…&p=…
//!
//! The version is in the address so that the view may keep what it is given
//! for good: a file that changes gets a new address, and the old picture is
//! simply never asked for again.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use tauri::http::{header, Request, Response, StatusCode};
use tauri::UriSchemeResponder;

use crate::cache::Cache;
use crate::pool::{Pool, Priority};
use crate::preview::{self, Tier};

pub struct Served {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

type Task = Box<dyn FnOnce() + Send>;

/// Makes previews and remembers them.
#[derive(Clone)]
pub struct Previews {
    pub cache: Cache,
    pub pool: Pool<Served>,
    /// Answers the cheap half of every request — is it already on disk? — on
    /// threads of its own. The view's requests arrive on the app's main
    /// thread, and nothing there should wait on a disk, however briefly.
    readers: Arc<Mutex<mpsc::Sender<Task>>>,
    /// Where previews nobody has asked for yet are made.
    ///
    /// The JPEG decoder spreads one picture's work across every core it can
    /// find, by borrowing a pool of threads the whole program shares. Letting
    /// background preparation borrow from the same pool meant a photo jumped
    /// to mid-preparation had to queue for cores behind work nobody was
    /// waiting on. MEASURED: its preview took 411 ms instead of about 120.
    ///
    /// So preparation is run inside a small pool of its own. The decoder
    /// borrows from whichever pool it finds itself in, which confines it to
    /// these few threads and leaves every other core for the photo on screen.
    idle: Arc<rayon::ThreadPool>,
}

/// One parsed request.
pub struct Wanted {
    pub path: PathBuf,
    pub version: String,
    pub tier: Tier,
}

/// The sizes a thumbnail may be asked for at. Anything else is rounded up to
/// one of these, so a view resized by a pixel does not start a second cache.
pub const THUMB_SIZES: [u32; 3] = [320, 640, 1280];
pub const FIT_SIZES: [u32; 3] = [1600, 2400, 3400];

fn bucket(sizes: &[u32], wanted: u32) -> u32 {
    sizes.iter().copied().find(|s| *s >= wanted).unwrap_or(*sizes.last().expect("non-empty"))
}

impl Previews {
    pub fn new(cache: Cache) -> Self {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        // A third of the machine for getting ahead, and never more.
        let idle_threads = (cores / 3).clamp(1, 4);
        let pool = Pool::with_idle_limit((cores * 3 / 4).clamp(3, 12), idle_threads);
        let idle = rayon::ThreadPoolBuilder::new()
            .num_threads(idle_threads)
            .thread_name(|i| format!("opencull-idle-{i}"))
            .build()
            .expect("build idle pool");

        let (tx, rx) = mpsc::channel::<Task>();
        let rx = Arc::new(Mutex::new(rx));
        for i in 0..2 {
            let rx = rx.clone();
            std::thread::Builder::new()
                .name(format!("opencull-reader-{i}"))
                .spawn(move || loop {
                    let task = rx.lock().unwrap().recv();
                    match task {
                        Ok(task) => task(),
                        Err(_) => break,
                    }
                })
                .expect("spawn cache reader");
        }
        Self { cache, pool, readers: Arc::new(Mutex::new(tx)), idle: Arc::new(idle) }
    }

    fn key(&self, wanted: &Wanted) -> PathBuf {
        self.cache.path_for(&wanted.path, &wanted.version, &wanted.tier.label())
    }

    /// Gets a preview, from disk if it has been made before, and tells `done`.
    ///
    /// Returns at once. `done` is called from another thread.
    pub fn get(
        &self,
        wanted: Wanted,
        priority: Priority,
        done: impl FnOnce(Result<Arc<Served>, String>) + Send + 'static,
    ) {
        self.fetch(wanted, priority, false, done);
    }

    /// `get`, saying whether the asker stops to wait for the answer.
    fn fetch(
        &self,
        wanted: Wanted,
        priority: Priority,
        waited: bool,
        done: impl FnOnce(Result<Arc<Served>, String>) + Send + 'static,
    ) {
        let this = self.clone();
        let task: Task = Box::new(move || {
            // The camera's own screen-sized preview is lifted straight out
            // of the file, here and now. It exists to be on screen before
            // anything that has to be made, so it does not wait in the queue
            // behind the things that do — and at a few hundred kilobytes and
            // no decoding, it does not hold this thread long enough to
            // matter. A file with none has one made and kept, like any other
            // rendering, below.
            if wanted.tier == Tier::Screen {
                match preview::lift_screen(&wanted.path) {
                    Ok(Some(r)) => return done(Ok(Arc::new(Served { bytes: r.bytes, mime: r.mime }))),
                    Ok(None) => {}
                    Err(e) => return done(Err(e)),
                }
            }

            let cached = this.key(&wanted);
            // The full-size picture is not kept: it is the file's own bytes,
            // so a copy in the cache would double the disk it takes to read
            // them from somewhere no faster.
            if wanted.tier != Tier::Full {
                if let Some(bytes) = this.cache.get(&cached) {
                    done(Ok(Arc::new(Served { bytes, mime: "image/jpeg" })));
                    return;
                }
            }
            let cache = this.cache.clone();
            let key = cached.to_string_lossy().into_owned();
            let idle = (priority == Priority::Idle).then(|| this.idle.clone());
            let work = move || {
                let rendered = match idle {
                    Some(pool) => pool.install(|| preview::render(&wanted.path, wanted.tier))?,
                    None => preview::render(&wanted.path, wanted.tier)?,
                };
                if wanted.tier != Tier::Full {
                    cache.put(&cached, &rendered.bytes);
                }
                Ok(Served { bytes: rendered.bytes, mime: rendered.mime })
            };
            // Someone waiting keeps spare-time work from being dropped when
            // what to prepare changes, and puts it ahead of that preparation.
            if waited {
                this.pool.submit_waited(key, priority, work, done);
            } else {
                this.pool.submit(key, priority, work, done);
            }
        });
        // A send only fails if the readers are gone, which is at shutdown.
        self.readers.lock().unwrap().send(task).ok();
    }

    /// The same, waited for. For work already off the main thread.
    pub fn get_blocking(&self, wanted: Wanted, priority: Priority) -> Result<Arc<Served>, String> {
        let (tx, rx) = mpsc::channel();
        self.fetch(wanted, priority, true, move |r| {
            tx.send(r).ok();
        });
        rx.recv().map_err(|_| "preview worker stopped".to_string())?
    }

    /// Whether a preview is already on disk, without reading it.
    pub fn has(&self, wanted: &Wanted) -> bool {
        self.key(wanted).exists()
    }
}

fn parse(request: &Request<Vec<u8>>) -> Option<Wanted> {
    let uri = request.uri();
    let kind = uri.path().trim_matches('/');
    let mut path = None;
    let mut version = String::new();
    let mut size = 0u32;

    for pair in uri.query()?.split('&') {
        let (name, value) = pair.split_once('=')?;
        let value = percent_encoding::percent_decode_str(value).decode_utf8().ok()?;
        match name {
            "p" => path = Some(PathBuf::from(value.as_ref())),
            "v" => version = value.into_owned(),
            "s" => size = value.parse().ok()?,
            _ => {}
        }
    }

    let tier = match kind {
        "thumb" => Tier::Thumb(bucket(&THUMB_SIZES, size)),
        "fit" => Tier::Fit(bucket(&FIT_SIZES, size)),
        "screen" => Tier::Screen,
        "full" => Tier::Full,
        _ => return None,
    };
    Some(Wanted { path: path?, version, tier })
}

fn reply(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        // The address changes when the file does, so this never goes stale.
        .header(header::CACHE_CONTROL, "private, max-age=31536000, immutable")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(body)
        .expect("fixed headers")
}

/// Answers one request from the view.
pub fn handle(previews: &Previews, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let Some(wanted) = parse(&request) else {
        responder.respond(reply(StatusCode::BAD_REQUEST, "text/plain", Vec::new()));
        return;
    };
    // Thumbnails fill cells; everything else is for the photo being looked at.
    let priority = match wanted.tier {
        Tier::Thumb(_) => Priority::Soon,
        Tier::Fit(_) | Tier::Screen | Tier::Full => Priority::Now,
    };
    previews.get(wanted, priority, move |result| match result {
        Ok(served) => {
            let mime = served.mime;
            // The bytes are shared when more than one request was waiting on
            // the same preview. Usually there is one, and nothing is copied.
            let bytes = Arc::try_unwrap(served).map(|s| s.bytes).unwrap_or_else(|s| s.bytes.clone());
            responder.respond(reply(StatusCode::OK, mime, bytes));
        }
        Err(_) => responder.respond(reply(StatusCode::NOT_FOUND, "text/plain", Vec::new())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(uri: &str) -> Request<Vec<u8>> {
        Request::builder().uri(uri).body(Vec::new()).unwrap()
    }

    #[test]
    fn an_address_names_a_file_a_size_and_a_version() {
        let w = parse(&request("opencull://localhost/thumb?s=300&v=18f3a-2a5b&p=%2FVolumes%2FPhoto%20Drive%2FA7V00001.ARW"))
            .unwrap();
        assert_eq!(w.path, PathBuf::from("/Volumes/Photo Drive/A7V00001.ARW"));
        assert_eq!(w.version, "18f3a-2a5b");
        assert_eq!(w.tier, Tier::Thumb(320), "300 rounds up to the size that is kept");
    }

    #[test]
    fn the_windows_form_of_the_address_reads_the_same() {
        let w = parse(&request("http://opencull.localhost/fit?s=2000&v=1&p=C%3A%5CPhotos%5CA.ARW")).unwrap();
        assert_eq!(w.path, PathBuf::from("C:\\Photos\\A.ARW"));
        assert_eq!(w.tier, Tier::Fit(2400));
    }

    #[test]
    fn sizes_round_up_and_stop_at_the_largest() {
        assert_eq!(bucket(&THUMB_SIZES, 1), 320);
        assert_eq!(bucket(&THUMB_SIZES, 321), 640);
        assert_eq!(bucket(&THUMB_SIZES, 9999), 1280);
        assert_eq!(bucket(&FIT_SIZES, 2401), 3400);
    }

    #[test]
    fn an_address_that_names_nothing_is_refused() {
        assert!(parse(&request("opencull://localhost/thumb?s=320")).is_none(), "no file");
        assert!(parse(&request("opencull://localhost/poster?p=%2Fa.arw")).is_none(), "no such size");
        assert!(parse(&request("opencull://localhost/full")).is_none(), "no query at all");
    }

    #[test]
    fn full_and_screen_need_no_size() {
        let w = parse(&request("opencull://localhost/full?v=1&p=%2Fa.arw")).unwrap();
        assert_eq!(w.tier, Tier::Full);
        let w = parse(&request("opencull://localhost/screen?v=1&p=%2Fa.arw")).unwrap();
        assert_eq!(w.tier, Tier::Screen);
    }
}
