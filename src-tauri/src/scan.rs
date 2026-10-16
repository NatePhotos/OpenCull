//! Listing a folder and deciding what in it is a photo.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::raw::{extension, JPEG_EXTENSIONS, RAW_EXTENSIONS};

/// Everything the scan treats as a photo. HEIC is absent on purpose: nothing
/// portable decodes it, and listing files that then draw as blank cells is
/// worse than leaving them out.
pub(crate) fn is_image(ext: &str) -> bool {
    RAW_EXTENSIONS.contains(&ext) || JPEG_EXTENSIONS.contains(&ext) || ext == "png"
}

/// Containers offered for playback; the webview decides codec support.
pub fn is_video(ext: &str) -> bool {
    matches!(ext, "mp4" | "mov" | "m4v" | "avi" | "mkv" | "webm" | "mts" | "m2ts" | "mpg" | "mpeg" | "3gp")
}

/// One photo as the scan found it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Scanned {
    /// The file that owns the photo's marks and sidecar.
    pub path: PathBuf,
    /// The file drawn for it. For a RAW with a JPEG of the same name beside
    /// it, the JPEG; otherwise the file itself.
    pub display: PathBuf,
    pub name: String,
    /// Folders between the opened folder and this photo, e.g. `100MSDCF`.
    /// `None` for a photo sitting directly in the folder that was opened.
    pub subpath: Option<String>,
    /// Changes whenever the drawn file does. Part of every preview's address,
    /// so a file edited or replaced on disk is never shown from a stale cache.
    ///
    /// It is the file's date and not also its size: a rating written into a
    /// JPEG can make the file a little longer without changing the picture,
    /// and its date is put back afterwards. Counting the size made every
    /// photo rated in a sitting a stranger the next time its folder opened,
    /// with its previews to make and its faces to find all over again.
    pub version: String,
    pub size: u64,
    /// When the owning file was last changed, in milliseconds. With its size,
    /// how anything remembered about the file knows whether it still applies.
    pub modified: u64,
}

/// Which of a folder's files are its photos.
///
/// A camera set to write both saves every frame twice, a RAW and a JPEG of
/// the same name. Which of the two a photographer culls is theirs to say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kinds {
    /// Everything. A RAW and its JPEG are one photo, drawn from the JPEG:
    /// marked together, moved together, binned together. Their marks are one
    /// sidecar, named for the frame, so a rating given to the pair is on both.
    #[default]
    Both,
    /// RAW files alone. A JPEG beside one is left exactly where it is.
    Raw,
    /// Everything that is not a RAW. A RAW beside one is left where it is.
    Jpeg,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub images: Vec<Scanned>,
    /// Photos not listed because of which kinds were asked for: a JPEG with
    /// no RAW when only RAWs are wanted, a RAW with no JPEG when only JPEGs
    /// are. A frame shown through its other file is not one of these. Only
    /// there to explain why a folder that looks full came up short or empty.
    pub left_out: usize,
    /// Folders sitting directly inside the one opened. Only there to explain
    /// why a folder that looks full came up empty.
    pub subfolder_count: usize,
}

/// Folders that are really single documents. Walking into a Photos library or
/// an app turns up tens of thousands of files that are not the photographer's
/// to cull.
const PACKAGES: &[&str] = &["app", "photoslibrary", "lrdata", "lrlibrary", "bundle", "framework", "cocatalog"];

pub(crate) fn hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// What a Sony card keeps beside its clips: low-resolution copies (SUB), a
/// small picture for each clip (THMBNL), and the camera's own records. None
/// of it is something the photographer shot, and importing it doubles every clip.
pub(crate) fn camera_support_folder(path: &Path) -> bool {
    let named = |p: &Path| p.file_name().and_then(|n| n.to_str()).map(str::to_lowercase);
    named(path).is_some_and(|n| matches!(n.as_str(), "sub" | "thmbnl" | "general" | "take"))
        && path.parent().and_then(named).is_some_and(|p| p == "m4root")
}

