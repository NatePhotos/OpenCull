//! Finding the pictures a camera already put inside its RAW files.
//!
//! No RAW is decoded here. A camera writes finished JPEGs into every RAW it
//! saves — usually a thumbnail, a screen-sized preview and a full-size one —
//! and says where they are in a directory at the front of the file. Reading
//! that directory and lifting a preview out takes well under a millisecond;
//! developing the sensor data takes seconds and needs a different decoder for
//! every body. So the whole pipeline is built on asking the file where its
//! previews are, and on telling a real one from the things that only look
//! like one.

pub mod bmff;
pub mod io;
pub mod jpeg;
pub mod tiff;

use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;

use io::{FileSource, ReadAt};
use tiff::Tiff;

/// File extensions treated as RAW. Matches the list the Mac app opens.
pub const RAW_EXTENSIONS: &[&str] =
    &["raw", "cr2", "cr3", "arw", "nef", "raf", "dng", "rw2", "orf", "pef", "srw"];

pub const JPEG_EXTENSIONS: &[&str] = &["jpg", "jpeg"];

pub fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

pub fn is_raw(path: &Path) -> bool {
    RAW_EXTENSIONS.contains(&extension(path).as_str())
}

pub fn is_jpeg(path: &Path) -> bool {
    JPEG_EXTENSIONS.contains(&extension(path).as_str())
}

/// One JPEG inside a file: where it is and how big a picture it holds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Embedded {
    pub offset: u64,
    pub len: u64,
    pub width: u32,
    pub height: u32,
}

impl Embedded {
    pub fn long_edge(&self) -> u32 {
        self.width.max(self.height)
    }
}

/// What the camera recorded about a frame. Numbers are left as numbers — how
/// to write "1/250s" is a decision for whatever is showing it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exif {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub focal_length_35mm: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter: Option<f64>,
    pub iso: Option<u32>,
    pub exposure_comp: Option<f64>,
    /// `YYYY:MM:DD HH:MM:SS`, exactly as the camera wrote it. A camera clock
    /// has no time zone, so this is deliberately never turned into an instant.
    pub date: Option<String>,
    /// Milliseconds on the camera's own clock, including the sub-second part.
    /// Only ever compared with another frame's — burst grouping is the gap
    /// between two of these — so it needs no time zone either.
    pub capture_ms: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// Everything learned from one look at a file.
#[derive(Clone, Debug, Default)]
pub struct Probe {
    /// Every real picture found inside, smallest first.
    pub embedded: Vec<Embedded>,
    /// Which way up the photograph is, 1–8, by the EXIF convention.
    pub orientation: u8,
    pub exif: Exif,
    /// A star rating the camera or another program wrote into the file itself.
    pub rating: Option<i32>,
    /// The XMP packet stored inside the file, if any.
    pub xmp: Option<String>,
}

impl Probe {
    /// The smallest embedded picture at least `long_edge` across, or the
    /// largest there is when none reaches it.
    ///
    /// Smallest that will do, because decoding is the cost and it scales with
    /// the pixels decoded, not the pixels wanted: a filmstrip thumbnail cut
    /// from the screen-sized preview is ready in a fraction of the time one
    /// cut from the 33-megapixel frame takes, and looks the same.
    pub fn pick(&self, long_edge: u32) -> Option<Embedded> {
        self.embedded
            .iter()
            .find(|e| e.long_edge() >= long_edge)
            .or(self.embedded.last())
            .copied()
    }

    pub fn largest(&self) -> Option<Embedded> {
        self.embedded.last().copied()
    }
}

/// Which kind of directory a TIFF offset leads to. They share a format and
/// reuse tag numbers for unrelated things, so a tag only means something once
/// it is known which of these it was found in.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    /// Describes an image: the photograph, a preview, a thumbnail.
    Image,
    /// Exposure settings.
    Exif,
    Gps,
}

/// Gathers what a walk over a file's directories turns up.
#[derive(Default)]
struct Found {
    ranges: Vec<(u64, u64)>,
    orientation: Option<u8>,
    exif: Exif,
    rating: Option<i32>,
    xmp: Option<String>,
    /// Dimensions the EXIF block states for the finished photograph.
    stated: (Option<u32>, Option<u32>),
    /// The largest image any directory describes, and whether it was marked as
    /// the full-resolution one rather than a reduced copy.
    largest: Option<(u32, u32, bool)>,
    subsec: Option<String>,
    fallback_date: Option<String>,
    iso_wide: Option<u32>,
    lat: (Option<f64>, Option<String>),
    lon: (Option<f64>, Option<String>),
}

/// How many directories one file may lead through. A healthy RAW has under a
/// dozen; this only stops a corrupt one sending the walk round in circles.
const MAX_DIRECTORIES: usize = 64;

