//! Optional MobileCLIP-S0 image embeddings. One shared session, bounded RAM,
//! versioned disk features; model installation never sends photographs out.
//!
//! LICENCE. The model is published under the Apple Machine Learning
//! Research Model License, which covers non-commercial research use; the
//! converted copy downloaded here is a derivative under the same terms. It
//! is never bundled: it is fetched only when the user asks, and the option is
//! off until then. The licence is stated in the docs so users can read it.
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};

pub const MODEL_BYTES: u64 = 45_543_630;
pub const MODEL_SHA: &str = "17d3c037b1d488c10c50e09f6009ea5a198caef4e0e8f4ea5617b7cb2d067ac0";
pub const MODEL_URL: &str = "https://huggingface.co/Xenova/mobileclip_s0/resolve/757d59c9c6870a76a4b0306f05f5061bca15c39f/onnx/vision_model.onnx";
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grouping {
    Loose,
    #[default]
    Normal,
    Tight,
}
impl Grouping {
    pub fn threshold(self) -> f32 {
        match self {
            Self::Loose => 0.70,
            Self::Normal => 0.80,
            Self::Tight => 0.90,
        }
    }
}
const MODEL_FILE: &str = "mobileclip-s0-vision-v1.onnx";
const DIM: usize = 512;
/// Fingerprints kept loaded across folders (about 60 MB); least recently used
/// folders are dropped to stay under it, but the folder in use always stays.
const VECTOR_BUDGET: usize = 30_000;
/// Folder store: this header, then fixed-size records of a 16-byte key and
/// DIM little-endian f32. Records are only ever appended.
const MAGIC: &[u8; 8] = b"OCMCLIP1";
const RECORD: usize = 16 + 4 * DIM;
static MODEL: OnceLock<Mutex<Option<Model>>> = OnceLock::new();
static INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub ready: bool,
    pub bytes: u64,
    pub model: &'static str,
    pub backend: &'static str,
}
pub fn status(root: &Path) -> Status {
    Status {
        ready: fs::metadata(root.join(MODEL_FILE)).is_ok_and(|m| m.len() == MODEL_BYTES),
        bytes: MODEL_BYTES,
        model: "MobileCLIP-S0",
        backend: if super::file_backend::accelerated() {
            "CUDA"
        } else {
            "CPU"
        },
    }
}
fn verified(path: &Path) -> Result<(), String> {
    let bytes = fs::read(path)
        .map_err(|_| "Download the MobileCLIP model in Settings → AI first.".to_string())?;
    if bytes.len() as u64 != MODEL_BYTES || format!("{:x}", Sha256::digest(&bytes)) != MODEL_SHA {
        return Err(
            "The MobileCLIP model is incomplete or damaged. Download it again in Settings → AI."
                .into(),
        );
    }
    Ok(())
}

pub async fn install(root: PathBuf) -> Result<Status, String> {
    use std::sync::atomic::Ordering;
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("The MobileCLIP download is already running.".into());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            INSTALLING.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset;
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let temporary = root.join(format!("model-{}.download", std::process::id()));
    let result = async {
        let client = reqwest::Client::builder()
            .https_only(true)
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|e| e.to_string())?;
        let mut response = client
            .get(MODEL_URL)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let mut file = fs::File::create(&temporary).map_err(|e| e.to_string())?;
        let mut written = 0;
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            written += chunk.len() as u64;
            if written > MODEL_BYTES {
                return Err("The MobileCLIP download exceeds its expected size.".into());
            }
            file.write_all(&chunk).map_err(|e| e.to_string())?;
        }
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        verified(&temporary)?;
        // File is only ever replaced by a complete, checksum-verified model.
        let destination = root.join(MODEL_FILE);
        if verified(&destination).is_ok() {
            fs::remove_file(&temporary).ok();
            return Ok(status(&root));
        }
        // Keep a damaged/old model recoverable if replacing it fails (Windows
        // cannot rename over an existing destination).
        let backup = root.join(format!("{MODEL_FILE}.old-{}", std::process::id()));
        let had_destination = destination.exists();
        if had_destination {
            fs::rename(&destination, &backup).map_err(|e| e.to_string())?;
        }
        if let Err(error) = fs::rename(&temporary, &destination) {
            if had_destination {
                fs::rename(&backup, &destination).ok();
            }
            return Err(error.to_string());
        }
        if had_destination {
            fs::remove_file(backup).ok();
        }
        Ok(status(&root))
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn normalized(mut vector: Vec<f32>) -> Result<Vec<f32>, String> {
    if vector.len() != DIM || vector.iter().any(|x| !x.is_finite()) {
        return Err("Invalid MobileCLIP embedding.".into());
    }
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !norm.is_finite() || norm < 1e-8 {
        return Err("Empty MobileCLIP embedding.".into());
    }
    vector.iter_mut().for_each(|x| *x /= norm);
    Ok(vector)
}
pub fn similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != DIM || b.len() != DIM || a.iter().chain(b).any(|v| !v.is_finite()) {
        return None;
    }
    Some(
        a.iter()
            .zip(b)
            .map(|(x, y)| x * y)
            .sum::<f32>()
            .clamp(-1.0, 1.0),
    )
}
pub fn matching(a: Option<&[f32]>, b: Option<&[f32]>, grouping: Grouping) -> bool {
    a.zip(b)
        .and_then(|(a, b)| similarity(a, b))
        .is_some_and(|s| s > grouping.threshold())
}

