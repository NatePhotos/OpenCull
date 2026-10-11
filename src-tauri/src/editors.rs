//! The editing programs Ship can hand photos to: which exist on this
//! computer, and how each is asked to open a set of files.
//!
//! Every program here is started the way the system's own "Open With" would
//! start it — the program, and the files as its arguments. Nothing is
//! scripted and nothing inside a catalogue is touched.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

/// One program Ship can open photos in.
pub struct Editor {
    pub id: &'static str,
    pub name: &'static str,
    /// Places it is usually installed, most likely first.
    paths: &'static [&'static str],
    /// How the system knows it when it is installed somewhere else (macOS).
    bundle_ids: &'static [&'static str],
    /// Opens a catalogue of its own before the photos, if one is chosen.
    pub takes_catalog: bool,
}

/// Shows the photos in the system's file browser. Always available.
pub const FILE_BROWSER: &str = "fileBrowser";

#[cfg(target_os = "macos")]
pub const EDITORS: &[Editor] = &[
    Editor {
        id: "lightroomClassic",
        name: "Lightroom Classic",
        paths: &["/Applications/Adobe Lightroom Classic/Adobe Lightroom Classic.app", "/Applications/Adobe Lightroom Classic.app"],
        bundle_ids: &["com.adobe.LightroomClassicCC7"],
        takes_catalog: true,
    },
    Editor {
        id: "lightroom",
        name: "Lightroom",
        paths: &["/Applications/Adobe Lightroom CC/Adobe Lightroom.app", "/Applications/Adobe Lightroom.app"],
        bundle_ids: &["com.adobe.lightroomCC", "com.adobe.mas.lightroomCC"],
        takes_catalog: false,
    },
    Editor {
        id: "aftershoot",
        name: "Aftershoot",
        paths: &["/Applications/Aftershoot EDITS.app", "/Applications/AfterShoot.app"],
        bundle_ids: &["com.electron.aftershoot"],
        takes_catalog: false,
    },
    Editor {
        id: "captureOne",
        name: "Capture One",
        paths: &["/Applications/Capture One.app", "/Applications/Capture One 23.app"],
        bundle_ids: &["com.captureone.captureone16", "com.captureone.captureone15"],
        takes_catalog: false,
    },
    Editor { id: "photos", name: "Photos", paths: &["/System/Applications/Photos.app"], bundle_ids: &["com.apple.Photos"], takes_catalog: false },
];

#[cfg(target_os = "windows")]
pub const EDITORS: &[Editor] = &[
    Editor {
        id: "lightroomClassic",
        name: "Lightroom Classic",
        paths: &["C:\\Program Files\\Adobe\\Adobe Lightroom Classic\\Lightroom.exe"],
        bundle_ids: &[],
        takes_catalog: true,
    },
    Editor {
        id: "lightroom",
        name: "Lightroom",
        paths: &["C:\\Program Files\\Adobe\\Adobe Lightroom CC\\lightroom.exe"],
        bundle_ids: &[],
        takes_catalog: false,
    },
];

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const EDITORS: &[Editor] = &[
    Editor { id: "darktable", name: "darktable", paths: &["/usr/bin/darktable", "/usr/local/bin/darktable"], bundle_ids: &[], takes_catalog: false },
];

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub takes_catalog: bool,
}

/// Every program Ship knows of on this kind of computer, and whether each is here.
pub fn list() -> Vec<Listed> {
    let browser = if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(target_os = "windows") {
        "Show in File Explorer"
    } else {
        "Show in file manager"
    };
    EDITORS
        .iter()
        .map(|e| Listed { id: e.id.into(), name: e.name.into(), installed: installed_at(e).is_some(), takes_catalog: e.takes_catalog })
        .chain(std::iter::once(Listed { id: FILE_BROWSER.into(), name: browser.into(), installed: true, takes_catalog: false }))
        .collect()
}

fn installed_at(editor: &Editor) -> Option<PathBuf> {
    editor.paths.iter().map(PathBuf::from).find(|p| p.exists()).or_else(|| by_bundle_id(editor))
}

