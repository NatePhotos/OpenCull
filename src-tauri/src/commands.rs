//! What the interface can ask the backend to do.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{Manager, State};

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
use crate::framing;
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
    /// Tells the copy now running to stop after the photo in hand.
    ingest_cancel: Mutex<Arc<AtomicBool>>,
    /// Which reading of a card's capture times and ratings is the wanted
    /// one. An earlier one, for a folder since left, stops when it sees this
    /// has moved on.
    ingest_reading: AtomicU64,
    /// What face analysis has found, remembered per folder.
    pub faces: ai::store::Store,
    /// Auto-cull's network, lessons and the colours it set.
    pub cull: Arc<crate::cull::Store>,
    /// Which face scan is the wanted one. An earlier one stops when it sees
    /// this has moved on.
    ai_scan: AtomicU64,
    /// The current visual regrouping request. A newer user choice invalidates
    /// older embedding work for the same session.
    ai_grouping_epoch: Mutex<(u64, u64)>,
    /// Whether analysis stands aside while the photographer is moving about.
    ai_yields: AtomicBool,
    /// The photos being looked at now, nearest first: what the scan
    /// analyses before carrying on with the rest of the folder.
    ai_front: Mutex<Vec<usize>>,
}

impl App {
    pub fn new(previews: Previews, times: Times, faces: ai::store::Store, cull: crate::cull::Store) -> Self {
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
            ingest_cancel: Default::default(),
            ingest_reading: AtomicU64::new(0),
            faces,
            cull: Arc::new(cull),
            ai_scan: AtomicU64::new(0),
            ai_grouping_epoch: Mutex::new((0, 0)),
            ai_yields: AtomicBool::new(true),
            ai_front: Default::default(),
        }
    }
}

type AppState<'a> = State<'a, Arc<App>>;

/// Advances grouping work only to a fresh revision. Multiple completed photo
/// runs may share a revision; a late request from an older setting may not.
fn accept_grouping_epoch(current: &mut (u64, u64), requested: (u64, u64)) -> bool {
    if current.0 == requested.0 && current.1 > requested.1 { return false; }
    *current = requested;
    true
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
pub async fn open_folder(app: AppState<'_>, path: String, include_subfolders: bool, kinds: Option<scan::Kinds>, ignore_videos: Option<bool>, members: Option<Vec<Scanned>>) -> Result<FolderReply, String> {
    let kinds = kinds.unwrap_or_default();
    let folder = PathBuf::from(&path);
    if !folder.is_dir() {
        return Err(format!("{path} is not a folder"));
    }
    let shared = app.inner().clone();
    let scanned_folder = folder.clone();
    let (result, read) = tauri::async_runtime::spawn_blocking(move || {
        let folder = scanned_folder;
        let result = match members {
            Some(images) => {
                for image in &images {
                    if !image.path.is_file() || !image.display.is_file() { return Err(format!("Reconnect the source containing {}", image.name)); }
                    let meta = image.path.metadata().map_err(|e| e.to_string())?;
                    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
                    if meta.len() != image.size || modified != image.modified { return Err(format!("{} changed since this session was saved. Start a new card review to avoid applying marks to a different file.", image.name)); }
                }
                scan::ScanResult { images, subfolder_count: 0, left_out: 0 }
            }
            None => scan::scan_folder_media(&folder, include_subfolders, kinds, !ignore_videos.unwrap_or(true)),
        };
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
                        .or_else(|| (Instant::now() < deadline).then(|| if scan::is_video(&raw::extension(&image.path)) { None } else { raw::capture_time(&image.path) }))
                })
                .collect()
        });

        let learned = result.images.iter().zip(&read).any(|(image, r)| r.is_some() && times::remembered(&known, image).is_none());
        if learned {
            shared.times.save(&folder, &result.images, &read);
        }
        Ok((result, read))
    })
    .await
    .map_err(|e| e.to_string())??;

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
    /// How the photo is turned and cropped, in the view.
    framing: framing::Framing,
    /// Whether it can be turned and cropped here.
    framable: bool,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MetaEvent {
    Batch { items: Vec<Meta> },
    /// Every photo has been read. `groups` gives each one's burst, by index.
    Done { groups: Vec<Option<u32>> },
}

fn read_meta(index: usize, image: &Scanned) -> Meta {
    if scan::is_video(&raw::extension(&image.path)) {
        return Meta { index, marks: xmp::read(&image.path, None), exif: Exif::default(), full_width: 0, full_height: 0, framing: framing::Framing::default(), framable: false };
    }
    let owner = raw::probe(&image.path).ok();
    let sidecar = xmp::sidecar_text(&image.path);
    let marks = xmp::read_from(sidecar.as_deref(), owner.as_ref());

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

    let seen = framing::seen(
        sidecar.as_deref(),
        owner.as_ref().and_then(|p| p.xmp.as_deref()),
        &framing::Shown {
            file: owner.as_ref().map_or(1, |p| p.orientation),
            drawn: shown.as_ref().map_or(1, |p| p.orientation),
            upright: (full_width, full_height),
            stated: owner.as_ref().and_then(|p| p.exif.width.zip(p.exif.height)),
        },
    );

    Meta { index, marks, exif: owner.map(|p| p.exif).unwrap_or_default(), full_width, full_height, framing: seen.framing, framable: seen.framable }
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

/// A photo's framing, as the interface has changed it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FramingChange {
    path: String,
    #[serde(default)]
    into: Vec<String>,
    framing: framing::Framing,
    /// The size of the upright picture the framing was made on.
    upright: (u32, u32),
    /// The photo's marks as they stand, for a sidecar that has to be made.
    seed: Marks,
}

/// Records how photos are cropped and turned. Returns at once; the sidecars
/// are written behind it, in the fields Adobe's programs read.
#[tauri::command]
pub fn set_framing(app: AppState, changes: Vec<FramingChange>) {
    for change in changes {
        let wanted = framing::Wanted { framing: change.framing, upright: change.upright, seed: change.seed };
        app.writer.enqueue_framing(PathBuf::from(change.path), wanted, change.into.into_iter().map(PathBuf::from).collect());
    }
}