/// Walks every directory reachable from `start`, collecting as it goes.
fn walk(tiff: &Tiff, start: u64, role: Role, found: &mut Found) {
    let mut pending = vec![(start, role)];
    let mut visited = HashSet::new();

    while let Some((offset, role)) = pending.pop() {
        if visited.len() >= MAX_DIRECTORIES || !visited.insert(offset) {
            continue;
        }
        let Some((entries, next)) = tiff.ifd(offset) else { continue };

        match role {
            Role::Image => {
                read_image_directory(tiff, &entries, found, &mut pending);
                // Image directories chain: the photograph, then its thumbnail.
                if next != 0 {
                    pending.push((next, Role::Image));
                }
            }
            Role::Exif => read_exif_directory(tiff, &entries, found),
            Role::Gps => read_gps_directory(tiff, &entries, found),
        }
    }
}

fn read_image_directory(
    tiff: &Tiff,
    entries: &[tiff::Entry],
    found: &mut Found,
    pending: &mut Vec<(u64, Role)>,
) {
    let mut strip = (None, None);
    let mut interchange = (None, None);
    let mut strips = 0usize;
    let mut compression = 0u64;
    let mut reduced = false;
    let mut size = (None, None);

    for e in entries {
        match e.tag {
            0x00FE => reduced = tiff.int(e).unwrap_or(0) & 1 == 1,
            0x0100 => size.0 = tiff.int(e).map(|v| v as u32),
            0x0101 => size.1 = tiff.int(e).map(|v| v as u32),
            0x0103 => compression = tiff.int(e).unwrap_or(0),
            0x010F => found.exif.make = found.exif.make.take().or_else(|| tiff.text(e)),
            0x0110 => found.exif.model = found.exif.model.take().or_else(|| tiff.text(e)),
            0x0111 => {
                strips = e.count as usize;
                strip.0 = tiff.int(e);
            }
            0x0117 => strip.1 = tiff.int(e),
            // The first orientation met is the photograph's. Later directories
            // describe thumbnails, which some cameras store already rotated.
            0x0112 => {
                if found.orientation.is_none() {
                    found.orientation =
                        tiff.int(e).map(|v| v as u8).filter(|v| (1..=8).contains(v));
                }
            }
            0x0132 => found.fallback_date = found.fallback_date.take().or_else(|| tiff.text(e)),
            0x014A => {
                for sub in tiff.ints(e, 8) {
                    pending.push((sub, Role::Image));
                }
            }
            0x0201 => interchange.0 = tiff.int(e),
            0x0202 => interchange.1 = tiff.int(e),
            // Panasonic keeps its preview as the tag's own value rather than
            // pointing at it, so the "offset" is where the value begins. Newer
            // bodies (the S5 II, say) add a full-size one under 0x0127.
            0x002E | 0x0127 if e.count > 1024 => found.ranges.push((e.data, e.count as u64)),
            0x02BC => {
                if found.xmp.is_none() {
                    found.xmp = tiff
                        .bytes(e, 512 * 1024)
                        .map(|b| String::from_utf8_lossy(&b).into_owned());
                }
            }
            0x4746 => found.rating = found.rating.or(tiff.int(e).map(|v| v as i32)),
            0x8769 => pending.extend(tiff.int(e).map(|o| (o, Role::Exif))),
            0x8825 => pending.extend(tiff.int(e).map(|o| (o, Role::Gps))),
            _ => {}
        }
    }

    if let (Some(w), Some(h)) = size {
        let better = match found.largest {
            None => true,
            // A directory marked as the real image beats any reduced copy;
            // between two of the same kind, the bigger one is the photograph.
            Some((lw, lh, was_full)) => {
                let full = !reduced;
                (full && !was_full)
                    || (full == was_full && (w as u64 * h as u64) > (lw as u64 * lh as u64))
            }
        };
        if better {
            found.largest = Some((w, h, !reduced));
        }
    }

    // Stated outright as a JPEG.
    if let (Some(at), Some(len)) = interchange {
        found.ranges.push((tiff.base() + at, len));
    }
    // Or stored as the directory's image data. Only when it says the data is
    // JPEG, and in one piece: skip this test and the sensor data is picked
    // every time, being the largest thing in the file and — in a Sony ARW —
    // filed under exactly the same tag.
    if let (Some(at), Some(len)) = strip {
        if strips == 1 && matches!(compression, 6 | 7) {
            found.ranges.push((tiff.base() + at, len));
        }
    }
}

