//! Auto-cull: colours a folder's photos from what the face scan found, and
//! learns the photographer's taste from the stars they give. Everything here
//! runs on this computer; nothing is sent anywhere.

pub mod decide;
pub mod features;
pub mod lessons;
pub mod light;
pub mod net;
pub mod rules;
pub mod starter;
pub mod train;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_128;

use crate::ai::store::Record;

/// One photo as the interface describes it: what the scan found, where it
/// stands among the frames it was compared with, and the photographer's stars.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub path: String,
    pub version: String,
    #[serde(flatten)]
    pub record: Record,
    /// Its mark set beside the other frames of its burst, out of 100.
    #[serde(default)]
    pub compared_score: Option<f32>,
    /// For each face: how open its eyes are as a share of that person's best in the burst.
    #[serde(default)]
    pub compared_eyes: Vec<Option<f32>>,
    #[serde(default)]
    pub rank: Option<u32>,
    #[serde(default)]
    pub count: Option<u32>,
    /// Frames sharing a number were compared with one another; none means it stands alone.
    #[serde(default)]
    pub pose: Option<i64>,
    #[serde(default)]
    pub rating: i32,
    /// When it was shot, in milliseconds; none if the camera did not say.
    #[serde(default)]
    pub time: Option<f64>,
    /// The photographer's flag: "pick", "reject" or "none".
    #[serde(default = "no_flag")]
    pub flag: String,
}

fn no_flag() -> String {
    "none".into()
}

/// A frame turned into numbers, ready for the network.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub path: String,
    pub version: String,
    pub readings: [f32; features::READINGS],
    pub x: Vec<f32>,
    pub rating: i32,
    /// 1 for a pick, -1 for a reject, 0 for neither.
    pub flag: i8,
}

/// A photo shot near the ones being judged, there only so their exposure can
/// be set against it. Only photos with a capture time are sent.
#[derive(Clone, Debug, Deserialize)]
pub struct Around {
    pub path: String,
    pub version: String,
    pub time: f64,
}

/// A colour OpenCull set, and the version of the file it was set on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Owned {
    pub version: String,
    pub color: String,
    /// Why it was given that colour, so the reason is still there when the
    /// folder is opened again.
    #[serde(default)]
    pub reason: String,
}

