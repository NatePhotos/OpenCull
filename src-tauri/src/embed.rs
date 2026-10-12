//! Writing a photo's marks into a JPEG itself.
//!
//! A RAW's marks live in a sidecar beside it, because nobody should write
//! into a RAW. A JPEG is different: Lightroom, Bridge and the rest keep a
//! JPEG's rating and label INSIDE the file, in its XMP packet, and read them
//! from nowhere else. A rating that is only in a sidecar is, to them, not
//! there.
//!
//! This is the one place the app writes into a photograph, and it is written
//! accordingly:
//!
//! - Only the XMP packet is touched — text, in the header. The picture is
//!   never decoded, re-encoded or moved through anything.
//! - Where the packet has room, as Adobe leaves it room, the new text is
//!   written over the old at the same length: nothing else in the file moves
//!   by a byte.
//! - Where it has not — no packet yet, or no room left in it — the file is
//!   written out afresh beside itself, every byte outside the packet checked
//!   against the original, and only then put in the original's place. A
//!   crash or a pulled drive part way leaves the original as it was.
//! - Anything not understood is left alone: a file that is not a JPEG, a
//!   packet that is not text, a packet too big for its segment, a file with
//!   an index of other pictures that a change of size would break.
//!
//! The file's date is put back afterwards. A preview made from it is still
//! a preview of it, and should not be made again for the sake of a star.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::raw::io::FileSource;
use crate::raw::jpeg;
use crate::framing::Stored;
use crate::xmp::{self, Marks};

/// What writing into a file came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The file already said this, or there was nothing to say.
    Unchanged,
    /// The packet was written over at the same length.
    InPlace,
    /// The file was written out afresh with a larger packet.
    Rewritten,
}

/// The room left in a packet written here, for changes made later.
const ROOM: usize = 2048;
const OPEN: &str = "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n";
const CLOSE: &str = "<?xpacket end=\"w\"?>";