/// Compares the way a file browser does: runs of digits by their value, and
/// letters without regard to case, so `IMG_9.ARW` comes before `IMG_10.ARW`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            // Leading zeroes do not make a number bigger.
            let da = a[si..i].iter().skip_while(|&&c| c == b'0').copied().collect::<Vec<_>>();
            let db = b[sj..j].iter().skip_while(|&&c| c == b'0').copied().collect::<Vec<_>>();
            let by_value = da.len().cmp(&db.len()).then_with(|| da.cmp(&db));
            if by_value != Ordering::Equal {
                return by_value;
            }
        } else {
            let (ca, cb) = (a[i].to_ascii_lowercase(), b[j].to_ascii_lowercase());
            if ca != cb {
                return ca.cmp(&cb);
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j)).then_with(|| a.cmp(b))
}

struct Found {
    path: PathBuf,
    ext: String,
    size: u64,
    modified: u128,
}

fn list(dir: &Path, recurse: bool, out: &mut Vec<Found>, subfolders: Option<&mut usize>, include_videos: bool) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut dirs = Vec::new();
    let mut count = 0usize;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if hidden(name) {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        // A symlink is followed for what it points at, so a shoot linked in
        // from another drive opens like any other.
        let is_dir = if kind.is_symlink() { path.is_dir() } else { kind.is_dir() };

        if is_dir {
            if camera_support_folder(&path) {
                continue;
            }
            count += 1;
            if recurse && !PACKAGES.contains(&extension(&path).as_str()) {
                dirs.push(path);
            }
            continue;
        }

        let ext = extension(&path);
        if !is_image(&ext) && !(include_videos && is_video(&ext)) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0);
        out.push(Found { path, ext, size: meta.len(), modified });
    }

    if let Some(total) = subfolders {
        *total = count;
    }
    for sub in dirs {
        list(&sub, true, out, None, include_videos);
    }
}

/// Lists `folder` and pairs each RAW with its JPEG.
///
/// With `include_subfolders` it descends the whole tree, so a shoot split
/// across a card's own folders — 100MSDCF, 101MSDCF — culls as one.
pub fn scan_folder(folder: &Path, include_subfolders: bool) -> ScanResult {
    scan_folder_of(folder, include_subfolders, Kinds::Both)
}

/// The same, listing only the kinds of file asked for.
pub fn scan_folder_of(folder: &Path, include_subfolders: bool, kinds: Kinds) -> ScanResult {
    scan_folder_media(folder, include_subfolders, kinds, false)
}

