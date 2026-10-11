//! Reading a few bytes from the right places in a very large file.
//!
//! A RAW is tens of megabytes, almost all of it sensor data this app never
//! looks at. What it wants is scattered and small: a directory at the front,
//! a tag here, a preview's header there. Reading the file whole to get at
//! those is the single most expensive thing a preview pipeline can do, most of
//! all off a memory card, so everything in `raw` reads by position instead.

use std::cell::RefCell;
use std::fs::File;
use std::io;
use std::path::Path;

/// Anything bytes can be read out of by position.
///
/// Two things implement it: a file on disk, and a block of bytes already in
/// memory. The second matters because metadata nests — the EXIF inside a JPEG
/// is a whole TIFF of its own, as is each metadata box inside a Canon CR3 —
/// and the same parser has to work on all of them.
pub trait ReadAt {
    /// How many bytes there are. Not called `len`: this is implemented for
    /// `&[u8]`, where a second `len` returning a different type would be
    /// picked over the slice's own whenever a reference to one was in hand.
    fn size(&self) -> u64;

    /// Exactly `len` bytes starting at `offset`, or `None` if the file does
    /// not hold all of them. Never a short read: a caller that asked for
    /// twelve bytes and got eight would parse garbage as a directory entry.
    fn read_at(&self, offset: u64, len: usize) -> Option<Vec<u8>>;
}

// On the reference rather than on `[u8]` itself: a parser takes `&dyn ReadAt`,
// and a bare slice has no size to make one from.
impl ReadAt for &[u8] {
    fn size(&self) -> u64 {
        <[u8]>::len(self) as u64
    }

    fn read_at(&self, offset: u64, len: usize) -> Option<Vec<u8>> {
        let start = usize::try_from(offset).ok()?;
        let end = start.checked_add(len)?;
        self.get(start..end).map(<[u8]>::to_vec)
    }
}

impl ReadAt for Vec<u8> {
    fn size(&self) -> u64 {
        self.as_slice().len() as u64
    }

    fn read_at(&self, offset: u64, len: usize) -> Option<Vec<u8>> {
        (&self.as_slice()).read_at(offset, len)
    }
}

/// How much is fetched around each small read.
///
/// A directory walk asks for two bytes, then twelve, then four, a few dozen
/// times over, nearly all of it within the same stretch of file. Fetching a
/// block around the first request turns the rest into memory lookups.
const BLOCK: u64 = 64 * 1024;

/// How many blocks are remembered. Sixteen is a megabyte, which covers every
/// directory this app has been pointed at with room to spare.
const MAX_BLOCKS: usize = 16;

/// Reads longer than this skip the block cache and go straight to the file.
/// They are previews being lifted out whole, and caching them would only
/// evict the directory blocks the next read wants.
const DIRECT_READ: usize = 4 * BLOCK as usize;

/// A file opened for positioned reads, with a small cache of recent blocks.
pub struct FileSource {
    file: File,
    len: u64,
    block: u64,
    blocks: RefCell<Vec<(u64, Vec<u8>)>>,
}

impl FileSource {
    pub fn open(path: &Path) -> io::Result<Self> {
        Self::open_with_block(path, BLOCK)
    }

    /// As `open`, fetching `block` bytes around each read instead of the
    /// usual amount. For a reader that wants one small thing from the front
    /// of a file and nothing else, where the usual block is mostly bytes it
    /// will never look at — paid for per photo, across a whole folder.
    pub fn open_with_block(path: &Path, block: u64) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self { file, len, block: block.max(512), blocks: RefCell::new(Vec::new()) })
    }

    /// The block holding `index`, read on first use.
    fn with_block<T>(&self, index: u64, f: impl FnOnce(&[u8]) -> T) -> Option<T> {
        let mut blocks = self.blocks.borrow_mut();
        if let Some(pos) = blocks.iter().position(|(i, _)| *i == index) {
            return Some(f(&blocks[pos].1));
        }

        let start = index * self.block;
        if start >= self.len {
            return None;
        }
        let size = self.block.min(self.len - start) as usize;
        let mut data = vec![0u8; size];
        pread_exact(&self.file, start, &mut data).ok()?;

        if blocks.len() >= MAX_BLOCKS {
            // Oldest out. The first block is the one exception — it holds the
            // file header and the first directory, and is read again by nearly
            // every later step.
            let victim = if blocks[0].0 == 0 && blocks.len() > 1 { 1 } else { 0 };
            blocks.remove(victim);
        }
        blocks.push((index, data));
        Some(f(&blocks.last().expect("just pushed").1))
    }
}

impl ReadAt for FileSource {
    fn size(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, len: usize) -> Option<Vec<u8>> {
        let end = offset.checked_add(len as u64)?;
        if end > self.len {
            return None;
        }
        if len == 0 {
            return Some(Vec::new());
        }

        if len > DIRECT_READ.max(4 * self.block as usize) {
            let mut out = vec![0u8; len];
            pread_exact(&self.file, offset, &mut out).ok()?;
            return Some(out);
        }

        let mut out = Vec::with_capacity(len);
        let mut pos = offset;
        while pos < end {
            let index = pos / self.block;
            let within = (pos - index * self.block) as usize;
            let want = (end - pos) as usize;
            let copied = self.with_block(index, |block| {
                let take = want.min(block.len().saturating_sub(within));
                out.extend_from_slice(&block[within..within + take]);
                take
            })?;
            if copied == 0 {
                return None;
            }
            pos += copied as u64;
        }
        Some(out)
    }
}

/// Fills `buf` from `offset`, whatever the platform calls that.
fn pread_exact(file: &File, mut offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
    while !buf.is_empty() {
        #[cfg(unix)]
        let read = std::os::unix::fs::FileExt::read_at(file, buf, offset)?;
        #[cfg(windows)]
        let read = std::os::windows::fs::FileExt::seek_read(file, buf, offset)?;

        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        offset += read as u64;
        buf = &mut buf[read..];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn slice_reads_are_exact_or_nothing() {
        let data: Vec<u8> = (0..10).collect();
        assert_eq!(data.read_at(2, 3), Some(vec![2, 3, 4]));
        assert_eq!(data.read_at(8, 2), Some(vec![8, 9]));
        assert_eq!(data.read_at(8, 3), None);
        assert_eq!(data.read_at(u64::MAX, 1), None);
    }

    #[test]
    fn file_reads_match_the_bytes_across_block_edges() {
        let dir = std::env::temp_dir().join(format!("opencull-io-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("blocks.bin");

        // Six and a bit blocks — longer than a direct read, so both paths are
        // exercised — each byte derived from its position so a read landing in
        // the wrong place cannot pass by coincidence.
        let size = (BLOCK * 6 + 1000) as usize;
        let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        std::fs::File::create(&path).unwrap().write_all(&data).unwrap();

        let src = FileSource::open(&path).unwrap();
        assert_eq!(src.size(), size as u64);

        // Straddling the first block boundary.
        let at = BLOCK as usize - 5;
        assert_eq!(src.read_at(at as u64, 10).unwrap(), &data[at..at + 10]);
        // Long enough to bypass the cache entirely.
        assert_eq!(src.read_at(100, DIRECT_READ + 7).unwrap(), &data[100..100 + DIRECT_READ + 7]);
        // The last bytes of the file, and one past them.
        assert_eq!(src.read_at(size as u64 - 4, 4).unwrap(), &data[size - 4..]);
        assert_eq!(src.read_at(size as u64 - 4, 5), None);

        std::fs::remove_dir_all(&dir).ok();
    }
}
