//! Reading a JPEG's headers without decoding it.
//!
//! Three questions get asked of a run of bytes that claims to be a JPEG, and
//! all three are answered from its first few segments:
//!
//! - Is it a picture at all? Cameras store sensor data as lossless JPEG under
//!   the very tags a preview uses, and it is the biggest thing in the file.
//! - How big is it? That decides which of a file's several previews to use.
//! - Which way up does it say it is?

use super::io::ReadAt;
use super::tiff::Tiff;

/// What a JPEG's frame header declares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// A lossless or differential frame, which is how a RAW stores its sensor
    /// plane. It opens without complaint and renders black.
    pub lossless: bool,
    pub precision: u8,
    pub width: u32,
    pub height: u32,
    pub components: u8,
}

/// The most pixels a picture may have and still be decoded. A header can
/// claim up to 65535 × 65535 — thirteen gigabytes once decoded — and a
/// claim like that from a damaged file must fail that one file, not take the
/// app down by running it out of memory.
///
/// 250 million is well above any camera made: the largest, at 151 million,
/// fits with room. Only a stitched panorama would pass it, and that fails
/// to preview rather than crashing.
pub const MAX_PIXELS: u64 = 250_000_000;

impl Frame {
    /// Something a screen can show: an ordinary 8-bit baseline or progressive
    /// picture, in grey or colour, of a size that can be decoded.
    pub fn is_picture(&self) -> bool {
        !self.lossless
            && self.precision == 8
            && self.width > 0
            && self.height > 0
            && matches!(self.components, 1 | 3)
            && self.pixels() <= MAX_PIXELS
    }

