//! Canon CR3, which is not a TIFF at all.
//!
//! A CR3 is the same container as an MP4: nested boxes, each announcing its
//! own length. The full-size JPEG is kept as the only sample of the first
//! track, and is found the way a video player finds a frame — the movie box,
//! then that track's sample table. A screen-sized preview and the metadata
//! live in boxes of Canon's own, tagged with fixed identifiers.

use super::io::ReadAt;

/// What a CR3 says it contains, as places in the file.
#[derive(Default, Debug)]
pub struct Cr3 {
    /// Byte ranges that may hold a JPEG, unverified. Other tracks hold the
    /// sensor data and are weeded out later for not being pictures.
    pub jpegs: Vec<(u64, u64)>,
    /// The camera directory (make, model, orientation): a whole TIFF.
    pub main: Option<u64>,
    /// The exposure directory: a TIFF whose *first* directory is EXIF.
    pub exif: Option<u64>,
    /// The GPS directory, likewise.
    pub gps: Option<u64>,
}

/// Canon's metadata box inside the movie box.
const CANON_META: [u8; 16] = [
    0x85, 0xC0, 0xB6, 0x87, 0x82, 0x0F, 0x11, 0xE0, 0x81, 0x11, 0xF4, 0xCE, 0x46, 0x2B, 0x6A, 0x48,
];

/// Canon's preview box at the top level of the file.
const CANON_PREVIEW: [u8; 16] = [
    0xEA, 0xF4, 0x2B, 0x5E, 0x1C, 0x98, 0x4B, 0x88, 0xB9, 0xFB, 0xB7, 0xDC, 0x40, 0x6E, 0x4D, 0x16,
];

/// The movie box is a few tens of kilobytes of tables. One claiming to be
/// larger than this is a misread length, not a movie box.
const MAX_MOOV: u64 = 16 * 1024 * 1024;

/// One box: its four-letter type, and where its contents are.
#[derive(Clone, Copy)]
struct BoxAt {
    kind: [u8; 4],
    body: u64,
    len: u64,
}

/// The boxes laid end to end between `start` and `end`.
fn boxes(src: &dyn ReadAt, start: u64, end: u64) -> Vec<BoxAt> {
    let mut found = Vec::new();
    let mut pos = start;
    while pos.checked_add(8).is_some_and(|p| p <= end) && found.len() < 4096 {
        let Some(head) = src.read_at(pos, 8) else { break };
        let mut size = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as u64;
        let mut header = 8u64;
        if size == 1 {
            // A length too large for four bytes follows in eight.
            let Some(wide) = src.read_at(pos + 8, 8) else { break };
            // From the file, so anything at all: checked before it is added.
            size = u64::from_be_bytes(wide.try_into().expect("eight bytes"));
            header = 16;
        } else if size == 0 {
            // Runs to the end of whatever holds it.
            size = end - pos;
        }
        let Some(next) = pos.checked_add(size) else { break };
        if size < header || next > end {
            break;
        }
        found.push(BoxAt {
            kind: [head[4], head[5], head[6], head[7]],
            body: pos + header,
            len: size - header,
        });
        pos = next;
    }
    found
}

fn child(src: &dyn ReadAt, parent: BoxAt, kind: &[u8; 4]) -> Option<BoxAt> {
    boxes(src, parent.body, parent.body.checked_add(parent.len)?).into_iter().find(|b| &b.kind == kind)
}

/// A JPEG beginning within the first bytes of `b`, running to its end. Canon
/// prefixes its previews with a short header of sizes that has changed between
/// bodies, so the start marker is searched for rather than assumed.
fn jpeg_inside(src: &dyn ReadAt, b: BoxAt) -> Option<(u64, u64)> {
    let window = b.len.min(64) as usize;
    let head = src.read_at(b.body, window)?;
    let at = head.windows(3).position(|w| w == [0xFF, 0xD8, 0xFF])? as u64;
    Some((b.body + at, b.len - at))
}

/// Reads the structure of a CR3. `None` if this is not an ISO container.
pub fn scan(src: &dyn ReadAt) -> Option<Cr3> {
    let top = boxes(src, 0, src.size());
    if top.first().map(|b| &b.kind) != Some(b"ftyp") {
        return None;
    }
    let mut out = Cr3::default();

    for b in &top {
        match &b.kind {
            b"moov" if b.len <= MAX_MOOV => scan_movie(src, *b, &mut out),
            b"uuid" if b.len > 16 => {
                if src.read_at(b.body, 16).as_deref() == Some(&CANON_PREVIEW[..]) {
                    // Eight bytes of version and count, then a PRVW box.
                    let inner = BoxAt { kind: *b"uuid", body: b.body + 24, len: b.len.saturating_sub(24) };
                    if let Some(prvw) = child(src, inner, b"PRVW") {
                        out.jpegs.extend(jpeg_inside(src, prvw));
                    }
                }
            }
            _ => {}
        }
    }
    Some(out)
}

