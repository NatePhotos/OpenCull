//! What the interface can ask the backend to do.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::pool::Priority;
use crate::preview::Tier;
use crate::raw::{self, Exif};
use crate::scan::{self, Scanned};
use crate::serve::{Previews, Wanted};
use crate::times::{self, Times};
use crate::xmp::{self, Marks};
use crate::ai;
use crate::burst;
use crate::editors;
use crate::ingest;
use crate::transfer;

/// One open folder, as the backend remembers it.
pub struct Session {
    folder: PathBuf,
    images: Vec<Scanned>,
    /// Every photo's capture time was already known when the folder opened,
    /// so there is nothing for the metadata pass to add to what is remembered.
    timed: bool,
    /// Set when the folder is closed, so work still running for it stops
    /// rather than finishing a scan nobody is looking at.
    closed: AtomicBool,
}

pub struct App {
    pub previews: Previews,
    pub writer: xmp::Writer,
    pub times: Times,
    sessions: Mutex<HashMap<u64, Arc<Session>>>,
    next_session: AtomicU64,
    /// Reads metadata. Its own pool rather than the shared one, because the
    /// JPEG decoder borrows the shared one and a folder's worth of file reads
    /// queued there would stall every preview behind them.
    metadata: rayon::ThreadPool,
    /// Set while photos are being copied off a card. Quitting is refused
    /// for as long as it is.
    pub ingesting: AtomicBool,
    /// Counts photos being moved, copied or shipped. Quitting is refused
    /// while any are, as it is during an ingest.
    pub busy: Busy,
    /// Tells the copy now running to stop after the photo in hand.
    ingest_cancel: Mutex<Arc<AtomicBool>>,
    /// Which reading of a card's capture times and ratings is the wanted
    /// one. An earlier one, for a folder since left, stops when it sees this
    /// has moved on.
    ingest_reading: AtomicU64,
    /// What face analysis has found, remembered per folder.
    pub faces: ai::store::Store,
    /// Which face scan is the wanted one. An earlier one stops when it sees
    /// this has moved on.
    ai_scan: AtomicU64,
    /// Whether analysis stands aside while the photographer is moving about.
    ai_yields: AtomicBool,
    /// The photos being looked at now, nearest first: what the scan
    /// analyses before carrying on with the rest of the folder.
    ai_front: Mutex<Vec<usize>>,
}

impl App {
    pub fn new(previews: Previews, times: Times, faces: ai::store::Store) -> Self {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        Self {
            previews,
            writer: xmp::Writer::default(),
            times,
            sessions: Mutex::new(HashMap::new()),
            next_session: AtomicU64::new(1),
            metadata: rayon::ThreadPoolBuilder::new()
                .num_threads(cores.clamp(2, 8))
                .thread_name(|i| format!("opencull-metadata-{i}"))
                .build()
                .expect("build metadata pool"),
            ingesting: AtomicBool::new(false),
            busy: Busy::default(),
            ingest_cancel: Default::default(),
            ingest_reading: AtomicU64::new(0),
            faces,
            ai_scan: AtomicU64::new(0),
            ai_yields: AtomicBool::new(true),
            ai_front: Default::default(),
        }
    }
}

type AppState<'a> = State<'a, Arc<App>>;

/// How many file operations are running that quitting would cut in half.
#[derive(Clone, Default)]
pub struct Busy(Arc<std::sync::atomic::AtomicUsize>);

impl Busy {
    /// Marks one as running until what is returned is dropped.
    pub fn hold(&self) -> BusyGuard {
        self.0.fetch_add(1, Ordering::SeqCst);
        BusyGuard(self.0.clone())
    }

    pub fn active(&self) -> bool {
        self.0.load(Ordering::SeqCst) > 0
    }
}

pub struct BusyGuard(Arc<std::sync::atomic::AtomicUsize>);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// How the app was started, read once from its command line.
///
///     opencull /path/to/folder           opens that folder straight away
///     opencull --selftest /path/to/folder
///
/// The second runs the whole app with no window on screen — opens the folder,
/// loads previews through the same route a person's clicks would, prints what
/// happened and how long it took, and exits. It exists because the parts most
/// likely to break on a new platform are the ones a compiler cannot see: the
/// preview protocol, the permissions, the stream of metadata.
#[derive(Clone, Default)]
pub struct Launch {
    pub open_path: Option<String>,
    pub self_test: bool,
}

