//! Combining independent source cards without guessing from names alone.
use crate::{
    raw,
    scan::{self, Scanned},
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Facts {
    make: String,
    model: String,
    time: u64,
}

fn pair(mut images: Vec<Scanned>, facts: impl Fn(&Path) -> Option<Facts>) -> Vec<Scanned> {
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, image) in images.iter().enumerate() {
        if image.path != image.display || !(raw::is_raw(&image.path) || raw::is_jpeg(&image.path)) {
            continue;
        }
        let Some(stem) = image.path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        groups.entry(stem.to_ascii_lowercase()).or_default().push(i);
    }
    let mut remove = HashSet::new();
    for group in groups.values() {
        if group.len() != 2 {
            continue;
        }
        let raws: Vec<_> = group
            .iter()
            .copied()
            .filter(|i| raw::is_raw(&images[*i].path))
            .collect();
        let jpegs: Vec<_> = group
            .iter()
            .copied()
            .filter(|i| raw::is_jpeg(&images[*i].path))
            .collect();
        if let ([r], [j]) = (raws.as_slice(), jpegs.as_slice()) {
            let Some(raw_facts) = facts(&images[*r].path) else {
                continue;
            };
            let Some(jpeg_facts) = facts(&images[*j].path) else {
                continue;
            };
            if raw_facts != jpeg_facts {
                continue;
            }
            images[*r].display = images[*j].path.clone();
            images[*r].version = images[*j].version.clone();
            remove.insert(*j);
        }
    }
    images
        .into_iter()
        .enumerate()
        .filter_map(|(i, image)| (!remove.contains(&i)).then_some(image))
        .collect()
}

pub fn scan_sources(sources: &[PathBuf], include_videos: bool) -> Result<Vec<Scanned>, String> {
    let mut seen = HashSet::new();
    let mut images = Vec::new();
    for source in sources {
        let root = source
            .canonicalize()
            .map_err(|e| format!("{}: {e}", source.display()))?;
        if !root.is_dir() {
            return Err(format!("{} is not a folder", root.display()));
        }
        for mut image in
            scan::scan_folder_media(&root, true, scan::Kinds::Both, include_videos).images
        {
            if !seen.insert(image.path.clone()) {
                continue;
            }
            let label = root.file_name().unwrap_or_default().to_string_lossy();
            image.subpath = Some(match image.subpath {
                Some(sub) => format!("{label}/{sub}"),
                None => label.to_string(),
            });
            images.push(image);
        }
    }
    let paired: HashSet<_> = images
        .iter()
        .filter(|p| p.path != p.display)
        .map(|p| p.display.clone())
        .collect();
    images.retain(|p| !paired.contains(&p.path));
    Ok(pair(images, |path| {
        let e = raw::probe(path).ok()?.exif;
        let make = e.make?.trim().to_lowercase();
        let model = e.model?.trim().to_lowercase();
        if make.is_empty() || model.is_empty() {
            return None;
        }
        Some(Facts {
            make,
            model,
            time: e.capture_ms?.to_bits(),
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image(path: &str) -> Scanned {
        Scanned {
            path: path.into(),
            display: path.into(),
            name: Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            subpath: None,
            version: "1".into(),
            size: 4,
            modified: 0,
        }
    }
    fn facts(_: &Path) -> Option<Facts> {
        Some(Facts {
            make: "Sony".into(),
            model: "A7".into(),
            time: 1234,
        })
    }
    #[test]
    fn raw_and_jpeg_on_two_cards_become_one_frame() {
        let result = pair(vec![image("/raw/A.ARW"), image("/jpeg/A.JPG")], facts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].display, Path::new("/jpeg/A.JPG"));
    }
    #[test]
    fn ambiguous_and_missing_metadata_are_never_paired() {
        let images = vec![image("/r1/A.ARW"), image("/r2/A.ARW"), image("/j/A.JPG")];
        assert_eq!(pair(images, facts).len(), 3);
        assert_eq!(
            pair(vec![image("/r/A.ARW"), image("/j/A.JPG")], |_| None).len(),
            2
        );
    }
    #[test]
    fn different_camera_or_time_is_not_a_match() {
        let files = vec![image("/r/A.ARW"), image("/j/A.JPG")];
        assert_eq!(
            pair(files.clone(), |p| {
                let mut f = facts(p)?;
                if raw::is_jpeg(p) {
                    f.time += 1;
                }
                Some(f)
            })
            .len(),
            2
        );
        assert_eq!(
            pair(files, |p| {
                let mut f = facts(p)?;
                if raw::is_jpeg(p) {
                    f.model = "Other".into();
                }
                Some(f)
            })
            .len(),
            2
        );
    }
    #[test]
    fn only_reads_metadata_for_possible_raw_jpeg_pairs() {
        let calls = std::cell::Cell::new(0);
        let images = vec![image("/r/A.ARW"), image("/j/B.JPG"), image("/r/C.ARW")];
        assert_eq!(
            pair(images, |_| {
                calls.set(calls.get() + 1);
                facts(Path::new("unused"))
            })
            .len(),
            3
        );
        assert_eq!(calls.get(), 0);
    }
    #[test]
    fn videos_are_omitted_unless_requested() {
        let root =
            std::env::temp_dir().join(format!("opencull-multicard-video-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let video = root.join("clip.MOV");
        std::fs::write(&video, b"video").unwrap();
        assert!(scan_sources(&[root.clone()], false).unwrap().is_empty());
        let included = scan_sources(&[root.clone()], true).unwrap();
        assert_eq!(included.len(), 1);
        let canonical_video = std::fs::canonicalize(&video).unwrap();
        assert_eq!(included[0].path, canonical_video);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn overlapping_sources_do_not_duplicate_files() {
        let root = std::env::temp_dir().join(format!("opencull-multicard-{}", std::process::id()));
        std::fs::create_dir_all(root.join("DCIM")).unwrap();
        std::fs::write(root.join("DCIM/A.JPG"), b"fake").unwrap();
        let result = scan_sources(&[root.clone(), root.join("DCIM")], false).unwrap();
        assert_eq!(result.len(), 1);
        assert!(scan_sources(&[root.join("missing")], false).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
