//! Ratings, flags and colour labels, kept in sidecar files beside the photos.
//!
//! The same files, in the same form, as the Mac app writes — a folder culled
//! in one opens in the other with every mark in place, and both stay readable
//! by Lightroom, which is where the photos are going next.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::raw::Probe;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flag {
    #[default]
    None,
    Pick,
    Reject,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    #[default]
    None,
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl Color {
    fn label(self) -> Option<&'static str> {
        Some(match self {
            Color::None => return None,
            Color::Red => "Red",
            Color::Yellow => "Yellow",
            Color::Green => "Green",
            Color::Blue => "Blue",
            Color::Purple => "Purple",
        })
    }

    fn parse(label: &str) -> Self {
        let lower = label.to_lowercase();
        if lower.contains("red") {
            Color::Red
        } else if lower.contains("yellow") {
            Color::Yellow
        } else if lower.contains("green") {
            Color::Green
        } else if lower.contains("blue") {
            Color::Blue
        } else if lower.contains("purple") || lower.contains("violet") {
            Color::Purple
        } else {
            Color::None
        }
    }
}

/// What a photographer has decided about one photo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marks {
    pub rating: u8,
    pub flag: Flag,
    pub color: Color,
}

impl Marks {
    pub fn is_empty(&self) -> bool {
        self.rating == 0 && self.flag == Flag::None && self.color == Color::None
    }
}

/// Where a photo's sidecar is, or would be.
///
/// `photo.xmp` normally. `photo.ARW.xmp` only when that one already exists and
/// the plain one does not — some programs write it that way, and the marks in
/// it should be read and updated where they are rather than duplicated.
pub fn sidecar_path(image: &Path) -> PathBuf {
    let standard = image.with_extension("xmp");
    let mut with_ext = image.as_os_str().to_owned();
    with_ext.push(".xmp");
    let with_ext = PathBuf::from(with_ext);
    if with_ext.exists() && !standard.exists() {
        with_ext
    } else {
        standard
    }
}

fn first_match(xml: &str, patterns: &[&'static str]) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    for pattern in patterns {
        let regex = {
            let mut cache = cache.lock().unwrap();
            cache.entry(pattern).or_insert_with(|| Regex::new(pattern).expect("fixed pattern")).clone()
        };
        if let Some(found) = regex.captures(xml).and_then(|c| c.get(1)) {
            return Some(found.as_str().to_string());
        }
    }
    None
}

