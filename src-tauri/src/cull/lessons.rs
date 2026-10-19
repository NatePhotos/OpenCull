//! What a culled shoot teaches, from the stars or the pick and reject flags
//! the photographer gave. A skipped frame in a burst is not a bad one, only
//! one not needed, so unrated or unflagged frames never count as rejects.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_128;

use super::features::FORMAT;
use super::Prepared;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Lesson {
    Keep { x: Vec<f32>, keep: bool },
    Prefer { better: Vec<f32>, worse: Vec<f32> },
    Behind { best: Vec<f32>, this: Vec<f32>, blue: bool },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shoot {
    pub folder: String,
    pub format: u32,
    pub taught_at: u64,
    pub lessons: Vec<Taught>,
    /// This shoot was tried and rejected, so nothing is learned from it.
    #[serde(default)]
    pub set_aside: bool,
    /// Learning from this shoot was checked and kept. It, and every shoot
    /// not set aside that was taught before it, is in the network in use.
    #[serde(default)]
    pub accepted: bool,
}

/// The shoots the network in use learned from: those not set aside, up to
/// the last one that was kept.
pub fn in_use(shoots: &[Shoot]) -> Vec<&Shoot> {
    let Some(last) = shoots.iter().filter(|s| s.accepted && !s.set_aside).map(|s| s.taught_at).max() else { return Vec::new() };
    shoots.iter().filter(|s| !s.set_aside && s.taught_at <= last).collect()
}

/// A lesson and the burst it came from, so a shoot can be tested on bursts it was not trained on.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Taught {
    pub group: u32,
    pub lesson: Lesson,
}

/// Which marks of the photographer's a shoot is taught from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Stars,
    Flags,
}

pub fn lessons_from(groups: &[Vec<Prepared>], source: Source) -> Vec<Taught> {
    let mut out = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        let g = g as u32;
        let mut push = |lesson: Lesson| out.push(Taught { group: g, lesson });
        if source == Source::Flags {
            for p in group {
                match p.flag {
                    1 => push(Lesson::Keep { x: p.x.clone(), keep: true }),
                    -1 => push(Lesson::Keep { x: p.x.clone(), keep: false }),
                    _ => {}
                }
            }
            if group.len() >= 2 {
                for pick in group.iter().filter(|p| p.flag == 1) {
                    for reject in group.iter().filter(|o| o.flag == -1) {
                        push(Lesson::Prefer { better: pick.x.clone(), worse: reject.x.clone() });
                    }
                }
            }
            continue;
        }
        for p in group {
            match p.rating {
                1 | 2 => push(Lesson::Keep { x: p.x.clone(), keep: false }),
                4 | 5 => push(Lesson::Keep { x: p.x.clone(), keep: true }),
                _ => {}
            }
        }
        if group.len() < 2 {
            continue;
        }
        for pick in group.iter().filter(|p| p.rating == 5) {
            for other in group.iter().filter(|o| (0..=4).contains(&o.rating)) {
                push(Lesson::Prefer { better: pick.x.clone(), worse: other.x.clone() });
                match other.rating {
                    4 => push(Lesson::Behind { best: pick.x.clone(), this: other.x.clone(), blue: true }),
                    1 | 2 => push(Lesson::Behind { best: pick.x.clone(), this: other.x.clone(), blue: false }),
                    _ => {}
                }
            }
        }
    }
    out
}

/// Makes each temporary file's name unique within this process.
static TEMP: AtomicU64 = AtomicU64::new(0);

pub struct LessonStore {
    dir: PathBuf,
}