fn read_exif_directory(tiff: &Tiff, entries: &[tiff::Entry], found: &mut Found) {
    for e in entries {
        match e.tag {
            0x829A => found.exif.shutter = tiff.real(e, 0).filter(|v| *v > 0.0),
            0x829D => found.exif.aperture = tiff.real(e, 0).filter(|v| *v > 0.0),
            0x8827 => found.exif.iso = tiff.int(e).map(|v| v as u32),
            // Where the real figure goes once it no longer fits in sixteen
            // bits; the short field is then left pinned at its maximum.
            0x8831 | 0x8832 => found.iso_wide = found.iso_wide.or(tiff.int(e).map(|v| v as u32)),
            0x9003 => found.exif.date = tiff.text(e),
            0x9004 => found.fallback_date = found.fallback_date.take().or_else(|| tiff.text(e)),
            0x9204 => found.exif.exposure_comp = tiff.real(e, 0),
            0x920A => found.exif.focal_length = tiff.real(e, 0).filter(|v| *v > 0.0),
            0x9291 => found.subsec = tiff.text(e),
            0xA002 => found.stated.0 = tiff.int(e).map(|v| v as u32),
            0xA003 => found.stated.1 = tiff.int(e).map(|v| v as u32),
            0xA405 => found.exif.focal_length_35mm = tiff.real(e, 0).filter(|v| *v > 0.0),
            0xA434 => found.exif.lens = tiff.text(e),
            _ => {}
        }
    }
}

fn read_gps_directory(tiff: &Tiff, entries: &[tiff::Entry], found: &mut Found) {
    // Degrees, minutes and seconds as three fractions.
    let degrees = |e: &tiff::Entry| -> Option<f64> {
        Some(tiff.real(e, 0)? + tiff.real(e, 1)? / 60.0 + tiff.real(e, 2)? / 3600.0)
    };
    for e in entries {
        match e.tag {
            1 => found.lat.1 = tiff.text(e),
            2 => found.lat.0 = degrees(e),
            3 => found.lon.1 = tiff.text(e),
            4 => found.lon.0 = degrees(e),
            _ => {}
        }
    }
}

/// `YYYY:MM:DD HH:MM:SS` as milliseconds on the camera's clock.
///
/// Counted from the same arbitrary origin for every frame, which is all a
/// comparison needs. `subsec` is the digits after the decimal point, as
/// cameras store them: "5" is half a second, "05" is a twentieth.
pub fn clock_ms(date: &str, subsec: Option<&str>) -> Option<f64> {
    let b = date.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |from: usize, len: usize| -> Option<i64> {
        std::str::from_utf8(&b[from..from + len]).ok()?.trim().parse().ok()
    };
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hour, minute, second) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    // A camera that has never had its clock set writes zeroes.
    if year < 1970 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    // Days since 1970-01-01, by the usual civil-calendar arithmetic.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    let whole = ((days * 24 + hour) * 60 + minute) * 60 + second;
    let fraction = subsec
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
        .and_then(|s| format!("0.{s}").parse::<f64>().ok())
        .unwrap_or(0.0);
    Some(whole as f64 * 1000.0 + fraction * 1000.0)
}

/// Turns what a walk gathered into its final form.
fn finish(mut found: Found, src: &dyn ReadAt) -> Probe {
    // Decide which of the claimed ranges are pictures. Every format has some
    // way of pointing at sensor data by mistake, so the claim is never taken
    // on trust: the bytes have to open as a JPEG and declare an ordinary
    // frame. This costs a few small reads per candidate and no decoding.
    let mut embedded: Vec<Embedded> = Vec::new();
    for (offset, len) in found.ranges.drain(..) {
        let Some(end) = offset.checked_add(len).filter(|end| *end <= src.size()) else { continue };
        let Some(frame) = jpeg::frame(src, offset, end) else { continue };
        if !frame.is_picture() {
            continue;
        }
        let candidate = Embedded { offset, len, width: frame.width, height: frame.height };
        if !embedded.iter().any(|e| e.offset == offset) {
            embedded.push(candidate);
        }
    }
    embedded.sort_by_key(|e| (e.width as u64 * e.height as u64, e.len));

    let mut exif = found.exif;
    if exif.date.is_none() {
        exif.date = found.fallback_date;
    }
    exif.capture_ms = exif.date.as_deref().and_then(|d| clock_ms(d, found.subsec.as_deref()));
    if matches!(exif.iso, None | Some(65535)) {
        exif.iso = found.iso_wide.or(exif.iso);
    }

    // The photograph's size. EXIF states the finished size outright, which is
    // the figure a photographer expects — a sensor directory counts the masked
    // border too, and reads a few dozen pixels too large each way.
    //
    // Where EXIF does not say, a full-size preview is the next best witness:
    // it *is* the finished photograph. It only counts when it really is full
    // size, though — an older body's half-size preview says nothing about how
    // big the photograph is.
    let from_directory = found.largest.map(|(w, h, _)| (w, h));
    let from_preview = embedded.last().map(|e| (e.width, e.height));
    let preview_is_full = match (from_directory, from_preview) {
        (Some((dw, dh)), Some((pw, ph))) => (pw as u64 * ph as u64) * 10 >= (dw as u64 * dh as u64) * 8,
        _ => false,
    };
    let (width, height) = match found.stated {
        (Some(w), Some(h)) if w > 0 && h > 0 => (Some(w), Some(h)),
        _ => match if preview_is_full { from_preview } else { from_directory.or(from_preview) } {
            Some((w, h)) => (Some(w), Some(h)),
            None => (None, None),
        },
    };
    exif.width = width;
    exif.height = height;

    exif.latitude = match found.lat {
        (Some(v), Some(r)) => Some(if r.starts_with('S') { -v } else { v }),
        _ => None,
    };
    exif.longitude = match found.lon {
        (Some(v), Some(r)) => Some(if r.starts_with('W') { -v } else { v }),
        _ => None,
    };

    Probe {
        embedded,
        orientation: found.orientation.unwrap_or(1),
        exif,
        rating: found.rating,
        xmp: found.xmp,
    }
}