/// Waits for every pending sidecar to reach the disk.
#[tauri::command]
pub async fn flush_marks(app: AppState<'_>) -> Result<(), String> {
    let writer = app.writer.clone();
    tauri::async_runtime::spawn_blocking(move || writer.flush()).await.map_err(|e| e.to_string())
}

#[derive(Deserialize)]
pub struct TrashItem {
    path: String,
    display: String,
}

/// Moves photos to the system's trash — each with its JPEG and its sidecar,
/// so nothing is left behind to look like an orphan later.
///
/// Returns the photos that could not be moved.
#[tauri::command]
pub async fn move_to_trash(app: AppState<'_>, items: Vec<TrashItem>) -> Result<Vec<String>, String> {
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
            let mut double = owner.as_os_str().to_owned();
            double.push(".xmp");
            for sidecar in [owner.with_extension("xmp"), PathBuf::from(double)] {
                if sidecar.exists() {
                    files.push(sidecar);
                }
            }
            if trash::delete_all(&files).is_err() {
                failed.push(item.path);
            }
        }
        failed
    })
    .await
    .map_err(|e| e.to_string())
}

/// The files that make up one photo: itself, and the JPEG beside a RAW.
fn transfer_job(item: &TrashItem, with_companion: bool) -> transfer::Job {
    let companions = if with_companion && item.display != item.path { vec![PathBuf::from(&item.display)] } else { Vec::new() };
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
    failed: Vec<String>,
    sidecar_failed: Vec<String>,
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
pub async fn transfer_photos(app: AppState<'_>, items: Vec<TrashItem>, destination: String, moving: bool) -> Result<Shipped, String> {
    let writer = app.writer.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // A rating still waiting to be written would otherwise stay behind.
        writer.flush();
        let jobs: Vec<transfer::Job> = items.iter().map(|i| transfer_job(i, true)).collect();
        transfer::run(&jobs, Path::new(&destination), moving, transfer::Conflict::KeepBoth, |_| {}).into()
    })
    .await
    .map_err(|e| e.to_string())
}

/// How many of these photos would land on a name already taken in
/// `destination`: theirs, or their JPEG's.
#[tauri::command]
pub async fn ship_conflicts(items: Vec<TrashItem>, destination: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let jobs: Vec<transfer::Job> = items.iter().map(|i| transfer_job(i, true)).collect();
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
    items: Vec<TrashItem>,
    destination: Option<String>,
    moving: bool,
    skip_duplicates: bool,
    progress: Channel<usize>,
) -> Result<Shipped, String> {
    let writer = app.writer.clone();
    tauri::async_runtime::spawn_blocking(move || {
        writer.flush();
        let Some(destination) = destination else {
            let placed = items.into_iter().map(|i| i.path).collect();
            return Shipped { placed, moved: Vec::new(), skipped: 0, failed: Vec::new(), sidecar_failed: Vec::new() };
        };
        let jobs: Vec<transfer::Job> = items.iter().map(|i| transfer_job(i, true)).collect();
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
pub async fn ai_scan(app: AppState<'_>, session: u64, order: Vec<usize>, advanced: bool, scene: bool, events: Channel<AiEvent>) -> Result<(), String> {
    let rules = if advanced { ai::Rules::Advanced } else { ai::Rules::Basic };
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
        let known = Mutex::new(app.faces.load(&session.folder, rules));

        // What is already known goes out at once.
        let mut todo = Vec::new();
        let mut ready = Vec::new();
        for index in order {
            let Some(image) = session.images.get(index) else { continue };
            if scan::is_video(&raw::extension(&image.path)) { continue; }
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
                        // With Advanced, the picture decoded for the scan also
                        // gives the photo's brightness, and its scene print
                        // when MobileCLIP grouping is on, from one small copy
                        // of it, so its thumbnail need not be made or read
                        // for either.
                        let light = |picture: &crate::preview::Upright| {
                            if rules == ai::Rules::Advanced {
                                let (small, w, h) = crate::cull::light::shrink_to_thumb(&picture.rgb, picture.width, picture.height);
                                app.cull.keep_light(&image.display.to_string_lossy(), &image.version, crate::cull::light::light_of_sized(&small, w as usize, h as usize));
                                if scene {
                                    if let Ok(print) = ai::scene::print(&small, w, h) {
                                        app.faces.scenes.put(&session.folder, &image.display, &image.version, print);
                                    }
                                }
                            }
                        };
                        let analysis = ai::analyze_then(&image.display, rules, light);
                        let record = analysis.as_ref().map(ai::store::Record::from).unwrap_or_default();
                        // The subjects' face prints stay here, for ranking
                        // a burst; the interface is never sent them.
                        if let Some(print) = analysis.ok().and_then(|a| a.prints) {
                            app.faces.prints.put(&session.folder, &image.display, &image.version, print);
                        }
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
                    app.faces.save(&session.folder, rules, &known.lock().unwrap());
                    app.faces.prints.save(&session.folder);
                    app.faces.scenes.save(&session.folder);
                    unsaved = 0;
                }
            }
            if !batch.is_empty() {
                events.send(AiEvent::Batch { items: batch }).ok();
            }
            if unsaved > 0 {
                app.faces.save(&session.folder, rules, &known.lock().unwrap());
                app.faces.prints.save(&session.folder);
                app.faces.scenes.save(&session.folder);
            }
        });
        events.send(AiEvent::Done { finished: queue.exhausted() && wanted() }).ok();
    })
    .await
    .map_err(|e| e.to_string())
}

/// Which grouping a run gets: MobileCLIP's, the scene network's with
/// MobileCLIP asked about the rest, or faces'.
#[derive(Debug, PartialEq)]
enum Grouper { Faces, MobileClip, Cascade }

/// Whether the scene network can settle anything in a run: with no prints it
/// asks MobileCLIP about every pair, which the batched path does faster.
fn cascade_worth_it(scene: &[Option<ai::scene::ScenePrint>]) -> bool {
    scene.iter().any(Option::is_some)
}