/// What became of a shoot's lessons after it was taught.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kept {
    /// Learned from: the retrained network is now in use.
    Learned,
    /// Too few rated photos to check; saved, to be learned from with later shoots.
    Saved,
    /// Checked and found no better: never learned from again.
    SetAside,
    /// Checked and found no better, but this folder had been learned from
    /// before: its earlier lessons stay, as does the network.
    Earlier,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnReply {
    pub learned: train::Learned,
    /// How well the network now in use did on bursts of this shoot it had not
    /// seen, keepers, rejects and burst order counting equally.
    pub matched: f32,
    pub kept: Kept,
    /// Whether the stars or the pick and reject flags were what it learned from.
    #[serde(default)]
    pub source: lessons::Source,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Status {
    /// Shoots the network in use learned from.
    pub shoots: usize,
    pub last: Option<LearnReply>,
}

/// Makes each temporary file's name unique within this process.
static TEMP: AtomicU64 = AtomicU64::new(0);

/// The network, the lessons and the record of which colours OpenCull set,
/// kept in the app's data folder. Never sent anywhere.
pub struct Store {
    dir: PathBuf,
    model: Mutex<Option<net::Net>>,
    /// Held while teaching or resetting, so the two cannot overlap.
    teach: Mutex<()>,
    /// Held while the shoot types are read and rewritten, so two changes cannot lose one another.
    kinds: Mutex<()>,
    /// Each photo's brightness, by path and version, so it is read once per run of the app.
    lights: Mutex<HashMap<(String, String), light::Light>>,
    /// Photos whose thumbnail could not be read for brightness, by path and
    /// version, so they are not tried again on every request this run.
    unreadable: Mutex<HashSet<(String, String)>>,
}

/// Past this many photos' brightness kept in memory, start again rather than grow without end.
const LIGHTS_KEPT: usize = 100_000;

/// Whether a network read from disk has the shape this version of the app feeds it.
fn fits(net: &net::Net) -> bool {
    net.inputs == features::INPUTS && net.hidden > 0 && net.params.len() == net.hidden * net.inputs + 3 * net.hidden + 2
}

/// The saved copy of the starting network, with what it was built for: a copy
/// from another version of the readings or of the starting rules is rebuilt,
/// even when its shape fits.
#[derive(Serialize, Deserialize)]
struct SavedStarter {
    format: u32,
    version: u32,
    net: net::Net,
}

/// The learned network as saved, with the readings it was taught on: one
/// taught on readings of another meaning is not used.
#[derive(Serialize, Deserialize)]
struct SavedModel {
    format: u32,
    net: net::Net,
}

impl SavedStarter {
    fn current(&self) -> bool {
        self.format == features::FORMAT && self.version == starter::STARTER_VERSION && fits(&self.net)
    }
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(dir.join("owned")).ok();
        Self { dir, model: Mutex::new(None), teach: Mutex::new(()), kinds: Mutex::new(()), lights: Mutex::new(HashMap::new()), unreadable: Mutex::new(HashSet::new()) }
    }

    pub fn light(&self, path: &str, version: &str) -> Option<light::Light> {
        self.lights.lock().unwrap_or_else(|e| e.into_inner()).get(&(path.to_string(), version.to_string())).copied()
    }

    pub fn keep_light(&self, path: &str, version: &str, light: light::Light) {
        let mut kept = self.lights.lock().unwrap_or_else(|e| e.into_inner());
        if kept.len() >= LIGHTS_KEPT {
            kept.clear();
        }
        kept.insert((path.to_string(), version.to_string()), light);
    }

    pub fn light_unreadable(&self, path: &str, version: &str) -> bool {
        self.unreadable.lock().unwrap_or_else(|e| e.into_inner()).contains(&(path.to_string(), version.to_string()))
    }

    pub fn mark_light_unreadable(&self, path: &str, version: &str) {
        let mut failed = self.unreadable.lock().unwrap_or_else(|e| e.into_inner());
        if failed.len() >= LIGHTS_KEPT {
            failed.clear();
        }
        failed.insert((path.to_string(), version.to_string()));
    }

    pub fn lessons(&self) -> lessons::LessonStore {
        lessons::LessonStore::new(self.dir.join("lessons"))
    }

    /// Held for as long as a shoot is being taught or everything is being forgotten.
    pub fn teach(&self) -> MutexGuard<'_, ()> {
        self.teach.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn write(path: PathBuf, bytes: Vec<u8>) -> Result<(), String> {
        let temp = path.with_extension(format!("tmp{}-{}", std::process::id(), TEMP.fetch_add(1, Ordering::Relaxed)));
        if let Err(e) = std::fs::write(&temp, bytes) {
            std::fs::remove_file(&temp).ok();
            return Err(e.to_string());
        }
        std::fs::rename(&temp, &path).map_err(|e| {
            std::fs::remove_file(&temp).ok();
            e.to_string()
        })
    }

    /// The network in use: the one learned, else the starting network, which
    /// is trained once and kept. A file of another shape or another readings
    /// format is ignored, as is a starting network saved by another version.
    pub fn model(&self) -> net::Net {
        let mut held = self.model.lock().unwrap();
        if let Some(net) = held.as_ref() {
            return net.clone();
        }
        let read = |name: &str| std::fs::read(self.dir.join(name)).ok();
        let learned = read("model.json")
            .and_then(|b| serde_json::from_slice::<SavedModel>(&b).ok())
            .filter(|s| s.format == features::FORMAT && fits(&s.net))
            .map(|s| s.net);
        let saved = || read("starter.json").and_then(|b| serde_json::from_slice::<SavedStarter>(&b).ok()).filter(SavedStarter::current).map(|s| s.net);
        let net = learned.or_else(saved).unwrap_or_else(|| {
            let net = starter::starting_net();
            let stamped = SavedStarter { format: features::FORMAT, version: starter::STARTER_VERSION, net };
            // Only a saved copy of something that can be rebuilt, so a failure is not worth reporting.
            Self::write(self.dir.join("starter.json"), serde_json::to_vec(&stamped).unwrap_or_default()).ok();
            stamped.net
        });
        *held = Some(net.clone());
        net
    }

    pub fn save_model(&self, net: &net::Net) -> Result<(), String> {
        let bytes = serde_json::to_vec(&SavedModel { format: features::FORMAT, net: net.clone() }).map_err(|e| format!("Couldn't save what OpenCull learned: {e}"))?;
        Self::write(self.dir.join("model.json"), bytes).map_err(|e| format!("Couldn't save what OpenCull learned: {e}"))?;
        *self.model.lock().unwrap() = Some(net.clone());
        Ok(())
    }

    pub fn status(&self) -> Status {
        std::fs::read(self.dir.join("status.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save_status(&self, status: &Status) {
        Self::write(self.dir.join("status.json"), serde_json::to_vec(status).unwrap_or_default()).ok();
    }

    fn owned_file(&self, folder: &str) -> PathBuf {
        self.dir.join("owned").join(format!("{:032x}.json", xxh3_128(folder.as_bytes())))
    }

    pub fn owned(&self, folder: &str) -> HashMap<String, Owned> {
        std::fs::read(self.owned_file(folder)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn set_owned(&self, folder: &str, owned: &HashMap<String, Owned>) -> Result<(), String> {
        let bytes = serde_json::to_vec(owned).map_err(|e| format!("Couldn't remember which colours Auto-label set: {e}"))?;
        Self::write(self.owned_file(folder), bytes).map_err(|e| format!("Couldn't remember which colours Auto-label set: {e}"))
    }

    fn read_kinds(&self) -> HashMap<String, features::ShootKind> {
        std::fs::read(self.dir.join("kinds.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    /// The type of shoot the photographer chose for `folder`, if they have chosen.
    pub fn kind(&self, folder: &str) -> Option<features::ShootKind> {
        let _held = self.kinds.lock().unwrap_or_else(|e| e.into_inner());
        self.read_kinds().get(folder).copied()
    }

    pub fn set_kind(&self, folder: &str, kind: features::ShootKind) -> Result<(), String> {
        let _held = self.kinds.lock().unwrap_or_else(|e| e.into_inner());
        let mut kinds = self.read_kinds();
        kinds.insert(folder.to_string(), kind);
        let bytes = serde_json::to_vec(&kinds).map_err(|e| format!("Couldn't remember the type of shoot: {e}"))?;
        Self::write(self.dir.join("kinds.json"), bytes).map_err(|e| format!("Couldn't remember the type of shoot: {e}"))
    }

    /// Back to the starting network, with every lesson forgotten. The record
    /// of which colours OpenCull set is kept, so they can still be cleared.
    pub fn reset(&self) -> Result<(), String> {
        let _teaching = self.teach();
        let mut result = Ok(());
        for name in ["model.json", "status.json"] {
            match std::fs::remove_file(self.dir.join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => result = result.and(Err(format!("Couldn't reset what OpenCull learned: {e}"))),
            }
        }
        let cleared = self.lessons().clear();
        // Forgotten whether or not every file could be removed, so the
        // network in use never outlives the reset.
        *self.model.lock().unwrap() = None;
        result.and(cleared)
    }
}

/// Teaches auto-cull the stars (or flags) given in `folder`, whose frames are `groups`,
/// and says what came of it. A folder taught before is taught afresh, except
/// that when its earlier lessons are in the network in use and the new ones
/// are checked and found no better, the earlier ones are kept: the current
/// network has already learned from them, so a retrained one can rarely beat
/// it there, and setting the folder aside would lose lessons that were kept.
pub fn teach(store: &Store, folder: String, groups: &[Vec<Prepared>], now: u64, source: lessons::Source) -> Result<LearnReply, String> {
    let lessons = lessons::lessons_from(groups, source);
    if lessons.is_empty() {
        return Err(match source {
            lessons::Source::Stars => "Nothing to learn yet: give some photos stars first (1–2 for rejects, 4–5 for keepers).",
            lessons::Source::Flags => "Nothing to learn yet: flag some photos first (Pick for keepers, Reject for rejects).",
        }.to_string());
    }
    if !train::trainable(lessons.iter().map(|t| &t.lesson)) {
        return Err(match source {
            lessons::Source::Stars => "Nothing to learn yet: give some 1–2★ rejects as well as keepers, or a 5★ pick in a burst.",
            lessons::Source::Flags => "Nothing to learn yet: flag some rejects as well as picks (or picks as well as rejects), so there's something to compare.",
        }.to_string());
    }
    let _teaching = store.teach();
    let saved = store.lessons();
    let mut shoots = saved.all();
    let earlier_in_use = lessons::in_use(&shoots).iter().any(|s| s.folder == folder);
    // Taught after every other shoot, even if the clock says otherwise.
    let taught_at = shoots.iter().map(|s| s.taught_at + 1).fold(now, u64::max);
    let earlier = shoots.iter().position(|s| s.folder == folder).map(|at| shoots.remove(at));
    let mut shoot = lessons::Shoot { folder, format: features::FORMAT, taught_at, lessons, set_aside: false, accepted: false };
    shoots.push(shoot.clone());
    let (net, learned) = train::learn(&store.model(), &shoots);
    shoots.pop();
    let kept = if learned.accepted {
        // The network first: lessons marked kept with no network saved to
        // match would be counted as learned from.
        store.save_model(&net)?;
        shoot.accepted = true;
        saved.save(&shoot)?;
        Kept::Learned
    } else if !learned.enough {
        saved.save(&shoot)?;
        Kept::Saved
    } else if earlier_in_use {
        Kept::Earlier
    } else {
        shoot.set_aside = true;
        saved.save(&shoot)?;
        Kept::SetAside
    };
    shoots.push(match (kept, earlier) { (Kept::Earlier, Some(e)) => e, _ => shoot });
    let matched = if learned.accepted { learned.after } else { learned.before };
    let reply = LearnReply { learned, matched, kept, source };
    store.save_status(&Status { shoots: lessons::in_use(&shoots).len(), last: Some(reply.clone()) });
    Ok(reply)
}

/// Puts frames into their poses and turns each into numbers. `light` and
/// `print` look up a frame's brightness (by its place in `frames`) and, if wanted, its fingerprint.
/// Every group has at least one frame; a frame with no pose is a group of its own.
/// Each frame also reads how much lighter or darker it is than the frames shot
/// around it. `around` are photos shot near these that are not being judged:
/// their light counts when a frame's exposure is set against the frames around
/// it, and they are never judged themselves.
pub fn prepare_groups(
    frames: &[Frame],
    around: &[(f64, light::Light)],
    kind: features::ShootKind,
    light: impl Fn(usize) -> Option<light::Light>,
    print: impl Fn(usize) -> Option<Vec<f32>>,
) -> Vec<Vec<Prepared>> {
    let known: Vec<Option<light::Light>> = (0..frames.len()).map(light).collect();
    let stops = stops_vs_neighbours(frames, &known, around);
    // Its other light readings need some value; the stops reading is the one set against other photos.
    let lights: Vec<light::Light> = known.iter().map(|l| l.unwrap_or_else(light::Light::neutral)).collect();
    let mut by_pose: Vec<(Option<i64>, Vec<usize>)> = Vec::new();
    for (i, f) in frames.iter().enumerate() {
        match f.pose.and_then(|p| by_pose.iter_mut().find(|(k, _)| *k == Some(p))) {
            Some((_, members)) => members.push(i),
            None => by_pose.push((f.pose, vec![i])),
        }
    }
    by_pose
        .into_iter()
        .map(|(_, members)| {
            let best = members.iter().map(|&i| features::mark(&frames[i])).fold(0.0f32, f32::max);
            members
                .into_iter()
                .map(|i| {
                    let f = &frames[i];
                    let readings = features::readings(f, lights[i], best, kind, stops[i]);
                    let embedding = print(i);
                    Prepared { path: f.path.clone(), version: f.version.clone(), x: features::inputs(&readings, embedding.as_deref()), readings, rating: f.rating, flag: flag_of(&f.flag) }
                })
                .collect()
        })
        .collect()
}

fn flag_of(flag: &str) -> i8 {
    match flag {
        "pick" => 1,
        "reject" => -1,
        _ => 0,
    }
}

/// Frames shot within this long of one another count as shot around each other.
const NEIGHBOUR_MS: f64 = 10_000.0;
/// With fewer than two of those, this many of the nearest in time are used.
const NEAREST: usize = 4;
/// ...but never one further away than this: a lone frame at a scene change has no neighbours.
const NEAREST_MAX_MS: f64 = 120_000.0;

/// For each frame, how many stops more light it caught than the frames shot
/// around it: log2 of its average light over their median, measured on the
/// decoded light (`Light::linear`), so one stop is twice the light. The photos
/// in `around` count as neighbours too. 0 when there are none. A frame whose
/// light is unknown reads 0 and is no one's neighbour, like a photo around
/// whose thumbnail could not be read.
fn stops_vs_neighbours(frames: &[Frame], lights: &[Option<light::Light>], around: &[(f64, light::Light)]) -> Vec<f32> {
    let n = frames.len();
    // Every timed photo of known light, judged or not: its time, its light, and which frame it is.
    let mut timed: Vec<(f64, f32, Option<usize>)> = (0..n)
        .filter_map(|i| Some((frames[i].time?, lights[i]?.linear, Some(i))))
        .chain(around.iter().map(|(t, l)| (*t, l.linear, None)))
        .collect();
    // Stable, so equal times keep frames before the photos around them, each in the order given.
    timed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut near_of: Vec<Vec<f32>> = vec![Vec::new(); n];
    for p in 0..timed.len() {
        let (t, _, Some(i)) = timed[p] else { continue };
        let mut near: Vec<f32> = Vec::new();
        let mut left = p;
        while left > 0 && t - timed[left - 1].0 <= NEIGHBOUR_MS {
            left -= 1;
            near.push(timed[left].1);
        }
        let mut right = p + 1;
        while right < timed.len() && timed[right].0 - t <= NEIGHBOUR_MS {
            near.push(timed[right].1);
            right += 1;
        }
        if near.len() < 2 {
            // Walk outwards from this frame, taking whichever side is closer in time.
            let (mut l, mut r) = (p, p + 1);
            near.clear();
            while near.len() < NEAREST && (l > 0 || r < timed.len()) {
                let before = (l > 0).then(|| t - timed[l - 1].0).filter(|d| *d <= NEAREST_MAX_MS);
                let after = (r < timed.len()).then(|| timed[r].0 - t).filter(|d| *d <= NEAREST_MAX_MS);
                match (before, after) {
                    (None, None) => break,
                    (Some(b), Some(a)) if b <= a => { l -= 1; near.push(timed[l].1); }
                    (Some(_), None) => { l -= 1; near.push(timed[l].1); }
                    _ => { near.push(timed[r].1); r += 1; }
                }
            }
        }
        near_of[i] = near;
    }
    // Only frames of known light are compared; their neighbours too.
    let known: Vec<usize> = (0..n).filter(|&i| lights[i].is_some()).collect();
    for (at, &i) in known.iter().enumerate().filter(|(_, &i)| frames[i].time.is_none()) {
        // No clock: the frames next to it, in the order given.
        near_of[i] = known[at.saturating_sub(NEAREST / 2)..(at + NEAREST / 2 + 1).min(known.len())].iter().filter(|&&k| k != i).filter_map(|&k| lights[k]).map(|l| l.linear).collect();
    }
    (0..n)
        .map(|i| {
            let mut means = std::mem::take(&mut near_of[i]);
            let Some(own) = lights[i] else { return 0.0 };
            if means.is_empty() {
                return 0.0;
            }
            means.sort_by(f32::total_cmp);
            let mid = means.len() / 2;
            let median = if means.len() % 2 == 1 { means[mid] } else { (means[mid - 1] + means[mid]) / 2.0 };
            // The small floor keeps a black frame finite; it is far below any real exposure.
            ((own.linear + 0.001) / (median + 0.001)).log2()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_starts_with_the_starting_network_and_forgets_on_reset() {
        let dir = std::env::temp_dir().join(format!("autocull-store-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let start = store.model();
        assert_eq!(start, starter::starting_net());
        let mut changed = start.clone();
        changed.red_line = 1.5;
        store.save_model(&changed).unwrap();
        assert_eq!(store.model().red_line, 1.5);
        let mut owned = HashMap::new();
        owned.insert("/a.ARW".to_string(), Owned { version: "v".into(), color: "red".into(), reason: String::new() });
        store.set_owned("/f", &owned).unwrap();
        assert_eq!(store.owned("/f"), owned);
        store.reset().unwrap();
        assert_eq!(store.model(), start);
        assert_eq!(store.status().shoots, 0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_type_of_shoot_is_kept_per_folder() {
        let dir = std::env::temp_dir().join(format!("autocull-kinds-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        assert_eq!(store.kind("/f"), None);
        store.set_kind("/f", features::ShootKind::Sports).unwrap();
        assert_eq!(store.kind("/f"), Some(features::ShootKind::Sports));
        assert_eq!(store.kind("/other"), None);
        assert_eq!(Store::new(dir.clone()).kind("/f"), Some(features::ShootKind::Sports));
        std::fs::remove_dir_all(dir).ok();
    }

    fn frame(path: &str, pose: Option<i64>, rating: i32) -> Frame {
        serde_json::from_value(serde_json::json!({ "path": path, "version": "v", "faces": [], "score": null, "focus": null, "gaze": null, "key": null, "pose": pose, "rating": rating })).unwrap()
    }

    fn frame_at(i: usize, ms: f64) -> Frame {
        let mut f = frame(&format!("/f/{i}.ARW"), None, 0);
        f.time = Some(ms);
        f
    }

    #[test]
    fn a_run_judged_with_the_photos_around_it_reads_the_same_as_the_whole_folder() {
        use features::*;
        // Two runs a minute apart; the dark frame is in the second.
        let times = [0.0, 1000.0, 2000.0, 60_000.0, 61_000.0, 62_000.0, 63_000.0];
        let lights = [0.5, 0.45, 0.5, 0.5, 0.1, 0.5, 0.55];
        let frames: Vec<Frame> = times.iter().enumerate().map(|(i, &t)| frame_at(i, t)).collect();
        let whole = prepare_groups(&frames, &[], ShootKind::Events, |i| Some(lit(lights[i])), |_| None);
        let stops = |groups: &[Vec<Prepared>], path: &str| groups.iter().flatten().find(|p| p.path == path).unwrap().readings[STOPS_VS_NEIGHBOURS];
        // The second run alone, told of the first run's photos.
        let around: Vec<(f64, light::Light)> = (0..3).map(|i| (times[i], lit(lights[i]))).collect();
        let run = prepare_groups(&frames[3..], &around, ShootKind::Events, |i| Some(lit(lights[3 + i])), |_| None);
        for i in 3..7 {
            let path = format!("/f/{i}.ARW");
            assert_eq!(stops(&run, &path), stops(&whole, &path), "{path}");
        }
        // ...and the first run alone, told of the second's.
        let around: Vec<(f64, light::Light)> = (3..7).map(|i| (times[i], lit(lights[i]))).collect();
        let run = prepare_groups(&frames[..3], &around, ShootKind::Events, |i| Some(lit(lights[i])), |_| None);
        for i in 0..3 {
            let path = format!("/f/{i}.ARW");
            assert_eq!(stops(&run, &path), stops(&whole, &path), "{path}");
        }
    }

    #[test]
    fn runs_close_in_time_read_each_other_when_judged_alone_or_in_a_slice() {
        use features::*;
        // The second run starts 5 s after the first ends, and is much lighter,
        // so the photos around it change what its frames are compared with.
        let times = [0.0, 1000.0, 2000.0, 7000.0, 8000.0, 9000.0, 10_000.0];
        let lights = [0.1, 0.1, 0.1, 0.5, 0.5, 0.5, 0.55];
        let frames: Vec<Frame> = times.iter().enumerate().map(|(i, &t)| frame_at(i, t)).collect();
        let whole = prepare_groups(&frames, &[], ShootKind::Events, |i| Some(lit(lights[i])), |_| None);
        let stops = |groups: &[Vec<Prepared>], path: &str| groups.iter().flatten().find(|p| p.path == path).unwrap().readings[STOPS_VS_NEIGHBOURS];
        let alone = prepare_groups(&frames[3..], &[], ShootKind::Events, |i| Some(lit(lights[3 + i])), |_| None);
        assert_ne!(stops(&alone, "/f/3.ARW"), stops(&whole, "/f/3.ARW"), "the setup must make the photos around matter");
        // (a) the second run, told of the first; (b) a slice of it, told of everything else.
        for (from, to) in [(3usize, 7usize), (3, 5)] {
            let around: Vec<(f64, light::Light)> = (0..7).filter(|i| !(from..to).contains(i)).map(|i| (times[i], lit(lights[i]))).collect();
            let run = prepare_groups(&frames[from..to], &around, ShootKind::Events, |i| Some(lit(lights[from + i])), |_| None);
            for i in from..to {
                let path = format!("/f/{i}.ARW");
                assert_eq!(stops(&run, &path), stops(&whole, &path), "{from}..{to} {path}");
            }
        }
    }

    #[test]
    fn photos_around_are_only_compared_with_never_judged() {
        let frames = vec![frame_at(0, 1000.0)];
        let around = [(0.0, light::Light::neutral()), (2000.0, light::Light::neutral())];
        let groups = prepare_groups(&frames, &around, features::ShootKind::Events, |_| Some(light::Light::neutral()), |_| None);
        assert_eq!(groups.iter().flatten().count(), 1);
        assert_eq!(groups[0][0].path, "/f/0.ARW");
    }

    #[test]
    fn a_frame_whose_light_is_unknown_reads_no_stops_and_is_left_out_of_its_neighbours() {
        use features::*;
        // The middle frame's thumbnail could not be read. Counted as mid-grey,
        // it would pull the first frame's median towards itself.
        let frames: Vec<Frame> = [0.0, 1000.0, 2000.0].iter().enumerate().map(|(i, &t)| frame_at(i, t)).collect();
        let lights = [Some(lit(0.1)), None, Some(lit(0.4))];
        let groups = prepare_groups(&frames, &[], ShootKind::Events, |i| lights[i], |_| None);
        let stops = |groups: &[Vec<Prepared>], path: &str| groups.iter().flatten().find(|p| p.path == path).unwrap().readings[STOPS_VS_NEIGHBOURS];
        assert_eq!(stops(&groups, "/f/1.ARW"), 0.0, "unknown light: no stops reading");
        // The others read exactly as if the unknown frame were not there at all.
        let without = [frames[0].clone(), frames[2].clone()];
        let known = [lit(0.1), lit(0.4)];
        let alone = prepare_groups(&without, &[], ShootKind::Events, |i| Some(known[i]), |_| None);
        for path in ["/f/0.ARW", "/f/2.ARW"] {
            assert_eq!(stops(&groups, path), stops(&alone, path), "{path}");
        }
        // The same with no clock: neighbours by order given.
        let untimed: Vec<Frame> = (0..3).map(|i| frame(&format!("/f/{i}.ARW"), None, 0)).collect();
        let groups = prepare_groups(&untimed, &[], ShootKind::Events, |i| lights[i], |_| None);
        let alone = prepare_groups(&[untimed[0].clone(), untimed[2].clone()], &[], ShootKind::Events, |i| Some(known[i]), |_| None);
        assert_eq!(stops(&groups, "/f/1.ARW"), 0.0);
        for path in ["/f/0.ARW", "/f/2.ARW"] {
            assert_eq!(stops(&groups, path), stops(&alone, path), "untimed {path}");
        }
    }

    #[test]
    fn a_frame_alone_reads_its_exposure_against_the_photos_around_it() {
        use features::*;
        let frames = vec![frame_at(0, 2000.0)];
        let around: Vec<(f64, light::Light)> = [0.0, 1000.0, 3000.0, 4000.0].iter().map(|&t| (t, lit(0.5))).collect();
        let groups = prepare_groups(&frames, &around, ShootKind::Events, |_| Some(lit(0.1)), |_| None);
        assert!(groups[0][0].readings[STOPS_VS_NEIGHBOURS] < -0.6, "{}", groups[0][0].readings[STOPS_VS_NEIGHBOURS]);
    }

    #[test]
    fn a_frame_as_the_interface_sends_it_is_understood() {
        let json = serde_json::json!({
            "path": "/shoot/A.ARW", "version": "123-456",
            "faces": [{
                "x": 0.1, "y": 0.2, "w": 0.05, "h": 0.07, "subject": true, "reason": null,
                "eyesShut": false, "lookingAway": true, "maybeAway": false, "sharpness": 80.0,
                "eye": 0.28, "eyesRead": true, "state": "Looking Down", "concern": 1, "excused": 0.85, "open": false
            }, {
                "x": 0.5, "y": 0.5, "w": 0.1, "h": 0.1, "subject": false, "reason": "Too small",
                "eyesShut": true, "lookingAway": false, "sharpness": null,
                "eye": 0.1, "eyesRead": false, "state": null
            }],
            "score": null, "focus": null, "gaze": null, "key": null,
            "comparedScore": 61.5, "comparedEyes": [0.9, null], "rank": 1, "count": 3, "pose": 2, "rating": 4
        });
        let f: Frame = serde_json::from_value(json).unwrap();
        assert_eq!(f.path, "/shoot/A.ARW");
        assert_eq!(f.record.faces.len(), 2);
        assert!(f.record.faces[0].looking_away && f.record.faces[0].eyes_read && f.record.faces[1].eyes_shut);
        assert_eq!(f.record.faces[0].state.as_deref(), Some("Looking Down"));
        assert_eq!((f.record.score, f.record.focus, f.record.gaze, f.record.key.is_none()), (None, None, None, true));
        assert_eq!(f.compared_score, Some(61.5));
        assert_eq!(f.compared_eyes, vec![Some(0.9), None]);
        assert_eq!((f.rank, f.count, f.pose, f.rating), (Some(1), Some(3), Some(2), 4));
    }

    #[test]
    fn frames_are_grouped_by_pose_and_no_group_is_empty() {
        let frames = vec![frame("/a", Some(1), 5), frame("/b", None, 0), frame("/c", Some(1), 2), frame("/d", Some(7), 0), frame("/e", None, 4)];
        let groups = prepare_groups(&frames, &[], features::ShootKind::Events, |_| Some(light::Light::default()), |_| None);
        let paths: Vec<Vec<&str>> = groups.iter().map(|g| g.iter().map(|p| p.path.as_str()).collect()).collect();
        assert_eq!(paths, vec![vec!["/a", "/c"], vec!["/b"], vec!["/d"], vec!["/e"]]);
        assert!(groups.iter().all(|g| !g.is_empty()));
        assert!(groups.iter().flatten().all(|p| p.x.len() == features::INPUTS));
        assert!(prepare_groups(&[], &[], features::ShootKind::Events, |_| Some(light::Light::default()), |_| None).is_empty());
    }

    #[test]
    fn a_frame_much_darker_than_the_frames_around_it_reads_negative_stops() {
        use features::*;
        let frames: Vec<Frame> = (0..5).map(|i| frame_at(i, i as f64 * 1000.0)).collect();
        let lights = [0.5, 0.5, 0.1, 0.5, 0.5];
        let groups = prepare_groups(&frames, &[], ShootKind::Events, |i| Some(lit(lights[i])), |_| None);
        let dark = groups.iter().flatten().find(|p| p.path == "/f/2.ARW").unwrap();
        assert!(dark.readings[STOPS_VS_NEIGHBOURS] < -0.6, "{}", dark.readings[STOPS_VS_NEIGHBOURS]); // about -2.3 stops / 3
        let ok = groups.iter().flatten().find(|p| p.path == "/f/0.ARW").unwrap();
        assert!(ok.readings[STOPS_VS_NEIGHBOURS].abs() < 0.1);
    }

    /// A light whose decoded (linear) mean is `linear`, with the encoded mean that goes with it.
    fn lit(linear: f32) -> light::Light {
        light::Light { mean: linear.powf(1.0 / 2.2), linear, ..light::Light::neutral() }
    }

    #[test]
    fn stops_are_real_stops_of_light_not_steps_of_encoded_brightness() {
        use features::*;
        let frames: Vec<Frame> = (0..5).map(|i| frame_at(i, i as f64 * 1000.0)).collect();
        // The middle frame got half the light of the others: one stop under.
        let lights = [0.4, 0.4, 0.2, 0.4, 0.4];
        let groups = prepare_groups(&frames, &[], ShootKind::Events, |i| Some(lit(lights[i])), |_| None);
        let dark = groups.iter().flatten().find(|p| p.path == "/f/2.ARW").unwrap();
        assert!((dark.readings[STOPS_VS_NEIGHBOURS] + 1.0 / 3.0).abs() < 0.03, "{}", dark.readings[STOPS_VS_NEIGHBOURS]);
    }

    #[test]
    fn a_frame_a_stop_and_a_half_under_its_neighbours_is_underexposed() {
        use features::*;
        let frames: Vec<Frame> = (0..5).map(|i| frame_at(i, i as f64 * 1000.0)).collect();
        let under = 0.4 / 2f32.powf(1.6);
        let lights = [0.4, 0.4, under, 0.4, 0.4];
        let groups = prepare_groups(&frames, &[], ShootKind::Events, |i| Some(lit(lights[i])), |_| None);
        let dark = groups.iter().flatten().find(|p| p.path == "/f/2.ARW").unwrap();
        assert!(rules::faults(&dark.readings).contains(&rules::Fault::Underexposed), "{}", dark.readings[STOPS_VS_NEIGHBOURS]);
        // A stop under is a different moment, not a misfire.
        let lights = [0.4, 0.4, 0.2, 0.4, 0.4];
        let groups = prepare_groups(&frames, &[], ShootKind::Events, |i| Some(lit(lights[i])), |_| None);
        let dark = groups.iter().flatten().find(|p| p.path == "/f/2.ARW").unwrap();
        assert!(!rules::faults(&dark.readings).contains(&rules::Fault::Underexposed));
    }

    #[test]
    fn a_lone_frame_far_from_every_other_has_no_exposure_jump() {
        use features::*;
        // Four frames seconds apart; the fifth is over two minutes after the last and very dark.
        let frames: Vec<Frame> = [0.0, 1000.0, 2000.0, 3000.0, 200_000.0].iter().enumerate().map(|(i, &t)| frame_at(i, t)).collect();
        let lights = [0.5, 0.5, 0.5, 0.5, 0.05];
        let groups = prepare_groups(&frames, &[], ShootKind::Events, |i| Some(lit(lights[i])), |_| None);
        let last = groups.iter().flatten().find(|p| p.path == "/f/4.ARW").unwrap();
        assert_eq!(last.readings[STOPS_VS_NEIGHBOURS], 0.0);
    }

    #[test]
    fn a_lone_frame_with_few_neighbours_in_time_uses_the_nearest_and_flags_are_read() {
        use features::*;
        // The first four are seconds apart; the fifth is a minute later and alone.
        let mut frames: Vec<Frame> = [0.0, 1000.0, 2000.0, 3000.0, 60_000.0].iter().enumerate().map(|(i, &t)| frame_at(i, t)).collect();
        frames[4].flag = "pick".into();
        frames[3].flag = "reject".into();
        let lights = [0.5, 0.5, 0.5, 0.5, 0.05];
        let groups = prepare_groups(&frames, &[], ShootKind::Portraits, |i| Some(lit(lights[i])), |_| None);
        let last = groups.iter().flatten().find(|p| p.path == "/f/4.ARW").unwrap();
        assert!(last.readings[STOPS_VS_NEIGHBOURS] < -0.6, "{}", last.readings[STOPS_VS_NEIGHBOURS]);
        assert_eq!((last.flag, ShootKind::of(&last.readings)), (1, ShootKind::Portraits));
        let rejected = groups.iter().flatten().find(|p| p.path == "/f/3.ARW").unwrap();
        assert_eq!(rejected.flag, -1);
        assert_eq!(groups.iter().flatten().find(|p| p.path == "/f/0.ARW").unwrap().flag, 0);
    }

    #[test]
    fn frames_without_a_clock_use_the_frames_beside_them() {
        use features::*;
        let frames: Vec<Frame> = (0..5).map(|i| frame(&format!("/f/{i}.ARW"), None, 0)).collect();
        let lights = [0.5, 0.5, 0.1, 0.5, 0.5];
        let groups = prepare_groups(&frames, &[], ShootKind::Events, |i| Some(lit(lights[i])), |_| None);
        let dark = groups.iter().flatten().find(|p| p.path == "/f/2.ARW").unwrap();
        assert!(dark.readings[STOPS_VS_NEIGHBOURS] < -0.6);
        let single = prepare_groups(&frames[..1], &[], ShootKind::Events, |_| Some(light::Light::neutral()), |_| None);
        assert_eq!(single[0][0].readings[STOPS_VS_NEIGHBOURS], 0.0);
    }

    // One frame per burst, so the shoot splits into many bursts. The
    // photographer keeps photos brighter than 0.35 and rejects darker ones,
    // or the opposite.
    fn rated(n: usize, seed: u32, dark_is_bad: bool) -> Vec<Vec<Prepared>> {
        rated_by(n, seed, |_| dark_is_bad)
    }

    // Stars that contradict themselves from one burst to the next, so
    // whatever is learned from half the bursts is wrong on the other half.
    fn contradictory(n: usize, seed: u32) -> Vec<Vec<Prepared>> {
        rated_by(n, seed, |i| i % 2 == 0)
    }

    fn rated_by(n: usize, seed: u32, dark_is_bad: impl Fn(usize) -> bool) -> Vec<Vec<Prepared>> {
        (0..n).map(|i| {
            let dark_is_bad = dark_is_bad(i);
            let light = ((i as u32 * 37 + seed) % 100) as f32 / 100.0;
            let mut r = [0.5; features::READINGS];
            r[features::KIND_PORTRAITS] = 0.0; r[features::KIND_SPORTS] = 0.0; r[features::KIND_EVENTS] = 1.0;
            r[features::STOPS_VS_NEIGHBOURS] = 0.0; r[features::HIGHLIGHTS] = 0.0;
            r[features::MARK] = 0.7;
            r[features::ANY_SHUT] = 0.0;
            r[features::LIGHT_MEAN] = light;
            let rating = if (light >= 0.35) == dark_is_bad { 5 } else { 1 };
            vec![Prepared { path: format!("/{seed}/{i}"), version: "v".into(), readings: r, x: features::inputs(&r, None), rating, flag: 0 }]
        }).collect()
    }

    #[test]
    fn a_photos_light_is_kept_by_path_and_version() {
        let (store, dir) = fresh_store("lights");
        assert_eq!(store.light("/a.ARW", "v1"), None);
        let l = light::Light { linear: 0.3, ..light::Light::neutral() };
        store.keep_light("/a.ARW", "v1", l);
        assert_eq!(store.light("/a.ARW", "v1"), Some(l));
        assert_eq!(store.light("/a.ARW", "v2"), None, "an edited file is read again");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_photo_whose_light_could_not_be_read_is_remembered_by_path_and_version() {
        let (store, dir) = fresh_store("unreadable");
        assert!(!store.light_unreadable("/a.ARW", "v1"));
        store.mark_light_unreadable("/a.ARW", "v1");
        assert!(store.light_unreadable("/a.ARW", "v1"));
        assert!(!store.light_unreadable("/a.ARW", "v2"), "an edited file is tried again");
        assert!(!store.light_unreadable("/b.ARW", "v1"));
        assert_eq!(store.light("/a.ARW", "v1"), None, "kept apart from the lights read");
        std::fs::remove_dir_all(dir).ok();
    }

    fn fresh_store(name: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!("autocull-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        (Store::new(dir.clone()), dir)
    }

    #[test]
    fn a_folder_taught_again_keeps_its_earlier_lessons_when_the_new_ones_are_no_better() {
        let (store, dir) = fresh_store("reteach");
        let first = teach(&store, "/a".into(), &rated(400, 1, true), 100, lessons::Source::Stars).unwrap();
        assert_eq!(first.kept, Kept::Learned, "{first:?}");
        let learned_net = store.model();
        let again = teach(&store, "/a".into(), &contradictory(400, 3), 200, lessons::Source::Stars).unwrap();
        assert!(again.learned.enough && !again.learned.accepted, "{again:?}");
        assert_eq!(again.kept, Kept::Earlier);
        let stored = store.lessons().all();
        assert_eq!(stored.len(), 1);
        assert!(!stored[0].set_aside && stored[0].accepted && stored[0].taught_at == 100, "the earlier lessons are still the ones stored");
        assert_eq!(store.model(), learned_net);
        assert_eq!(store.status().shoots, 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_new_folder_that_is_no_better_is_set_aside_and_not_counted() {
        let (store, dir) = fresh_store("setaside");
        assert_eq!(teach(&store, "/a".into(), &rated(400, 1, true), 100, lessons::Source::Stars).unwrap().kept, Kept::Learned);
        let other = teach(&store, "/b".into(), &contradictory(400, 3), 200, lessons::Source::Stars).unwrap();
        assert_eq!(other.kept, Kept::SetAside, "{other:?}");
        assert!(other.learned.reason.is_some());
        let stored = store.lessons().all();
        assert!(stored.iter().any(|s| s.folder == "/b" && s.set_aside));
        assert_eq!(store.status().shoots, 1, "only the shoot the network learned from is counted");
        std::fs::remove_dir_all(dir).ok();
    }

    fn flagged(groups: Vec<Vec<Prepared>>, f: impl Fn(&Prepared) -> i8) -> Vec<Vec<Prepared>> {
        groups.into_iter().map(|g| g.into_iter().map(|mut p| { p.flag = f(&p); p.rating = 0; p }).collect()).collect()
    }

    #[test]
    fn a_shoot_of_only_picks_has_nothing_to_compare_and_is_not_learned() {
        let (store, dir) = fresh_store("onlypicks");
        let picks = flagged(rated(40, 1, true), |_| 1);
        let err = teach(&store, "/a".into(), &picks, 100, lessons::Source::Flags).unwrap_err();
        assert!(err.starts_with("Nothing to learn yet"), "{err}");
        assert!(store.lessons().all().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_flags_shoot_with_picks_and_rejects_is_taught_and_says_it_learned_from_flags() {
        let (store, dir) = fresh_store("flags");
        let both = flagged(rated(400, 1, true), |p| if p.rating >= 4 { 1 } else { -1 });
        let reply = teach(&store, "/a".into(), &both, 100, lessons::Source::Flags).unwrap();
        assert_eq!(reply.source, lessons::Source::Flags);
        assert!(reply.learned.enough, "{reply:?}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_saved_network_of_another_shape_is_ignored() {
        let dir = std::env::temp_dir().join(format!("autocull-shape-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        let mut wrong_inputs = starter::starting_net();
        wrong_inputs.inputs += 1;
        std::fs::write(dir.join("model.json"), serde_json::to_vec(&serde_json::json!({ "format": features::FORMAT, "net": wrong_inputs })).unwrap()).unwrap();
        let mut short = starter::starting_net();
        short.params.pop();
        std::fs::write(dir.join("starter.json"), serde_json::to_vec(&short).unwrap()).unwrap();
        assert_eq!(store.model(), starter::starting_net(), "both files are the wrong shape, so the starting network is rebuilt");
        let healed: SavedStarter = serde_json::from_slice(&std::fs::read(dir.join("starter.json")).unwrap()).unwrap();
        assert!(healed.current());
        assert_eq!(healed.net, starter::starting_net());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_learned_network_from_another_format_or_unstamped_is_not_used() {
        let (store, dir) = fresh_store("modelformat");
        let mut learned = starter::starting_net();
        learned.red_line = 1.5;
        for bytes in [
            serde_json::to_vec(&serde_json::json!({ "format": features::FORMAT + 1, "net": learned })).unwrap(),
            serde_json::to_vec(&learned).unwrap(), // saved before networks were stamped
        ] {
            let store = Store::new(dir.clone());
            std::fs::write(dir.join("model.json"), bytes).unwrap();
            assert_eq!(store.model(), starter::starting_net());
        }
        store.save_model(&learned).unwrap();
        assert_eq!(Store::new(dir.clone()).model().red_line, 1.5, "one saved by this version is read back");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_network_that_cannot_be_saved_leaves_no_lessons_marked_kept() {
        let (store, dir) = fresh_store("saveorder");
        // A folder where the network's file should be, so it cannot be written.
        std::fs::create_dir_all(dir.join("model.json").join("in-the-way")).unwrap();
        let err = teach(&store, "/a".into(), &rated(400, 1, true), 100, lessons::Source::Stars).unwrap_err();
        assert!(err.starts_with("Couldn't save what OpenCull learned"), "{err}");
        assert!(store.lessons().all().iter().all(|s| !s.accepted), "no lessons may claim a network that was never saved");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_starting_network_saved_by_another_version_is_rebuilt() {
        let dir = std::env::temp_dir().join(format!("autocull-stale-{}", std::process::id()));
        let starter_file = |net: &net::Net, format: u32, version: u32| serde_json::to_vec(&serde_json::json!({ "format": format, "version": version, "net": net })).unwrap();
        let mut stale = starter::starting_net();
        stale.red_line = 9.0; // the right shape, but not what this version would build
        // Saved by an earlier version of the starting rules, or with readings of another meaning.
        for bytes in [
            starter_file(&stale, features::FORMAT, starter::STARTER_VERSION + 1),
            starter_file(&stale, features::FORMAT + 1, starter::STARTER_VERSION),
            serde_json::to_vec(&stale).unwrap(), // saved before files were stamped
        ] {
            std::fs::remove_dir_all(&dir).ok();
            let store = Store::new(dir.clone());
            std::fs::write(dir.join("starter.json"), bytes).unwrap();
            assert_eq!(store.model(), starter::starting_net());
            // ...and the file is replaced by one this version reads back.
            assert_eq!(Store::new(dir.clone()).model(), starter::starting_net());
        }
        // One saved by this version is used as it is.
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        std::fs::write(dir.join("starter.json"), starter_file(&stale, features::FORMAT, starter::STARTER_VERSION)).unwrap();
        assert_eq!(store.model().red_line, 9.0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_save_that_cannot_be_written_says_so() {
        let dir = std::env::temp_dir().join(format!("autocull-unwritable-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        std::fs::remove_dir_all(&dir).unwrap();
        let said = store.set_owned("/f", &HashMap::new()).unwrap_err();
        assert!(said.starts_with("Couldn't remember which colours Auto-label set"), "{said}");
        let said = store.save_model(&starter::starting_net()).unwrap_err();
        assert!(said.starts_with("Couldn't save what OpenCull learned"), "{said}");
    }
}