/// Same resize-short-edge, centre crop and [0,1] RGB scaling as the model's
/// pinned preprocessor_config.json; no CLIP mean/std normalization.
fn pixels(rgb: &[u8], width: u32, height: u32) -> Result<Vec<f32>, String> {
    use fast_image_resize as fr;
    if width == 0 || height == 0 {
        return Err("No image pixels for MobileCLIP.".into());
    }
    let side = width.min(height);
    let left = (width - side) / 2;
    let top = (height - side) / 2;
    let mut square = Vec::with_capacity(side as usize * side as usize * 3);
    for y in top..top + side {
        let start = ((y * width + left) * 3) as usize;
        square.extend_from_slice(
            rgb.get(start..start + side as usize * 3)
                .ok_or("Truncated MobileCLIP image.")?,
        );
    }
    let source = fr::images::ImageRef::new(side, side, &square, fr::PixelType::U8x3)
        .map_err(|e| e.to_string())?;
    let mut dest = fr::images::Image::new(256, 256, fr::PixelType::U8x3);
    fr::Resizer::new()
        .resize(
            &source,
            &mut dest,
            &fr::ResizeOptions::new()
                .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Bilinear)),
        )
        .map_err(|e| e.to_string())?;
    let image = dest.into_vec();
    let mut tensor = vec![0.0; 3 * 256 * 256];
    for (i, pixel) in image.chunks_exact(3).enumerate() {
        for channel in 0..3 {
            tensor[channel * 256 * 256 + i] = pixel[channel] as f32 / 255.0;
        }
    }
    Ok(tensor)
}

/// One fingerprint's address: the photo's folder and the hash of model,
/// preprocessing, path and version.
#[derive(Clone, Debug)]
struct Key {
    folder: PathBuf,
    id: u128,
}
/// One folder's fingerprints as loaded from its file.
#[derive(Default)]
struct Folder {
    vectors: HashMap<u128, Vec<f32>>,
    /// Length of the header plus whole records on disk; 0 when the file is
    /// missing or its header is wrong.
    good: u64,
    /// The file's tail has been checked this session, so appends go straight on.
    ready: bool,
}
#[cfg(test)]
thread_local! {
    static FOLDER_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
fn folder_file(root: &Path, folder: &Path) -> PathBuf {
    let name = xxhash_rust::xxh3::xxh3_128(folder.to_string_lossy().as_bytes());
    root.join("embeddings")
        .join("folders")
        .join(format!("{name:032x}.bin"))
}
/// Whole records only: a partial one at the end is skipped, a wrong header
/// reads as empty, and a repeated key keeps its later record.
fn read_folder(file: &Path) -> Folder {
    #[cfg(test)]
    FOLDER_READS.with(|n| n.set(n.get() + 1));
    let Ok(bytes) = fs::read(file) else {
        return Folder::default();
    };
    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
        return Folder::default();
    }
    let whole = (bytes.len() - MAGIC.len()) / RECORD;
    let end = MAGIC.len() + whole * RECORD;
    let mut vectors = HashMap::with_capacity(whole);
    for record in bytes[MAGIC.len()..end].chunks_exact(RECORD) {
        let id = u128::from_le_bytes(record[..16].try_into().expect("16-byte key"));
        let vector: Vec<f32> = record[16..]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().expect("4-byte float")))
            .collect();
        // Zero or garbage vectors are left out so they get recomputed.
        let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if vector.iter().all(|x| x.is_finite()) && (norm - 1.0).abs() <= 1e-3 {
            vectors.insert(id, vector);
        }
    }
    Folder {
        vectors,
        good: end as u64,
        ready: false,
    }
}

