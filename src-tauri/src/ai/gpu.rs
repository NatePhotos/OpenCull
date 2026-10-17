//! Finding faces on the Mac's graphics chip, through Core ML.
//!
//! MEASURED on an M4 Max, 919 photos with every core at work: 44 photos a
//! second with the finder on the processor, 57 with it on the graphics chip,
//! and the same faces and the same verdicts on every one of them. While
//! another program kept the chip busy, its jobs took ten times as long (115 ms
//! against 10) and an analysis left on it ran at half speed — so the chip is
//! used only while it is quick, and the processor takes over while it is not.
//!
//! Core ML is quick only when every size is fixed as a model is prepared, so
//! a copy of the finder is prepared for each picture size it meets — a shoot
//! has two, across and upright — and kept, in memory and on disk.
//!
//! Only the finder comes here. The judge and the eye and mouth models are
//! small; MEASURED, moving them too made the whole slower, not faster.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ort::session::Session;
use ort::value::TensorRef;

use super::onnx::{builder, OwnedInput};

/// Where prepared copies are kept between runs; the system's temporary folder until told.
static CACHE: OnceLock<PathBuf> = OnceLock::new();
/// Set once Core ML has failed: the processor does everything from then on.
static BROKEN: AtomicBool = AtomicBool::new(false);
static PACE: Mutex<Pace> = Mutex::new(Pace::new());

struct Prepared {
    session: Mutex<Session>,
}
type Key = (usize, Vec<Vec<usize>>);
static PREPARED: OnceLock<Mutex<HashMap<Key, Arc<Prepared>>>> = OnceLock::new();
static NAMES: OnceLock<Mutex<HashMap<usize, Vec<Vec<String>>>>> = OnceLock::new();


/// Keeps prepared copies in `dir`, so the next launch need not prepare them again.
pub fn use_cache(dir: PathBuf) {
    CACHE.set(dir).ok();
}

/// Whether the chip or the processor should take the finder: whichever gets
/// more photos analysed a second, measured.
///
/// The chip is quicker per job, but it takes jobs one after another, and when
/// another program is busy on it the photos queue for it. MEASURED on an M4
/// Max: 56 photos a second with the chip free against 42 on the processor;
/// with another program busy on the chip, 19 left on it against 25 on the
/// processor. Nothing short of trying tells the two apart on every Mac, so it
/// tries: the chip first, the processor for a few seconds, then whichever
/// managed more; and every half minute the other again, in case things changed.
struct Pace {
    chip: bool,
    since: Option<Instant>,
    trial: bool,
    rate: [Option<f32>; 2],
    finished: VecDeque<Instant>,
    /// Whether the first, short stint is over.
    begun: bool,
}

/// The first stint on the chip; a stint on the winner; a trial of the other; and
/// the time just after a change in which jobs already under way are still finishing.
const FIRST: Duration = Duration::from_secs(5);
const STINT: Duration = Duration::from_secs(30);
const TRIAL: Duration = Duration::from_secs(6);
const SETTLE: Duration = Duration::from_secs(2);
/// Too few photos finished in a trial to say anything (someone looking at one photo, say).
const ENOUGH: usize = 10;

impl Pace {
    const fn new() -> Self {
        Pace { chip: true, since: None, trial: false, rate: [None, None], finished: VecDeque::new(), begun: false }
    }

    fn slot(chip: bool) -> usize {
        usize::from(chip)
    }

    /// Photos a second finished since the last change, leaving out the settling time.
    fn measured(&self, since: Instant, now: Instant) -> Option<f32> {
        let from = since + SETTLE;
        let n = self.finished.iter().filter(|t| **t >= from).count();
        let span = now.saturating_duration_since(from).as_secs_f32();
        (n >= ENOUGH && span > 0.0).then(|| n as f32 / span)
    }