/// Looks inside a file: its previews, its orientation, its EXIF.
///
/// Identifies the container from its first bytes rather than its name, so a
/// file renamed by an ingest tool, or a RAW with an unfamiliar extension,
/// is still read for what it is.
pub fn probe(path: &Path) -> std::io::Result<Probe> {
    let src = FileSource::open(path)?;
    Ok(probe_source(&src))
}

pub fn probe_source(src: &dyn ReadAt) -> Probe {
    let head = src.read_at(0, 16.min(src.size() as usize)).unwrap_or_default();
    let mut found = Found::default();

    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        // A JPEG. The whole file is the one picture; its EXIF is a TIFF
        // tucked into an early segment.
        found.ranges.push((0, src.size()));
        read_jpeg_metadata(src, 0, src.size(), &mut found);
    } else if head.starts_with(b"FUJIFILMCCD-RAW") {
        // Fuji states it outright, at fixed positions in the file header. The
        // preview is a complete JPEG with the frame's EXIF inside it, so the
        // metadata comes from there too.
        if let Some(h) = src.read_at(84, 8) {
            let at = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64;
            let len = u32::from_be_bytes([h[4], h[5], h[6], h[7]]) as u64;
            if at > 0 && len > 0 && at + len <= src.size() {
                found.ranges.push((at, len));
                read_jpeg_metadata(src, at, at + len, &mut found);
            }
        }
    } else if head.len() >= 8 && &head[4..8] == b"ftyp" {
        if let Some(cr3) = bmff::scan(src) {
            found.ranges.extend(cr3.jpegs);
            // Canon splits what a TIFF keeps together into three boxes, each
            // a small TIFF whose first directory is the one that matters.
            for (at, role) in [(cr3.main, Role::Image), (cr3.exif, Role::Exif), (cr3.gps, Role::Gps)] {
                if let Some(tiff) = at.and_then(|at| Tiff::open(src, at)) {
                    walk(&tiff, tiff.first_ifd(), role, &mut found);
                }
            }
        }
    } else if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        // A PNG states its size in the first chunk, at a fixed position, and
        // carries nothing else this app reads.
        if let Some(h) = src.read_at(16, 8) {
            found.stated = (
                Some(u32::from_be_bytes([h[0], h[1], h[2], h[3]])),
                Some(u32::from_be_bytes([h[4], h[5], h[6], h[7]])),
            );
        }
    } else if let Some(tiff) = Tiff::open(src, 0) {
        walk(&tiff, tiff.first_ifd(), Role::Image, &mut found);
    }

    let mut probe = finish(found, src);

    // Nothing declared, or nothing that held up. Some cameras keep their only
    // usable preview somewhere no standard directory mentions — inside the
    // maker's private notes, typically — so look for it the slow way rather
    // than show nothing.
    let searchable = !head.starts_with(&[0xFF, 0xD8, 0xFF]) && !head.starts_with(b"\x89PNG");
    if probe.embedded.is_empty() && searchable {
        probe.embedded = scan_for_jpegs(src);
    }
    probe
}

/// How much of a file is fetched at a time when only its capture time is
/// wanted. The directories that hold it sit in the first few kilobytes of
/// every format here, so this is one small read per photo in the usual case.
const TIME_BLOCK: u64 = 16 * 1024;

/// When a photo was taken, on the camera's clock — and nothing else.
///
/// The same figure `probe` reports as `exif.capture_ms`, found by a much
/// shorter route. A full probe also locates and checks every preview in the
/// file, which means reads scattered through tens of megabytes; this follows
/// two pointers from the front of the file and stops.
///
/// It exists so a folder can be put in the order it was shot before the first
/// photo is shown. Filenames cannot do that when two cameras were used: each
/// numbers its own frames, and sorted by name one camera's whole day comes
/// before the other's.
pub fn capture_time(path: &Path) -> Option<f64> {
    let src = FileSource::open_with_block(path, TIME_BLOCK).ok()?;
    capture_time_source(&src)
}