impl LessonStore {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        Self { dir }
    }

    fn file_for(&self, folder: &str) -> PathBuf {
        self.dir.join(format!("{:032x}.json", xxh3_128(folder.as_bytes())))
    }

    pub fn save(&self, shoot: &Shoot) -> Result<(), String> {
        let bytes = serde_json::to_vec(shoot).map_err(|e| format!("Couldn't save what OpenCull learned: {e}"))?;
        let path = self.file_for(&shoot.folder);
        let temp = path.with_extension(format!("tmp{}-{}", std::process::id(), TEMP.fetch_add(1, Ordering::Relaxed)));
        if let Err(e) = std::fs::write(&temp, bytes) {
            std::fs::remove_file(&temp).ok();
            return Err(format!("Couldn't save what OpenCull learned: {e}"));
        }
        std::fs::rename(&temp, &path).map_err(|e| {
            std::fs::remove_file(&temp).ok();
            format!("Couldn't save what OpenCull learned: {e}")
        })
    }

    pub fn all(&self) -> Vec<Shoot> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        let mut shoots: Vec<Shoot> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter_map(|b| serde_json::from_slice::<Shoot>(&b).ok())
            .filter(|s| s.format == FORMAT)
            .collect();
        shoots.sort_by_key(|s| s.taught_at);
        shoots
    }

    pub fn clear(&self) -> Result<(), String> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("Couldn't clear what OpenCull learned: {e}")),
        };
        for e in entries.flatten() {
            std::fs::remove_file(e.path()).map_err(|err| format!("Couldn't clear what OpenCull learned: {err}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cull::features::{inputs, HIGHLIGHTS, KIND_EVENTS, KIND_PORTRAITS, KIND_SPORTS, READINGS, STOPS_VS_NEIGHBOURS};
    use crate::cull::Prepared;

    fn p(name: &str, rating: i32) -> Prepared {
        let mut r = [0.5; READINGS];
        r[KIND_PORTRAITS] = 0.0; r[KIND_SPORTS] = 0.0; r[KIND_EVENTS] = 1.0; r[STOPS_VS_NEIGHBOURS] = 0.0; r[HIGHLIGHTS] = 0.0;
        Prepared { path: name.into(), version: "v".into(), readings: r, x: inputs(&r, None), rating, flag: 0 }
    }

    fn count(l: &[Taught]) -> (usize, usize, usize, usize, usize) {
        let keep_yes = l.iter().filter(|t| matches!(t.lesson, Lesson::Keep { keep: true, .. })).count();
        let keep_no = l.iter().filter(|t| matches!(t.lesson, Lesson::Keep { keep: false, .. })).count();
        let prefer = l.iter().filter(|t| matches!(t.lesson, Lesson::Prefer { .. })).count();
        let blue = l.iter().filter(|t| matches!(t.lesson, Lesson::Behind { blue: true, .. })).count();
        let yellow = l.iter().filter(|t| matches!(t.lesson, Lesson::Behind { blue: false, .. })).count();
        (keep_yes, keep_no, prefer, blue, yellow)
    }

    #[test]
    fn stars_teach_what_the_photographer_means_by_them() {
        let lessons = lessons_from(&[vec![p("a", 5), p("b", 4), p("c", 0), p("d", 2), p("e", 3)], vec![p("f", 1)], vec![p("g", 3)], vec![p("h", 0)]], Source::Stars);
        // keep: a(5) b(4); reject: d(2) f(1); 3★ and unrated teach no keep lesson
        // prefer: a over b, c, d, e; behind: b blue, d not blue
        assert_eq!(count(&lessons), (2, 2, 4, 1, 1));
    }

    #[test]
    fn flags_teach_picks_as_keepers_rejects_as_rejects_and_picks_over_rejects() {
        let pf = |n: &str, f: i8| { let mut p = p(n, 0); p.flag = f; p };
        let l = lessons_from(&[vec![pf("a", 1), pf("b", -1), pf("c", 0), pf("d", 1)], vec![pf("e", -1)]], Source::Flags);
        // keep: a, d; reject: b, e; prefer: a>b, d>b; no Behind lessons; unflagged c teaches nothing
        assert_eq!(count(&l), (2, 2, 2, 0, 0));
    }

    #[test]
    fn stars_ignore_flags_and_flags_ignore_stars() {
        let mut a = p("a", 5); a.flag = -1;
        assert_eq!(count(&lessons_from(&[vec![a.clone()]], Source::Stars)), (1, 0, 0, 0, 0));
        assert_eq!(count(&lessons_from(&[vec![a]], Source::Flags)), (0, 1, 0, 0, 0));
    }

    #[test]
    fn a_shoot_with_no_stars_teaches_nothing() {
        assert!(lessons_from(&[vec![p("a", 0), p("b", 0)]], Source::Stars).is_empty());
    }

    #[test]
    fn saving_a_folder_again_replaces_its_lessons() {
        let dir = std::env::temp_dir().join(format!("autocull-lessons-{}", std::process::id()));
        let store = LessonStore::new(dir.clone());
        let shoot = |n: usize| Shoot { folder: "/x".into(), format: crate::cull::features::FORMAT, taught_at: n as u64, lessons: lessons_from(&[vec![p("a", 1)]; 1], Source::Stars).into_iter().cycle().take(n).collect(), set_aside: false, accepted: false };
        store.save(&shoot(1)).unwrap();
        store.save(&shoot(3)).unwrap();
        let all = store.all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].lessons.len(), 3);
        store.clear().unwrap();
        assert!(store.all().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn saving_into_an_unwritable_place_says_so() {
        // A regular file where the store folder should be: nothing can be written into it.
        let file = std::env::temp_dir().join(format!("autocull-lessons-file-{}", std::process::id()));
        std::fs::write(&file, b"not a folder").unwrap();
        let store = LessonStore::new(file.clone());
        let shoot = Shoot { folder: "/y".into(), format: crate::cull::features::FORMAT, taught_at: 1, lessons: lessons_from(&[vec![p("a", 5), p("b", 1)]], Source::Stars), set_aside: false, accepted: false };
        let err = store.save(&shoot).unwrap_err();
        assert!(err.starts_with("Couldn't save what OpenCull learned"), "{err}");
        std::fs::remove_file(file).ok();
    }

    #[test]
    fn all_leaves_out_shoots_taught_under_another_format() {
        let dir = std::env::temp_dir().join(format!("autocull-lessons-format-{}", std::process::id()));
        let store = LessonStore::new(dir.clone());
        let current = Shoot { folder: "/current".into(), format: crate::cull::features::FORMAT, taught_at: 1, lessons: Vec::new(), set_aside: false, accepted: false };
        let older = Shoot { folder: "/older".into(), format: crate::cull::features::FORMAT + 1, taught_at: 2, lessons: Vec::new(), set_aside: false, accepted: false };
        store.save(&current).unwrap();
        store.save(&older).unwrap();
        let all = store.all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].folder, "/current");
        store.clear().unwrap();
        std::fs::remove_dir_all(dir).ok();
    }
}