    fn use_chip(&mut self, now: Instant) -> bool {
        let since = *self.since.get_or_insert(now);
        let length = if self.trial { TRIAL } else if self.begun { STINT } else { FIRST };
        if now.saturating_duration_since(since) >= length {
            // Each stint or trial measures only its own: an old figure says nothing about now.
            self.rate[Self::slot(self.chip)] = self.measured(since, now);
            if std::env::var_os("OPENCULL_GPU_LOG").is_some() {
                eprintln!("[gpu] {} {} over: chip {:?} processor {:?} a second", if self.trial { "trial" } else { "stint" }, if self.chip { "on chip" } else { "on processor" }, self.rate[1], self.rate[0]);
            }
            if self.trial {
                // The trial is over: whichever managed more; the chip, if there was too little to tell.
                self.chip = match (self.rate[Self::slot(true)], self.rate[Self::slot(false)]) {
                    (Some(chip), Some(processor)) => chip >= processor,
                    _ => true,
                };
                self.trial = false;
            } else {
                // Time to try the other again.
                self.begun = true;
                self.chip = !self.chip;
                self.trial = true;
            }
            self.since = Some(now);
        }
        self.chip
    }

    fn finished(&mut self, now: Instant) {
        self.finished.push_back(now);
        while self.finished.front().is_some_and(|t| now.saturating_duration_since(*t) > STINT + TRIAL) {
            self.finished.pop_front();
        }
    }
}

/// Tells the pace a finder job has finished, on whichever did it.
pub fn finished() {
    PACE.lock().unwrap_or_else(|e| e.into_inner()).finished(Instant::now());
}

/// The name of each size in each input, as the model file calls them ("h", "w").
fn names(bytes: &'static [u8]) -> Result<Vec<Vec<String>>, String> {
    let names = NAMES.get_or_init(Default::default);
    if let Some(n) = names.lock().unwrap_or_else(|e| e.into_inner()).get(&(bytes.as_ptr() as usize)) {
        return Ok(n.clone());
    }
    let plain = builder()?.commit_from_memory(bytes).map_err(|e| e.to_string())?;
    let found: Vec<Vec<String>> = plain
        .inputs()
        .iter()
        .map(|i| match i.dtype() {
            ort::value::ValueType::Tensor { dimension_symbols, .. } => dimension_symbols.iter().map(|s| s.to_string()).collect(),
            _ => Vec::new(),
        })
        .collect();
    names.lock().unwrap_or_else(|e| e.into_inner()).insert(bytes.as_ptr() as usize, found.clone());
    Ok(found)
}

fn prepared(bytes: &'static [u8], shapes: &[Vec<usize>]) -> Result<Arc<Prepared>, String> {
    let key = (bytes.as_ptr() as usize, shapes.to_vec());
    // Held while preparing: two photos preparing the same copy at once collide in the cache.
    let mut all = PREPARED.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = all.get(&key) {
        return Ok(p.clone());
    }
    let mut b = builder()?;
    for (names, shape) in names(bytes)?.iter().zip(shapes) {
        for (name, size) in names.iter().zip(shape) {
            if !name.is_empty() {
                b = b.with_dimension_override(name, *size as i64).map_err(|e| e.to_string())?;
            }
        }
    }
    // Core ML names its saved copies after the model alone, so a copy made for
    // one size would be handed back for another: each size has a folder.
    let sizes = shapes.iter().map(|s| s.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("x")).collect::<Vec<_>>().join("_");
    let root = CACHE.get().cloned().unwrap_or_else(|| std::env::temp_dir().join("opencull-coreml"));
    let dir = root.join(format!("{:x}-{sizes}", xxhash_rust::xxh3::xxh3_64(bytes)));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let chip = ort::ep::CoreML::default()
        .with_compute_units(ort::ep::coreml::ComputeUnits::CPUAndGPU)
        .with_model_format(ort::ep::coreml::ModelFormat::MLProgram)
        .with_static_input_shapes(true)
        .with_model_cache_dir(dir.to_string_lossy())
        .build()
        .error_on_failure();
    let session = b
        .with_execution_providers([chip])
        .map_err(|e| e.to_string())?
        .commit_from_memory(bytes)
        .map_err(|e| e.to_string())?;
    let p = Arc::new(Prepared { session: Mutex::new(session) });
    all.insert(key, p.clone());
    Ok(p)
}