/// Finds a program the system knows about that is not where it is usually put.
#[cfg(target_os = "macos")]
fn by_bundle_id(editor: &Editor) -> Option<PathBuf> {
    editor.bundle_ids.iter().find_map(|id| {
        let out = std::process::Command::new("/usr/bin/mdfind").arg(format!("kMDItemCFBundleIdentifier == '{id}'")).output().ok()?;
        String::from_utf8_lossy(&out.stdout).lines().map(PathBuf::from).find(|p| p.exists())
    })
}

#[cfg(not(target_os = "macos"))]
fn by_bundle_id(_editor: &Editor) -> Option<PathBuf> {
    None
}

/// One program to start, and what to hand it.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// The most bytes of file names handed over in one go. Systems cap what a
/// program can be started with — about a megabyte on a Mac, 32 thousand
/// characters on Windows — and a large ship must be split to fit under it.
const ARGS_LIMIT: usize = if cfg!(target_os = "windows") { 30_000 } else { 800_000 };

/// How `program` is started with `files`. More than one launch only when the
/// names would not fit in a single one.
pub fn launches(program: &Path, files: &[PathBuf]) -> Vec<Launch> {
    let mut out = Vec::new();
    let mut batch: Vec<String> = Vec::new();
    let mut size = 0;
    for file in files {
        let arg = file.to_string_lossy().into_owned();
        if !batch.is_empty() && size + arg.len() + 3 > ARGS_LIMIT {
            out.push(launch(program, std::mem::take(&mut batch)));
            size = 0;
        }
        size += arg.len() + 3;
        batch.push(arg);
    }
    if !batch.is_empty() {
        out.push(launch(program, batch));
    }
    out
}

#[cfg(target_os = "macos")]
fn launch(program: &Path, files: Vec<String>) -> Launch {
    // Through `open`, so the files reach a program that is already running
    // instead of starting a second copy of it.
    let mut args = vec!["-a".to_string(), program.to_string_lossy().into_owned()];
    args.extend(files);
    Launch { program: PathBuf::from("/usr/bin/open"), args }
}

#[cfg(not(target_os = "macos"))]
fn launch(program: &Path, files: Vec<String>) -> Launch {
    Launch { program: program.to_path_buf(), args: files }
}

/// How long an editor is given to open its catalogue before the photos follow.
const CATALOG_HEAD_START: std::time::Duration = std::time::Duration::from_millis(1500);

/// Opens `files` in the editor called `id`, after `catalog` if one is given.
pub fn open(id: &str, files: &[PathBuf], catalog: Option<&Path>) -> Result<(), String> {
    let editor = EDITORS.iter().find(|e| e.id == id).ok_or_else(|| format!("no editing program called {id}"))?;
    // Only full paths, which can never be mistaken for an option as a name
    // beginning with "-" could.
    if let Some(odd) = files.iter().map(PathBuf::as_path).chain(catalog).find(|f| !f.is_absolute()) {
        return Err(format!("{} is not a full path to a file", odd.display()));
    }
    let program = installed_at(editor).ok_or_else(|| format!("{} is not installed on this computer", editor.name))?;

    if let Some(catalog) = catalog.filter(|c| editor.takes_catalog && c.exists()) {
        start(&launches(&program, &[catalog.to_path_buf()]))?;
        std::thread::sleep(CATALOG_HEAD_START);
    }
    start(&launches(&program, files))
}

fn start(launches: &[Launch]) -> Result<(), String> {
    for l in launches {
        let mut child = std::process::Command::new(&l.program).args(&l.args).spawn().map_err(|e| format!("could not start {}: {e}", l.program.display()))?;
        // Reaped in the background, so a finished program leaves no zombie.
        std::thread::Builder::new().name("opencull-editor-wait".into()).spawn(move || child.wait()).ok();
    }
    Ok(())
}

/// The most catalogues listed, and how deep the search for them goes.
const MAX_CATALOGS: usize = 20;
const MAX_DEPTH: usize = 4;