impl Launch {
    pub fn from_args() -> Self {
        let mut launch = Launch::default();
        for arg in std::env::args().skip(1) {
            if arg == "--selftest" {
                launch.self_test = true;
            } else if !arg.starts_with('-') && Path::new(&arg).is_dir() {
                launch.open_path = Some(arg);
            }
        }
        launch
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    platform: &'static str,
    version: &'static str,
    cache_dir: String,
    open_path: Option<String>,
    self_test: bool,
}

#[tauri::command]
pub fn app_info(app: AppState, launch: State<Launch>) -> AppInfo {
    AppInfo {
        platform: std::env::consts::OS,
        version: env!("CARGO_PKG_VERSION"),
        cache_dir: app.previews.cache.dir().to_string_lossy().into_owned(),
        open_path: launch.open_path.clone(),
        self_test: launch.self_test,
    }
}

/// A line from the interface for the app's log: an error it caught, or a
/// step of the self-test. Printed, so it can be read from a terminal.
#[tauri::command]
pub fn report(message: String) {
    println!("[view] {message}");
}

/// Waits, on the backend's clock.
///
/// Only the self-test uses it. A window nobody is looking at has its timers
/// slowed by the system — MEASURED: a 120 ms pause took 317 ms — so a pause
/// taken in the interface says more about the window than about the app.
#[tauri::command]
pub async fn pause(ms: u64) {
    tauri::async_runtime::spawn_blocking(move || std::thread::sleep(std::time::Duration::from_millis(ms.min(5_000))))
        .await
        .ok();
}

/// Ends a self-test run with the given exit code.
///
/// Takes its previews with it. They are copies of someone's photographs, made
/// only to be timed, and have no business outliving the run in a temporary
/// folder nobody will think to look in.
#[tauri::command]
pub fn finish_self_test(handle: tauri::AppHandle, app: AppState, launch: State<Launch>, ok: bool) {
    if launch.self_test {
        // The previews and the remembered times sit side by side in one
        // folder made for this run; it all goes. Closed first, and anything
        // still queued abandoned, so that a preview finishing late cannot put
        // the folder back.
        app.previews.cache.close();
        app.previews.pool.clear_queued();
        std::thread::sleep(Duration::from_millis(250));
        if let Some(root) = app.previews.cache.dir().parent() {
            std::fs::remove_dir_all(root).ok();
        }
        handle.exit(if ok { 0 } else { 1 });
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderReply {
    session: u64,
    images: Vec<Scanned>,
    subfolder_count: usize,
    /// Photos not listed because of which kinds of file were asked for.
    left_out: usize,
    /// When each photo was taken, where that is known yet.
    times: Vec<Option<f64>>,
    /// Every photo's time is in `times`, so the folder can be shown in the
    /// order it was shot from the very first frame.
    timed: bool,
    /// Each photo's burst, when `timed`.
    groups: Option<Vec<Option<u32>>>,
}

/// How long opening a folder may spend learning capture times before it
/// gives up waiting and shows the folder anyway.
///
/// Worth waiting for, because without the times the folder can only be shown
/// by filename — and with two cameras that is one camera's whole day and then
/// the other's, with the loupe sitting on a frame that is first in neither.
/// MEASURED off a USB SSD, nothing cached: 2,147 photos in 162 ms.
///
/// Not worth waiting for indefinitely. A card reader is far slower than that,
/// and a photographer looking at "Opening folder…" for ten seconds has been
/// made to wait for a sort order. Past this, the folder opens by filename and
/// settles into shooting order when the times arrive, as it always used to.
const TIME_BUDGET: Duration = Duration::from_millis(1500);

/// Lists a folder, and learns when each photo was taken.
///
/// Ratings and the rest of the EXIF follow separately, so the first photo is
/// on screen before either — but the capture times are read here, by the
/// short route, because the order of the folder depends on them.
#[tauri::command]
pub async fn open_folder(app: AppState<'_>, path: String, include_subfolders: bool, kinds: Option<scan::Kinds>) -> Result<FolderReply, String> {
    let kinds = kinds.unwrap_or_default();
    let folder = PathBuf::from(&path);
    if !folder.is_dir() {
        return Err(format!("{path} is not a folder"));
    }
    let shared = app.inner().clone();
    let scanned_folder = folder.clone();
    let (result, read) = tauri::async_runtime::spawn_blocking(move || {
        let folder = scanned_folder;
        let result = scan::scan_folder_of(&folder, include_subfolders, kinds);
        let known = shared.times.load(&folder);
        let deadline = Instant::now() + TIME_BUDGET;

        // What is remembered is used as it is; what is not is read, for as
        // long as the budget lasts. `None` marks a photo not got to.
        let read: Vec<Option<Option<f64>>> = shared.metadata.install(|| {
            result
                .images
                .par_iter()
                .map(|image| {
                    times::remembered(&known, image)
                        .or_else(|| (Instant::now() < deadline).then(|| raw::capture_time(&image.path)))
                })
                .collect()
        });

        let learned = result.images.iter().zip(&read).any(|(image, r)| r.is_some() && times::remembered(&known, image).is_none());
        if learned {
            shared.times.save(&folder, &result.images, &read);
        }
        (result, read)
    })
    .await
    .map_err(|e| e.to_string())?;

    let timed = read.iter().all(Option::is_some);
    let times: Vec<Option<f64>> = read.into_iter().map(Option::flatten).collect();
    let groups = timed.then(|| burst::group(&times));

    let session = app.next_session.fetch_add(1, Ordering::SeqCst);
    app.sessions.lock().unwrap().insert(
        session,
        Arc::new(Session { folder, images: result.images.clone(), timed, closed: AtomicBool::new(false) }),
    );
    Ok(FolderReply { session, images: result.images, subfolder_count: result.subfolder_count, left_out: result.left_out, times, timed, groups })
}

#[tauri::command]
pub fn close_folder(app: AppState, session: u64) {
    if let Some(s) = app.sessions.lock().unwrap().remove(&session) {
        s.closed.store(true, Ordering::SeqCst);
    }
    // Thumbnails queued for a folder nobody is looking at would only hold up
    // the next one's.
    app.previews.pool.clear_queued();
}

/// What is known about one photo beyond its name.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    index: usize,
    marks: Marks,
    exif: Exif,
    /// The largest picture there is to show for this photo, the right way up.
    /// Known before any pixel is loaded, so the loupe can lay it out — and
    /// work out what "100%" means — without waiting.
    full_width: u32,
    full_height: u32,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MetaEvent {
    Batch { items: Vec<Meta> },
    /// Every photo has been read. `groups` gives each one's burst, by index.
    Done { groups: Vec<Option<u32>> },
}

fn read_meta(index: usize, image: &Scanned) -> Meta {
    let owner = raw::probe(&image.path).ok();
    let marks = xmp::read(&image.path, owner.as_ref());

    // For a RAW drawn through its JPEG, the picture's size and facing are the
    // JPEG's; everything else describes the RAW.
    let shown = if image.display != image.path { raw::probe(&image.display).ok() } else { owner.clone() };
    let (mut full_width, mut full_height) = shown
        .as_ref()
        .and_then(|p| p.largest().map(|e| (e.width, e.height)))
        .or_else(|| shown.as_ref().and_then(|p| p.exif.width.zip(p.exif.height)))
        .unwrap_or((0, 0));
    if shown.as_ref().is_some_and(|p| p.orientation >= 5) {
        std::mem::swap(&mut full_width, &mut full_height);
    }

    Meta { index, marks, exif: owner.map(|p| p.exif).unwrap_or_default(), full_width, full_height }
}

/// How many photos are read between reports. Small enough that the first
/// screenful of ratings appears almost at once; large enough that a folder of
/// ten thousand is not ten thousand messages.
const META_BATCH: usize = 64;

/// Reads every photo's marks and EXIF, reporting as it goes.
#[tauri::command]
pub fn load_metadata(app: AppState, session: u64, on_event: Channel<MetaEvent>) -> Result<(), String> {
    let Some(session) = app.sessions.lock().unwrap().get(&session).cloned() else {
        return Err("that folder is no longer open".into());
    };
    let app = app.inner().clone();

    std::thread::Builder::new()
        .name("opencull-metadata".into())
        .spawn(move || {
            let mut times: Vec<Option<f64>> = vec![None; session.images.len()];
            // In folder order, so the photos on screen first are read first.
            for (chunk_index, chunk) in session.images.chunks(META_BATCH).enumerate() {
                if session.closed.load(Ordering::SeqCst) {
                    return;
                }
                let start = chunk_index * META_BATCH;
                let items: Vec<Meta> = app.metadata.install(|| {
                    chunk.par_iter().enumerate().map(|(i, image)| read_meta(start + i, image)).collect()
                });
                for item in &items {
                    times[item.index] = item.exif.capture_ms;
                }
                if on_event.send(MetaEvent::Batch { items }).is_err() {
                    return; // the window went away
                }
            }
            if !session.closed.load(Ordering::SeqCst) {
                // The folder was too slow to time in full as it opened. It has
                // been now, so the next opening need not wait.
                if !session.timed {
                    let read: Vec<Option<Option<f64>>> = times.iter().map(|t| Some(*t)).collect();
                    app.times.save(&session.folder, &session.images, &read);
                }
                on_event.send(MetaEvent::Done { groups: burst::group(&times) }).ok();
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Deserialize)]
pub struct Ahead {
    path: String,
    version: String,
}

/// Makes loupe-sized previews for photos the photographer is about to reach.
///
/// `items` is nearest first. Nothing is returned: this only warms the cache,
/// so that arriving at the photo finds its preview already made.
#[tauri::command]
pub fn prefetch(app: AppState, items: Vec<Ahead>, size: u32) {
    let sizes = crate::serve::FIT_SIZES;
    let tier = Tier::Fit(sizes.iter().copied().find(|s| *s >= size).unwrap_or(sizes[sizes.len() - 1]));
    // Furthest first: the queue serves its newest request first, so the
    // nearest photo has to be the last one asked for.
    for item in items.into_iter().rev() {
        let path = PathBuf::from(item.path);
        if let Some(screen) = made_screen(&path, &item.version).filter(|w| !app.previews.has(w)) {
            app.previews.get(screen, Priority::Later, |_| {});
        }
        let wanted = Wanted { path, version: item.version, tier };
        if !app.previews.has(&wanted) {
            app.previews.get(wanted, Priority::Later, |_| {});
        }
    }
}

/// The screen-sized picture a photo needs MADE for it, if it is of a kind
/// that holds none of its own: a JPEG. (A RAW that turns out to hold none
/// has one made the first time it is asked for; telling which those are
/// means opening every file, which looking ahead cannot afford.)
fn made_screen(path: &std::path::Path, version: &str) -> Option<Wanted> {
    crate::raw::is_jpeg(path).then(|| Wanted { path: path.to_path_buf(), version: version.to_string(), tier: Tier::Screen })
}

/// Prepares loupe previews for a long run of photos, while nothing else is
/// going on.
///
/// `items` is nearest first. This replaces whatever was being prepared
/// before: the photographer has moved, and what is worth having ready has
/// moved with them. Returns how many previews there are to make — those
/// already on disk are skipped.
///
/// It is what makes a photo *jumped* to open as fast as one stepped to. The
/// handful either side are looked ahead to as a matter of course; this is the
/// rest of the folder, done on spare time.
#[tauri::command]
pub async fn warm(app: AppState<'_>, items: Vec<Ahead>, size: u32) -> Result<usize, String> {
    let previews = app.previews.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let sizes = crate::serve::FIT_SIZES;
        let tier = Tier::Fit(sizes.iter().copied().find(|s| *s >= size).unwrap_or(sizes[sizes.len() - 1]));
        previews.pool.clear_idle();
        let mut queued = 0;
        // Furthest first, for the same reason as looking ahead: the queue
        // serves its newest request first.
        for item in items.into_iter().rev() {
            let path = PathBuf::from(item.path);
            // The loupe rendering is asked for first and so made second: the
            // small picture is what a held arrow key shows, and the cheaper.
            let wanted = Wanted { path: path.clone(), version: item.version.clone(), tier };
            if !previews.has(&wanted) {
                previews.get(wanted, Priority::Idle, |_| {});
                queued += 1;
            }
            if let Some(screen) = made_screen(&path, &item.version).filter(|w| !previews.has(w)) {
                previews.get(screen, Priority::Idle, |_| {});
                queued += 1;
            }
        }
        queued
    })
    .await
    .map_err(|e| e.to_string())
}

/// How many of the previews being prepared are still to make.
#[tauri::command]
pub fn warm_remaining(app: AppState) -> usize {
    app.previews.pool.idle_remaining()
}

#[derive(Deserialize)]
pub struct MarkChange {
    path: String,
    /// The photo's JPEGs, which are to hold the marks inside themselves as
    /// well. None, for a photo with no JPEG or when that is turned off.
    #[serde(default)]
    into: Vec<String>,
    #[serde(flatten)]
    marks: Marks,
}

/// Records new marks for photos. Returns at once; the sidecars are written
/// behind it.
#[tauri::command]
pub fn set_marks(app: AppState, changes: Vec<MarkChange>) {
    for change in changes {
        app.writer.enqueue_with(PathBuf::from(change.path), change.marks, change.into.into_iter().map(PathBuf::from).collect());
    }
}

/// Waits for every pending sidecar to reach the disk.
#[tauri::command]
pub async fn flush_marks(app: AppState<'_>) -> Result<(), String> {
    let writer = app.writer.clone();
    tauri::async_runtime::spawn_blocking(move || writer.flush()).await.map_err(|e| e.to_string())
}

/// One photo as the app knows it: its file, and the JPEG shown for it — the
/// same file, or a RAW's JPEG.
#[derive(Deserialize)]
pub struct PhotoFiles {
    path: String,
    display: String,
}

/// A photo that could not be trashed, and why.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashFailure {
    path: String,
    reason: String,
}

/// Moves photos to the system's trash — each with its JPEG and its sidecar,
/// so nothing is left behind to look like an orphan later.
///
/// Returns the photos that could not be moved, each with why.
#[tauri::command]
pub async fn move_to_trash(app: AppState<'_>, items: Vec<PhotoFiles>) -> Result<Vec<TrashFailure>, String> {
    let writer = app.writer.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // A sidecar still queued would be written straight back after its
        // photo had gone.
        writer.flush();
        let mut failed = Vec::new();
        for item in items {
            let owner = PathBuf::from(&item.path);
            let mut files = vec![owner.clone()];
            if item.display != item.path {
                files.push(PathBuf::from(&item.display));
            }
            files.extend(xmp::sidecars_of(&owner));
            if let Err(e) = trash::delete_all(&files) {
                failed.push(TrashFailure { path: item.path, reason: e.to_string() });
            }
        }
        failed
    })
    .await
    .map_err(|e| e.to_string())
}