pub fn capture_time_source(src: &dyn ReadAt) -> Option<f64> {
    let head = src.read_at(0, 16.min(src.size() as usize))?;

    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        let tiff = Tiff::open(src, jpeg::exif_tiff(src, 0, src.size())?)?;
        time_from_image_directory(&tiff)
    } else if head.starts_with(b"FUJIFILMCCD-RAW") {
        let h = src.read_at(84, 8)?;
        let at = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64;
        let len = u32::from_be_bytes([h[4], h[5], h[6], h[7]]) as u64;
        let tiff = Tiff::open(src, jpeg::exif_tiff(src, at, at.checked_add(len)?)?)?;
        time_from_image_directory(&tiff)
    } else if head.len() >= 8 && &head[4..8] == b"ftyp" {
        let cr3 = bmff::scan(src)?;
        // Canon keeps the exposure directory in a box of its own; the camera
        // directory is the fallback, as it is everywhere else.
        let from_exif = cr3
            .exif
            .and_then(|at| Tiff::open(src, at))
            .and_then(|tiff| tiff.ifd(tiff.first_ifd()).and_then(|(entries, _)| time_from_exif_entries(&tiff, &entries)));
        from_exif.or_else(|| {
            let tiff = Tiff::open(src, cr3.main?)?;
            time_from_image_directory(&tiff)
        })
    } else {
        time_from_image_directory(&Tiff::open(src, 0)?)
    }
}

/// From a TIFF's first directory: follow its pointer to the exposure
/// directory and read the time there. The order of preference matches the
/// full probe's exactly, so the two can never disagree about a photo:
/// the moment of capture; failing that the file's own date; failing that the
/// moment it was digitised.
fn time_from_image_directory(tiff: &Tiff) -> Option<f64> {
    let (entries, _) = tiff.ifd(tiff.first_ifd())?;
    let file_date = entries.iter().find(|e| e.tag == 0x0132).and_then(|e| tiff.text(e));
    let exif = entries
        .iter()
        .find(|e| e.tag == 0x8769)
        .and_then(|e| tiff.int(e))
        .and_then(|at| tiff.ifd(at))
        .map(|(entries, _)| entries);

    let subsec = exif.as_ref().and_then(|d| d.iter().find(|e| e.tag == 0x9291)).and_then(|e| tiff.text(e));
    let text = |tag: u16| exif.as_ref().and_then(|d| d.iter().find(|e| e.tag == tag)).and_then(|e| tiff.text(e));
    let date = text(0x9003).or(file_date).or_else(|| text(0x9004))?;
    clock_ms(&date, subsec.as_deref())
}

fn time_from_exif_entries(tiff: &Tiff, entries: &[tiff::Entry]) -> Option<f64> {
    let text = |tag: u16| entries.iter().find(|e| e.tag == tag).and_then(|e| tiff.text(e));
    let date = text(0x9003).or_else(|| text(0x9004))?;
    clock_ms(&date, text(0x9291).as_deref())
}

/// A JPEG's own metadata: the EXIF block it carries, and any XMP.
fn read_jpeg_metadata(src: &dyn ReadAt, start: u64, end: u64, found: &mut Found) {
    if let Some(tiff) = jpeg::exif_tiff(src, start, end).and_then(|at| Tiff::open(src, at)) {
        // The ranges a JPEG's EXIF points at are its own thumbnail, which is
        // smaller than anything this app draws. They are dropped so the file
        // itself stays the only candidate.
        let before = found.ranges.len();
        walk(&tiff, tiff.first_ifd(), Role::Image, found);
        found.ranges.truncate(before);
    }
    if found.xmp.is_none() {
        found.xmp = jpeg::xmp_packet(src, start, end);
    }
}

/// How far into a file the slow search looks. Measured across Sony, Canon,
/// Nikon and DNG files by the Mac app: an embedded preview always begins in
/// the first sixth of the file, the rest being sensor data.
const SCAN_WINDOW: u64 = 24 * 1024 * 1024;

/// Nothing smaller than this is worth finding by brute force; it would be a
/// thumbnail, and a blurry one.
const SCAN_MIN_BYTES: u64 = 200_000;