pub struct Model {
    root: PathBuf,
    session: Option<Session>,
    memory: HashMap<PathBuf, Folder>,
    recent: VecDeque<PathBuf>,
    pub computed: usize,
    pub cached: usize,
}
impl Model {
    fn open(root: &Path) -> Result<Self, String> {
        let path = root.join(MODEL_FILE);
        verified(&path)?;
        Ok(Self {
            root: root.into(),
            session: None,
            memory: HashMap::new(),
            recent: VecDeque::new(),
            computed: 0,
            cached: 0,
        })
    }
    fn key(path: &Path, version: &str) -> Key {
        Key {
            folder: path.parent().unwrap_or(Path::new("")).to_path_buf(),
            id: xxhash_rust::xxh3::xxh3_128(
                format!(
                    "{MODEL_SHA}\0preprocess-v4-screen-fit\0{}\0{version}",
                    path.to_string_lossy()
                )
                .as_bytes(),
            ),
        }
    }
    /// The loaded folder, reading its file on first use. Least recently used
    /// folders are dropped first, before the read, to stay within the budget.
    fn folder(&mut self, folder: &Path) -> &mut Folder {
        self.folder_within(folder, VECTOR_BUDGET)
    }
    fn folder_within(&mut self, folder: &Path, budget: usize) -> &mut Folder {
        if let Some(at) = self.recent.iter().position(|f| f == folder) {
            if let Some(f) = self.recent.remove(at) {
                self.recent.push_back(f);
            }
        } else {
            let file = folder_file(&self.root, folder);
            let incoming = fs::metadata(&file).map_or(0, |m| {
                (m.len() as usize).saturating_sub(MAGIC.len()) / RECORD
            });
            let mut held: usize = self.memory.values().map(|f| f.vectors.len()).sum();
            while held + incoming > budget {
                let Some(old) = self.recent.pop_front() else {
                    break;
                };
                if let Some(gone) = self.memory.remove(&old) {
                    held -= gone.vectors.len();
                }
            }
            self.recent.push_back(folder.into());
            self.memory.insert(folder.into(), read_folder(&file));
        }
        self.memory.get_mut(folder).expect("folder just loaded")
    }
    /// Adds one whole record to the folder's file. The first append of a
    /// session cuts off any torn tail (or rewrites a wrong header) so new
    /// records always start on a record boundary.
    fn append(&mut self, key: &Key, vector: &[f32]) -> Result<(), String> {
        let file = folder_file(&self.root, &key.folder);
        let mut record = Vec::with_capacity(RECORD);
        record.extend_from_slice(&key.id.to_le_bytes());
        vector
            .iter()
            .for_each(|x| record.extend_from_slice(&x.to_le_bytes()));
        let slot = self.folder(&key.folder);
        fs::create_dir_all(file.parent().expect("folder file has parent"))
            .map_err(|e| e.to_string())?;
        let mut out = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&file)
            .map_err(|e| e.to_string())?;
        // Shorter than what we last knew: deleted or cut outside the app.
        // Start again from whatever the file really holds.
        let on_disk = out.metadata().map_err(|e| e.to_string())?.len();
        if on_disk < slot.good {
            *slot = read_folder(&file);
        }
        if !slot.ready {
            if slot.good == 0 {
                out.set_len(0).map_err(|e| e.to_string())?;
                out.write_all(MAGIC).map_err(|e| e.to_string())?;
                slot.good = MAGIC.len() as u64;
            } else {
                out.set_len(slot.good).map_err(|e| e.to_string())?;
            }
        }
        // A failed write may leave a torn tail; recheck before the next one.
        slot.ready = false;
        out.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        out.write_all(&record).map_err(|e| e.to_string())?;
        slot.good += RECORD as u64;
        slot.ready = true;
        slot.vectors.insert(key.id, vector.to_vec());
        Ok(())
    }
    fn cached(&mut self, key: &Key) -> Option<Vec<f32>> {
        if let Some(v) = self.folder(&key.folder).vectors.get(&key.id) {
            let v = v.clone();
            self.cached += 1;
            return Some(v);
        }
        // An older per-photo file: read once, move into the folder file.
        let old = self
            .root
            .join("embeddings")
            .join(format!("{:032x}.json", key.id));
        let v = normalized(serde_json::from_slice(&fs::read(&old).ok()?).ok()?).ok()?;
        if self.append(key, &v).is_ok() {
            fs::remove_file(&old).ok();
        }
        self.cached += 1;
        Some(v)
    }
    fn persist(&mut self, key: Key, vector: Vec<f32>) -> Result<Vec<f32>, String> {
        self.append(&key, &vector)?;
        self.computed += 1;
        if self.computed % 32 == 0 {
            eprintln!(
                "[mobileclip] {} new embeddings saved, {} cache hits",
                self.computed, self.cached
            );
        }
        Ok(vector)
    }
    fn infer(&mut self, key: Key, input: Vec<f32>) -> Result<Vec<f32>, String> {
        let inputs = vec![("pixel_values".to_owned(), vec![1, 3, 256, 256], input)];
        let outputs = vec!["image_embeds".to_owned()];
        let result = if super::file_backend::accelerated() {
            match super::file_backend::run(self.root.join(MODEL_FILE), inputs, outputs.clone()) {
                Ok(reply) => {
                    match reply.result {
                        Ok(out) => out,
                        Err(error) => {
                            eprintln!("[mobileclip] accelerated inference failed; retrying on CPU: {error}");
                            self.infer_cpu(&reply.inputs, &outputs)?
                        }
                    }
                }
                Err(error) => {
                    return Err(format!(
                        "Could not start accelerated MobileCLIP inference: {error}"
                    ))
                }
            }
        } else {
            self.infer_cpu(&inputs, &outputs)?
        };
        let (shape, values) = result
            .into_iter()
            .next()
            .ok_or("Missing MobileCLIP embedding.")?;
        if shape != [1, DIM] {
            return Err("Unexpected MobileCLIP output shape.".into());
        }
        let vector = normalized(values)?;
        self.persist(key, vector)
    }

    fn infer_cpu(
        &mut self,
        inputs: &[(String, Vec<usize>, Vec<f32>)],
        outputs: &[String],
    ) -> Result<Vec<(Vec<usize>, Vec<f32>)>, String> {
        if self.session.is_none() {
            let builder = Session::builder().map_err(|e| e.to_string())?;
            let builder = builder
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|e| e.to_string())?;
            let mut builder = builder.with_intra_threads(2).map_err(|e| e.to_string())?;
            self.session = Some(
                builder
                    .commit_from_file(self.root.join(MODEL_FILE))
                    .map_err(|e| e.to_string())?,
            );
            eprintln!("[mobileclip] CPU model ready (2 inference threads)");
        }
        let session = self.session.as_mut().expect("just opened");
        let given = inputs
            .iter()
            .map(|(name, shape, data)| {
                Tensor::from_array((shape.clone(), data.clone()))
                    .map(|tensor| (name.clone(), tensor))
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let values = session
            .run(given)
            .map_err(|e| format!("MobileCLIP failed on CPU: {e}"))?;
        outputs
            .iter()
            .map(|name| {
                let (shape, data) = values[name.as_str()]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?;
                Ok((
                    shape.iter().map(|dim| *dim as usize).collect(),
                    data.to_vec(),
                ))
            })
            .collect()
    }
    /// Cache before decoding. Four bounded workers prepare misses, then a single
    /// model session performs inference; no full-folder RGB buffers accumulate.
    pub fn embeddings(
        &mut self,
        paths: &[(&Path, &str)],
        cache: &crate::cache::Cache,
        wanted: impl Fn() -> bool + Sync,
    ) -> Result<Vec<Vec<f32>>, String> {
        use rayon::prelude::*;
        static DECODERS: OnceLock<rayon::ThreadPool> = OnceLock::new();
        let pool = DECODERS.get_or_init(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(4)
                .thread_name(|i| format!("mobileclip-decode-{i}"))
                .build()
                .expect("decoder pool")
        });
        let mut out = Vec::with_capacity(paths.len());
        for chunk in paths.chunks(4) {
            if !wanted() {
                return Err("Grouping cancelled.".into());
            }
            let work = chunk
                .iter()
                .map(|(path, version)| {
                    let key = Self::key(path, version);
                    let found = self.cached(&key);
                    (key, *path, *version, found)
                })
                .collect::<Vec<_>>();
            let prepared = pool.install(|| {
                work.into_par_iter()
                    .map(|(key, path, version, found)| {
                        if let Some(v) = found {
                            return Ok((key, Some(v), None));
                        }
                        if !wanted() {
                            return Err("Grouping cancelled.".to_string());
                        }
                        let image = crate::preview::grouping_pixels(path, version, cache)?;
                        Ok((
                            key,
                            None,
                            Some(pixels(&image.rgb, image.width, image.height)?),
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()
            })?;
            for (key, found, input) in prepared {
                if !wanted() {
                    return Err("Grouping cancelled.".into());
                }
                out.push(if let Some(v) = found {
                    v
                } else {
                    self.infer(key, input.unwrap())?
                });
            }
        }
        Ok(out)
    }
    #[cfg(test)]
    pub fn embedding(&mut self, path: &Path, version: &str) -> Result<Vec<f32>, String> {
        let key = Self::key(path, version);
        if let Some(v) = self.cached(&key) {
            return Ok(v);
        }
        let image = crate::preview::decode_upright(path, 512)?;
        self.infer(key, pixels(&image.rgb, image.width, image.height)?)
    }
}
pub fn with_model<T>(
    root: &Path,
    f: impl FnOnce(&mut Model) -> Result<T, String>,
) -> Result<T, String> {
    let mut slot = MODEL
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|e| e.to_string())?;
    if slot.as_ref().is_none_or(|m| m.root != root) {
        *slot = Some(Model::open(root)?);
    }
    f(slot.as_mut().unwrap())
}
pub fn clear(root: &Path) {
    if let Ok(mut slot) = MODEL.get_or_init(|| Mutex::new(None)).lock() {
        if let Some(model) = slot.as_mut().filter(|m| m.root == root) {
            model.memory.clear();
            model.recent.clear();
        }
        let _ = fs::remove_dir_all(root.join("embeddings"));
    }
}
pub fn cache_bytes(root: &Path) -> u64 {
    fn tree(dir: &Path) -> u64 {
        fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| Some((e.path(), e.metadata().ok()?)))
            .map(|(path, meta)| {
                if meta.is_dir() {
                    tree(&path)
                } else {
                    meta.len()
                }
            })
            .sum()
    }
    tree(&root.join("embeddings"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_features_cannot_join_a_group() {
        assert!(!matching(None, None, Grouping::Normal));
        assert!(!matching(
            Some(&[0.0; 4]),
            Some(&[0.0; 4]),
            Grouping::Normal
        ));
        assert!(normalized(vec![0.0; DIM]).is_err());
        let mut a = vec![0.0; DIM];
        a[0] = 1.0;
        let mut b = vec![0.0; DIM];
        b[1] = 1.0;
        assert!(matching(Some(&a), Some(&a), Grouping::Normal));
        assert!(!matching(Some(&a), Some(&b), Grouping::Normal));
        a[0] = f32::NAN;
        assert!(!matching(Some(&a), Some(&b), Grouping::Normal));
    }
    #[test]
    fn grouping_presets_use_strict_similarity_cutoffs() {
        assert_eq!(
            [
                Grouping::Loose.threshold(),
                Grouping::Normal.threshold(),
                Grouping::Tight.threshold()
            ],
            [0.70, 0.80, 0.90]
        );
        let mut a = vec![0.; DIM];
        a[0] = 1.;
        for mode in [Grouping::Loose, Grouping::Normal, Grouping::Tight] {
            let threshold = mode.threshold();
            let vector = |value: f32| {
                let mut v = vec![0.; DIM];
                v[0] = value;
                v[1] = (1. - value * value).sqrt();
                v
            };
            assert!(!matching(Some(&a), Some(&vector(threshold)), mode));
            assert!(!matching(Some(&a), Some(&vector(threshold - 0.001)), mode));
            assert!(matching(Some(&a), Some(&vector(threshold + 0.001)), mode));
        }
        let mut b = vec![0f32; DIM];
        b[0] = 0.88690656;
        b[1] = (1. - b[0] * b[0]).sqrt();
        assert!(matching(Some(&a), Some(&b), Grouping::Loose));
        assert!(matching(Some(&a), Some(&b), Grouping::Normal));
        assert!(!matching(Some(&a), Some(&b), Grouping::Tight));
    }
    #[test]
    fn preprocessing_centre_crops_scales_rgb_and_keeps_channels_in_order() {
        let mut rgb = vec![0u8; 512 * 256 * 3];
        for y in 0..256 {
            for x in 128..384 {
                rgb[(y * 512 + x) * 3..][..3].copy_from_slice(&[255, 128, 0]);
            }
        }
        let p = pixels(&rgb, 512, 256).unwrap();
        assert_eq!(p[0], 1.0);
        assert!((p[256 * 256] - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(p[2 * 256 * 256], 0.0);
        assert!(pixels(&[], 20, 20).is_err());
    }
    #[test]
    fn embeddings_survive_model_recreation_and_are_read_without_inference() {
        let root = std::env::temp_dir().join(format!(
            "opencull-mobileclip-disk-cache-{}",
            std::process::id()
        ));
        fs::remove_dir_all(&root).ok();
        let key = Model::key(Path::new("/photos/frame.jpg"), "version-1");
        let vector = {
            let mut v = vec![0.0; DIM];
            v[4] = 1.0;
            v
        };
        let mut first = Model {
            root: root.clone(),
            session: None,
            memory: HashMap::new(),
            recent: VecDeque::new(),
            computed: 0,
            cached: 0,
        };
        assert_eq!(first.persist(key.clone(), vector.clone()).unwrap(), vector);
        assert_eq!(first.computed, 1);
        drop(first);

        let mut reopened = Model {
            root: root.clone(),
            session: None,
            memory: HashMap::new(),
            recent: VecDeque::new(),
            computed: 0,
            cached: 0,
        };
        assert_eq!(reopened.cached(&key), Some(vector));
        assert_eq!(
            reopened.computed, 0,
            "restoring an embedding must not run ONNX again"
        );
        assert_eq!(reopened.cached, 1);
        fs::remove_dir_all(root).ok();
    }
    fn key_hex(key: &Key) -> String {
        format!("{:032x}", key.id)
    }
    const RECORD: u64 = 16 + 4 * DIM as u64;
    fn blank(root: &Path) -> Model {
        Model {
            root: root.into(),
            session: None,
            memory: HashMap::new(),
            recent: VecDeque::new(),
            computed: 0,
            cached: 0,
        }
    }
    fn scratch(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("opencull-mobileclip-{name}-{}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        root
    }
    fn unit(i: usize) -> Vec<f32> {
        let mut v = vec![0.0; DIM];
        v[i] = 1.0;
        v
    }
    fn folder_files(root: &Path) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = fs::read_dir(root.join("embeddings").join("folders"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        files.sort();
        files
    }
    fn json_files(root: &Path) -> usize {
        fs::read_dir(root.join("embeddings"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .count()
    }
    #[test]
    fn fingerprints_for_one_folder_live_in_one_file_and_survive_recreation() {
        let root = scratch("pack-one");
        let keys: Vec<_> = (0..3)
            .map(|i| Model::key(Path::new(&format!("/photos/a/{i}.jpg")), "v1"))
            .collect();
        let other = Model::key(Path::new("/photos/b/0.jpg"), "v1");
        let mut first = blank(&root);
        for (i, key) in keys.iter().enumerate() {
            first.persist(key.clone(), unit(i)).unwrap();
        }
        first.persist(other.clone(), unit(9)).unwrap();
        drop(first);
        let files = folder_files(&root);
        assert_eq!(files.len(), 2, "one file per folder, not per photo");
        assert_eq!(json_files(&root), 0);
        let mut sizes: Vec<u64> = files
            .iter()
            .map(|f| fs::metadata(f).unwrap().len())
            .collect();
        sizes.sort();
        assert_eq!(sizes, [8 + RECORD, 8 + 3 * RECORD]);
        assert_eq!(&fs::read(&files[0]).unwrap()[..8], b"OCMCLIP1");
        let mut reopened = blank(&root);
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(reopened.cached(key), Some(unit(i)));
        }
        assert_eq!(reopened.cached(&other), Some(unit(9)));
        assert_eq!(reopened.computed, 0);
        fs::remove_dir_all(root).ok();
    }
    #[test]
    fn new_photos_only_append_and_old_records_still_read() {
        let root = scratch("pack-append");
        let key = |i: usize| Model::key(Path::new(&format!("/photos/a/{i}.jpg")), "v1");
        let mut first = blank(&root);
        first.persist(key(0), unit(0)).unwrap();
        first.persist(key(1), unit(1)).unwrap();
        drop(first);
        let file = folder_files(&root).remove(0);
        let before = fs::read(&file).unwrap();
        assert_eq!(before.len() as u64, 8 + 2 * RECORD);
        let mut second = blank(&root);
        second.persist(key(2), unit(2)).unwrap();
        drop(second);
        let after = fs::read(&file).unwrap();
        assert_eq!(after.len() as u64, 8 + 3 * RECORD);
        assert_eq!(&after[..before.len()], &before[..], "nothing rewritten");
        let mut third = blank(&root);
        for i in 0..3 {
            assert_eq!(third.cached(&key(i)), Some(unit(i)));
        }
        assert_eq!(folder_files(&root).len(), 1);
        fs::remove_dir_all(root).ok();
    }
    #[test]
    fn a_later_record_for_the_same_key_wins() {
        let root = scratch("pack-dup");
        let key = Model::key(Path::new("/photos/a/0.jpg"), "v1");
        let mut first = blank(&root);
        first.persist(key.clone(), unit(1)).unwrap();
        first.persist(key.clone(), unit(2)).unwrap();
        drop(first);
        assert_eq!(blank(&root).cached(&key), Some(unit(2)));
        fs::remove_dir_all(root).ok();
    }
    #[test]
    fn a_torn_last_record_is_skipped_and_the_next_append_lands_cleanly() {
        let root = scratch("pack-torn");
        let key = |i: usize| Model::key(Path::new(&format!("/photos/a/{i}.jpg")), "v1");
        let mut first = blank(&root);
        for i in 0..3 {
            first.persist(key(i), unit(i)).unwrap();
        }
        drop(first);
        let file = folder_files(&root).remove(0);
        fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_len(8 + 3 * RECORD - 100)
            .unwrap();
        let mut second = blank(&root);
        assert_eq!(second.cached(&key(0)), Some(unit(0)));
        assert_eq!(second.cached(&key(1)), Some(unit(1)));
        assert_eq!(second.cached(&key(2)), None);
        second.persist(key(3), unit(3)).unwrap();
        drop(second);
        assert_eq!(fs::metadata(&file).unwrap().len(), 8 + 3 * RECORD);
        let mut third = blank(&root);
        for i in [0, 1, 3] {
            assert_eq!(third.cached(&key(i)), Some(unit(i)));
        }
        assert_eq!(third.cached(&key(2)), None);
        fs::remove_dir_all(root).ok();
    }
    #[test]
    fn a_wrong_header_reads_as_empty_and_is_rewritten() {
        let root = scratch("pack-header");
        let key = |i: usize| Model::key(Path::new(&format!("/photos/a/{i}.jpg")), "v1");
        let mut first = blank(&root);
        first.persist(key(0), unit(0)).unwrap();
        first.persist(key(1), unit(1)).unwrap();
        drop(first);
        let file = folder_files(&root).remove(0);
        let mut bytes = fs::read(&file).unwrap();
        bytes[..8].copy_from_slice(b"NOTMYFMT");
        fs::write(&file, bytes).unwrap();
        let mut second = blank(&root);
        assert_eq!(second.cached(&key(0)), None);
        assert_eq!(second.cached(&key(1)), None);
        second.persist(key(2), unit(2)).unwrap();
        drop(second);
        let bytes = fs::read(&file).unwrap();
        assert_eq!(&bytes[..8], b"OCMCLIP1");
        assert_eq!(bytes.len() as u64, 8 + RECORD);
        let mut third = blank(&root);
        assert_eq!(third.cached(&key(2)), Some(unit(2)));
        assert_eq!(third.cached(&key(0)), None);
        fs::remove_dir_all(root).ok();
    }
    #[test]
    fn an_old_per_photo_json_file_is_moved_into_the_folder_file() {
        let root = scratch("pack-legacy");
        let key = Model::key(Path::new("/photos/a/0.jpg"), "v1");
        let old = root
            .join("embeddings")
            .join(format!("{}.json", key_hex(&key)));
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        let mut raw = vec![0.0f32; DIM];
        raw[3] = 2.0;
        fs::write(&old, serde_json::to_vec(&raw).unwrap()).unwrap();
        let mut first = blank(&root);
        assert_eq!(first.cached(&key), Some(unit(3)), "read normalised");
        assert!(!old.exists(), "old file deleted once moved");
        assert_eq!((first.cached, first.computed), (1, 0));
        drop(first);
        let files = folder_files(&root);
        assert_eq!(files.len(), 1);
        assert_eq!(fs::metadata(&files[0]).unwrap().len(), 8 + RECORD);
        let mut second = blank(&root);
        assert_eq!(second.cached(&key), Some(unit(3)));
        assert_eq!(fs::metadata(&files[0]).unwrap().len(), 8 + RECORD);
        fs::remove_dir_all(root).ok();
    }
    #[test]
    fn clear_removes_everything_and_cache_bytes_counts_old_and_new_files() {
        let root = scratch("pack-clear");
        let key = Model::key(Path::new("/photos/a/0.jpg"), "v1");
        let mut model = blank(&root);
        model.persist(key.clone(), unit(0)).unwrap();
        fs::write(root.join("embeddings").join("0123.json"), [b'x'; 10]).unwrap();
        assert_eq!(cache_bytes(&root), 8 + RECORD + 10);
        clear(&root);
        assert!(!root.join("embeddings").exists());
        assert_eq!(cache_bytes(&root), 0);
        fs::remove_dir_all(root).ok();
    }
    fn reads() -> usize {
        FOLDER_READS.with(|n| n.get())
    }
    #[test]
    fn interleaved_photos_from_many_folders_read_each_folder_file_once() {
        let root = scratch("pack-thrash");
        let key = |f: usize, i: usize| Model::key(Path::new(&format!("/photos/{f}/{i}.jpg")), "v1");
        let mut first = blank(&root);
        for f in 0..6 {
            for i in 0..3 {
                first.persist(key(f, i), unit(f * 3 + i)).unwrap();
            }
        }
        drop(first);
        let mut model = blank(&root);
        let before = reads();
        for i in 0..3 {
            for f in 0..6 {
                assert_eq!(model.cached(&key(f, i)), Some(unit(f * 3 + i)));
            }
        }
        assert_eq!(reads() - before, 6, "one read per folder, not per photo");
        for i in 3..6 {
            for f in 0..6 {
                assert_eq!(model.cached(&key(f, i)), None);
                model.persist(key(f, i), unit(100 + f * 3 + i)).unwrap();
            }
        }
        assert_eq!(reads() - before, 6, "misses and appends do not reload");
        fs::remove_dir_all(root).ok();
    }
    #[test]
    fn the_vector_budget_drops_the_oldest_folders_before_reading_and_keeps_the_current() {
        let root = scratch("pack-budget");
        let mut first = blank(&root);
        for (f, count) in [2usize, 2, 2, 5].into_iter().enumerate() {
            for i in 0..count {
                let key = Model::key(Path::new(&format!("/photos/{f}/{i}.jpg")), "v1");
                first.persist(key, unit(f * 5 + i)).unwrap();
            }
        }
        drop(first);
        let folder = |f: usize| PathBuf::from(format!("/photos/{f}"));
        let mut model = blank(&root);
        model.folder_within(&folder(0), 5);
        model.folder_within(&folder(1), 5);
        assert_eq!(model.memory.len(), 2);
        model.folder_within(&folder(0), 5);
        model.folder_within(&folder(2), 5);
        let mut kept: Vec<_> = model.memory.keys().cloned().collect();
        kept.sort();
        assert_eq!(kept, [folder(0), folder(2)], "least recently used dropped");
        model.folder_within(&folder(3), 3);
        let kept: Vec<_> = model.memory.keys().cloned().collect();
        assert_eq!(
            kept,
            [folder(3)],
            "the folder in use stays even over budget"
        );
        assert_eq!(model.memory[&folder(3)].vectors.len(), 5);
        fs::remove_dir_all(root).ok();
    }
    #[test]
    fn a_file_deleted_or_cut_outside_the_app_is_repaired_on_the_next_append() {
        let root = scratch("pack-outside");
        let key = |i: usize| Model::key(Path::new(&format!("/photos/a/{i}.jpg")), "v1");
        let mut model = blank(&root);
        model.persist(key(0), unit(0)).unwrap();
        model.persist(key(1), unit(1)).unwrap();
        let file = folder_files(&root).remove(0);
        fs::remove_file(&file).unwrap();
        model.persist(key(2), unit(2)).unwrap();
        let bytes = fs::read(&file).unwrap();
        assert_eq!(&bytes[..8], b"OCMCLIP1");
        assert_eq!(bytes.len() as u64, 8 + RECORD);
        fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_len(4)
            .unwrap();
        model.persist(key(3), unit(3)).unwrap();
        drop(model);
        let mut reopened = blank(&root);
        assert_eq!(reopened.cached(&key(3)), Some(unit(3)));
        assert_eq!(reopened.cached(&key(2)), None);
        assert_eq!(fs::metadata(&file).unwrap().len(), 8 + RECORD);
        fs::remove_dir_all(root).ok();
    }
    #[test]
    fn zero_or_unnormalised_records_are_not_returned() {
        let root = scratch("pack-norm");
        let key = |i: usize| Model::key(Path::new(&format!("/photos/a/{i}.jpg")), "v1");
        let mut first = blank(&root);
        first.persist(key(0), vec![0.0; DIM]).unwrap();
        let mut big = vec![0.0; DIM];
        big[0] = 3.0;
        first.persist(key(1), big).unwrap();
        first.persist(key(2), unit(2)).unwrap();
        drop(first);
        let mut second = blank(&root);
        assert_eq!(second.cached(&key(0)), None);
        assert_eq!(second.cached(&key(1)), None);
        assert_eq!(second.cached(&key(2)), Some(unit(2)));
        fs::remove_dir_all(root).ok();
    }
    #[test]
    fn grouping_cancellation_stops_before_decoding_or_initializing_onnx() {
        let root =
            std::env::temp_dir().join(format!("opencull-mobileclip-cancel-{}", std::process::id()));
        let cache = crate::cache::Cache::new(root.join("previews"));
        let missing = root.join("does-not-exist.jpg");
        let mut model = Model {
            root: root.clone(),
            session: None,
            memory: HashMap::new(),
            recent: VecDeque::new(),
            computed: 0,
            cached: 0,
        };
        let result = model.embeddings(&[(&missing, "v1")], &cache, || false);
        assert_eq!(result.unwrap_err(), "Grouping cancelled.");
        assert!(model.session.is_none());
        assert_eq!(model.computed, 0);
        fs::remove_dir_all(root).ok();
    }
    #[test]
    #[ignore = "requires downloaded MobileCLIP model and two local reference photos"]
    fn real_model_separates_reference_photos_and_reuses_disk_features() {
        let root =
            std::env::temp_dir().join(format!("opencull-mobileclip-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::copy(
            std::env::var("OPENCULL_MOBILECLIP_TEST_MODEL").unwrap(),
            root.join(MODEL_FILE),
        )
        .unwrap();
        let a = PathBuf::from(std::env::var("OPENCULL_GROUPING_TEST_A").unwrap());
        let b = PathBuf::from(std::env::var("OPENCULL_GROUPING_TEST_B").unwrap());
        let mut model = Model::open(&root).unwrap();
        let start = std::time::Instant::now();
        let cache = crate::cache::Cache::new(root.join("previews"));
        let visual = model
            .embeddings(&[(&a, "test-v1"), (&b, "test-v1")], &cache, || true)
            .unwrap();
        let (av, bv) = (&visual[0], &visual[1]);
        eprintln!(
            "two real embeddings: {:?}; similarity {:?}",
            start.elapsed(),
            similarity(&av, &bv)
        );
        assert!(!matching(Some(&av), Some(&bv), Grouping::Tight));
        assert_eq!(model.computed, 2);
        eprintln!(
            "MobileCLIP inference provider: {}",
            if super::super::file_backend::accelerated() {
                "registered accelerator"
            } else {
                "CPU"
            }
        );
        drop(model);
        let mut reopened = Model::open(&root).unwrap();
        let start = std::time::Instant::now();
        assert!(reopened
            .embedding(&a, "test-v1")
            .unwrap()
            .iter()
            .zip(av)
            .all(|(a, b)| (a - b).abs() < 1e-6));
        eprintln!("reopened disk cache: {:?}", start.elapsed());
        assert_eq!(reopened.computed, 0);
        assert_eq!(reopened.cached, 1);
        fs::remove_dir_all(root).unwrap();
    }
}
