//! Just enough TIFF to read a camera's directories.
//!
//! Nearly every RAW format is a TIFF with a different name on it — ARW, NEF,
//! CR2, DNG, PEF, SRW, ORF, RW2 — and so is the EXIF block inside a JPEG and
//! each metadata box inside a Canon CR3. One reader covers all of them.
//!
//! It reads by position and never loads the file. A directory says where a
//! value is; only that value is fetched.

use super::io::ReadAt;

/// One row of a directory: a tag, and where its values are.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub tag: u16,
    pub kind: u16,
    pub count: u32,
    /// Where the first value byte is, as an offset into the source — already
    /// resolved, whether the values sat inside the entry or were pointed at.
    pub data: u64,
}

pub struct Tiff<'a> {
    src: &'a dyn ReadAt,
    /// Where the TIFF header sits in the source. Zero for a RAW file; some way
    /// in for the EXIF block of a JPEG. Every offset a TIFF states is measured
    /// from here, not from the start of the file.
    base: u64,
    big_endian: bool,
    first: u64,
}

/// Bytes per value, by TIFF type number. Zero marks a type this reader does
/// not know, and an entry of unknown type is skipped rather than guessed at.
const TYPE_SIZE: [u64; 14] = [0, 1, 1, 2, 4, 8, 1, 1, 2, 4, 8, 4, 8, 4];

/// A directory larger than this is not a directory. Real ones run to a few
/// dozen entries; a stray pointer into sensor data reads as tens of thousands.
const MAX_ENTRIES: u16 = 1000;

impl<'a> Tiff<'a> {
    /// Opens the TIFF whose header is at `base`, or `None` if there is not one.
    pub fn open(src: &'a dyn ReadAt, base: u64) -> Option<Self> {
        let head = src.read_at(base, 8)?;
        let big_endian = match (head[0], head[1]) {
            (0x49, 0x49) => false, // "II" — Intel
            (0x4D, 0x4D) => true,  // "MM" — Motorola
            _ => return None,
        };
        let u16_at = |i: usize| {
            if big_endian {
                u16::from_be_bytes([head[i], head[i + 1]])
            } else {
                u16::from_le_bytes([head[i], head[i + 1]])
            }
        };
        // 42 is TIFF. Olympus and Panasonic kept the structure and changed the
        // number: "RO"/"RS" for ORF, 0x55 for RW2. Everything after it is the
        // same, so they are let through rather than failed on a signature.
        if !matches!(u16_at(2), 42 | 0x4F52 | 0x5352 | 0x0055) {
            return None;
        }
        let first = if big_endian {
            u32::from_be_bytes([head[4], head[5], head[6], head[7]])
        } else {
            u32::from_le_bytes([head[4], head[5], head[6], head[7]])
        } as u64;
        Some(Self { src, base, big_endian, first })
    }

    /// Offset of the first directory, relative to the header.
    pub fn first_ifd(&self) -> u64 {
        self.first
    }

    pub fn base(&self) -> u64 {
        self.base
    }

    fn u16_of(&self, b: &[u8]) -> u16 {
        if self.big_endian {
            u16::from_be_bytes([b[0], b[1]])
        } else {
            u16::from_le_bytes([b[0], b[1]])
        }
    }

    fn u32_of(&self, b: &[u8]) -> u32 {
        if self.big_endian {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        }
    }