/// Searches the front of a file for anything that opens as a JPEG.
///
/// Compressed sensor data throws up byte pairs that look like a start marker
/// by the hundred, so each one is checked by reading its frame header — a
/// false start has none — before any time is spent finding where it ends.
fn scan_for_jpegs(src: &dyn ReadAt) -> Vec<Embedded> {
    let window = SCAN_WINDOW.min(src.size());
    let Some(data) = src.read_at(0, window as usize) else { return Vec::new() };

    let mut found: Vec<Embedded> = Vec::new();
    let mut from = 0usize;
    while let Some(hit) = memchr::memmem::find(&data[from..], &[0xFF, 0xD8, 0xFF]) {
        let start = from + hit;
        from = start + 3;

        let Some(frame) = jpeg::frame(src, start as u64, src.size()) else { continue };
        if !frame.is_picture() || frame.width.max(frame.height) < 1000 {
            continue;
        }
        // An end marker cannot occur inside compressed image data, so the
        // first one after the headers is the end. Searched for in the rest of
        // the file, not just the window: a preview may begin inside it and
        // run past it.
        let Some(rest) = src.read_at(start as u64, (src.size() - start as u64) as usize) else {
            continue;
        };
        let body_from = jpeg_header_len(&rest);
        let Some(end) = memchr::memmem::find(&rest[body_from..], &[0xFF, 0xD9]) else { continue };
        let len = (body_from + end + 2) as u64;
        if len < SCAN_MIN_BYTES {
            continue;
        }
        found.push(Embedded { offset: start as u64, len, width: frame.width, height: frame.height });
        // Nothing inside this picture is another picture worth having.
        from = start + len as usize;
        if from >= data.len() {
            break;
        }
    }
    found.sort_by_key(|e| (e.width as u64 * e.height as u64, e.len));
    found
}

/// How many bytes of headers precede a JPEG's compressed data. The search for
/// an end marker starts after them, because a header may hold an EXIF
/// thumbnail — a complete small JPEG, end marker and all.
fn jpeg_header_len(jpeg: &[u8]) -> usize {
    let mut pos = 2usize;
    while pos + 4 <= jpeg.len() {
        if jpeg[pos] != 0xFF {
            break;
        }
        let marker = jpeg[pos + 1];
        if matches!(marker, 0xFF) {
            pos += 1;
            continue;
        }
        if matches!(marker, 0x01 | 0xD0..=0xD8) {
            pos += 2;
            continue;
        }
        let len = u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]) as usize;
        if len < 2 {
            break;
        }
        pos += 2 + len;
        if marker == 0xDA {
            break;
        }
    }
    pos.min(jpeg.len())
}

#[cfg(test)]
mod tests {
    use super::jpeg::testing::{exif_segment, jpeg as fake_jpeg};
    use super::tiff::testing::*;
    use super::*;

    /// A TIFF with the given directories, followed by `blobs` appended whole.
    /// Returns the file and the offset each blob landed at.
    fn raw_with(ifds: impl Fn(&[u32]) -> Vec<Vec<(u16, u16, Vec<u8>, u32)>>, blobs: &[Vec<u8>]) -> Vec<u8> {
        // Two passes: lay the directories out with placeholder offsets to
        // learn how long they are, then again with the real ones.
        let placeholder: Vec<u32> = vec![0; blobs.len()];
        let header_len = build(&ifds(&placeholder)).len();
        let mut offsets = Vec::new();
        let mut at = header_len as u32;
        for b in blobs {
            offsets.push(at);
            at += b.len() as u32;
        }
        let mut file = build(&ifds(&offsets));
        assert_eq!(file.len(), header_len, "directory length must not depend on the offsets");
        for b in blobs {
            file.extend_from_slice(b);
        }
        file
    }

    #[test]
    fn a_sony_style_raw_yields_its_previews_and_skips_the_sensor_data() {
        let preview = fake_jpeg(0xC0, 8, 1616, 1080, &[]);
        let full = fake_jpeg(0xC0, 8, 7008, 4672, &[]);
        // The sensor plane: a lossless frame, under the strip tags, marked as
        // JPEG-compressed. Bigger than everything else, as it is in a real one.
        let mut sensor = fake_jpeg(0xC3, 14, 7040, 4688, &[]);
        sensor.resize(4000, 0);

        let file = raw_with(
            |at| {
                vec![
                    // IFD0: the screen-sized preview, and the camera's facts.
                    vec![
                        long(0x00FE, 1),
                        long(0x0100, 1616),
                        long(0x0101, 1080),
                        short(0x0103, 6),
                        ascii(0x010F, "SONY"),
                        ascii(0x0110, "ILCE-7M5"),
                        short(0x0112, 8),
                        long(0x0201, at[0]),
                        long(0x0202, preview.len() as u32),
                    ],
                    // The directory chained after it: full-size preview.
                    vec![
                        short(0x0103, 6),
                        long(0x0201, at[1]),
                        long(0x0202, full.len() as u32),
                    ],
                    // And the sensor data, reached by the chain as well here.
                    vec![
                        long(0x00FE, 0),
                        long(0x0100, 7040),
                        long(0x0101, 4688),
                        short(0x0103, 7),
                        long(0x0111, at[2]),
                        long(0x0117, sensor.len() as u32),
                    ],
                ]
            },
            &[preview.clone(), full.clone(), sensor.clone()],
        );

        let probe = probe_source(&file);
        assert_eq!(probe.orientation, 8);
        assert_eq!(probe.exif.make.as_deref(), Some("SONY"));
        assert_eq!(probe.exif.model.as_deref(), Some("ILCE-7M5"));

        // Two pictures, smallest first, and no third.
        let sizes: Vec<_> = probe.embedded.iter().map(|e| (e.width, e.height)).collect();
        assert_eq!(sizes, vec![(1616, 1080), (7008, 4672)]);

        // A thumbnail is cut from the small one; the loupe gets the big one.
        assert_eq!(probe.pick(320).unwrap().width, 1616);
        assert_eq!(probe.pick(1600).unwrap().width, 1616);
        assert_eq!(probe.pick(2400).unwrap().width, 7008);
        // Asked for more than exists, the largest is what there is.
        assert_eq!(probe.pick(20_000).unwrap().width, 7008);

        // With no EXIF size stated, the full-size preview is the photograph:
        // 7008 across, not the 7040 the sensor directory counts with its
        // masked border.
        assert_eq!((probe.exif.width, probe.exif.height), (Some(7008), Some(4672)));
    }