fn scan_movie(src: &dyn ReadAt, moov: BoxAt, out: &mut Cr3) {
    for b in boxes(src, moov.body, moov.body + moov.len) {
        match &b.kind {
            b"uuid" if b.len > 16 => {
                if src.read_at(b.body, 16).as_deref() != Some(&CANON_META[..]) {
                    continue;
                }
                let inner = BoxAt { kind: *b"uuid", body: b.body + 16, len: b.len - 16 };
                for meta in boxes(src, inner.body, inner.body + inner.len) {
                    match &meta.kind {
                        b"CMT1" => out.main = Some(meta.body),
                        b"CMT2" => out.exif = Some(meta.body),
                        b"CMT4" => out.gps = Some(meta.body),
                        b"THMB" => out.jpegs.extend(jpeg_inside(src, meta)),
                        _ => {}
                    }
                }
            }
            b"trak" => out.jpegs.extend(first_sample(src, b)),
            _ => {}
        }
    }
}

/// Where a track's first sample is: trak → mdia → minf → stbl, then the chunk
/// offset table for the position and the sample size table for the length.
fn first_sample(src: &dyn ReadAt, trak: BoxAt) -> Option<(u64, u64)> {
    let mdia = child(src, trak, b"mdia")?;
    let minf = child(src, mdia, b"minf")?;
    let stbl = child(src, minf, b"stbl")?;

    // Both offset tables open with four bytes of version and four of count.
    let offset = if let Some(co64) = child(src, stbl, b"co64") {
        u64::from_be_bytes(src.read_at(co64.body + 8, 8)?.try_into().ok()?)
    } else {
        let stco = child(src, stbl, b"stco")?;
        u32::from_be_bytes(src.read_at(stco.body + 8, 4)?.try_into().ok()?) as u64
    };

    // A size shared by every sample sits in the header; otherwise the first
    // entry follows the count.
    let stsz = child(src, stbl, b"stsz")?;
    let uniform = u32::from_be_bytes(src.read_at(stsz.body + 4, 4)?.try_into().ok()?) as u64;
    let len = if uniform > 0 {
        uniform
    } else {
        u32::from_be_bytes(src.read_at(stsz.body + 12, 4)?.try_into().ok()?) as u64
    };

    (offset > 0 && len > 0).then_some((offset, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn a_box_claiming_a_size_past_the_end_of_numbers_is_refused() {
        // size == 1: the real size follows in eight bytes, here the largest
        // there is. Added to its position, it would wrap around.
        let mut bytes = 1u32.to_be_bytes().to_vec();
        bytes.extend_from_slice(b"moov");
        bytes.extend_from_slice(&u64::MAX.to_be_bytes());
        bytes.extend_from_slice(&[0; 16]);
        assert!(boxes(&bytes.as_slice(), 0, bytes.len() as u64).is_empty());
    }

    fn full(version_and_rest: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 0, 0, 0];
        out.extend_from_slice(version_and_rest);
        out
    }

    /// A track whose single sample is at `offset` and `len` bytes long.
    fn track(offset: u32, len: u32) -> Vec<u8> {
        let stco = bx(b"stco", &full(&[&1u32.to_be_bytes()[..], &offset.to_be_bytes()[..]].concat()));
        let stsz = bx(
            b"stsz",
            &full(&[&0u32.to_be_bytes()[..], &1u32.to_be_bytes()[..], &len.to_be_bytes()[..]].concat()),
        );
        let stbl = bx(b"stbl", &[stco, stsz].concat());
        bx(b"trak", &bx(b"mdia", &bx(b"minf", &stbl)))
    }

    #[test]
    fn finds_each_tracks_sample_and_canons_metadata() {
        let mut meta = CANON_META.to_vec();
        meta.extend(bx(b"CMT1", b"II*\0camera"));
        meta.extend(bx(b"CMT2", b"II*\0exposure"));
        let mut thumb = vec![0, 0, 0, 0, 0, 160, 0, 120, 0, 0, 0, 5, 0, 1, 0, 0];
        thumb.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xDB, 0x00]);
        meta.extend(bx(b"THMB", &thumb));

        let moov = bx(b"moov", &[bx(b"uuid", &meta), track(5000, 900), track(9000, 40_000)].concat());
        let file = [bx(b"ftyp", b"crx isom"), moov].concat();

        let cr3 = scan(&file).unwrap();
        // The thumbnail, then one range per track, in the order met.
        assert_eq!(cr3.jpegs.len(), 3);
        assert_eq!(cr3.jpegs[1], (5000, 900));
        assert_eq!(cr3.jpegs[2], (9000, 40_000));
        // The thumbnail's range starts at its marker, past Canon's header.
        let (at, len) = cr3.jpegs[0];
        assert_eq!(&file[at as usize..at as usize + 3], &[0xFF, 0xD8, 0xFF]);
        assert_eq!(len, 5);

        let main = cr3.main.unwrap() as usize;
        assert_eq!(&file[main..main + 4], b"II*\0");
        assert!(cr3.exif.is_some());
        assert!(cr3.gps.is_none());
    }

    #[test]
    fn something_that_does_not_open_with_ftyp_is_not_a_cr3() {
        assert!(scan(&vec![0x49u8, 0x49, 42, 0, 8, 0, 0, 0]).is_none());
        assert!(scan(&bx(b"moov", b"")).is_none());
    }

    #[test]
    fn a_box_claiming_more_than_the_file_holds_ends_the_walk() {
        let mut file = bx(b"ftyp", b"crx ");
        file.extend_from_slice(&9_999_999u32.to_be_bytes());
        file.extend_from_slice(b"moov");
        let cr3 = scan(&file).unwrap();
        assert!(cr3.jpegs.is_empty());
    }
}