    /// The directory at `offset` (relative to the header): its entries, and
    /// the offset of the directory chained after it, or zero.
    pub fn ifd(&self, offset: u64) -> Option<(Vec<Entry>, u64)> {
        if offset == 0 {
            return None;
        }
        let at = self.base.checked_add(offset)?;
        let count = self.u16_of(&self.src.read_at(at, 2)?);
        if count == 0 || count > MAX_ENTRIES {
            return None;
        }

        // The whole directory in one read: twelve bytes per entry, then the
        // link to the next. A file cut short may lack the link, which is not a
        // reason to throw the entries away.
        let body_len = count as usize * 12;
        let body = self.src.read_at(at + 2, body_len)?;
        let next = self
            .src
            .read_at(at + 2 + body_len as u64, 4)
            .map(|b| self.u32_of(&b) as u64)
            .unwrap_or(0);

        let mut entries = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let e = &body[i * 12..i * 12 + 12];
            let kind = self.u16_of(&e[2..4]);
            let count = self.u32_of(&e[4..8]);
            let size = TYPE_SIZE.get(kind as usize).copied().unwrap_or(0);
            if size == 0 {
                continue;
            }
            // Four bytes or fewer live in the entry itself; anything longer is
            // elsewhere and the entry holds its address.
            let inline = at + 2 + (i as u64) * 12 + 8;
            let data = if size * count as u64 <= 4 {
                inline
            } else {
                let Some(elsewhere) = self.base.checked_add(self.u32_of(&e[8..12]) as u64) else { continue };
                elsewhere
            };
            entries.push(Entry { tag: self.u16_of(&e[0..2]), kind, count, data });
        }
        Some((entries, next))
    }

    /// Up to `max` whole-number values. Covers BYTE, SHORT, LONG and IFD, which
    /// is every type an offset, a length or a dimension is stored as.
    pub fn ints(&self, e: &Entry, max: usize) -> Vec<u64> {
        let n = (e.count as usize).min(max);
        let width = match e.kind {
            1 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 13 => 4,
            _ => return Vec::new(),
        };
        let Some(raw) = self.src.read_at(e.data, n * width) else {
            return Vec::new();
        };
        raw.chunks_exact(width)
            .map(|c| match width {
                1 => c[0] as u64,
                2 => self.u16_of(c) as u64,
                _ => self.u32_of(c) as u64,
            })
            .collect()
    }

    pub fn int(&self, e: &Entry) -> Option<u64> {
        self.ints(e, 1).first().copied()
    }

    /// A text value, trimmed of the NULs and spaces cameras pad them with.
    pub fn text(&self, e: &Entry) -> Option<String> {
        if !matches!(e.kind, 2 | 7 | 1) || e.count == 0 || e.count > 4096 {
            return None;
        }
        let raw = self.src.read_at(e.data, e.count as usize)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let text = String::from_utf8_lossy(&raw[..end]).trim().to_string();
        (!text.is_empty()).then_some(text)
    }

    /// The raw bytes of a value, for blocks this reader hands on unparsed.
    pub fn bytes(&self, e: &Entry, max: usize) -> Option<Vec<u8>> {
        let size = TYPE_SIZE.get(e.kind as usize).copied().unwrap_or(0);
        let len = (size * e.count as u64).min(max as u64) as usize;
        self.src.read_at(e.data, len)
    }

    /// The `index`th value as a number, whatever it was stored as. Exposure
    /// times and apertures are fractions; ISO is a whole number; a few bodies
    /// write floats. A fraction over zero reads as absent rather than infinite.
    pub fn real(&self, e: &Entry, index: usize) -> Option<f64> {
        if index >= e.count as usize {
            return None;
        }
        let size = TYPE_SIZE.get(e.kind as usize).copied()? as usize;
        let b = self.src.read_at(e.data + (index * size) as u64, size)?;
        Some(match e.kind {
            1 | 7 => b[0] as f64,
            6 => b[0] as i8 as f64,
            3 => self.u16_of(&b) as f64,
            8 => self.u16_of(&b) as i16 as f64,
            4 | 13 => self.u32_of(&b) as f64,
            9 => self.u32_of(&b) as i32 as f64,
            5 => {
                let den = self.u32_of(&b[4..8]);
                if den == 0 {
                    return None;
                }
                self.u32_of(&b[0..4]) as f64 / den as f64
            }
            10 => {
                let den = self.u32_of(&b[4..8]) as i32;
                if den == 0 {
                    return None;
                }
                self.u32_of(&b[0..4]) as i32 as f64 / den as f64
            }
            11 => f32::from_bits(self.u32_of(&b)) as f64,
            12 => {
                let bits = if self.big_endian {
                    u64::from_be_bytes(b[..8].try_into().ok()?)
                } else {
                    u64::from_le_bytes(b[..8].try_into().ok()?)
                };
                f64::from_bits(bits)
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
pub(crate) mod testing {
    /// One directory entry: tag, type, value bytes, count.
    pub type Field = (u16, u16, Vec<u8>, u32);

    /// Builds a little-endian TIFF from directories of `(tag, type, values)`,
    /// laid out one after another and chained in order. Values longer than
    /// four bytes are placed after all the directories and pointed at, the way
    /// a real file does it.
    pub fn build(ifds: &[Vec<Field>]) -> Vec<u8> {
        let mut out = vec![0x49, 0x49, 42, 0, 8, 0, 0, 0];
        let dir_bytes: usize = ifds.iter().map(|d| 2 + d.len() * 12 + 4).sum();
        let mut heap_at = 8 + dir_bytes;
        let mut heap: Vec<u8> = Vec::new();

        let mut dir_start = 8usize;
        for (i, dir) in ifds.iter().enumerate() {
            out.extend_from_slice(&(dir.len() as u16).to_le_bytes());
            for (tag, kind, value, count) in dir {
                out.extend_from_slice(&tag.to_le_bytes());
                out.extend_from_slice(&kind.to_le_bytes());
                out.extend_from_slice(&count.to_le_bytes());
                if value.len() <= 4 {
                    let mut inline = value.clone();
                    inline.resize(4, 0);
                    out.extend_from_slice(&inline);
                } else {
                    out.extend_from_slice(&(heap_at as u32).to_le_bytes());
                    heap.extend_from_slice(value);
                    heap_at += value.len();
                }
            }
            dir_start += 2 + dir.len() * 12 + 4;
            let next = if i + 1 < ifds.len() { dir_start as u32 } else { 0 };
            out.extend_from_slice(&next.to_le_bytes());
        }
        out.extend_from_slice(&heap);
        out
    }

    pub fn short(tag: u16, v: u16) -> (u16, u16, Vec<u8>, u32) {
        (tag, 3, v.to_le_bytes().to_vec(), 1)
    }

    pub fn long(tag: u16, v: u32) -> (u16, u16, Vec<u8>, u32) {
        (tag, 4, v.to_le_bytes().to_vec(), 1)
    }

    pub fn ascii(tag: u16, s: &str) -> (u16, u16, Vec<u8>, u32) {
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        let n = b.len() as u32;
        (tag, 2, b, n)
    }

    pub fn rational(tag: u16, num: u32, den: u32) -> (u16, u16, Vec<u8>, u32) {
        let mut b = num.to_le_bytes().to_vec();
        b.extend_from_slice(&den.to_le_bytes());
        (tag, 5, b, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn reads_inline_and_pointed_values() {
        let file = build(&[vec![
            short(0x0112, 6),
            ascii(0x010F, "SONY"),
            ascii(0x0110, "ILCE-7M5"),
            rational(0x829A, 1, 250),
        ]]);
        let tiff = Tiff::open(&file, 0).unwrap();
        let (entries, next) = tiff.ifd(tiff.first_ifd()).unwrap();
        assert_eq!(next, 0);
        assert_eq!(entries.len(), 4);

        let by = |tag| entries.iter().find(|e| e.tag == tag).unwrap();
        assert_eq!(tiff.int(by(0x0112)), Some(6));
        // Four bytes with the NUL would not fit inline; five do not, so "SONY"
        // exercises the pointed path and the trim.
        assert_eq!(tiff.text(by(0x010F)).as_deref(), Some("SONY"));
        assert_eq!(tiff.text(by(0x0110)).as_deref(), Some("ILCE-7M5"));
        assert!((tiff.real(by(0x829A), 0).unwrap() - 0.004).abs() < 1e-9);
    }

    #[test]
    fn follows_the_chain_to_the_next_directory() {
        let file = build(&[vec![short(0x0112, 1)], vec![long(0x0100, 160)]]);
        let tiff = Tiff::open(&file, 0).unwrap();
        let (_, next) = tiff.ifd(tiff.first_ifd()).unwrap();
        let (second, end) = tiff.ifd(next).unwrap();
        assert_eq!(tiff.int(&second[0]), Some(160));
        assert_eq!(end, 0);
    }

    #[test]
    fn offsets_are_measured_from_the_header_not_the_file() {
        // The same TIFF, sitting 30 bytes into something else — which is how
        // EXIF sits inside a JPEG.
        let inner = build(&[vec![ascii(0x0110, "ILCE-7M5")]]);
        let mut outer = vec![0xAAu8; 30];
        outer.extend_from_slice(&inner);

        let tiff = Tiff::open(&outer, 30).unwrap();
        let (entries, _) = tiff.ifd(tiff.first_ifd()).unwrap();
        assert_eq!(tiff.text(&entries[0]).as_deref(), Some("ILCE-7M5"));
    }

    #[test]
    fn rejects_what_is_not_a_tiff_and_what_is_not_a_directory() {
        assert!(Tiff::open(&vec![0xFFu8, 0xD8, 0xFF, 0xE1, 0, 0, 0, 0], 0).is_none());

        // A valid header whose first "directory" claims 60,000 entries.
        let mut bogus = vec![0x49, 0x49, 42, 0, 8, 0, 0, 0];
        bogus.extend_from_slice(&60_000u16.to_le_bytes());
        bogus.resize(4096, 0);
        let tiff = Tiff::open(&bogus, 0).unwrap();
        assert!(tiff.ifd(tiff.first_ifd()).is_none());
    }

    #[test]
    fn a_fraction_over_zero_is_absent() {
        let file = build(&[vec![rational(0x829A, 1, 0)]]);
        let tiff = Tiff::open(&file, 0).unwrap();
        let (entries, _) = tiff.ifd(tiff.first_ifd()).unwrap();
        assert_eq!(tiff.real(&entries[0], 0), None);
    }
}