pub fn scan_folder_media(folder: &Path, include_subfolders: bool, kinds: Kinds, include_videos: bool) -> ScanResult {
    let mut found = Vec::new();
    let mut subfolder_count = 0;
    list(folder, include_subfolders, &mut found, Some(&mut subfolder_count), include_videos);

    // Only one kind wanted: the other is dropped here, before anything is
    // paired, so that a frame is its own file and brings nothing with it.
    let mut left_out = 0;
    if kinds != Kinds::Both {
        let is_raw = |f: &Found| RAW_EXTENSIONS.contains(&f.ext.as_str());
        let frame = |f: &Found| (f.path.parent().map(Path::to_path_buf).unwrap_or_default(), f.path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase());
        let wanted = |f: &Found| is_video(&f.ext) || is_raw(f) == (kinds == Kinds::Raw);
        let shown: HashSet<(PathBuf, String)> = found.iter().filter(|f| wanted(f)).map(frame).collect();
        left_out = found.iter().filter(|f| !wanted(f) && !shown.contains(&frame(f))).map(frame).collect::<HashSet<_>>().len();
        found.retain(wanted);
    }

    // Pair only exact, case-insensitive base names **within one folder**:
    // IMG_1234.ARW + IMG_1234.JPG. The folder has to be part of the key.
    // Cameras restart numbering at 0001 in every card folder, so keyed on the
    // bare name a scan spanning subfolders pairs a RAW with a same-numbered
    // JPEG from a different day's shooting.
    let key = |p: &Path| -> (PathBuf, String) {
        (
            p.parent().map(Path::to_path_buf).unwrap_or_default(),
            p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase(),
        )
    };

    let mut jpeg_for: HashMap<(PathBuf, String), usize> = HashMap::new();
    for (i, f) in found.iter().enumerate() {
        if JPEG_EXTENSIONS.contains(&f.ext.as_str()) {
            jpeg_for
                .entry(key(&f.path))
                // Deterministic if both .jpg and .jpeg somehow exist.
                .and_modify(|existing| {
                    if natural_cmp(&name_of(&f.path), &name_of(&found[*existing].path)) == Ordering::Less {
                        *existing = i;
                    }
                })
                .or_insert(i);
        }
    }

    let paired: HashSet<usize> = found
        .iter()
        .filter(|f| RAW_EXTENSIONS.contains(&f.ext.as_str()))
        .filter_map(|f| jpeg_for.get(&key(&f.path)).copied())
        .collect();

    let mut images: Vec<Scanned> = Vec::with_capacity(found.len());
    for (i, f) in found.iter().enumerate() {
        if paired.contains(&i) {
            continue; // shown through its RAW
        }
        let shown = if RAW_EXTENSIONS.contains(&f.ext.as_str()) {
            jpeg_for.get(&key(&f.path)).map(|&j| &found[j]).unwrap_or(f)
        } else {
            f
        };
        let subpath = f
            .path
            .parent()
            .and_then(|p| p.strip_prefix(folder).ok())
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .filter(|rel| !rel.is_empty());
        images.push(Scanned {
            path: f.path.clone(),
            display: shown.path.clone(),
            name: name_of(&f.path),
            subpath,
            version: format!("{:x}", shown.modified),
            size: f.size,
            modified: f.modified as u64,
        });
    }

    // Folder first, then name — not the file's date. Cameras number frames in
    // the order they were shot, so the name tracks capture time; a file's date
    // only records when it was copied, and an offload that writes the last
    // frame first leaves those running backwards. Whole folders stay together
    // because frame numbers restart in each one.
    images.sort_by(|a, b| {
        natural_cmp(a.subpath.as_deref().unwrap_or(""), b.subpath.as_deref().unwrap_or(""))
            .then_with(|| natural_cmp(&a.name, &b.name))
    });

    ScanResult { images, left_out, subfolder_count }
}

fn name_of(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string()
}

/// A few photos to stand for a folder on the home screen: spread across it
/// rather than the first few, which in a real shoot are test frames of a
/// wall. Looks one level down when the folder itself holds none, since a card
/// keeps its photos in a folder of its own.
pub fn cover_images(folder: &Path, count: usize) -> Vec<Scanned> {
    let images = cover_pool(folder, false);
    // A folder of nothing but clips has no photographs to show: its clips stand in.
    let images = if images.is_empty() { cover_pool(folder, true).into_iter().filter(|i| is_video(&extension(&i.path))).collect() } else { images };
    if images.len() <= count {
        return images;
    }
    let step = images.len() as f64 / count as f64;
    (0..count).map(|i| images[(i as f64 * step) as usize].clone()).collect()
}