/// Lightroom Classic catalogues under `roots`, most recently used first.
///
/// Shallow and capped on purpose: this runs when Ship opens, and a search
/// that walked a whole photo archive would still be going when it closed.
pub fn catalogs(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<(PathBuf, SystemTime)> = Vec::new();
    for root in roots {
        walk(root, 0, &mut found);
    }
    found.sort_by_key(|(_, used)| std::cmp::Reverse(*used));
    found.dedup_by(|a, b| a.0 == b.0);
    found.into_iter().map(|(path, _)| path).collect()
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<(PathBuf, SystemTime)>) {
    if depth > MAX_DEPTH || found.len() >= MAX_CATALOGS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if found.len() >= MAX_CATALOGS {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            // A catalogue's preview store is a folder of hundreds of thousands
            // of files with no catalogue in it; so is anything that is really
            // a package, like a photo library.
            if lower.ends_with(".lrdata") || lower.ends_with(".photoslibrary") || lower.ends_with(".app") {
                continue;
            }
            walk(&path, depth + 1, found);
        } else if lower.ends_with(".lrcat") {
            let used = entry.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            found.push((path, used));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_is_not_a_full_path_is_never_handed_to_an_editor() {
        let refused = open("lightroom", &[PathBuf::from("-n")], None).unwrap_err();
        assert!(refused.contains("not a full path"), "{refused}");
    }

    #[test]
    fn the_file_browser_is_always_offered_and_always_last() {
        let listed = list();
        let last = listed.last().expect("at least the file browser");
        assert_eq!(last.id, FILE_BROWSER);
        assert!(last.installed);
        assert_eq!(listed.iter().filter(|l| l.id == FILE_BROWSER).count(), 1);
    }

    #[test]
    fn every_editor_has_its_own_id() {
        let mut ids: Vec<&str> = EDITORS.iter().map(|e| e.id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }

    #[test]
    fn a_few_files_go_in_one_launch_in_the_order_given() {
        let files = [PathBuf::from("/shoot/A.ARW"), PathBuf::from("/shoot/B with space.ARW")];
        let out = launches(Path::new("/Applications/Editor.app"), &files);
        assert_eq!(out.len(), 1);
        let args = &out[0].args;
        assert_eq!(&args[args.len() - 2..], ["/shoot/A.ARW", "/shoot/B with space.ARW"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn on_a_mac_files_are_handed_over_through_open() {
        let out = launches(Path::new("/Applications/Editor.app"), &[PathBuf::from("/shoot/A.ARW")]);
        assert_eq!(out[0].program, PathBuf::from("/usr/bin/open"));
        assert_eq!(out[0].args, ["-a", "/Applications/Editor.app", "/shoot/A.ARW"]);
    }

    #[test]
    fn more_names_than_fit_are_split_and_none_is_lost() {
        let files: Vec<PathBuf> = (0..20_000).map(|i| PathBuf::from(format!("/a/long/enough/folder/name/for/a/shoot/DSC{i:05}.ARW"))).collect();
        let out = launches(Path::new("/Applications/Editor.app"), &files);
        assert!(out.len() > 1, "20,000 names do not fit in one launch");
        let handed: usize = out.iter().map(|l| l.args.iter().filter(|a| a.ends_with(".ARW")).count()).sum();
        assert_eq!(handed, files.len());
        for l in &out {
            assert!(l.args.iter().map(|a| a.len() + 3).sum::<usize>() <= ARGS_LIMIT + 200);
        }
    }

    #[test]
    fn nothing_to_open_starts_nothing() {
        assert!(launches(Path::new("/Applications/Editor.app"), &[]).is_empty());
    }

    #[test]
    fn an_unknown_editor_is_refused_rather_than_guessed_at() {
        assert!(open("notAnEditor", &[PathBuf::from("/x.ARW")], None).is_err());
    }

    #[test]
    fn catalogues_are_found_newest_first_and_previews_are_not_searched() {
        let root = std::env::temp_dir().join(format!("opencull-catalogs-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let old = root.join("Lightroom/Old/Old.lrcat");
        let new = root.join("Lightroom/New/New.lrcat");
        let buried = root.join("Lightroom/New/New Previews.lrdata/inside.lrcat");
        let hidden = root.join(".hidden/Hidden.lrcat");
        let deep = root.join("a/b/c/d/e/f/Deep.lrcat");
        for file in [&old, &new, &buried, &hidden, &deep] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "x").unwrap();
        }
        let earlier = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
        std::fs::File::options().write(true).open(&old).unwrap().set_modified(earlier).unwrap();

        let found = catalogs(&[root.clone(), root.clone()]);

        assert_eq!(found, vec![new, old], "newest first, each once, and nothing from inside a preview store, a hidden folder or too far down");
        std::fs::remove_dir_all(&root).ok();
    }
}