/// A photo's scene print made from the small picture grouping reads anyway
/// (a saved preview, else the file's own small preview), for photos analysed
/// before visual grouping was on. In a test of 29,956 pairs it grouped as the
/// scan's own prints do, to within 0.2%.
fn preview_scene_print(path: &Path, version: &str, cache: &crate::cache::Cache) -> Option<ai::scene::ScenePrint> {
    let picture = crate::preview::grouping_pixels(path, version, cache).ok()?;
    print_of_picture(&picture)
}

/// The scene print of a decoded picture, from the same small copy the scan uses.
fn print_of_picture(picture: &crate::preview::Upright) -> Option<ai::scene::ScenePrint> {
    let (small, w, h) = crate::cull::light::shrink_to_thumb(&picture.rgb, picture.width, picture.height);
    ai::scene::print(&small, w, h).ok()
}

/// The frames of a run that will be compared with another: those with a
/// neighbour (before or after) taken within POSE_GAP_MS. A frame without a
/// time is counted, as making its print costs a few milliseconds.
fn compared_frames(times: &[Option<f64>]) -> Vec<bool> {
    let near = |a: Option<f64>, b: Option<f64>| matches!((a, b), (Some(a), Some(b)) if (b - a).abs() <= ai::series::POSE_GAP_MS);
    (0..times.len()).map(|i| {
        let Some(t) = times[i] else { return true };
        (i > 0 && near(Some(t), times[i - 1])) || (i + 1 < times.len() && near(Some(t), times[i + 1]))
    }).collect()
}

fn grouper(advanced: bool, mobile_clip: bool) -> Grouper {
    match (advanced, mobile_clip) {
        (true, true) => Grouper::Cascade,
        (false, true) => Grouper::MobileClip,
        _ => Grouper::Faces,
    }
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
    #[serde(default)]
    index: Option<usize>,
}