    #[test]
    fn exposure_settings_come_from_the_exif_directory() {
        let exif_dir = vec![
            rational(0x829A, 1, 250),
            rational(0x829D, 28, 10),
            short(0x8827, 400),
            ascii(0x9003, "2026:08:01 14:03:22"),
            ascii(0x9291, "45"),
            rational(0x920A, 85, 1),
            long(0xA002, 7008),
            long(0xA003, 4672),
            ascii(0xA434, "FE 85mm F1.4 GM II"),
        ];
        // The first directory is two entries long, so the second starts right
        // after its header, entries and link.
        let first_len = 2 + 2 * 12 + 4;
        let pointer = 8 + first_len as u32;
        let mut file = build(&[vec![ascii(0x0110, "ILCE-7M5"), long(0x8769, pointer)], exif_dir]);
        // The builder chains its directories. A real file reaches its EXIF
        // only through the pointer, so the chain link is cut here.
        let link = 8 + 2 + 2 * 12;
        file[link..link + 4].copy_from_slice(&[0, 0, 0, 0]);

        let probe = probe_source(&file);
        let e = &probe.exif;
        assert!((e.shutter.unwrap() - 0.004).abs() < 1e-9);
        assert!((e.aperture.unwrap() - 2.8).abs() < 1e-9);
        assert_eq!(e.iso, Some(400));
        assert_eq!(e.focal_length, Some(85.0));
        assert_eq!(e.lens.as_deref(), Some("FE 85mm F1.4 GM II"));
        assert_eq!(e.date.as_deref(), Some("2026:08:01 14:03:22"));
        assert_eq!((e.width, e.height), (Some(7008), Some(4672)));
        // 0.45 of a second past the whole one.
        let whole = clock_ms("2026:08:01 14:03:22", None).unwrap();
        assert!((e.capture_ms.unwrap() - whole - 450.0).abs() < 1e-6);
    }

    /// The short route and the long one must always name the same instant.
    /// If they ever differed, a folder would open in one order and then
    /// shuffle itself into another a second later.
    fn assert_same_time(file: &Vec<u8>) -> Option<f64> {
        let quick = capture_time_source(file);
        assert_eq!(quick, probe_source(file).exif.capture_ms);
        quick
    }

    #[test]
    fn the_quick_time_matches_the_full_probe_in_every_container() {
        let exif_dir = || vec![ascii(0x9003, "2026:10:06 19:41:07"), ascii(0x9291, "218")];
        let pointer = 8 + (2 + 2 * 12 + 4) as u32;
        let cut_chain = |mut f: Vec<u8>| {
            let link = 8 + 2 + 2 * 12;
            f[link..link + 4].copy_from_slice(&[0, 0, 0, 0]);
            f
        };
        let tiff = cut_chain(build(&[vec![ascii(0x0110, "ILCE-7M5"), long(0x8769, pointer)], exif_dir()]));
        let expected = clock_ms("2026:10:06 19:41:07", Some("218"));

        // A RAW that is a TIFF.
        assert_eq!(assert_same_time(&tiff), expected);

        // A JPEG, whose EXIF is that same TIFF in a segment.
        let jpeg_file = fake_jpeg(0xC0, 8, 7008, 4672, &[exif_segment(&tiff)]);
        assert_eq!(assert_same_time(&jpeg_file), expected);

        // A Fuji RAW, read through the JPEG inside it.
        let mut raf = b"FUJIFILMCCD-RAW 0201FF129502X-T2".to_vec();
        raf.resize(84, 0);
        raf.extend_from_slice(&148u32.to_be_bytes());
        raf.extend_from_slice(&(jpeg_file.len() as u32).to_be_bytes());
        raf.resize(148, 0);
        raf.extend_from_slice(&jpeg_file);
        assert_eq!(assert_same_time(&raf), expected);
    }