/// What a folder, or failing that the first folder inside it that has any, holds.
fn cover_pool(folder: &Path, videos: bool) -> Vec<Scanned> {
    let mut images = scan_folder_media(folder, false, Kinds::Both, videos).images;
    if images.is_empty() {
        if let Ok(entries) = std::fs::read_dir(folder) {
            let mut subs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && !hidden(&name_of(p)))
                .collect();
            subs.sort_by(|a, b| natural_cmp(&name_of(a), &name_of(b)));
            for sub in subs.into_iter().take(8) {
                images = scan_folder_media(&sub, false, Kinds::Both, videos).images;
                if !images.is_empty() {
                    break;
                }
            }
        }
    }
    images
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opencull-scan-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        for f in files {
            let path = dir.join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"x").unwrap();
        }
        dir
    }

    #[test]
    fn a_folder_of_only_clips_uses_them_as_its_cover() {
        let dir = scratch("clips-cover", &["a.MP4", "b.MP4"]);
        assert_eq!(cover_images(&dir, 4).len(), 2);
        let mixed = scratch("photo-cover", &["a.MP4", "b.JPG"]);
        let cover = cover_images(&mixed, 4);
        assert_eq!(cover.len(), 1);
        assert!(!is_video(&extension(&cover[0].path)));
    }

    #[test]
    fn a_sony_cards_proxies_and_thumbnails_are_not_imported_with_its_clips() {
        let dir = scratch("sony-card", &["M4ROOT/CLIP/C0001.MP4", "M4ROOT/CLIP/C0002.MP4", "M4ROOT/SUB/C0001S03.MP4", "M4ROOT/THMBNL/C0001T01.JPG", "M4ROOT/GENERAL/SONYCARD.IND"]);
        let found = scan_folder_media(&dir.join("M4ROOT"), true, Kinds::Both, true);
        assert_eq!(found.images.len(), 2);
        assert!(found.images.iter().all(|i| i.path.to_string_lossy().contains("CLIP")));
        // Only M4ROOT's own folders are skipped: a folder called SUB elsewhere is a person's.
        let other = scratch("own-sub", &["Shoot/SUB/A.JPG"]);
        assert_eq!(scan_folder_media(&other, true, Kinds::Both, false).images.len(), 1);
    }

    fn names(result: &ScanResult) -> Vec<String> {
        result
            .images
            .iter()
            .map(|i| match &i.subpath {
                Some(s) => format!("{s}/{}", i.name),
                None => i.name.clone(),
            })
            .collect()
    }

    #[test]
    fn numbers_sort_by_value_and_case_is_ignored() {
        let mut v = vec!["IMG_10.ARW", "img_9.arw", "IMG_100.ARW", "IMG_2.ARW", "a.jpg", "B.jpg"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a.jpg", "B.jpg", "IMG_2.ARW", "img_9.arw", "IMG_10.ARW", "IMG_100.ARW"]);
        assert_eq!(natural_cmp("A7V00099", "A7V00100"), Ordering::Less);
        assert_eq!(natural_cmp("same", "same"), Ordering::Equal);
    }

    #[test]
    fn a_raw_and_its_jpeg_are_one_photo_drawn_from_the_jpeg() {
        let dir = scratch("pair", &["A7V00001.ARW", "A7V00001.JPG", "A7V00002.ARW", "A7V00003.JPG", "notes.txt"]);
        let result = scan_folder(&dir, false);

        assert_eq!(names(&result), vec!["A7V00001.ARW", "A7V00002.ARW", "A7V00003.JPG"]);
        assert_eq!(result.images[0].display, dir.join("A7V00001.JPG"));
        assert_eq!(result.images[1].display, dir.join("A7V00002.ARW"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn asked_for_raws_alone_a_frame_is_its_raw_and_brings_no_jpeg() {
        let dir = scratch("rawonly", &["A7V00001.ARW", "A7V00001.JPG", "A7V00002.ARW", "A7V00003.JPG", "logo.png"]);
        let result = scan_folder_of(&dir, false, Kinds::Raw);

        assert_eq!(names(&result), vec!["A7V00001.ARW", "A7V00002.ARW"]);
        assert_eq!(result.images[0].display, dir.join("A7V00001.ARW"), "drawn from itself: the JPEG is not part of it");
        assert_eq!(result.left_out, 2, "the JPEG with no RAW, and the logo");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn asked_for_jpegs_alone_the_raws_are_left_where_they_are() {
        let dir = scratch("jpegonly", &["A7V00001.ARW", "A7V00001.JPG", "A7V00002.ARW", "A7V00003.JPG", "logo.png"]);
        let result = scan_folder_of(&dir, false, Kinds::Jpeg);

        assert_eq!(names(&result), vec!["A7V00001.JPG", "A7V00003.JPG", "logo.png"]);
        assert!(result.images.iter().all(|i| i.display == i.path));
        assert_eq!(result.left_out, 1, "the RAW with no JPEG; the other is shown through its JPEG");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_pair_keeps_one_sidecar_whichever_of_the_two_is_being_culled() {
        // What makes a mark given to one of a pair a mark on both.
        let dir = scratch("onesidecar", &["A7V00001.ARW", "A7V00001.JPG"]);
        let owner = |kinds| scan_folder_of(&dir, false, kinds).images[0].path.clone();
        let sidecar = crate::xmp::sidecar_path(&owner(Kinds::Both));
        assert_eq!(sidecar, dir.join("A7V00001.xmp"));
        assert_eq!(crate::xmp::sidecar_path(&owner(Kinds::Raw)), sidecar);
        assert_eq!(crate::xmp::sidecar_path(&owner(Kinds::Jpeg)), sidecar);
        assert_eq!(scan_folder_of(&dir, false, Kinds::Both).left_out, 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn subfolders_are_counted_but_not_entered_unless_asked() {
        let dir = scratch("subs", &["top.ARW", "100MSDCF/A.ARW", "101MSDCF/A.ARW", ".hidden/B.ARW"]);

        let shallow = scan_folder(&dir, false);
        assert_eq!(names(&shallow), vec!["top.ARW"]);
        assert_eq!(shallow.subfolder_count, 2, "the hidden one does not count");

        let deep = scan_folder(&dir, true);
        assert_eq!(names(&deep), vec!["top.ARW", "100MSDCF/A.ARW", "101MSDCF/A.ARW"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_raw_is_never_paired_with_a_jpeg_from_another_folder() {
        // Two card folders that both number from 0001. The JPEG in the second
        // belongs to that folder's frame, not to the RAW in the first.
        let dir = scratch("crossfolder", &["100MSDCF/A7V00001.ARW", "101MSDCF/A7V00001.JPG"]);
        let result = scan_folder(&dir, true);

        assert_eq!(names(&result), vec!["100MSDCF/A7V00001.ARW", "101MSDCF/A7V00001.JPG"]);
        assert_eq!(result.images[0].display, dir.join("100MSDCF/A7V00001.ARW"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn whole_folders_stay_together_in_the_order_the_camera_wrote_them() {
        let dir = scratch("order", &["101MSDCF/A7V00001.ARW", "100MSDCF/A7V09999.ARW", "100MSDCF/A7V09998.ARW"]);
        let result = scan_folder(&dir, true);
        assert_eq!(
            names(&result),
            vec!["100MSDCF/A7V09998.ARW", "100MSDCF/A7V09999.ARW", "101MSDCF/A7V00001.ARW"]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_folder_named_like_a_photo_is_not_a_photo_and_packages_are_not_entered() {
        let dir = scratch("oddities", &["Selects.jpg/real.ARW", "Library.photoslibrary/originals/x.ARW"]);
        let result = scan_folder(&dir, true);
        assert_eq!(names(&result), vec!["Selects.jpg/real.ARW"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_cover_looks_one_level_down_and_spreads_across_the_shoot() {
        let files: Vec<String> = (0..20).map(|i| format!("DCIM/100MSDCF/A{i:02}.ARW")).collect();
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        let dir = scratch("cover", &refs);

        assert!(cover_images(&dir, 4).is_empty(), "two levels down is further than it looks");
        let cover = cover_images(&dir.join("DCIM"), 4);
        let picked: Vec<_> = cover.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(picked, vec!["A00.ARW", "A05.ARW", "A10.ARW", "A15.ARW"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)] mod media_tests {
    use super::*;
    #[test] fn videos_are_opt_in_and_never_pair_with_stills() {
        let root = std::env::temp_dir().join(format!("opencull-media-{}", std::process::id())); std::fs::create_dir_all(&root).unwrap();
        for name in ["A.MP4", "A.JPG", "B.MOV"] { std::fs::write(root.join(name), b"test media").unwrap(); }
        assert_eq!(scan_folder(&root, false).images.len(), 1);
        let result = scan_folder_media(&root, false, Kinds::Both, true); assert_eq!(result.images.len(), 3);
        assert!(result.images.iter().all(|p| p.path == p.display));
        assert_eq!(scan_folder_media(&root, false, Kinds::Raw, true).images.len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test] fn video_only_card_folders_are_visible_when_enabled() {
        let root = std::env::temp_dir().join(format!("opencull-video-only-{}", std::process::id()));
        std::fs::create_dir_all(root.join("PRIVATE")).unwrap(); std::fs::write(root.join("PRIVATE/A.MTS"), b"test").unwrap();
        assert!(crate::ingest::photo_folders(&root).is_empty());
        assert_eq!(crate::ingest::photo_folders_media(&root, true).len(), 1);
        assert_eq!(crate::ingest::scan_media(&root, true).len(), 1); std::fs::remove_dir_all(root).unwrap();
    }
}