/// Reads marks out of XMP, in either of the two forms programs write it:
/// as attributes on the description, or as elements inside it.
pub fn parse(xml: &str) -> Marks {
    let mut marks = Marks::default();

    if let Some(v) = first_match(xml, &[r#"xmp:Rating="(-?\d+)""#, r"<xmp:Rating>(-?\d+)</xmp:Rating>"])
        .and_then(|s| s.parse::<i32>().ok())
    {
        // A rating of minus one is how Bridge says "rejected".
        if v == -1 {
            marks.flag = Flag::Reject;
        } else {
            marks.rating = v.clamp(0, 5) as u8;
        }
    }

    if let Some(label) = first_match(xml, &[r#"xmp:Label="([^"]+)""#, r"<xmp:Label>([^<]+)</xmp:Label>"]) {
        marks.color = Color::parse(&label);
    }

    if let Some(v) =
        first_match(xml, &[r#"crs:PickStatus="(-?\d+)""#, r"<crs:PickStatus>(-?\d+)</crs:PickStatus>"])
            .and_then(|s| s.parse::<i32>().ok())
    {
        match v {
            1 => marks.flag = Flag::Pick,
            -1 => marks.flag = Flag::Reject,
            0 if marks.flag != Flag::Reject => marks.flag = Flag::None,
            _ => {}
        }
    } else if first_match(xml, &[r#"photoshop:Urgency="(\d+)""#]).as_deref() == Some("1") {
        marks.flag = Flag::Pick;
    }
    marks
}

/// A photo's marks: from its sidecar if it has one, otherwise from whatever
/// the camera wrote into the file itself.
///
/// A sidecar wins outright, even an empty one. It is the newer statement, and
/// merging the two would bring back a rating the photographer had cleared.
pub fn read(image: &Path, probe: Option<&Probe>) -> Marks {
    read_from(sidecar_text(image).as_deref(), probe)
}

/// What a photo's sidecar holds, under either of its names, if it has one
/// that can be read. Read once and handed to everything that wants it.
pub fn sidecar_text(image: &Path) -> Option<String> {
    let standard = image.with_extension("xmp");
    let mut with_ext = image.as_os_str().to_owned();
    with_ext.push(".xmp");
    [standard, PathBuf::from(with_ext)].iter().find_map(|candidate| std::fs::read_to_string(candidate).ok())
}

/// `read`, given the sidecar's text.
pub fn read_from(sidecar: Option<&str>, probe: Option<&Probe>) -> Marks {
    if let Some(xml) = sidecar {
        return parse(xml);
    }

    let Some(probe) = probe else { return Marks::default() };
    let mut marks = probe.xmp.as_deref().map(parse).unwrap_or_default();
    if marks.rating == 0 {
        if let Some(r) = probe.rating {
            marks.rating = r.clamp(0, 5) as u8;
        }
    }
    marks
}

pub(crate) fn fresh(marks: Marks) -> String {
    let rating = if marks.rating > 0 { format!("\n   xmp:Rating=\"{}\"", marks.rating) } else { String::new() };
    let label = marks.color.label().map(|l| format!("\n   xmp:Label=\"{l}\"")).unwrap_or_default();
    let pick = match marks.flag {
        Flag::Pick => "\n   crs:PickStatus=\"1\"",
        Flag::Reject => "\n   crs:PickStatus=\"-1\"",
        Flag::None => "",
    };
    format!(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 7.0-c000 1.000000, 2022/01/01-00:00:00        ">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"{rating}{label}{pick}>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#
    )
}

/// Changes three values in an existing sidecar and nothing else.
///
/// A sidecar written by Lightroom holds every develop setting for the photo.
/// Rewriting the file from scratch to change a star rating would throw all of
/// that away, so the existing text is edited in place.
pub(crate) fn update(xml: &str, marks: Marks) -> String {
    let mut out = xml.to_string();
    // A value may only be written under a prefix the document has declared.
    // One Lightroom wrote declares both of these; one from a camera, or from
    // another program, may declare neither — and a prefix used without being
    // declared is not XML, which costs a reader the whole document and not
    // just the one value.
    let declared = |out: &mut String, ns: &str, uri: &str| {
        if !out.contains(&format!("xmlns:{ns}=")) {
            if let Some(at) = out.find("<rdf:Description") {
                out.insert_str(at + "<rdf:Description".len(), &format!("\n    xmlns:{ns}=\"{uri}\""));
            }
        }
    };

    // The element form of each is removed first, so a value never ends up
    // stated twice in two forms that then disagree.
    for (ns, name) in [("xmp", "Rating"), ("xmp", "Label"), ("crs", "PickStatus")] {
        let tag = format!("{ns}:{name}");
        for pattern in [format!(r"\n?[ \t]*<{tag}>[^<]*</{tag}>"), format!(r"\n?[ \t]*<{tag}\s*/>")] {
            out = Regex::new(&pattern).expect("built pattern").replace_all(&out, "").into_owned();
        }
    }

    let pick = match marks.flag {
        Flag::Pick => Some("1".to_string()),
        Flag::Reject => Some("-1".to_string()),
        Flag::None => None,
    };
    let values = [
        ("xmp", "Rating", (marks.rating > 0).then(|| marks.rating.to_string())),
        ("xmp", "Label", marks.color.label().map(str::to_string)),
        ("crs", "PickStatus", pick),
    ];
    for (ns, name, value) in values {
        let attribute = Regex::new(&format!(r#"{ns}:{name}="[^"]*""#)).expect("built pattern");
        match value {
            Some(v) => {
                let replacement = format!(r#"{ns}:{name}="{v}""#);
                declared(&mut out, ns, if ns == "xmp" { "http://ns.adobe.com/xap/1.0/" } else { "http://ns.adobe.com/camera-raw-settings/1.0/" });
                if attribute.is_match(&out) {
                    out = attribute.replace_all(&out, regex::NoExpand(&replacement)).into_owned();
                } else if let Some(at) = out.find("<rdf:Description") {
                    out.insert_str(at + "<rdf:Description".len(), &format!("\n   {replacement}"));
                }
            }
            None => {
                let with_space = Regex::new(&format!(r#"\s*{ns}:{name}="[^"]*""#)).expect("built pattern");
                out = with_space.replace_all(&out, "").into_owned();
            }
        }
    }
    out
}

/// Writes a photo's marks to its sidecar.
///
/// A photo with nothing marked and no sidecar gets none. An empty sidecar is
/// not harmless: a reader prefers a sidecar and stops at it, so one that says
/// nothing overrides a rating the camera embedded in the file.
pub fn write(image: &Path, marks: Marks) -> std::io::Result<()> {
    let path = sidecar_path(image);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.is_empty() && marks.is_empty() {
        return Ok(());
    }
    let text = if existing.is_empty() { fresh(marks) } else { update(&existing, marks) };

    // Written beside the target and renamed over it, so a crash or a pulled
    // drive mid-write leaves the old sidecar rather than half of a new one.
    let mut temp = path.as_os_str().to_owned();
    temp.push(".opencull-tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, &path).inspect_err(|_| {
        std::fs::remove_file(&temp).ok();
    })
}

/// Writes sidecars in the background, so marking a photo never waits on a disk.
///
/// Holding a key down to rate a run of frames produces writes faster than a
/// card or a network share completes them. Only the latest marks for each
/// photo are kept: three presses on one photo are one write of the last, not
/// three writes racing each other to be the one that lands.
#[derive(Clone, Default)]
pub struct Writer {
    shared: Arc<(Mutex<WriterState>, Condvar)>,
}

/// What could not be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrote {
    /// Marks, into a JPEG. They are in the sidecar all the same.
    Jpeg,
    /// A crop or a turn, into the sidecar. It is saved nowhere.
    Framing,
    /// A crop, into a JPEG. It is in the sidecar all the same.
    JpegCrop,
}

/// Told when something could not be written: the file, why, and what.
type Failed = Arc<dyn Fn(&Path, &str, Wrote) + Send + Sync>;

/// What is waiting to be written for one photo. Marks and framing are given
/// separately and kept together, so a star pressed while a crop waits is
/// written with it rather than over it.
#[derive(Default)]
struct Change {
    marks: Option<Marks>,
    framing: Option<crate::framing::Wanted>,
    /// The JPEGs of the photo, which keep what is theirs inside themselves.
    into: Vec<PathBuf>,
}

#[derive(Default)]
struct WriterState {
    /// For each photo, the latest of what is to be written for it.
    pending: HashMap<PathBuf, Change>,
    writing: usize,
    started: bool,
    failed: Option<Failed>,
}

impl Writer {
    pub fn enqueue(&self, image: PathBuf, marks: Marks) {
        self.enqueue_with(image, marks, Vec::new());
    }

    /// Has `report` called whenever marks cannot be written into a JPEG, or
    /// a photo's framing cannot be written at all. Marks are in the sidecar
    /// all the same, so nothing of them is lost but other programs seeing
    /// them; a framing that could not be written is lost, and neither should
    /// pass in silence.
    pub fn on_failure(&self, report: impl Fn(&Path, &str, Wrote) + Send + Sync + 'static) {
        self.shared.0.lock().unwrap().failed = Some(Arc::new(report));
    }

    /// Queues marks to be written beside `image`, and into each of `into` —
    /// the JPEGs of the photo, which keep their marks inside themselves.
    pub fn enqueue_with(&self, image: PathBuf, marks: Marks, into: Vec<PathBuf>) {
        self.enqueue_change(image, into, |change| change.marks = Some(marks));
    }

    /// Queues a photo's framing to be written beside `image`, and its crop
    /// into each of `into`.
    pub fn enqueue_framing(&self, image: PathBuf, framing: crate::framing::Wanted, into: Vec<PathBuf>) {
        self.enqueue_change(image, into, |change| change.framing = Some(framing));
    }

    fn enqueue_change(&self, image: PathBuf, into: Vec<PathBuf>, set: impl FnOnce(&mut Change)) {
        let (lock, wake) = &*self.shared;
        let mut state = lock.lock().unwrap();
        let change = state.pending.entry(image).or_default();
        set(change);
        change.into = into;
        if !state.started {
            state.started = true;
            let shared = self.shared.clone();
            std::thread::Builder::new()
                .name("opencull-xmp".into())
                .spawn(move || Self::drain(shared))
                .expect("spawn sidecar writer");
        }
        wake.notify_all();
    }

    fn drain(shared: Arc<(Mutex<WriterState>, Condvar)>) {
        let (lock, wake) = &*shared;
        loop {
            let job = {
                let mut state = lock.lock().unwrap();
                while state.pending.is_empty() {
                    state = wake.wait(state).unwrap();
                }
                let path = state.pending.keys().next().cloned().expect("not empty");
                let change = state.pending.remove(&path).expect("present");
                state.writing += 1;
                (path, change, state.failed.clone())
            };
            let (path, change, failed) = job;
            let report = |file: &Path, why: &str, what: Wrote| {
                if let Some(report) = &failed {
                    report(file, why, what);
                }
            };
            // A framing takes any waiting marks into the sidecar with it, in
            // one write. Marks alone go as they always have.
            let stored = match (&change.framing, change.marks) {
                (Some(framing), marks) => match crate::framing::write(&path, marks, framing) {
                    Ok(stored) => Some(stored),
                    Err(e) => {
                        eprintln!("framing could not be written for {}: {e}", path.display());
                        report(&path, &e.to_string(), Wrote::Framing);
                        None
                    }
                },
                (None, Some(marks)) => {
                    if let Err(e) = write(&path, marks) {
                        eprintln!("sidecar write failed for {}: {e}", path.display());
                    }
                    None
                }
                (None, None) => None,
            };
            for jpeg in &change.into {
                if let Some(marks) = change.marks {
                    if let Err(e) = crate::embed::write(jpeg, marks) {
                        eprintln!("could not write marks into {}: {e}", jpeg.display());
                        report(jpeg, &e.to_string(), Wrote::Jpeg);
                    }
                }
                // Only a crop that reached the sidecar goes into the JPEG:
                // the two must never disagree.
                if let Some(stored) = &stored {
                    if let Err(e) = crate::embed::write_crop(jpeg, stored.as_ref()) {
                        eprintln!("could not write a crop into {}: {e}", jpeg.display());
                        report(jpeg, &e.to_string(), Wrote::JpegCrop);
                    }
                }
            }
            lock.lock().unwrap().writing -= 1;
            wake.notify_all();
        }
    }

    /// Waits until every queued write has reached the disk. Called before the
    /// app closes and before files are moved, so marks are never left behind.
    pub fn flush(&self) {
        let (lock, wake) = &*self.shared;
        let mut state = lock.lock().unwrap();
        while !state.pending.is_empty() || state.writing > 0 {
            state = wake.wait(state).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opencull-xmp-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const LIGHTROOM: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   xmp:Rating="3"
   xmp:Label="Blue"
   crs:Exposure2012="+0.35"
   crs:Contrast2012="+12">
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

    #[test]
    fn reads_both_the_attribute_and_the_element_forms() {
        let m = parse(LIGHTROOM);
        assert_eq!(m, Marks { rating: 3, flag: Flag::None, color: Color::Blue });

        let elements = "<xmp:Rating>5</xmp:Rating><xmp:Label>Red</xmp:Label><crs:PickStatus>1</crs:PickStatus>";
        assert_eq!(parse(elements), Marks { rating: 5, flag: Flag::Pick, color: Color::Red });
    }

    #[test]
    fn a_rating_of_minus_one_is_a_reject() {
        assert_eq!(parse(r#"xmp:Rating="-1""#).flag, Flag::Reject);
        // And a pick status of zero does not un-reject it.
        assert_eq!(parse(r#"xmp:Rating="-1" crs:PickStatus="0""#).flag, Flag::Reject);
    }

    #[test]
    fn what_is_written_reads_back() {
        let dir = scratch("roundtrip");
        let photo = dir.join("A7V00001.ARW");
        for marks in [
            Marks { rating: 5, flag: Flag::Pick, color: Color::Green },
            Marks { rating: 0, flag: Flag::Reject, color: Color::None },
            Marks { rating: 2, flag: Flag::None, color: Color::Purple },
        ] {
            write(&photo, marks).unwrap();
            assert_eq!(read(&photo, None), marks);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn changing_a_rating_leaves_the_develop_settings_alone() {
        let dir = scratch("preserve");
        let photo = dir.join("A7V00002.ARW");
        std::fs::write(dir.join("A7V00002.xmp"), LIGHTROOM).unwrap();

        write(&photo, Marks { rating: 5, flag: Flag::Pick, color: Color::None }).unwrap();
        let text = std::fs::read_to_string(dir.join("A7V00002.xmp")).unwrap();

        assert!(text.contains(r#"crs:Exposure2012="+0.35""#));
        assert!(text.contains(r#"crs:Contrast2012="+12""#));
        assert!(text.contains(r#"xmp:Rating="5""#));
        assert!(text.contains(r#"crs:PickStatus="1""#));
        assert!(!text.contains("xmp:Label"), "a cleared label is removed, not left stale");
        assert_eq!(text.matches("xmp:Rating").count(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn nothing_marked_and_no_sidecar_writes_no_sidecar() {
        let dir = scratch("empty");
        let photo = dir.join("A7V00003.ARW");
        write(&photo, Marks::default()).unwrap();
        assert!(!dir.join("A7V00003.xmp").exists());

        // But clearing marks that were written does update the file.
        write(&photo, Marks { rating: 4, ..Default::default() }).unwrap();
        write(&photo, Marks::default()).unwrap();
        assert!(dir.join("A7V00003.xmp").exists());
        assert_eq!(read(&photo, None), Marks::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_existing_double_extension_sidecar_is_the_one_used() {
        let dir = scratch("doubleext");
        let photo = dir.join("A7V00004.ARW");
        std::fs::write(dir.join("A7V00004.ARW.xmp"), LIGHTROOM).unwrap();

        assert_eq!(read(&photo, None).rating, 3);
        write(&photo, Marks { rating: 1, ..Default::default() }).unwrap();
        assert!(!dir.join("A7V00004.xmp").exists(), "must not create a second sidecar beside it");
        assert_eq!(read(&photo, None).rating, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_sidecar_outranks_what_the_camera_embedded() {
        let dir = scratch("precedence");
        let photo = dir.join("A7V00005.ARW");
        let embedded = Probe { rating: Some(4), ..Default::default() };

        // No sidecar: the camera's rating shows.
        assert_eq!(read(&photo, Some(&embedded)).rating, 4);
        // A sidecar that clears it: the clearing wins.
        write(&photo, Marks { rating: 2, ..Default::default() }).unwrap();
        write(&photo, Marks::default()).unwrap();
        assert_eq!(read(&photo, Some(&embedded)).rating, 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_writer_keeps_only_the_latest_marks_and_flush_waits_for_them() {
        let dir = scratch("writer");
        let photo = dir.join("A7V00006.ARW");
        let writer = Writer::default();
        for rating in 1..=5 {
            writer.enqueue(photo.clone(), Marks { rating, ..Default::default() });
        }
        writer.flush();
        assert_eq!(read(&photo, None).rating, 5);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn marks_for_a_pair_go_beside_the_raw_and_into_its_jpeg() {
        let dir = scratch("pair");
        let (raw, jpeg) = (dir.join("A7V00007.ARW"), dir.join("A7V00007.JPG"));
        std::fs::write(&jpeg, crate::raw::jpeg::testing::jpeg(0xC0, 8, 6000, 4000, &[])).unwrap();

        let writer = Writer::default();
        writer.enqueue_with(raw.clone(), Marks { rating: 4, ..Default::default() }, vec![jpeg.clone()]);
        writer.flush();

        assert_eq!(read(&raw, None).rating, 4, "beside the RAW, where a RAW's marks live");
        let bytes = std::fs::read(&jpeg).unwrap();
        let inside = crate::raw::jpeg::xmp_packet(&bytes.as_slice(), 0, bytes.len() as u64).expect("the JPEG now carries a packet");
        assert_eq!(parse(&inside).rating, 4, "and inside the JPEG, where a JPEG's do");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_jpeg_that_cannot_be_written_into_is_reported_and_the_sidecar_is_written_anyway() {
        let dir = scratch("refused");
        let (raw, not_a_jpeg) = (dir.join("A7V00008.ARW"), dir.join("A7V00008.JPG"));
        std::fs::write(&not_a_jpeg, b"not a photograph").unwrap();

        let told = Arc::new(Mutex::new(Vec::new()));
        let writer = Writer::default();
        let heard = told.clone();
        writer.on_failure(move |path, why, _| heard.lock().unwrap().push((path.to_path_buf(), why.to_string())));
        writer.enqueue_with(raw.clone(), Marks { rating: 2, ..Default::default() }, vec![not_a_jpeg.clone()]);
        writer.flush();

        assert_eq!(read(&raw, None).rating, 2);
        assert_eq!(std::fs::read(&not_a_jpeg).unwrap(), b"not a photograph", "left exactly as it was");
        let told = told.lock().unwrap();
        assert_eq!(told.len(), 1);
        assert_eq!(told[0].0, not_a_jpeg);
        std::fs::remove_dir_all(&dir).ok();
    }
}