/// Marks the frames of each pose against one another: who blinked in which.
/// Each run is photos taken one soon after another, in the order they were
/// shot; it is split here into bursts and, if `poses`, poses shot again.
#[tauri::command]
pub async fn ai_compare(
    app: AppState<'_>,
    handle: tauri::AppHandle,
    runs: Vec<Vec<RunFrame>>,
    poses: bool,
    session: Option<u64>,
    revision: Option<u64>,
    mobile_clip: Option<bool>,
    mobile_clip_grouping: Option<ai::mobileclip::Grouping>,
    advanced: Option<bool>,
) -> Result<Vec<Vec<ai::series::Compared>>, String> {
    let app = app.inner().clone();
    let source = if let Some(id) = session { Some(app.sessions.lock().unwrap().get(&id).cloned().ok_or("that folder is no longer open")?) } else { None };
    let epoch = (session.unwrap_or(0), revision.unwrap_or(0));
    if session.is_some() {
        let mut current = app.ai_grouping_epoch.lock().unwrap();
        if !accept_grouping_epoch(&mut current, epoch) { return Err("Grouping cancelled.".into()); }
    }
    if runs.is_empty() { return Ok(Vec::new()); }
    let root = mobileclip_root(&handle)?;
    let advanced = advanced.unwrap_or(false);

    tauri::async_runtime::spawn_blocking(move || {
        let wanted = || source.as_ref().is_none_or(|s| !s.closed.load(Ordering::SeqCst))
            && (session.is_none() || *app.ai_grouping_epoch.lock().unwrap() == epoch);
        fn frames_of(run: &[RunFrame]) -> Vec<ai::series::Frame<'_>> {
            run.iter().map(|f| ai::series::Frame { record: &f.record, time: f.time, burst: f.burst, upright: f.upright }).collect()
        }
        let compare = |run: &[RunFrame], visual: Option<&[Vec<f32>]>| {
            ai::series::compare_run_with_visual(&frames_of(run), poses, visual, mobile_clip_grouping.unwrap_or_default())
        };
        let choice = grouper(advanced, mobile_clip.unwrap_or(false));
        match choice {
            Grouper::Faces => return Ok(runs.iter().map(|run| compare(run, None)).collect()),
            Grouper::MobileClip | Grouper::Cascade => {}
        }
        let source = source.as_ref().ok_or("Open a folder before using MobileCLIP.")?;
        ai::mobileclip::with_model(&root, |model| {
            let start = Instant::now();
            let cached_before = model.cached;
            let computed_before = model.computed;
            let results = runs.iter().map(|run| {
                while wanted() && app.ai_yields.load(Ordering::Relaxed) && app.previews.pool.busy() {
                    std::thread::sleep(Duration::from_millis(60));
                }
                if !wanted() { return Err("Grouping cancelled.".to_string()); }
                let paths = run.iter().map(|frame| {
                    let photo = frame.index.and_then(|i| source.images.get(i)).ok_or("Invalid grouping photo index.")?;
                    Ok((photo.display.as_path(), photo.version.as_str()))
                }).collect::<Result<Vec<_>, String>>()?;
                if choice == Grouper::Cascade {
                    // The scene network's prints, per frame, as far as the scan has made them.
                    let mut scene: Vec<Option<ai::scene::ScenePrint>> = run.iter().map(|f| {
                        let photo = f.index.and_then(|i| source.images.get(i))?;
                        app.faces.scenes.get(&source.folder, &photo.display, &photo.version)
                    }).collect();
                    // Photos analysed before visual grouping was on have none: made now, once, from the
                    // saved preview, and only for frames that will be compared with a neighbour.
                    let times: Vec<Option<f64>> = run.iter().map(|f| f.time).collect();
                    let mut made = false;
                    for (i, needed) in compared_frames(&times).into_iter().enumerate() {
                        if !needed || scene[i].is_some() { continue; }
                        if !wanted() { return Err("Grouping cancelled.".to_string()); }
                        let Some(photo) = run[i].index.and_then(|j| source.images.get(j)) else { continue };
                        if let Some(print) = preview_scene_print(&photo.display, &photo.version, &app.previews.cache) {
                            app.faces.scenes.put(&source.folder, &photo.display, &photo.version, print.clone());
                            scene[i] = Some(print);
                            made = true;
                        }
                    }
                    if made { app.faces.scenes.save(&source.folder); }
                    if cascade_worth_it(&scene) {
                        // The scene network settles most pairs; MobileCLIP is asked only about the rest, one photo at a time.
                        let frames = frames_of(run);
                        let mut cancelled = false;
                        let marked = ai::series::compare_run_cascade(&frames, poses, &scene, |i| {
                            if !wanted() { cancelled = true; return None; }
                            model.embeddings(&paths[i..=i], &app.previews.cache, wanted).ok().and_then(|mut v| v.pop())
                        }, mobile_clip_grouping.unwrap_or_default());
                        if cancelled || !wanted() { return Err("Grouping cancelled.".to_string()); }
                        return Ok(marked);
                    }
                }
                let embeddings = model.embeddings(&paths, &app.previews.cache, wanted)?;
                Ok(compare(run, Some(&embeddings)))
            }).collect::<Result<Vec<_>, String>>();
            eprintln!("[mobileclip] {} cached, {} new embeddings in {:?}", model.cached - cached_before, model.computed - computed_before, start.elapsed());
            results
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Brightness of a photo, read once and kept: worked out by the face scan
/// from the picture it decoded, or else from the photo's small thumbnail.
/// None if there is no thumbnail to read; a photo found to have none is not
/// tried again this run.
///
/// The thumbnail is asked for as spare-time work (`Priority::Idle`), the one
/// level that neither starts the pool's quiet period nor makes `busy()` true:
/// the face scan does not stand aside for it. It is made only while nothing
/// else is being asked for, on the few workers and cores set aside for
/// getting ahead, so it never takes a worker from browsing.
fn light_for(app: &App, path: &str, version: &str) -> Option<crate::cull::light::Light> {
    if let Some(light) = app.cull.light(path, version) {
        return Some(light);
    }
    if app.cull.light_unreadable(path, version) {
        return None;
    }
    let ask = || app.previews.get_blocking(Wanted { path: PathBuf::from(path), version: version.to_string(), tier: Tier::Thumb(crate::cull::light::THUMB_EDGE) }, Priority::Idle);
    let abandoned = |r: &Result<Arc<crate::serve::Served>, String>| matches!(r, Err(e) if e == crate::pool::ABANDONED);
    // Waited for, so the pool keeps this when the photographer moves on and
    // preparation starts again. Should it be dropped all the same, that
    // says nothing of the photo, so it is asked for again, a few times at most.
    let mut served = ask();
    for _ in 0..2 {
        if !abandoned(&served) { break; }
        served = ask();
    }
    if abandoned(&served) {
        return None;
    }
    let light = served.ok().and_then(|served| {
        let mut decoder = jpeg_decoder::Decoder::new(&served.bytes[..]);
        let rgb = decoder.decode().ok()?;
        let info = decoder.info()?;
        (info.pixel_format == jpeg_decoder::PixelFormat::RGB24).then(|| crate::cull::light::light_of_sized(&rgb, info.width as usize, info.height as usize))
    });
    match light {
        Some(light) => app.cull.keep_light(path, version, light),
        None => app.cull.mark_light_unreadable(path, version),
    }
    light
}

/// Where brightness is read for judging: a few at a time, in a pool of their
/// own, made once. Each waits for a preview worker, and must not hold up the
/// shared pool the decoders borrow.
fn light_readers() -> Option<&'static rayon::ThreadPool> {
    static READERS: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    READERS.get_or_init(|| rayon::ThreadPoolBuilder::new().num_threads(4).thread_name(|i| format!("opencull-light-{i}")).build().ok()).as_ref()
}

/// Frames grouped and turned into numbers. MobileCLIP fingerprints are not
/// read: the judging network was made without them.
fn autocull_groups(app: &App, frames: &[crate::cull::Frame], around: &[crate::cull::Around], kind: crate::cull::features::ShootKind) -> Vec<Vec<crate::cull::Prepared>> {
    let (lights, around_lights): (Vec<Option<crate::cull::light::Light>>, Vec<Option<crate::cull::light::Light>>) = {
        let frame_light = |f: &crate::cull::Frame| light_for(app, &f.path, &f.version);
        let around_light = |a: &crate::cull::Around| light_for(app, &a.path, &a.version);
        match light_readers() {
            Some(pool) => pool.install(|| (frames.par_iter().map(frame_light).collect(), around.par_iter().map(around_light).collect())),
            None => (frames.iter().map(frame_light).collect(), around.iter().map(around_light).collect()),
        }
    };
    // A photo around whose thumbnail cannot be read is left out, not counted as mid-grey.
    let around: Vec<(f64, crate::cull::light::Light)> = around.iter().zip(around_lights).filter_map(|(a, l)| l.map(|l| (a.time, l))).collect();
    let sessions = open_sessions(app);
    let folders = folders_of(&sessions, frames);
    // Looked up only for frames with others in their group: a lone frame is not ranked.
    crate::cull::prepare_groups(frames, &around, kind, |i| lights[i], no_fingerprint, |i| face_print_of(&app.faces.prints, folders[i], &frames[i]))
}

/// Judging gives no frame a MobileCLIP fingerprint.
fn no_fingerprint(_: usize) -> Option<Vec<f32>> { None }

/// The folders open now, the one opened last first.
fn open_sessions(app: &App) -> Vec<Arc<Session>> {
    let mut open: Vec<(u64, Arc<Session>)> = app.sessions.lock().unwrap().iter().map(|(id, s)| (*id, s.clone())).collect();
    open.sort_by(|a, b| b.0.cmp(&a.0));
    open.into_iter().map(|(_, s)| s).collect()
}

/// For each frame, the open folder it was found in: the one whose photos
/// include the file it is drawn from. That is where the scan kept its face
/// print, whatever folder the file is in: a review of several cards is opened
/// on the first card and holds the photos of all of them. Of two open folders
/// holding the same photo, the first given. None for a photo of no open folder.
fn folders_of<'a>(sessions: &'a [Arc<Session>], frames: &[crate::cull::Frame]) -> Vec<Option<&'a Path>> {
    let mut folder_of: HashMap<&Path, Option<&'a Path>> = frames.iter().map(|f| (Path::new(&f.path), None)).collect();
    for session in sessions {
        for image in &session.images {
            if let Some(slot @ None) = folder_of.get_mut(image.display.as_path()) {
                *slot = Some(&session.folder);
            }
        }
    }
    frames.iter().map(|f| folder_of[Path::new(&f.path)]).collect()
}

/// A frame's face print, if the Advanced scan made one for this version of
/// its file, kept under `folder`, the open folder it was found in. Only the
/// Advanced scan makes prints, and only Advanced judges.
fn face_print_of(store: &ai::store::PrintStore, folder: Option<&Path>, frame: &crate::cull::Frame) -> Option<ai::faceprint::PhotoPrint> {
    store.get(folder?, Path::new(&frame.path), &frame.version)
}

/// The new judging's verdict on these frames, set against the photos shot around them.
#[tauri::command]
pub async fn autocull_judge(app: AppState<'_>, _handle: tauri::AppHandle, frames: Vec<crate::cull::Frame>, around: Vec<crate::cull::Around>, mobile_clip: bool, kind: crate::cull::features::ShootKind) -> Result<Vec<crate::cull::decide::Judgement>, String> {
    let app = app.inner().clone();
    let _ = mobile_clip; // Kept so the interface need not change; judging no longer reads MobileCLIP.
    tauri::async_runtime::spawn_blocking(move || {
        let groups = autocull_groups(&app, &frames, &around, kind);
        Ok(crate::cull::decide::decide(&app.cull.model(), &groups))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn autocull_learn(app: AppState<'_>, _handle: tauri::AppHandle, folder: String, frames: Vec<crate::cull::Frame>, mobile_clip: bool, kind: crate::cull::features::ShootKind, source: crate::cull::lessons::Source) -> Result<crate::cull::LearnReply, String> {
    let app = app.inner().clone();
    let _ = mobile_clip; // Kept so the interface need not change; judging no longer reads MobileCLIP.
    tauri::async_runtime::spawn_blocking(move || {
        let groups = autocull_groups(&app, &frames, &[], kind);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        crate::cull::teach(&app.cull, folder, &groups, now, source)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command(async)]
pub fn autocull_kind(app: AppState, folder: String) -> Option<crate::cull::features::ShootKind> { app.cull.kind(&folder) }

#[tauri::command(async)]
pub fn autocull_set_kind(app: AppState, folder: String, kind: crate::cull::features::ShootKind) -> Result<(), String> { app.cull.set_kind(&folder, kind) }

#[tauri::command(async)]
pub fn autocull_status(app: AppState) -> crate::cull::Status { app.cull.status() }

#[tauri::command(async)]
pub fn autocull_reset(app: AppState) -> Result<(), String> { app.cull.reset() }

#[tauri::command(async)]
pub fn autocull_owned(app: AppState, folder: String) -> HashMap<String, crate::cull::Owned> { app.cull.owned(&folder) }

#[tauri::command(async)]
pub fn autocull_set_owned(app: AppState, folder: String, owned: HashMap<String, crate::cull::Owned>) -> Result<(), String> { app.cull.set_owned(&folder, &owned) }

fn mobileclip_root(handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(handle.path().app_cache_dir().map_err(|e| e.to_string())?.join("mobileclip-s0"))
}

#[tauri::command]
pub fn mobileclip_status(handle: tauri::AppHandle) -> Result<ai::mobileclip::Status, String> {
    Ok(ai::mobileclip::status(&mobileclip_root(&handle)?))
}

#[tauri::command]
pub async fn mobileclip_install(handle: tauri::AppHandle) -> Result<ai::mobileclip::Status, String> {
    ai::mobileclip::install(mobileclip_root(&handle)?).await
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
pub async fn ai_cache_usage(app: AppState<'_>, handle: tauri::AppHandle) -> Result<(u64, usize), String> {
    let store = app.faces.clone();
    let root = mobileclip_root(&handle)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (bytes, folders) = store.usage();
        (bytes + ai::mobileclip::cache_bytes(&root), folders)
    }).await.map_err(|e| e.to_string())
}

/// Forgets all past analysis. Folders are analysed again when next opened.
#[tauri::command]
pub async fn ai_clear_cache(app: AppState<'_>, handle: tauri::AppHandle) -> Result<(), String> {
    app.ai_scan.fetch_add(1, Ordering::SeqCst);
    {
        let mut epoch = app.ai_grouping_epoch.lock().unwrap();
        epoch.1 = epoch.1.wrapping_add(1);
    }
    let store = app.faces.clone();
    let root = mobileclip_root(&handle)?;
    tauri::async_runtime::spawn_blocking(move || { store.clear(); ai::mobileclip::clear(&root); }).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod grouper_tests {
    use super::{cascade_worth_it, compared_frames, grouper, preview_scene_print, Grouper};
    use crate::ai::scene::ScenePrint;

    #[test]
    fn only_frames_with_a_near_neighbour_are_compared() {
        assert_eq!(compared_frames(&[Some(0.0), Some(1000.0), Some(20000.0), None]), vec![true, true, false, true]);
        assert_eq!(compared_frames(&[]), Vec::<bool>::new());
        assert_eq!(compared_frames(&[Some(0.0)]), vec![false]);
        assert_eq!(compared_frames(&[Some(0.0), Some(5000.0), Some(10001.0)]), vec![true, true, false]);
    }

    #[test]
    fn a_print_is_made_from_the_saved_preview_as_the_scan_makes_it() {
        let (w, h) = (640u32, 427u32);
        let rgb: Vec<u8> = (0..w * h).flat_map(|i| { let (x, y) = (i % w, i / w); [(x * 255 / w) as u8, (y * 255 / h) as u8, ((x + y) % 256) as u8] }).collect();
        let jpeg = crate::preview::encode_jpeg(rgb, w, h, 90).unwrap();
        let root = std::env::temp_dir().join(format!("opencull-preview-print-{}", std::process::id()));
        let cache = crate::cache::Cache::new(root.join("previews"));
        let path = std::path::Path::new("/nowhere/A.RAW");
        cache.put(&cache.path_for(path, "v1", &crate::preview::Tier::Thumb(640).label()), &jpeg);
        let got = preview_scene_print(path, "v1", &cache).expect("a print from the cached preview");
        // The picture as grouping reads it, so the decoder the app is built with is the one checked.
        // The file does not exist, so this can only be the cached preview.
        let picture = crate::preview::grouping_pixels(path, "v1", &cache).unwrap();
        let (small, sw, sh) = crate::cull::light::shrink_to_thumb(&picture.rgb, picture.width, picture.height);
        let want = crate::ai::scene::print(&small, sw, sh).unwrap();
        assert_eq!(got.0.len(), want.0.len());
        assert!(got.0.iter().zip(&want.0).all(|(a, b)| (a - b).abs() < 1e-5));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn the_cascade_is_used_only_when_a_print_exists() {
        assert!(!cascade_worth_it(&[]));
        assert!(!cascade_worth_it(&[None, None]));
        assert!(cascade_worth_it(&[None, Some(ScenePrint(vec![1.0]))]));
    }

    #[test]
    fn grouping_is_chosen_by_advanced_and_mobileclip() {
        assert_eq!(grouper(false, false), Grouper::Faces, "Basic as today");
        assert_eq!(grouper(false, true), Grouper::MobileClip);
        assert_eq!(grouper(true, false), Grouper::Faces, "Advanced without MobileCLIP groups as today");
        assert_eq!(grouper(true, true), Grouper::Cascade);
    }
}

#[cfg(test)]
mod ai_grouping_epoch_tests {
    use super::accept_grouping_epoch;

    #[test]
    fn accepts_parallel_runs_at_one_revision_but_rejects_older_settings() {
        let mut current = (0, 0);
        assert!(accept_grouping_epoch(&mut current, (7, 2)));
        assert!(accept_grouping_epoch(&mut current, (7, 2)), "other ready runs share the same grouping revision");
        assert!(accept_grouping_epoch(&mut current, (7, 3)));
        assert!(!accept_grouping_epoch(&mut current, (7, 2)));
        assert_eq!(current, (7, 3));
    }
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
pub async fn ingest_browse(path: String, ignore_videos: Option<bool>) -> Result<Browsed, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&path);
        Browsed { folders: ingest::photo_folders_media(path, !ignore_videos.unwrap_or(true)), image_count: ingest::shallow_count_media(path, !ignore_videos.unwrap_or(true)) }
    })
    .await
    .map_err(|e| e.to_string())
}

/// The photos a folder on a card has to offer.
#[tauri::command]
pub async fn ingest_scan(path: String, ignore_videos: Option<bool>) -> Result<Vec<Scanned>, String> {
    let found = tauri::async_runtime::spawn_blocking(move || ingest::scan_media(Path::new(&path), !ignore_videos.unwrap_or(true))).await.map_err(|e| e.to_string())?;
    allow_shown_videos(&found);
    Ok(found)
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
    /// "success", "cancelled" or "failed".
    status: &'static str,
    message: Option<String>,
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
            return Ingested { status: "failed", message: Some(format!("Could not use {}: {e}", destination.display())) };
        }
        let workers = ingest::parallel_copies(&destination);
        let report = |p: ingest::Progress| {
            // Nothing more is said once the copy has been called off, so a
            // late report cannot put the bar back after it was taken down.
            if !cancel.load(Ordering::SeqCst) {
                progress.send(p).ok();
            }
        };
        match ingest::copy(&items, &destination, policy, workers, &cancel, &report) {
            ingest::Outcome::Success => Ingested { status: "success", message: None },
            ingest::Outcome::Cancelled => Ingested { status: "cancelled", message: None },
            ingest::Outcome::Failed(why) => Ingested { status: "failed", message: Some(why) },
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
    let found = tauri::async_runtime::spawn_blocking(move || scan::cover_images(Path::new(&path), 4))
        .await
        .map_err(|e| e.to_string())?;
    allow_shown_videos(&found);
    Ok(found)
}

/// Lets clips that were shown to the person, on the home page or in a card's contents, be opened for their still before any folder is open.
fn allow_shown_videos(found: &[Scanned]) {
    if let Ok(mut shown) = COVER_VIDEOS.lock() {
        for image in found.iter().filter(|i| scan::is_video(&raw::extension(&i.path))) {
            if let Ok(real) = Path::new(&image.path).canonicalize() { shown.insert(real); }
        }
    }
}

/// Clips just shown on the home page or in a card's contents: the only videos, besides an open folder's, that may be opened.
static COVER_VIDEOS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> = std::sync::LazyLock::new(Default::default);

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

#[derive(Deserialize)]
pub struct ReviewCopyItem {
    path: PathBuf, display: PathBuf, marks: Marks,
    framing: framing::Framing, upright: (u32, u32),
}

fn copy_review_items(items: &[ReviewCopyItem], sources: &[PathBuf], destination: &Path, cancel: &AtomicBool, progress: impl Fn(usize)) -> Result<(transfer::Outcome, HashMap<String, String>), String> {
    let destination = destination.canonicalize().map_err(|e| e.to_string())?;
    let roots = sources.iter().map(|source| {
        let root = source.canonicalize().map_err(|e| format!("Reconnect {}: {e}", source.display()))?;
        if !root.is_dir() { return Err(format!("{} is not a source folder", root.display())); }
        Ok(root)
    }).collect::<Result<Vec<_>, String>>()?;
    for root in &roots {
        if destination.starts_with(root) { return Err("Choose a destination outside the source cards".into()); }
    }
    let mut outcome = transfer::Outcome::default();
    let mut completed = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) { break; }
        let owner = item.path.canonicalize().map_err(|e| format!("Reconnect {}: {e}", item.path.display()))?;
        let display = item.display.canonicalize().map_err(|e| format!("Reconnect {}: {e}", item.display.display()))?;
        if !owner.is_file() || !roots.iter().any(|root| owner.starts_with(root)) {
            return Err(format!("{} is outside the selected source folders", item.path.display()));
        }
        if !display.is_file() || !roots.iter().any(|root| display.starts_with(root)) {
            return Err(format!("{} is outside the selected source folders", item.display.display()));
        }
        let companions = if display != owner { vec![display] } else { vec![] };
        let job = transfer::Job { owner, companions };
        let mut result = transfer::run(&[job], &destination, false, transfer::Conflict::KeepBoth, |_| {});
        if result.failed.is_empty() {
            if let Some(target) = result.placed.first() {
                if let Err(e) = xmp::write(target, item.marks) { result.sidecar_failed.push(format!("{}: {e}", target.display())); }
                if !item.framing.is_plain() {
                    let wanted = framing::Wanted { framing: item.framing, upright: item.upright, seed: item.marks };
                    if let Err(e) = framing::write(target, Some(item.marks), &wanted) { result.sidecar_failed.push(format!("{}: {e}", target.display())); }
                }
            }
        }
        if result.failed.is_empty() && result.sidecar_failed.is_empty() {
            if let Some(target) = result.placed.first() { completed.insert(item.path.to_string_lossy().into_owned(), target.to_string_lossy().into_owned()); }
        }
        outcome.placed.extend(result.placed); outcome.failed.extend(result.failed); outcome.sidecar_failed.extend(result.sidecar_failed);
        progress(i + 1);
    }
    Ok((outcome, completed))
}

#[derive(Serialize)]
pub struct ReviewCopied { #[serde(flatten)] result: Shipped, completed: HashMap<String, String> }

#[tauri::command]
pub async fn review_copy(app: AppState<'_>, items: Vec<ReviewCopyItem>, sources: Vec<PathBuf>, destination: String, progress: Channel<usize>) -> Result<ReviewCopied, String> {
    let app = app.inner().clone();
    if app.ingesting.swap(true, Ordering::SeqCst) { return Err("A copy is already running".into()); }
    let running = Ingesting(app.clone());
    let cancel = Arc::new(AtomicBool::new(false)); *app.ingest_cancel.lock().unwrap() = cancel.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _running = running;
        copy_review_items(&items, &sources, Path::new(&destination), &cancel, |done| { progress.send(done).ok(); }).map(|(result, completed)| ReviewCopied { result: result.into(), completed })
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)] mod review_tests {
    use super::*;
    #[test] fn review_copy_keeps_sources_unchanged_and_writes_destination_marks() {
        let root = std::env::temp_dir().join(format!("opencull-review-copy-{}", std::process::id()));
        let source = root.join("card"); let dest = root.join("dest"); std::fs::create_dir_all(&source).unwrap(); std::fs::create_dir_all(&dest).unwrap();
        let raw = source.join("A.RAW"); let jpeg = source.join("A.JPG");
        std::fs::write(&raw, b"raw bytes").unwrap(); std::fs::write(&jpeg, b"jpeg bytes").unwrap();
        let marks = Marks { rating: 4, ..Default::default() };
        let item = ReviewCopyItem { path: raw.clone(), display: jpeg.clone(), marks, framing: framing::Framing::default(), upright: (0, 0) };
        let (result, completed) = copy_review_items(&[item], &[source.clone()], &dest, &AtomicBool::new(false), |_| {}).unwrap();
        assert!(result.failed.is_empty()); assert!(result.sidecar_failed.is_empty()); assert_eq!(completed.len(), 1);
        assert_eq!(std::fs::read(raw).unwrap(), b"raw bytes"); assert_eq!(std::fs::read(jpeg).unwrap(), b"jpeg bytes");
        assert!(!source.join("A.xmp").exists()); assert_eq!(xmp::read(&dest.join("A.RAW"), None).rating, 4);
        assert_eq!(std::fs::read_dir(&source).unwrap().count(), 2); std::fs::remove_dir_all(root).unwrap();
    }
    #[test] fn destination_on_a_source_card_is_refused() {
        let root = std::env::temp_dir().join(format!("opencull-review-same-{}", std::process::id())); std::fs::create_dir_all(&root).unwrap();
        assert!(copy_review_items(&[], &[root.clone()], &root, &AtomicBool::new(false), |_| {}).is_err()); std::fs::remove_dir_all(root).unwrap();
    }
    #[test] fn review_copy_refuses_files_outside_selected_sources() {
        let root = std::env::temp_dir().join(format!("opencull-review-scope-{}", std::process::id()));
        let source = root.join("card"); let destination = root.join("destination");
        std::fs::create_dir_all(&source).unwrap(); std::fs::create_dir_all(&destination).unwrap();
        let outside = root.join("outside.JPG"); std::fs::write(&outside, b"outside source").unwrap();
        let item = ReviewCopyItem { path: outside.clone(), display: outside.clone(), marks: Marks::default(), framing: framing::Framing::default(), upright: (0, 0) };
        assert!(copy_review_items(&[item], &[source], &destination, &AtomicBool::new(false), |_| {}).is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"outside source");
        assert_eq!(std::fs::read_dir(destination).unwrap().count(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Selected cards are scanned as one shoot; disconnected sources are reported.
#[tauri::command]
pub async fn ingest_sources(sources: Vec<PathBuf>, ignore_videos: Option<bool>) -> Result<Vec<Scanned>, String> {
    let include_videos = !ignore_videos.unwrap_or(true);
    let found = tauri::async_runtime::spawn_blocking(move || crate::multicard::scan_sources(&sources, include_videos)).await.map_err(|e| e.to_string())??;
    allow_shown_videos(&found);
    Ok(found)
}

/// Grant only the requested video already belonging to an open session.
#[tauri::command]
pub fn video_source(handle: tauri::AppHandle, app: AppState, path: String) -> Result<String, String> {
    use tauri::Manager;
    let requested = PathBuf::from(path).canonicalize().map_err(|e| e.to_string())?;
    if !scan::is_video(&raw::extension(&requested)) { return Err("Not a supported video container".into()); }
    let sessions = app.sessions.lock().map_err(|e| e.to_string())?;
    let on_cover = COVER_VIDEOS.lock().is_ok_and(|shown| shown.contains(&requested));
    if !on_cover && !sessions.values().any(|s| s.images.iter().any(|p| p.path.canonicalize().ok().as_ref() == Some(&requested))) { return Err("Video is not in the selected session".into()); }
    handle.asset_protocol_scope().allow_file(&requested).map_err(|e| e.to_string())?;
    crate::video_stream::url(requested)
}

#[cfg(test)]
mod print_lookup_tests {
    use super::*;

    fn session(folder: &str, photos: &[&str]) -> Arc<Session> {
        let images = photos
            .iter()
            .map(|p| Scanned { path: PathBuf::from(p), display: PathBuf::from(p), name: String::new(), subpath: None, version: "v1".into(), size: 0, modified: 0 })
            .collect();
        Arc::new(Session { folder: PathBuf::from(folder), images, timed: true, closed: AtomicBool::new(false) })
    }

    fn frame(path: &str, version: &str) -> crate::cull::Frame {
        serde_json::from_value(serde_json::json!({ "path": path, "version": version, "faces": [], "score": null, "focus": null, "gaze": null, "key": null })).unwrap()
    }

    fn print(seed: f32) -> ai::faceprint::PhotoPrint {
        ai::faceprint::PhotoPrint { mean: (0..512).map(|i| seed + i as f32 / 1000.0).collect(), min: (0..512).map(|i| seed - i as f32 / 1000.0).collect() }
    }

    fn prints_of(sessions: &[Arc<Session>], store: &ai::store::PrintStore, frames: &[crate::cull::Frame]) -> Vec<Option<ai::faceprint::PhotoPrint>> {
        folders_of(sessions, frames).into_iter().zip(frames).map(|(folder, f)| face_print_of(store, folder, f)).collect()
    }

    #[test]
    fn judging_finds_the_prints_the_scan_kept_by_path_and_version() {
        let dir = std::env::temp_dir().join(format!("opencull-judge-prints-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let (a, b, c) = ("/shoots/wedding/100MSDCF/A.JPG", "/shoots/wedding/B.JPG", "/shoots/wedding/C.JPG");
        // As the scan keeps them: under the folder opened, by the path drawn from and its version.
        let store = ai::store::Store::new(dir.clone());
        store.prints.put(Path::new("/shoots/wedding"), Path::new(a), "v1", print(0.25));
        store.prints.put(Path::new("/shoots/wedding"), Path::new(b), "v1", print(0.5));
        store.prints.save(Path::new("/shoots/wedding"));

        // Read back afresh, as after the app is opened again.
        let again = ai::store::Store::new(dir.clone());
        let sessions = [session("/shoots/wedding", &[a, b, c])];
        let frames = [frame(a, "v1"), frame(b, "v2"), frame(c, "v1"), frame("/elsewhere/D.JPG", "v1")];
        let got = prints_of(&sessions, &again.prints, &frames);
        assert!(got == vec![Some(print(0.25)), None, None, None], "found in a subfolder; not for a changed file, a photo with none, or one of no open folder");
        assert!(prints_of(&[], &again.prints, &frames[..1]) == vec![None], "nothing is looked up with no folder open");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_review_of_several_cards_finds_the_prints_of_every_card() {
        let dir = std::env::temp_dir().join(format!("opencull-judge-cards-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let (on_a, on_b) = ("/Volumes/A/DCIM/1.JPG", "/Volumes/B/DCIM/2.JPG");
        // The review is opened on card A and holds the photos of both cards; the scan keeps them all under card A.
        let store = ai::store::Store::new(dir.clone());
        store.prints.put(Path::new("/Volumes/A"), Path::new(on_a), "v1", print(0.25));
        store.prints.put(Path::new("/Volumes/A"), Path::new(on_b), "v1", print(0.5));
        let sessions = [session("/Volumes/A", &[on_a, on_b])];
        let frames = [frame(on_a, "v1"), frame(on_b, "v1")];
        assert!(prints_of(&sessions, &store.prints, &frames) == vec![Some(print(0.25)), Some(print(0.5))]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn auto_label_reads_no_mobileclip_fingerprints() {
        // The judging network was made without them: HAS_EMBEDDING must read 0 for every frame.
        let grouped = |path: &str| -> crate::cull::Frame {
            serde_json::from_value(serde_json::json!({ "path": path, "version": "v1", "faces": [], "score": null, "focus": null, "gaze": null, "key": null, "pose": 1 })).unwrap()
        };
        let frames = [grouped("/shoots/a.JPG"), grouped("/shoots/b.JPG")];
        let groups = crate::cull::prepare_groups(&frames, &[], crate::cull::features::ShootKind::Events, |_| None, no_fingerprint, |_| None);
        assert_eq!(groups.iter().map(Vec::len).sum::<usize>(), 2);
        assert!(groups.iter().flatten().all(|p| p.x[crate::cull::features::HAS_EMBEDDING] == 0.0));
    }

    #[test]
    fn a_photo_belongs_to_the_open_folder_whose_photos_include_it() {
        let (outer, inner) = (session("/shoots", &["/shoots/x.JPG"]), session("/shoots/wedding", &["/shoots/wedding/A.JPG"]));
        let frames = [frame("/shoots/wedding/A.JPG", "v1"), frame("/shoots/x.JPG", "v1"), frame("/shoots/wedding/B.JPG", "v1")];
        let both = [outer.clone(), inner.clone()];
        assert_eq!(folders_of(&both, &frames), vec![Some(Path::new("/shoots/wedding")), Some(Path::new("/shoots")), None], "inside a folder is not enough: it must be one of its photos");
        // Held by two open folders: the first given, which is the one opened last.
        let twice = session("/shoots", &["/shoots/x.JPG", "/shoots/wedding/A.JPG"]);
        assert_eq!(folders_of(&[inner.clone(), twice.clone()], &frames[..1]), vec![Some(Path::new("/shoots/wedding"))]);
        assert_eq!(folders_of(&[twice, inner], &frames[..1]), vec![Some(Path::new("/shoots"))]);
    }
}