/// Runs the finder on the graphics chip, if it is quick just now. Hands the
/// job back, untouched, for the processor when it is not, or when Core ML
/// cannot be used at all.
pub fn run<T, F: FnOnce(&[(Vec<usize>, &[f32])]) -> T>(bytes: &'static [u8], inputs: Vec<OwnedInput>, outputs: &[String], read: F) -> Result<Result<T, String>, (Vec<OwnedInput>, F)> {
    if BROKEN.load(Ordering::Relaxed) || !PACE.lock().unwrap_or_else(|e| e.into_inner()).use_chip(Instant::now()) {
        return Err((inputs, read));
    }
    let shapes: Vec<Vec<usize>> = inputs.iter().map(|i| i.1.clone()).collect();
    let prepared = match prepared(bytes, &shapes) {
        Ok(p) => p,
        Err(e) => {
            give_up(&e);
            return Err((inputs, read));
        }
    };
    let mut session = prepared.session.lock().unwrap_or_else(|e| e.into_inner());
    let mut given = Vec::with_capacity(inputs.len());
    for (name, shape, data) in &inputs {
        match TensorRef::from_array_view((shape.clone(), data.as_slice())) {
            Ok(t) => given.push((name.as_str(), t)),
            Err(e) => return Ok(Err(e.to_string())),
        }
    }
    let results = match session.run(given) {
        Ok(r) => r,
        Err(e) => {
            give_up(&e.to_string());
            return Err((inputs, read));
        }
    };
    let mut taken = Vec::with_capacity(outputs.len());
    for name in outputs {
        match results[name.as_str()].try_extract_tensor::<f32>() {
            Ok((shape, data)) => taken.push((shape.iter().map(|d| *d as usize).collect::<Vec<usize>>(), data)),
            Err(e) => return Ok(Err(e.to_string())),
        }
    }
    let done = read(&taken);
    PACE.lock().unwrap_or_else(|e| e.into_inner()).finished(Instant::now());
    Ok(Ok(done))
}

fn give_up(why: &str) {
    if !BROKEN.swap(true, Ordering::Relaxed) {
        eprintln!("[ai] the graphics chip could not be used for finding faces; the processor will: {why}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the pace for `secs` seconds with photos finishing at `chip_rate` a second while the
    /// chip has the finder and `cpu_rate` while the processor has it; the share of the time on the chip.
    fn share_on_chip(chip_rate: f32, cpu_rate: f32, secs: u32) -> f32 {
        let start = Instant::now();
        let mut p = Pace::new();
        let (mut on_chip, steps) = (0, secs * 100);
        let mut owed = 0.0;
        for i in 0..steps {
            let now = start + Duration::from_millis(i as u64 * 10);
            let chip = p.use_chip(now);
            on_chip += u32::from(chip);
            owed += if chip { chip_rate } else { cpu_rate } / 100.0;
            while owed >= 1.0 {
                p.finished(now);
                owed -= 1.0;
            }
        }
        on_chip as f32 / steps as f32
    }

    #[test]
    fn a_free_chip_keeps_the_finder_nearly_all_the_time() {
        // An M4 Max as measured, chip free: 56 a second on it, 42 on the processor.
        // All but the trials of the processor: six seconds in every thirty-six.
        assert!(share_on_chip(56.0, 42.0, 120) > 0.75);
    }

    #[test]
    fn a_busy_chip_gives_the_finder_to_the_processor_nearly_all_the_time() {
        // Another program busy on the chip: 19 a second left on it, 25 on the processor.
        assert!(share_on_chip(19.0, 25.0, 120) < 0.2);
    }

    #[test]
    fn with_too_few_photos_to_measure_the_chip_is_kept() {
        // Someone looking through photos one at a time: nothing to compare.
        assert!(share_on_chip(1.0, 1.0, 120) > 0.75);
    }
}