    #[test]
    fn the_quick_time_falls_back_the_same_way_the_full_probe_does() {
        // No capture time in EXIF, but the file has a date of its own.
        let pointer = 8 + (2 + 2 * 12 + 4) as u32;
        let mut f = build(&[
            vec![ascii(0x0132, "2026:10:06 20:00:00"), long(0x8769, pointer)],
            vec![ascii(0x9004, "2026:10:06 21:00:00")],
        ]);
        let link = 8 + 2 + 2 * 12;
        f[link..link + 4].copy_from_slice(&[0, 0, 0, 0]);
        assert_eq!(assert_same_time(&f), clock_ms("2026:10:06 20:00:00", None));

        // Nothing anywhere: no time, and no disagreement about that either.
        let bare = build(&[vec![ascii(0x0110, "ILCE-7M5")]]);
        assert_eq!(assert_same_time(&bare), None);
    }

    #[test]
    fn a_jpeg_is_its_own_single_picture() {
        let exif = build(&[vec![short(0x0112, 6), ascii(0x0110, "ILCE-7M5")]]);
        let file = fake_jpeg(0xC0, 8, 7008, 4672, &[exif_segment(&exif)]);

        let probe = probe_source(&file);
        assert_eq!(probe.orientation, 6);
        assert_eq!(probe.embedded.len(), 1);
        assert_eq!(probe.embedded[0].offset, 0);
        assert_eq!(probe.embedded[0].len, file.len() as u64);
        assert_eq!(probe.exif.model.as_deref(), Some("ILCE-7M5"));
    }

    #[test]
    fn a_fuji_raw_is_read_through_its_preview() {
        let exif = build(&[vec![short(0x0112, 1), ascii(0x0110, "X-T2")]]);
        let preview = fake_jpeg(0xC0, 8, 1920, 1280, &[exif_segment(&exif)]);

        let mut file = b"FUJIFILMCCD-RAW 0201FF129502X-T2".to_vec();
        file.resize(84, 0);
        file.extend_from_slice(&148u32.to_be_bytes());
        file.extend_from_slice(&(preview.len() as u32).to_be_bytes());
        file.resize(148, 0);
        file.extend_from_slice(&preview);
        file.extend_from_slice(&[0u8; 500]); // sensor data follows

        let probe = probe_source(&file);
        assert_eq!(probe.embedded.len(), 1);
        assert_eq!((probe.embedded[0].width, probe.embedded[0].height), (1920, 1280));
        assert_eq!(probe.embedded[0].offset, 148);
        assert_eq!(probe.exif.model.as_deref(), Some("X-T2"));
    }

    #[test]
    fn camera_clocks_compare_across_a_month_and_a_leap_day() {
        let ms = |s| clock_ms(s, None).unwrap();
        assert_eq!(ms("2026:08:01 00:00:01") - ms("2026:07:31 23:59:59"), 2000.0);
        // 2024 was a leap year, so February ran to the 29th.
        assert_eq!(ms("2024:03:01 00:00:00") - ms("2024:02:28 00:00:00"), 2.0 * 86_400_000.0);
        assert_eq!(ms("1970:01:01 00:00:00"), 0.0);
    }

    #[test]
    fn subseconds_are_a_decimal_fraction_not_a_count() {
        let base = clock_ms("2026:08:01 14:03:22", None).unwrap();
        let with = |s| clock_ms("2026:08:01 14:03:22", Some(s)).unwrap() - base;
        assert!((with("5") - 500.0).abs() < 1e-6);
        assert!((with("05") - 50.0).abs() < 1e-6);
        assert!((with("123") - 123.0).abs() < 1e-6);
        // Junk in the field is no fraction, not a failure of the whole date.
        assert_eq!(with("  "), 0.0);
        assert_eq!(with("ab"), 0.0);
    }

    #[test]
    fn an_unset_camera_clock_is_no_time_at_all() {
        assert_eq!(clock_ms("0000:00:00 00:00:00", None), None);
        assert_eq!(clock_ms("garbage", None), None);
    }

    #[test]
    fn an_unrecognised_file_falls_back_to_searching_for_a_preview() {
        // Not a TIFF, not a JPEG, not anything known — with a real-sized
        // picture some way in, and a false start marker before it.
        let mut picture = fake_jpeg(0xC0, 8, 4000, 3000, &[]);
        let scan_at = picture.len() - 2; // just before the end marker
        picture.splice(scan_at..scan_at, vec![0x55u8; SCAN_MIN_BYTES as usize]);

        let mut file = vec![0u8; 5000];
        file[100..103].copy_from_slice(&[0xFF, 0xD8, 0xFF]); // looks like a start; is not one
        let at = file.len();
        file.extend_from_slice(&picture);
        file.extend_from_slice(&[0u8; 3000]);

        let probe = probe_source(&file);
        assert_eq!(probe.embedded.len(), 1);
        assert_eq!(probe.embedded[0].offset, at as u64);
        assert_eq!(probe.embedded[0].len, picture.len() as u64);
        assert_eq!(probe.embedded[0].width, 4000);
    }
}