    pub fn pixels(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// One header segment: its marker, and where its payload is.
struct Segment {
    marker: u8,
    payload: u64,
    len: usize,
}

/// Segments are walked by their own stated lengths, a few bytes read per
/// step. A header with more than this many is not a header.
const MAX_SEGMENTS: usize = 256;

/// Fill bytes allowed between segments. The standard sets no limit, but a
/// real file has a handful; a run of thousands is damage, and each one costs
/// a read.
const MAX_FILL: usize = 1024;

/// Every header segment of the JPEG starting at `start`, up to and including
/// the start of the image data. `end` bounds the walk.
fn segments(src: &dyn ReadAt, start: u64, end: u64) -> Vec<Segment> {
    let mut found = Vec::new();
    match src.read_at(start, 3) {
        Some(soi) if soi == [0xFF, 0xD8, 0xFF] => {}
        _ => return found,
    }

    let mut pos = start + 2;
    let mut fill = 0usize;
    while pos + 4 <= end && found.len() < MAX_SEGMENTS {
        let Some(head) = src.read_at(pos, 4) else { break };
        if head[0] != 0xFF {
            break;
        }
        let marker = head[1];
        match marker {
            // Padding between segments.
            0xFF => {
                fill += 1;
                if fill > MAX_FILL {
                    break;
                }
                pos += 1;
                continue;
            }
            // Markers that stand alone, with no length after them.
            0x01 | 0xD0..=0xD8 => {
                pos += 2;
                continue;
            }
            // End of image before any image: nothing here.
            0xD9 => break,
            _ => {}
        }
        let len = u16::from_be_bytes([head[2], head[3]]) as usize;
        if len < 2 {
            break;
        }
        found.push(Segment { marker, payload: pos + 4, len: len - 2 });
        // Start of scan: compressed data follows, and it is not segmented.
        if marker == 0xDA {
            break;
        }
        pos += 2 + len as u64;
    }
    found
}

/// The frame header of the JPEG at `start`, or `None` if there is no JPEG
/// there or its header never states one.
pub fn frame(src: &dyn ReadAt, start: u64, end: u64) -> Option<Frame> {
    for seg in segments(src, start, end) {
        // C0–CF are frame headers, except C4 (a Huffman table), C8 (reserved)
        // and CC (arithmetic conditioning), which only share the range.
        let is_frame = (0xC0..=0xCF).contains(&seg.marker)
            && !matches!(seg.marker, 0xC4 | 0xC8 | 0xCC);
        if !is_frame || seg.len < 6 {
            continue;
        }
        let b = src.read_at(seg.payload, 6)?;
        return Some(Frame {
            lossless: matches!(seg.marker, 0xC3 | 0xC7 | 0xCB | 0xCF),
            precision: b[0],
            height: u16::from_be_bytes([b[1], b[2]]) as u32,
            width: u16::from_be_bytes([b[3], b[4]]) as u32,
            components: b[5],
        });
    }
    None
}

const EXIF_HEADER: &[u8] = b"Exif\0\0";
const XMP_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

/// Where the EXIF block's TIFF header is inside the JPEG at `start`.
pub fn exif_tiff(src: &dyn ReadAt, start: u64, end: u64) -> Option<u64> {
    for seg in segments(src, start, end) {
        if seg.marker == 0xE1 && seg.len > EXIF_HEADER.len() + 8 {
            let head = src.read_at(seg.payload, EXIF_HEADER.len())?;
            if head == EXIF_HEADER {
                return Some(seg.payload + EXIF_HEADER.len() as u64);
            }
        }
    }
    None
}

/// The XMP packet a JPEG carries, if it carries one.
pub fn xmp_packet(src: &dyn ReadAt, start: u64, end: u64) -> Option<String> {
    for seg in segments(src, start, end) {
        if seg.marker == 0xE1 && seg.len > XMP_HEADER.len() {
            let head = src.read_at(seg.payload, XMP_HEADER.len())?;
            if head == XMP_HEADER {
                let body = src.read_at(
                    seg.payload + XMP_HEADER.len() as u64,
                    seg.len - XMP_HEADER.len(),
                )?;
                return Some(String::from_utf8_lossy(&body).into_owned());
            }
        }
    }
    None
}

/// Where the things are that writing into a JPEG's header has to know of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// The XMP packet: where its segment starts (the marker), where the
    /// packet's own text starts, and how long that text is.
    pub xmp: Option<(u64, u64, usize)>,
    /// Where a new segment goes if there is none: after the leading JFIF and
    /// EXIF blocks, ahead of everything else.
    pub insert_at: u64,
    /// A multi-picture index, if the file has one: where its segment starts.
    /// It finds the file's other pictures by how far they are from itself,
    /// so nothing after it may grow or shrink.
    pub index_at: Option<u64>,
}

/// Reads a JPEG's header for where its XMP is and where one could go.
/// `None` for something that is not a JPEG.
pub fn layout(src: &dyn ReadAt) -> Option<Layout> {
    let all = segments(src, 0, src.size());
    if all.is_empty() {
        return None;
    }
    let mut found = Layout { xmp: None, insert_at: 2, index_at: None };
    let mut leading = true;
    for seg in &all {
        let starts_with = |header: &[u8]| seg.len >= header.len() && src.read_at(seg.payload, header.len()).as_deref() == Some(header);
        let start = seg.payload - 4;
        if leading && (seg.marker == 0xE0 || (seg.marker == 0xE1 && starts_with(EXIF_HEADER))) {
            found.insert_at = seg.payload + seg.len as u64;
        } else {
            leading = false;
        }
        if seg.marker == 0xE1 && found.xmp.is_none() && seg.len > XMP_HEADER.len() && starts_with(XMP_HEADER) {
            found.xmp = Some((start, seg.payload + XMP_HEADER.len() as u64, seg.len - XMP_HEADER.len()));
        }
        if seg.marker == 0xE2 && found.index_at.is_none() && starts_with(b"MPF\0") {
            found.index_at = Some(start);
        }
    }
    Some(found)
}

/// A whole XMP segment, marker and all, holding `packet`. `None` if the
/// packet is too long for one segment to hold.
pub fn xmp_segment(packet: &[u8]) -> Option<Vec<u8>> {
    let len = u16::try_from(2 + XMP_HEADER.len() + packet.len()).ok()?;
    let mut out = vec![0xFF, 0xE1];
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(XMP_HEADER);
    out.extend_from_slice(packet);
    Some(out)
}

const ICC_HEADER: &[u8] = b"ICC_PROFILE\0";

/// The colour profile a JPEG carries, reassembled from however many segments
/// it was split across.
pub fn icc_profile(jpeg: &[u8]) -> Option<Vec<u8>> {
    let mut chunks: Vec<(u8, &[u8])> = Vec::new();
    for seg in segments(&jpeg, 0, jpeg.len() as u64) {
        if seg.marker != 0xE2 || seg.len < ICC_HEADER.len() + 2 {
            continue;
        }
        let at = seg.payload as usize;
        let body = jpeg.get(at..at + seg.len)?;
        if &body[..ICC_HEADER.len()] == ICC_HEADER {
            // One byte of sequence number, one of total, then the data.
            chunks.push((body[ICC_HEADER.len()], &body[ICC_HEADER.len() + 2..]));
        }
    }
    if chunks.is_empty() {
        return None;
    }
    chunks.sort_by_key(|(seq, _)| *seq);
    Some(chunks.into_iter().flat_map(|(_, data)| data.iter().copied()).collect())
}

/// Gives a JPEG a colour profile, placed directly after its start marker.
///
/// A segment holds at most 64 KB, so a large profile is split across several,
/// each numbered "n of m" as the format asks.
pub fn with_icc_profile(mut jpeg: Vec<u8>, profile: &[u8]) -> Vec<u8> {
    const MAX_CHUNK: usize = 65_533 - 14; // segment limit, less the header and the two counters
    if profile.is_empty() || jpeg.len() < 2 || jpeg[..2] != [0xFF, 0xD8] {
        return jpeg;
    }
    let chunks: Vec<&[u8]> = profile.chunks(MAX_CHUNK).collect();
    if chunks.len() > 255 {
        return jpeg; // larger than the format can number; not a real profile
    }
    let mut block = Vec::with_capacity(profile.len() + chunks.len() * 18);
    for (i, chunk) in chunks.iter().enumerate() {
        block.extend_from_slice(&[0xFF, 0xE2]);
        block.extend_from_slice(&((chunk.len() + 2 + ICC_HEADER.len() + 2) as u16).to_be_bytes());
        block.extend_from_slice(ICC_HEADER);
        block.extend_from_slice(&[(i + 1) as u8, chunks.len() as u8]);
        block.extend_from_slice(chunk);
    }
    jpeg.splice(2..2, block);
    jpeg
}

/// Where the orientation value sits inside a JPEG's own EXIF: its position in
/// the buffer, and the byte order it is written in.
fn orientation_slot(jpeg: &[u8]) -> Option<(usize, bool)> {
    let base = exif_tiff(&jpeg, 0, jpeg.len() as u64)?;
    let tiff = Tiff::open(&jpeg, base)?;
    let (entries, _) = tiff.ifd(tiff.first_ifd())?;
    let entry = entries.iter().find(|e| e.tag == 0x0112 && e.kind == 3 && e.count == 1)?;
    let big_endian = jpeg.get(base as usize)? == &0x4D;
    Some((entry.data as usize, big_endian))
}

/// The orientation a JPEG states for itself, 1–8, if it states one.
pub fn own_orientation(jpeg: &[u8]) -> Option<u8> {
    let (at, big_endian) = orientation_slot(jpeg)?;
    let b = jpeg.get(at..at + 2)?;
    let value = if big_endian { u16::from_be_bytes([b[0], b[1]]) } else { u16::from_le_bytes([b[0], b[1]]) };
    (1..=8).contains(&value).then_some(value as u8)
}

/// Makes a JPEG say it is `orientation`, 1–8.
///
/// The preview inside a RAW usually has no idea which way up it is. The
/// camera wrote that once, in the RAW's own directory, and the preview is
/// simply the sensor's rows in the order they came off it. Shown as-is, every
/// portrait frame lies on its side.
///
/// Rotating the pixels would mean decoding and re-encoding ten megabytes of
/// JPEG to move no information. Instead the picture is told what the RAW
/// already knows, in the one place a web view looks: its EXIF. The view then
/// rotates it while drawing, which it would have done for any phone photo.
///
/// Two cases. If the JPEG has an orientation, it is overwritten in place — two
/// bytes. If it has none, a minimal EXIF block holding nothing else is slipped
/// in directly after the start marker, where a reader finds it first.
pub fn set_orientation(mut jpeg: Vec<u8>, orientation: u8) -> Vec<u8> {
    if !(1..=8).contains(&orientation) || jpeg.len() < 4 || jpeg[..2] != [0xFF, 0xD8] {
        return jpeg;
    }

    if let Some((at, big_endian)) = orientation_slot(&jpeg) {
        if at + 2 <= jpeg.len() {
            let bytes = if big_endian {
                (orientation as u16).to_be_bytes()
            } else {
                (orientation as u16).to_le_bytes()
            };
            jpeg[at..at + 2].copy_from_slice(&bytes);
            return jpeg;
        }
    }

    // Upright and saying nothing is already correct.
    if orientation == 1 {
        return jpeg;
    }

    #[rustfmt::skip]
    let block: [u8; 36] = [
        0xFF, 0xE1, 0x00, 0x22,                 // APP1, 34 bytes including this length
        b'E', b'x', b'i', b'f', 0x00, 0x00,
        b'M', b'M', 0x00, 0x2A,                 // big-endian TIFF
        0x00, 0x00, 0x00, 0x08,                 // first directory follows the header
        0x00, 0x01,                             // one entry
        0x01, 0x12, 0x00, 0x03,                 // Orientation, SHORT
        0x00, 0x00, 0x00, 0x01,                 // one value
        0x00, orientation, 0x00, 0x00,          // the value, padded
        0x00, 0x00, 0x00, 0x00,                 // no next directory
    ];
    jpeg.splice(2..2, block);
    jpeg
}

#[cfg(test)]
pub(crate) mod testing {
    /// A header-only JPEG: start marker, optional extra segments, a frame
    /// header of the given kind, and a start-of-scan. Enough for every reader
    /// in this module, none of which decodes anything.
    pub fn jpeg(sof: u8, precision: u8, width: u16, height: u16, extra: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8];
        for (marker, payload) in extra {
            out.extend_from_slice(&[0xFF, *marker]);
            out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
            out.extend_from_slice(payload);
        }
        out.extend_from_slice(&[0xFF, sof, 0x00, 0x11, precision]);
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0x00, 0x00, 0xFF, 0xD9]);
        out
    }

    pub fn exif_segment(tiff: &[u8]) -> (u8, Vec<u8>) {
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(tiff);
        (0xE1, payload)
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::raw::tiff::testing as tiff;

    #[test]
    fn a_header_claiming_more_pixels_than_can_be_decoded_is_not_a_picture() {
        let huge = jpeg(0xC0, 8, 65535, 65535, &[]);
        let f = frame(&huge.as_slice(), 0, huge.len() as u64).unwrap();
        assert!(!f.is_picture());
        let large = jpeg(0xC0, 8, 14204, 10652, &[]); // 151 MP, the largest camera
        assert!(frame(&large.as_slice(), 0, large.len() as u64).unwrap().is_picture());
    }

    #[test]
    fn a_long_run_of_fill_bytes_ends_the_walk() {
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend(std::iter::repeat_n(0xFF, 10_000));
        bytes.extend_from_slice(&jpeg(0xC0, 8, 6000, 4000, &[])[2..]);
        assert!(frame(&bytes.as_slice(), 0, bytes.len() as u64).is_none());
    }

    #[test]
    fn reads_dimensions_from_a_baseline_frame() {
        let j = jpeg(0xC0, 8, 7008, 4672, &[]);
        let f = frame(&j, 0, j.len() as u64).unwrap();
        assert_eq!((f.width, f.height, f.components), (7008, 4672, 3));
        assert!(f.is_picture());
    }

    #[test]
    fn a_lossless_frame_is_sensor_data_not_a_picture() {
        // What a lossless-compressed RAW stores under its preview's own tags.
        let j = jpeg(0xC3, 14, 256, 256, &[]);
        let f = frame(&j, 0, j.len() as u64).unwrap();
        assert!(f.lossless);
        assert!(!f.is_picture());
    }

    #[test]
    fn a_huffman_table_is_not_mistaken_for_a_frame() {
        // C4 sits inside the C0–CF range and comes before the frame in many
        // files. Reading it as one gives dimensions made of table bytes.
        let j = jpeg(0xC0, 8, 1616, 1080, &[(0xC4, vec![0x00, 0x09, 0x99, 0x09, 0x99, 0x03, 0, 0])]);
        let f = frame(&j, 0, j.len() as u64).unwrap();
        assert_eq!((f.width, f.height), (1616, 1080));
    }

    #[test]
    fn finds_the_frame_behind_a_large_exif_block() {
        let big = vec![0u8; 60_000];
        let j = jpeg(0xC2, 8, 6000, 4000, &[(0xE1, big)]);
        let f = frame(&j, 0, j.len() as u64).unwrap();
        assert_eq!((f.width, f.height), (6000, 4000));
    }

    #[test]
    fn something_that_is_not_a_jpeg_has_no_frame() {
        let not = vec![0x49, 0x49, 42, 0, 8, 0, 0, 0];
        assert_eq!(frame(&not, 0, not.len() as u64), None);
    }

    #[test]
    fn an_orientation_already_present_is_rewritten_in_place() {
        let exif = tiff::build(&[vec![tiff::short(0x0112, 1), tiff::ascii(0x0110, "ILCE-7M5")]]);
        let j = jpeg(0xC0, 8, 100, 50, &[exif_segment(&exif)]);
        assert_eq!(own_orientation(&j), Some(1));

        let len = j.len();
        let turned = set_orientation(j, 6);
        assert_eq!(turned.len(), len, "rewriting must not move anything");
        assert_eq!(own_orientation(&turned), Some(6));
        // And the picture is still the picture.
        assert_eq!(frame(&turned, 0, turned.len() as u64).unwrap().width, 100);
    }

    #[test]
    fn an_orientation_is_added_where_there_was_none() {
        let j = jpeg(0xC0, 8, 1616, 1080, &[]);
        assert_eq!(own_orientation(&j), None);

        let turned = set_orientation(j, 8);
        assert_eq!(own_orientation(&turned), Some(8));
        let f = frame(&turned, 0, turned.len() as u64).unwrap();
        assert_eq!((f.width, f.height), (1616, 1080));
    }

    #[test]
    fn an_added_orientation_is_found_before_exif_that_lacks_one() {
        // EXIF without an orientation entry: the new block has to go in front
        // of it, because a reader stops at the first EXIF it meets.
        let exif = tiff::build(&[vec![tiff::ascii(0x0110, "ILCE-7M5")]]);
        let j = jpeg(0xC0, 8, 100, 50, &[exif_segment(&exif)]);
        assert_eq!(own_orientation(&j), None);
        assert_eq!(own_orientation(&set_orientation(j, 3)), Some(3));
    }

    #[test]
    fn a_colour_profile_split_across_segments_comes_back_whole() {
        let part = |seq: u8, data: &[u8]| {
            let mut p = b"ICC_PROFILE\0".to_vec();
            p.extend_from_slice(&[seq, 2]);
            p.extend_from_slice(data);
            (0xE2u8, p)
        };
        // Written out of order, as nothing forbids.
        let j = jpeg(0xC0, 8, 10, 10, &[part(2, b"WORLD"), part(1, b"HELLO")]);
        assert_eq!(icc_profile(&j).as_deref(), Some(&b"HELLOWORLD"[..]));
        assert_eq!(icc_profile(&jpeg(0xC0, 8, 10, 10, &[])), None);
    }

    #[test]
    fn a_colour_profile_written_reads_back_and_the_picture_is_untouched() {
        let plain = jpeg(0xC0, 8, 640, 480, &[]);
        // Longer than one segment can hold, so it has to be split.
        let profile: Vec<u8> = (0..150_000u32).map(|i| (i % 251) as u8).collect();

        let tagged = with_icc_profile(plain.clone(), &profile);
        assert_eq!(icc_profile(&tagged).as_deref(), Some(&profile[..]));
        let f = frame(&tagged, 0, tagged.len() as u64).unwrap();
        assert_eq!((f.width, f.height), (640, 480));

        // Nothing to add is nothing changed.
        assert_eq!(with_icc_profile(plain.clone(), &[]), plain);
    }

    #[test]
    fn upright_with_nothing_said_is_left_alone() {
        let j = jpeg(0xC0, 8, 100, 50, &[]);
        let same = set_orientation(j.clone(), 1);
        assert_eq!(same, j);
    }
}