fn err(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

/// Spaces, in lines, to exactly `len` bytes.
fn padding(len: usize) -> String {
    (0..len).map(|i| if i % 100 == 99 { '\n' } else { ' ' }).collect()
}

/// Where a packet's own text ends and its trailer begins, and whether the
/// trailer says the packet may be written over. `None` for a packet with no
/// trailer.
fn trailer(packet: &str) -> Option<(usize, bool)> {
    let at = packet.rfind("<?xpacket end=")?;
    let writable = packet[at..].starts_with("<?xpacket end=\"w\"") || packet[at..].starts_with("<?xpacket end='w'");
    Some((at, writable))
}

/// `changed` made exactly as long as `original`, by taking up or giving
/// back the room at its end. `None` if there is not the room, or the packet
/// says it is not to be written over.
fn fitted(original: &str, changed: &str) -> Option<String> {
    let (at, writable) = trailer(changed)?;
    if !writable {
        return None;
    }
    let (body, close) = changed.split_at(at);
    let body = body.trim_end_matches([' ', '\n', '\r', '\t']);
    // At least a line's worth is always left, so a packet is never packed
    // tight against its trailer.
    let room = original.len().checked_sub(body.len() + close.len()).filter(|room| *room >= 1)?;
    Some(format!("{body}{}{close}", padding(room)))
}

/// `changed` with fresh room at its end, for a packet that is being given a
/// segment of a new size anyway.
fn roomy(changed: &str) -> String {
    match trailer(changed) {
        Some((at, _)) => format!("{}\n{}{CLOSE}", changed[..at].trim_end_matches([' ', '\n', '\r', '\t']), padding(ROOM)),
        None => changed.to_string(),
    }
}

/// Writes `marks` into the JPEG at `path`.
pub fn write(path: &Path, marks: Marks) -> io::Result<Outcome> {
    rewrite(path, &|packet| Ok(Some(xmp::update(packet, marks))), &|| Ok((!marks.is_empty()).then(|| xmp::fresh(marks))))
}

/// Writes a crop into the JPEG at `path`, or takes one out, as Adobe's
/// programs keep a JPEG's crop: in its packet, against the picture as it is
/// stored. Which way up the JPEG is, is never touched: the file says that
/// itself, and saying it a second time here is how two programs come to
/// disagree about it.
pub fn write_crop(path: &Path, crop: Option<&Stored>) -> io::Result<Outcome> {
    rewrite(
        path,
        &|packet| {
            // A picture that already has its settings in it is an exported
            // one. A crop added to it would be a crop of a crop.
            if crate::sidecar::read(packet).map_err(err)?.already_applied {
                return Ok(None);
            }
            crate::sidecar::change(packet, crop, None).map(Some).map_err(err)
        },
        &|| match crop {
            Some(crop) => crate::sidecar::change(&xmp::fresh(Marks::default()), Some(crop), None).map(Some).map_err(err),
            None => Ok(None),
        },
    )
}

/// Changes the JPEG's packet with `change`, or — if it has none — gives it
/// the one `fresh` makes. Either may say there is nothing to do.
fn rewrite(path: &Path, change: &dyn Fn(&str) -> io::Result<Option<String>>, fresh: &dyn Fn() -> io::Result<Option<String>>) -> io::Result<Outcome> {
    let before = std::fs::metadata(path)?;
    let source = FileSource::open(path)?;
    let layout = jpeg::layout(&source).ok_or_else(|| err("not a JPEG"))?;
    use crate::raw::io::ReadAt;

    let done = match layout.xmp {
        Some((segment_at, packet_at, len)) => {
            let bytes = source.read_at(packet_at, len).ok_or_else(|| err("the file ends inside its XMP packet"))?;
            let original = String::from_utf8(bytes).map_err(|_| err("its XMP packet is not text"))?;
            let Some(changed) = change(&original)?.filter(|changed| *changed != original) else {
                return Ok(Outcome::Unchanged);
            };
            drop(source);
            match fitted(&original, &changed) {
                Some(same_length) => {
                    over(path, packet_at, original.as_bytes(), same_length.as_bytes())?;
                    Outcome::InPlace
                }
                None => {
                    // The segment changes size, and so everything after it
                    // moves. An index of other pictures ahead of it would be
                    // left pointing at the wrong places.
                    if layout.index_at.is_some_and(|index| index < segment_at) {
                        return Err(err("its XMP packet is full, and making room would break the file's index of pictures"));
                    }
                    let segment = jpeg::xmp_segment(roomy(&changed).as_bytes()).ok_or_else(|| err("its XMP packet is too large to add to"))?;
                    afresh(path, segment_at, packet_at + len as u64, &segment)?;
                    Outcome::Rewritten
                }
            }
        }
        None => {
            // Nothing marked and nothing there: an empty packet says nothing
            // and is not worth opening a photograph to write.
            let Some(new) = fresh()? else {
                return Ok(Outcome::Unchanged);
            };
            drop(source);
            let packet = format!("{OPEN}{new}\n{}{CLOSE}", padding(ROOM));
            let segment = jpeg::xmp_segment(packet.as_bytes()).ok_or_else(|| err("packet too large"))?;
            afresh(path, layout.insert_at, layout.insert_at, &segment)?;
            Outcome::Rewritten
        }
    };

    // The date it had: what it shows has not changed.
    if let Ok(modified) = before.modified() {
        if let Ok(file) = OpenOptions::new().write(true).open(path) {
            file.set_modified(modified).ok();
        }
    }
    Ok(done)
}

/// Writes `new` over `old` at `at`. The two are the same length, and `old`
/// is checked to be what is there first: a file changed by something else
/// since it was read is not written into blind.
fn over(path: &Path, at: u64, old: &[u8], new: &[u8]) -> io::Result<()> {
    debug_assert_eq!(old.len(), new.len());
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let mut there = vec![0u8; old.len()];
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(&mut there)?;
    if there != old {
        return Err(err("the file changed while it was being read"));
    }
    file.seek(SeekFrom::Start(at))?;
    file.write_all(new)?;
    file.sync_data()
}

static STAGED: AtomicU64 = AtomicU64::new(0);

/// Writes the file out again with `segment` in place of bytes `from..to`
/// (which may be none at all), checks it, and puts it where the original was.
fn afresh(path: &Path, from: u64, to: u64, segment: &[u8]) -> io::Result<()> {
    let folder = path.parent().ok_or_else(|| err("no folder"))?;
    let name = path.file_name().ok_or_else(|| err("no name"))?.to_string_lossy();
    let staged: PathBuf = folder.join(format!(".opencull-staging-{}-{}-{name}", std::process::id(), STAGED.fetch_add(1, Ordering::Relaxed)));

    let written = (|| -> io::Result<()> {
        let mut original = File::open(path)?;
        let size = original.metadata()?.len();
        if to > size || from > to {
            return Err(err("the file changed while it was being read"));
        }
        {
            let mut out = File::create(&staged)?;
            io::copy(&mut (&mut original).take(from), &mut out)?;
            out.write_all(segment)?;
            original.seek(SeekFrom::Start(to))?;
            io::copy(&mut original, &mut out)?;
            out.sync_all()?;
        }

        // Every byte that was not the packet must be exactly where it should
        // be in the new file, and the new file exactly as long as that makes
        // it. Nothing is put in the original's place on trust.
        let mut copy = File::open(&staged)?;
        if copy.metadata()?.len() != size - (to - from) + segment.len() as u64 {
            return Err(err("the new file is the wrong length"));
        }
        original.seek(SeekFrom::Start(0))?;
        same(&mut original, &mut copy, from)?;
        let mut middle = vec![0u8; segment.len()];
        copy.read_exact(&mut middle)?;
        if middle != segment {
            return Err(err("the new file does not hold what was written"));
        }
        original.seek(SeekFrom::Start(to))?;
        same(&mut original, &mut copy, size - to)?;

        let permissions = original.metadata()?.permissions();
        // Both let go of before the swap: not every system will replace a
        // file that something still has open.
        drop(copy);
        drop(original);
        std::fs::set_permissions(&staged, permissions).ok();
        std::fs::rename(&staged, path)
    })();
    if written.is_err() {
        std::fs::remove_file(&staged).ok();
    }
    written
}

/// Checks that the next `len` bytes of two files are the same.
fn same(a: &mut File, b: &mut File, len: u64) -> io::Result<()> {
    let (mut left, mut one, mut other) = (len, vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    while left > 0 {
        let n = left.min(one.len() as u64) as usize;
        a.read_exact(&mut one[..n])?;
        b.read_exact(&mut other[..n])?;
        if one[..n] != other[..n] {
            return Err(err("the new file differs from the original outside its XMP packet"));
        }
        left -= n as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::io::ReadAt;
    use crate::raw::jpeg::testing::{exif_segment, jpeg as header};
    use crate::xmp::{Color, Flag};

    const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

    fn xmp(packet: &str) -> (u8, Vec<u8>) {
        (0xE1, [XMP, packet.as_bytes()].concat())
    }

    /// A JPEG with the given header segments and a stretch of "picture"
    /// after them that no change may disturb.
    fn photo(name: &str, extra: &[(u8, Vec<u8>)]) -> (PathBuf, Vec<u8>) {
        let mut bytes = header(0xC0, 8, 6000, 4000, extra);
        bytes.truncate(bytes.len() - 2);
        let picture: Vec<u8> = (0..200_000u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8).filter(|b| *b != 0xFF).collect();
        bytes.extend_from_slice(&picture);
        bytes.extend_from_slice(&[0xFF, 0xD9]);
        let path = std::env::temp_dir().join(format!("opencull-embed-{}-{name}.jpg", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        (path, bytes)
    }

    fn packet_of(path: &Path) -> Option<String> {
        let bytes = std::fs::read(path).unwrap();
        jpeg::xmp_packet(&bytes.as_slice(), 0, bytes.len() as u64)
    }

    fn marks(rating: u8) -> Marks {
        Marks { rating, flag: Flag::None, color: Color::None }
    }

    fn adobe(rating: u8, room: usize) -> String {
        format!(
            "{OPEN}<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  <rdf:Description rdf:about=\"\"\n    xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n    xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n   xmp:Rating=\"{rating}\"\n   dc:format=\"image/jpeg\">\n  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n{}{CLOSE}",
            padding(room)
        )
    }

    #[test]
    fn a_packet_with_room_is_written_over_and_nothing_else_moves() {
        let tiff = b"II*\0\x08\0\0\0\0\0\0\0\0\0".to_vec();
        let (path, before) = photo("inplace", &[exif_segment(&tiff), xmp(&adobe(5, 4097))]);

        assert_eq!(write(&path, Marks { rating: 3, flag: Flag::Pick, color: Color::Red }).unwrap(), Outcome::InPlace);
        let after = std::fs::read(&path).unwrap();
        assert_eq!(after.len(), before.len(), "not a byte longer or shorter");

        let (_, at, len) = jpeg::layout(&before.as_slice()).unwrap().xmp.unwrap();
        let (at, end) = (at as usize, at as usize + len);
        assert_eq!(after[..at], before[..at], "everything before the packet is as it was");
        assert_eq!(after[end..], before[end..], "and everything after it, the picture included");

        let packet = packet_of(&path).unwrap();
        assert_eq!(xmp::parse(&packet), Marks { rating: 3, flag: Flag::Pick, color: Color::Red });
        assert!(packet.contains("dc:format=\"image/jpeg\""), "what else the packet said, it still says");
        assert!(packet.contains("xmlns:crs="), "a prefix is declared before it is used");
        assert!(packet.ends_with(CLOSE));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn saying_what_the_file_already_says_does_not_touch_it() {
        let (path, before) = photo("same", &[xmp(&adobe(4, 500))]);
        assert_eq!(write(&path, marks(4)).unwrap(), Outcome::Unchanged);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_jpeg_with_no_packet_is_given_one_and_the_picture_is_untouched() {
        let tiff = b"II*\0\x08\0\0\0\0\0\0\0\0\0".to_vec();
        let (path, before) = photo("fresh", &[exif_segment(&tiff), (0xE2, b"ICC_PROFILE\0\x01\x01profile".to_vec())]);
        assert_eq!(packet_of(&path), None);

        assert_eq!(write(&path, marks(5)).unwrap(), Outcome::Rewritten);
        let after = std::fs::read(&path).unwrap();
        let layout = jpeg::layout(&after.as_slice()).unwrap();
        let (segment_at, at, len) = layout.xmp.unwrap();
        let insert = jpeg::layout(&before.as_slice()).unwrap().insert_at as usize;
        assert_eq!(segment_at as usize, insert, "straight after the EXIF block");
        assert_eq!(after[..insert], before[..insert]);
        assert_eq!(after[at as usize + len..], before[insert..], "the rest of the file follows, byte for byte");
        assert_eq!(xmp::parse(&packet_of(&path).unwrap()).rating, 5);

        // And the next change has room, so the file is not written out again.
        assert_eq!(write(&path, marks(2)).unwrap(), Outcome::InPlace);
        assert_eq!(std::fs::read(&path).unwrap().len(), after.len());
        assert_eq!(xmp::parse(&packet_of(&path).unwrap()).rating, 2);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_crop_goes_into_the_packet_beside_the_marks_and_comes_out_again() {
        let crop = Stored { left: 0.1, top: 0.2, right: 0.9, bottom: 0.8, angle: -1.25 };
        let crop_of = |path: &Path| crate::sidecar::read(&packet_of(path).unwrap()).unwrap().crop;

        // A packet with room: written over, and the rating in it kept.
        let (path, before) = photo("crop-inplace", &[xmp(&adobe(5, 4097))]);
        assert_eq!(write_crop(&path, Some(&crop)).unwrap(), Outcome::InPlace);
        assert_eq!(std::fs::read(&path).unwrap().len(), before.len());
        assert_eq!(crop_of(&path), Some(crop));
        assert_eq!(xmp::parse(&packet_of(&path).unwrap()).rating, 5);
        assert!(!packet_of(&path).unwrap().contains("Orientation"), "which way up a JPEG is, is the file's to say");

        assert_eq!(write_crop(&path, None).unwrap(), Outcome::InPlace);
        assert_eq!(crop_of(&path), None);
        // Taken out again, all that is left of it is the namespace it named.
        let after = std::fs::read(&path).unwrap();
        assert_eq!(after.len(), before.len());
        assert!(!packet_of(&path).unwrap().contains("Crop"));
        let (_, at, len) = jpeg::layout(&before.as_slice()).unwrap().xmp.unwrap();
        assert_eq!(after[at as usize + len..], before[at as usize + len..], "the picture untouched throughout");
        std::fs::remove_file(path).ok();

        // No packet at all: no crop to take out is nothing to do, and a crop
        // to put in makes one.
        let (path, before) = photo("crop-fresh", &[]);
        assert_eq!(write_crop(&path, None).unwrap(), Outcome::Unchanged);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(write_crop(&path, Some(&crop)).unwrap(), Outcome::Rewritten);
        assert_eq!(crop_of(&path), Some(crop));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_picture_with_its_settings_already_in_it_is_not_cropped_again() {
        let exported = adobe(3, 2000).replace("xmp:Rating=", "xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" crs:AlreadyApplied=\"True\" xmp:Rating=");
        let (path, before) = photo("crop-applied", &[xmp(&exported)]);
        let crop = Stored { left: 0.1, top: 0.2, right: 0.9, bottom: 0.8, angle: 0.0 };
        assert_eq!(write_crop(&path, Some(&crop)).unwrap(), Outcome::Unchanged);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn nothing_marked_and_nothing_there_leaves_the_file_alone() {
        let (path, before) = photo("empty", &[]);
        assert_eq!(write(&path, Marks::default()).unwrap(), Outcome::Unchanged);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_packet_with_no_room_left_is_given_more_and_the_picture_is_untouched() {
        let (path, before) = photo("full", &[xmp(&adobe(1, 2))]);
        assert_eq!(write(&path, Marks { rating: 5, flag: Flag::Reject, color: Color::Purple }).unwrap(), Outcome::Rewritten);
        let after = std::fs::read(&path).unwrap();
        let (_, at, len) = jpeg::layout(&before.as_slice()).unwrap().xmp.unwrap();
        let (_, new_at, new_len) = jpeg::layout(&after.as_slice()).unwrap().xmp.unwrap();
        assert_eq!(after[new_at as usize + new_len..], before[at as usize + len..]);
        assert_eq!(xmp::parse(&packet_of(&path).unwrap()), Marks { rating: 5, flag: Flag::Reject, color: Color::Purple });
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_packet_marked_not_to_be_written_over_is_replaced_whole_not_written_into() {
        let read_only = adobe(1, 4000).replace("end=\"w\"", "end=\"r\"");
        let (path, _) = photo("readonly", &[xmp(&read_only)]);
        assert_eq!(write(&path, marks(4)).unwrap(), Outcome::Rewritten);
        assert_eq!(xmp::parse(&packet_of(&path).unwrap()).rating, 4);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_file_whose_index_of_pictures_would_break_is_refused_and_left_as_it_was() {
        // The index comes BEFORE the packet, and the packet has no room: to
        // change it is to move everything the index points at.
        let (path, before) = photo("index", &[(0xE2, b"MPF\0II*\0index".to_vec()), xmp(&adobe(1, 2))]);
        let longer = Marks { rating: 5, flag: Flag::Reject, color: Color::Purple };
        assert!(write(&path, longer).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        // A change that happens to be no longer is no trouble: it fits.
        assert_eq!(write(&path, marks(5)).unwrap(), Outcome::InPlace);

        // With room, it is written over in place, and the index is none the wiser.
        let (path2, _) = photo("index-room", &[(0xE2, b"MPF\0II*\0index".to_vec()), xmp(&adobe(1, 3000))]);
        assert_eq!(write(&path2, marks(5)).unwrap(), Outcome::InPlace);
        std::fs::remove_file(path).ok();
        std::fs::remove_file(path2).ok();
    }

    #[test]
    fn a_new_packet_goes_ahead_of_an_index_of_pictures_so_the_index_still_holds() {
        let tiff = b"II*\0\x08\0\0\0\0\0\0\0\0\0".to_vec();
        let (path, before) = photo("index-after", &[exif_segment(&tiff), (0xE2, b"MPF\0II*\0index".to_vec())]);
        assert_eq!(write(&path, marks(3)).unwrap(), Outcome::Rewritten);
        let after = std::fs::read(&path).unwrap();
        let layout = jpeg::layout(&after.as_slice()).unwrap();
        assert!(layout.xmp.unwrap().0 < layout.index_at.unwrap(), "the index and what it points at move together");
        let grown = after.len() - before.len();
        assert_eq!(layout.index_at.unwrap() as usize, jpeg::layout(&before.as_slice()).unwrap().index_at.unwrap() as usize + grown);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn what_is_not_a_jpeg_or_not_text_is_refused_and_left_as_it_was() {
        let path = std::env::temp_dir().join(format!("opencull-embed-{}-notjpeg.jpg", std::process::id()));
        std::fs::write(&path, b"this is not a photograph").unwrap();
        assert!(write(&path, marks(5)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"this is not a photograph");
        std::fs::remove_file(&path).ok();

        let (path, before) = photo("binary", &[(0xE1, [XMP, &[0xFFu8, 0xFE, 0x00, 0x80][..]].concat())]);
        assert!(write(&path, marks(5)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn the_files_date_is_what_it_was_and_nothing_is_left_beside_it() {
        let (path, _) = photo("date", &[]);
        let then = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        File::options().write(true).open(&path).unwrap().set_modified(then).unwrap();

        assert_eq!(write(&path, marks(5)).unwrap(), Outcome::Rewritten);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), then);
        assert_eq!(write(&path, marks(1)).unwrap(), Outcome::InPlace);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), then);

        let stem = path.file_name().unwrap().to_string_lossy().into_owned();
        let left: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.contains("staging") && n.contains(&stem)).collect();
        assert!(left.is_empty(), "left behind: {left:?}");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_rating_cleared_is_cleared_in_the_file_too() {
        let (path, _) = photo("clear", &[xmp(&adobe(5, 1000))]);
        assert_eq!(write(&path, Marks::default()).unwrap(), Outcome::InPlace);
        let packet = packet_of(&path).unwrap();
        assert_eq!(xmp::parse(&packet), Marks::default());
        assert!(!packet.contains("xmp:Rating"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn padding_is_exactly_as_long_as_asked() {
        for len in [0, 1, 99, 100, 101, 2048] {
            assert_eq!(padding(len).len(), len);
        }
        assert_eq!(source_len(&adobe(1, 10)), adobe(9, 10).len());
    }

    fn source_len(text: &str) -> usize {
        (&text.as_bytes()).size() as usize
    }
}
