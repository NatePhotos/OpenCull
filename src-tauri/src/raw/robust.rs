//! Damaged files never take the app down.
//!
//! Every reader here parses bytes nobody vouches for. These tests feed each
//! one thousands of broken files and plain noise, and ask only that it
//! answers rather than panicking. They run in debug builds, where unchecked
//! arithmetic that overflows panics. For a longer search, see `src-tauri/fuzz`.

use super::io::ReadAt;
use super::jpeg::testing::{exif_segment, jpeg};
use super::tiff::testing::{ascii, build, long};
use super::{bmff, capture_time_source, jpeg as jpeg_reader, probe_source};

/// A small, fixed random source, so a failure can be run again exactly.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Good files of every kind the readers know, to break.
fn seeds() -> Vec<Vec<u8>> {
    let pointer = 8 + (2 + 2 * 12 + 4) as u32;
    let tiff = build(&[
        vec![ascii(0x0110, "ILCE-7M5"), long(0x8769, pointer)],
        vec![ascii(0x9003, "2026:10:06 19:41:07"), ascii(0x9291, "218")],
    ]);
    let jpeg_file = jpeg(0xC0, 8, 7008, 4672, &[exif_segment(&tiff)]);

    let mut raf = b"FUJIFILMCCD-RAW 0201FF129502X-T2".to_vec();
    raf.resize(84, 0);
    raf.extend_from_slice(&148u32.to_be_bytes());
    raf.extend_from_slice(&(jpeg_file.len() as u32).to_be_bytes());
    raf.resize(148, 0);
    raf.extend_from_slice(&jpeg_file);

    let cr3 = bmff::testing::cr3(&tiff, &build(&[vec![ascii(0x9003, "2026:10:06 19:41:07")]]));

    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&6000u32.to_be_bytes());
    png.extend_from_slice(&4000u32.to_be_bytes());

    // A RAW whose only preview is found by the slow search.
    let mut hidden = build(&[vec![ascii(0x0110, "ODD")]]);
    hidden.resize(4096, 0x11);
    let mut big = jpeg(0xC0, 8, 4000, 3000, &[]);
    big.truncate(big.len() - 2);
    big.resize(big.len() + 210_000, 0x22);
    big.extend_from_slice(&[0xFF, 0xD9]);
    hidden.extend_from_slice(&big);

    vec![tiff, jpeg_file, raf, cr3, png, hidden]
}

/// One way of breaking a file.
fn damage(rng: &mut Rng, mut bytes: Vec<u8>) -> Vec<u8> {
    for _ in 0..1 + rng.below(4) {
        if bytes.is_empty() {
            break;
        }
        let at = rng.below(bytes.len());
        match rng.below(8) {
            // A byte changed.
            0 => bytes[at] = rng.next() as u8,
            // Cut short.
            1 => bytes.truncate(at),
            // A length or an offset replaced with something large.
            2 => {
                let value = [u32::MAX, u32::MAX - 7, 0x8000_0000, rng.next() as u32][rng.below(4)];
                for (i, b) in value.to_le_bytes().into_iter().enumerate() {
                    if let Some(slot) = bytes.get_mut(at + i) {
                        *slot = b;
                    }
                }
            }
            // A run of 0xFF, which JPEG readers treat as padding.
            3 => {
                let run = rng.below(3000);
                bytes.splice(at..at, std::iter::repeat_n(0xFF, run));
            }
            // A stretch zeroed: a cut-off write.
            4 => {
                let end = (at + rng.below(64)).min(bytes.len());
                bytes[at..end].fill(0);
            }
            // A CR3 box size made huge, as four bytes or as the eight-byte
            // size that a size of one says follows.
            6 | 7 => {
                let boxes: Vec<usize> = bytes.windows(4).enumerate().filter(|(_, w)| [*b"ftyp", *b"moov", *b"uuid", *b"CMT1", *b"CMT2", *b"trak"].contains(&[w[0], w[1], w[2], w[3]])).map(|(i, _)| i).filter(|i| *i >= 4).collect();
                if let Some(&kind) = boxes.get(rng.below(boxes.len())) {
                    let size_at = kind - 4;
                    if rng.below(2) == 0 {
                        bytes[size_at..kind].copy_from_slice(&[u32::MAX, u32::MAX - 3, 0x7FFF_FFFF][rng.below(3)].to_be_bytes());
                    } else {
                        bytes[size_at..kind].copy_from_slice(&1u32.to_be_bytes());
                        let wide = [u64::MAX, u64::MAX - 15, 1 << 63][rng.below(3)].to_be_bytes();
                        let at = kind + 4;
                        let room = bytes.len().saturating_sub(at).min(8);
                        bytes[at..at + room].copy_from_slice(&wide[..room]);
                    }
                }
            }
            // A piece of the file copied over another, so pointers lead back
            // into it: loops a reader must not follow forever.
            _ => {
                let from = rng.below(bytes.len());
                let len = rng.below(32).min(bytes.len() - from).min(bytes.len() - at);
                let piece = bytes[from..from + len].to_vec();
                bytes[at..at + len].copy_from_slice(&piece);
            }
        }
    }
    bytes
}

/// Everything that reads a file's bytes, on one file. Answers are not
/// checked; only that each gives one.
fn read_all_ways(bytes: &[u8]) {
    let src: &dyn ReadAt = &bytes;
    let probe = probe_source(src);
    for e in &probe.embedded {
        assert!(e.offset.checked_add(e.len).is_some_and(|end| end <= bytes.len() as u64), "a preview reaching past the end of the file");
    }
    let _ = capture_time_source(src);
    let _ = jpeg_reader::frame(src, 0, src.size());
    let _ = jpeg_reader::layout(src);
    let _ = jpeg_reader::exif_tiff(src, 0, src.size());
    let _ = jpeg_reader::xmp_packet(src, 0, src.size());
    let _ = bmff::scan(src);
    let _ = jpeg_reader::set_orientation(bytes.to_vec(), 6);
}

#[test]
fn damaged_files_of_every_kind_are_read_without_panicking() {
    let mut rng = Rng(0x0C0F_FEE5);
    for seed in seeds() {
        read_all_ways(&seed);
        for _ in 0..600 {
            read_all_ways(&damage(&mut rng, seed.clone()));
        }
    }
}

#[test]
fn noise_is_read_without_panicking() {
    let mut rng = Rng(0x5EED);
    for _ in 0..500 {
        let len = rng.below(4096);
        let mut noise: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        // Half get a real format's header, to get past the first check.
        let heads: [&[u8]; 5] = [b"II*\0", b"MM\0*", &[0xFF, 0xD8, 0xFF], b"\0\0\0\x18ftypcrx ", b"FUJIFILMCCD-RAW "];
        if rng.below(2) == 0 {
            let head = heads[rng.below(heads.len())];
            noise.splice(0..0, head.iter().copied());
        }
        read_all_ways(&noise);
    }
}