/// The files that make up one photo: itself, and the JPEG beside a RAW.
fn transfer_job(item: &PhotoFiles) -> transfer::Job {
    let companions = if item.display != item.path { vec![PathBuf::from(&item.display)] } else { Vec::new() };
    transfer::Job { owner: PathBuf::from(&item.path), companions }
}

fn strings(paths: Vec<PathBuf>) -> Vec<String> {
    paths.into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shipped {
    /// Where each photo is now, in the order given.
    placed: Vec<String>,
    /// The photos that are no longer where they started.
    moved: Vec<String>,
    skipped: usize,
    failed: Vec<transfer::Failure>,
    sidecar_failed: Vec<transfer::Failure>,
}

impl From<transfer::Outcome> for Shipped {
    fn from(o: transfer::Outcome) -> Self {
        Shipped { placed: strings(o.placed), moved: strings(o.moved), skipped: o.skipped.len(), failed: o.failed, sidecar_failed: o.sidecar_failed }
    }
}

/// Copies or moves photos to another folder — each with its JPEG and its
/// ratings. Nothing already there is ever replaced: a photo whose name is
/// taken goes alongside under a new one.
#[tauri::command]
pub async fn transfer_photos(app: AppState<'_>, items: Vec<PhotoFiles>, destination: String, moving: bool) -> Result<Shipped, String> {
    let writer = app.writer.clone();
    let _busy = app.busy.hold();
    tauri::async_runtime::spawn_blocking(move || {
        // A rating still waiting to be written would otherwise stay behind.
        writer.flush();
        let jobs: Vec<transfer::Job> = items.iter().map(transfer_job).collect();
        transfer::run(&jobs, Path::new(&destination), moving, transfer::Conflict::KeepBoth, |_| {}).into()
    })
    .await
    .map_err(|e| e.to_string())
}

/// How many of these photos would land on a name already taken in
/// `destination`: theirs, their JPEG's, or a sidecar's.
#[tauri::command]
pub async fn ship_conflicts(items: Vec<PhotoFiles>, destination: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let jobs: Vec<transfer::Job> = items.iter().map(transfer_job).collect();
        transfer::conflicts(&jobs, Path::new(&destination))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Ship's file step: writes out every pending rating, then — if a
/// destination is given — copies or moves the photos there, each with its
/// JPEG and its ratings. `progress` hears how many photos are done.
#[tauri::command]
pub async fn ship(
    app: AppState<'_>,
    items: Vec<PhotoFiles>,
    destination: Option<String>,
    moving: bool,
    skip_duplicates: bool,
    progress: Channel<usize>,
) -> Result<Shipped, String> {
    let writer = app.writer.clone();
    let _busy = app.busy.hold();
    tauri::async_runtime::spawn_blocking(move || {
        writer.flush();
        let Some(destination) = destination else {
            let placed = items.into_iter().map(|i| i.path).collect();
            return Shipped { placed, moved: Vec::new(), skipped: 0, failed: Vec::new(), sidecar_failed: Vec::new() };
        };
        let jobs: Vec<transfer::Job> = items.iter().map(transfer_job).collect();
        let conflict = if skip_duplicates { transfer::Conflict::Skip } else { transfer::Conflict::KeepBoth };
        transfer::run(&jobs, Path::new(&destination), moving, conflict, |done| {
            progress.send(done).ok();
        })
        .into()
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Analysed {
    index: usize,
    #[serde(flatten)]
    record: ai::store::Record,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AiEvent {
    Batch { items: Vec<Analysed> },
    /// The scan has ended. `finished` is false if it was stopped part way.
    Done { finished: bool },
}

/// How many photos are analysed between reports to the interface.
const AI_BATCH: usize = 12;
/// How many freshly analysed photos between saves of what has been found.
const AI_SAVE_EVERY: usize = 150;

/// Finds and judges the faces in a folder's photos, in the order given —
/// nearest the photo being looked at first — reporting as it goes.
///
/// Photos analysed before come straight from memory. The rest are worked
/// through on spare time: whenever the photographer is being served, the
/// scan waits. Asking again, or closing the folder, stops this one.
#[tauri::command]
pub async fn ai_scan(app: AppState<'_>, session: u64, order: Vec<usize>, events: Channel<AiEvent>) -> Result<(), String> {
    let Some(session) = app.sessions.lock().unwrap().get(&session).cloned() else {
        return Err("that folder is no longer open".into());
    };
    let app = app.inner().clone();
    let mine = app.ai_scan.fetch_add(1, Ordering::SeqCst) + 1;
    // The order given starts from the photo on screen; whatever an earlier
    // scan was told to look at first has nothing to say about this one.
    app.ai_front.lock().unwrap().clear();

    tauri::async_runtime::spawn_blocking(move || {
        let wanted = || !session.closed.load(Ordering::SeqCst) && app.ai_scan.load(Ordering::SeqCst) == mine;
        let known = Mutex::new(app.faces.load(&session.folder));

        // What is already known goes out at once.
        let mut todo = Vec::new();
        let mut ready = Vec::new();
        for index in order {
            let Some(image) = session.images.get(index) else { continue };
            match ai::store::remembered(&known.lock().unwrap(), &image.display, &image.version) {
                Some(record) => ready.push(Analysed { index, record }),
                None => todo.push(index),
            }
        }
        for chunk in ready.chunks(256) {
            if events.send(AiEvent::Batch { items: chunk.to_vec() }).is_err() {
                return;
            }
        }

        // A few at a time: enough to get through a folder in a minute or two,
        // few enough that the rest of the machine does not feel it.
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let workers = (cores / 2).clamp(1, 6).min(todo.len().max(1));
        let queue = ai::queue::Queue::new(todo);
        let (tx, rx) = std::sync::mpsc::channel::<Analysed>();
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let tx = tx.clone();
                scope.spawn(|| {
                    let tx = tx;
                    loop {
                        // Whoever is turning pages comes first.
                        while wanted() && app.ai_yields.load(Ordering::Relaxed) && app.previews.pool.busy() {
                            std::thread::sleep(Duration::from_millis(60));
                        }
                        if !wanted() {
                            return;
                        }
                        // What is being looked at now, then the rest.
                        let front = app.ai_front.lock().unwrap().clone();
                        let Some(index) = queue.take(&front) else { return };
                        let image = &session.images[index];
                        // A photo that cannot be analysed is remembered as
                        // having no faces, so it is not tried on every open.
                        let record = ai::analyze(&image.display).map(|a| ai::store::Record::from(&a)).unwrap_or_default();
                        known.lock().unwrap().insert(image.display.to_string_lossy().into_owned(), (image.version.clone(), record.clone()));
                        if tx.send(Analysed { index, record }).is_err() {
                            return;
                        }
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
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if batch.len() >= AI_BATCH || (!batch.is_empty() && last.elapsed() >= Duration::from_millis(400)) {
                    events.send(AiEvent::Batch { items: std::mem::take(&mut batch) }).ok();
                    last = Instant::now();
                }
                if unsaved >= AI_SAVE_EVERY {
                    app.faces.save(&session.folder, &known.lock().unwrap());
                    unsaved = 0;
                }
            }
            if !batch.is_empty() {
                events.send(AiEvent::Batch { items: batch }).ok();
            }
            if unsaved > 0 {
                app.faces.save(&session.folder, &known.lock().unwrap());
            }
        });
        events.send(AiEvent::Done { finished: queue.exhausted() && wanted() }).ok();
    })
    .await
    .map_err(|e| e.to_string())
}

/// A photo of a run to be compared, as the interface sends it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFrame {
    #[serde(flatten)]
    record: ai::store::Record,
    time: Option<f64>,
    burst: Option<u32>,
    #[serde(default)]
    upright: Option<bool>,
}

/// Marks the frames of each pose against one another: who blinked in which.
/// Each run is photos taken one soon after another, in the order they were
/// shot; it is split here into bursts and, if `poses`, poses shot again.
#[tauri::command]
pub fn ai_compare(runs: Vec<Vec<RunFrame>>, poses: bool) -> Vec<Vec<ai::series::Compared>> {
    runs.iter()
        .map(|run| ai::series::compare_run(&run.iter().map(|f| ai::series::Frame { record: &f.record, time: f.time, burst: f.burst, upright: f.upright }).collect::<Vec<_>>(), poses))
        .collect()
}

/// Tells the scan now running which photos are being looked at, nearest
/// first, so that it analyses those before the rest of the folder.
#[tauri::command]
pub fn ai_look_at(app: AppState, order: Vec<usize>) {
    *app.ai_front.lock().unwrap() = order;
}

/// Stops the face scan now running.
#[tauri::command]
pub fn ai_stop(app: AppState) {
    app.ai_scan.fetch_add(1, Ordering::SeqCst);
}

/// Whether analysis stands aside while the photographer is moving between photos.
#[tauri::command]
pub fn ai_set_yield(app: AppState, on: bool) {
    app.ai_yields.store(on, Ordering::Relaxed);
}

/// How much is remembered of past analysis: bytes, and folders.
#[tauri::command]
pub async fn ai_cache_usage(app: AppState<'_>) -> Result<(u64, usize), String> {
    let store = app.faces.clone();
    tauri::async_runtime::spawn_blocking(move || store.usage()).await.map_err(|e| e.to_string())
}

/// Forgets all past analysis. Folders are analysed again when next opened.
#[tauri::command]
pub async fn ai_clear_cache(app: AppState<'_>) -> Result<(), String> {
    app.ai_scan.fetch_add(1, Ordering::SeqCst);
    let store = app.faces.clone();
    tauri::async_runtime::spawn_blocking(move || store.clear()).await.map_err(|e| e.to_string())
}

/// The cards and outside drives attached.
#[tauri::command]
pub async fn ingest_volumes() -> Result<Vec<ingest::Volume>, String> {
    tauri::async_runtime::spawn_blocking(ingest::volumes).await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Browsed {
    /// The folders inside that have photos somewhere in them.
    folders: Vec<ingest::Folder>,
    /// Photos sitting directly in the folder itself.
    image_count: usize,
}

/// What is inside a folder on a card or drive, for the source browser.
#[tauri::command]
pub async fn ingest_browse(path: String) -> Result<Browsed, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&path);
        Browsed { folders: ingest::photo_folders(path), image_count: ingest::shallow_count(path) }
    })
    .await
    .map_err(|e| e.to_string())
}

/// The photos a folder on a card has to offer.
#[tauri::command]
pub async fn ingest_scan(path: String) -> Result<Vec<Scanned>, String> {
    tauri::async_runtime::spawn_blocking(move || ingest::scan(Path::new(&path))).await.map_err(|e| e.to_string())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IngestDetail {
    index: usize,
    capture_ms: Option<f64>,
    rating: u8,
}

/// Reads when each photo was taken and the stars given in the camera,
/// reporting in batches. Asking again, for another folder, stops this one.
#[tauri::command]
pub async fn ingest_details(app: AppState<'_>, paths: Vec<String>, found: Channel<Vec<IngestDetail>>) -> Result<(), String> {
    let app = app.inner().clone();
    let mine = app.ingest_reading.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        for (batch, chunk) in paths.chunks(META_BATCH).enumerate() {
            if app.ingest_reading.load(Ordering::SeqCst) != mine {
                return;
            }
            let read: Vec<IngestDetail> = app.metadata.install(|| {
                chunk
                    .par_iter()
                    .enumerate()
                    .map(|(i, path)| {
                        let (capture_ms, rating) = ingest::details(Path::new(path));
                        IngestDetail { index: batch * META_BATCH + i, capture_ms, rating }
                    })
                    .collect()
            });
            if found.send(read).is_err() {
                return;
            }
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// How many of these photos want a name that is already taken — at the
/// destination, or by another of them.
#[tauri::command]
pub async fn ingest_conflicts(items: Vec<ingest::Item>, destination: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || ingest::conflict_count(&items, Path::new(&destination))).await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ingested {
    status: IngestStatus,
    message: Option<String>,
    /// Photos that arrived, but whose ratings did not come with them.
    warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IngestStatus {
    Success,
    Cancelled,
    Failed,
}

/// Clears the mark that a copy is running, however the copy ends.
struct Ingesting(Arc<App>);

impl Drop for Ingesting {
    fn drop(&mut self) {
        self.0.ingesting.store(false, Ordering::SeqCst);
    }
}

/// Copies photos off a card. `progress` hears how far along it is.
#[tauri::command]
pub async fn ingest_copy(
    app: AppState<'_>,
    items: Vec<ingest::Item>,
    destination: String,
    policy: ingest::Policy,
    progress: Channel<ingest::Progress>,
) -> Result<Ingested, String> {
    let app = app.inner().clone();
    if app.ingesting.swap(true, Ordering::SeqCst) {
        return Err("a copy is already running".into());
    }
    let running = Ingesting(app.clone());
    let cancel = Arc::new(AtomicBool::new(false));
    *app.ingest_cancel.lock().unwrap() = cancel.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let _running = running;
        let destination = PathBuf::from(destination);
        if let Err(e) = std::fs::create_dir_all(&destination) {
            return Ingested { status: IngestStatus::Failed, message: Some(format!("Could not use {}: {e}", destination.display())), warnings: Vec::new() };
        }
        let workers = ingest::parallel_copies(&destination);
        let report = |p: ingest::Progress| {
            // Nothing more is said once the copy has been called off, so a
            // late report cannot put the bar back after it was taken down.
            if !cancel.load(Ordering::SeqCst) {
                progress.send(p).ok();
            }
        };
        let copied = ingest::copy(&items, &destination, policy, workers, &cancel, &report);
        let warnings = copied.warnings;
        match copied.outcome {
            ingest::Outcome::Success => Ingested { status: IngestStatus::Success, message: None, warnings },
            ingest::Outcome::Cancelled => Ingested { status: IngestStatus::Cancelled, message: None, warnings },
            ingest::Outcome::Failed(why) => Ingested { status: IngestStatus::Failed, message: Some(why), warnings },
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Stops the copy off a card after the photo in hand.
#[tauri::command]
pub fn ingest_cancel(app: AppState) {
    app.ingest_cancel.lock().unwrap().store(true, Ordering::SeqCst);
}

/// The editing programs Ship can open photos in, and which are installed.
#[tauri::command]
pub async fn editing_apps() -> Result<Vec<editors::Listed>, String> {
    tauri::async_runtime::spawn_blocking(editors::list).await.map_err(|e| e.to_string())
}

/// Lightroom Classic catalogues in the usual places, most recently used first.
#[tauri::command]
pub async fn lightroom_catalogs(handle: tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri::Manager;
    let roots: Vec<PathBuf> = [handle.path().picture_dir(), handle.path().document_dir()].into_iter().flatten().collect();
    tauri::async_runtime::spawn_blocking(move || strings(editors::catalogs(&roots))).await.map_err(|e| e.to_string())
}

/// Opens photos in an editing program, or shows them in the file browser.
#[tauri::command]
pub async fn open_in_editor(editor: String, files: Vec<String>, catalog: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
        if files.is_empty() {
            return Ok(());
        }
        if editor == editors::FILE_BROWSER {
            return tauri_plugin_opener::reveal_items_in_dir(&files).map_err(|e| e.to_string());
        }
        editors::open(&editor, &files, catalog.as_deref().map(Path::new))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Whether each path is still there, file or folder.
#[tauri::command]
pub fn paths_exist(paths: Vec<String>) -> Vec<bool> {
    paths.iter().map(|p| Path::new(p).exists()).collect()
}

#[derive(Serialize)]
pub struct Histogram {
    r: Vec<u32>,
    g: Vec<u32>,
    b: Vec<u32>,
    luma: Vec<u32>,
}

/// How a photo's tones are distributed, counted from its thumbnail. A few
/// hundred pixels across is plenty for a distribution, and is already made.
#[tauri::command]
pub async fn histogram(app: AppState<'_>, path: String, version: String) -> Result<Histogram, String> {
    let previews = app.previews.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let wanted = Wanted { path: PathBuf::from(path), version, tier: Tier::Thumb(320) };
        let served = previews.get_blocking(wanted, Priority::Now)?;
        let mut decoder = jpeg_decoder::Decoder::new(&served.bytes[..]);
        let pixels = decoder.decode().map_err(|e| e.to_string())?;

        let mut h = Histogram { r: vec![0; 256], g: vec![0; 256], b: vec![0; 256], luma: vec![0; 256] };
        for px in pixels.chunks_exact(3) {
            h.r[px[0] as usize] += 1;
            h.g[px[1] as usize] += 1;
            h.b[px[2] as usize] += 1;
            let luma = 0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32;
            h.luma[(luma.round() as usize).min(255)] += 1;
        }
        Ok(h)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A few photos to stand for a folder on the home screen.
#[tauri::command]
pub async fn folder_cover(path: String) -> Result<Vec<Scanned>, String> {
    tauri::async_runtime::spawn_blocking(move || scan::cover_images(Path::new(&path), 4))
        .await
        .map_err(|e| e.to_string())
}

/// Which of these folders are still where they were.
#[tauri::command]
pub fn folders_exist(paths: Vec<String>) -> Vec<bool> {
    paths.iter().map(|p| Path::new(p).is_dir()).collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheUsage {
    bytes: u64,
    count: usize,
    limit: u64,
}

#[tauri::command]
pub async fn cache_usage(app: AppState<'_>) -> Result<CacheUsage, String> {
    let cache = app.previews.cache.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (bytes, count) = cache.usage();
        CacheUsage { bytes, count, limit: cache.limit() }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Sets how large the preview cache may grow, and brings it under that now.
#[tauri::command]
pub async fn set_cache_limit(app: AppState<'_>, limit: u64) -> Result<(), String> {
    let cache = app.previews.cache.clone();
    cache.set_limit(limit);
    tauri::async_runtime::spawn_blocking(move || {
        cache.trim(cache.limit());
    })
    .await
    .map_err(|e| e.to_string())
}

/// Brings the preview cache under `limit` bytes; zero empties it.
#[tauri::command]
pub async fn trim_cache(app: AppState<'_>, limit: u64) -> Result<u64, String> {
    let cache = app.previews.cache.clone();
    tauri::async_runtime::spawn_blocking(move || cache.trim(limit)).await.map_err(|e| e.to_string())
}
