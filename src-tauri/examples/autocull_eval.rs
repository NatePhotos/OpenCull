//! Measures auto-cull on a folder of rated RAW photos: how often it colours
//! the 1-2 star photos red and the 4-5 star ones green or blue, and how often
//! the 5-star frame of a pose is its first or top-two choice. Printed for the
//! starting network, then for a network taught on the first half of the shoot
//! and judged on the second half (as kept or not by the check, and as trained
//! regardless of the check). Also times the face scan per photo and the extra
//! its picture costs to read brightness from, and checks that brightness read
//! that way gives the same verdicts as from a 320 px picture. Only numbers are
//! printed.
//!
//!     cargo run --release --features turbo-system --example autocull_eval -- \
//!         <folder> [limit] [type]
//!
//! `folder` holds the rated photos; `limit` is the most photos to use (default
//! 500); `type` is events, portraits or sports (default events), and comes
//! after the limit. Photos whose analysis failed are counted apart, not as
//! photos without people.
//!
//! Photographs are only read. Stars come from the .xmp file beside each photo
//! (or the file itself); the first photos of the shoot, by capture time, are used.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use opencull_lib::ai::{self, series, store::Record};
use opencull_lib::cull::{self, decide, light, lessons, starter, train};
use opencull_lib::{preview, raw};
use rayon::prelude::*;

/// Two photos further apart than this are not in the same run.
const RUN_GAP_MS: f64 = 2_000.0;

struct Shot {
    path: PathBuf,
    time: f64,
    rating: i32,
}

fn sidecar_rating(path: &Path) -> Option<i32> {
    let text = std::fs::read_to_string(path.with_extension("xmp")).ok()?;
    let at = text.find("xmp:Rating=\"")? + "xmp:Rating=\"".len();
    text[at..].split('"').next()?.parse().ok()
}

/// Cuts the photos into runs wherever the clock jumps, and marks each run.
fn frames_of(shots: &[Shot], records: &[Record]) -> Vec<cull::Frame> {
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for i in 0..shots.len() {
        match runs.last_mut() {
            Some(run) if shots[i].time - shots[*run.last().unwrap()].time <= RUN_GAP_MS => run.push(i),
            _ => runs.push(vec![i]),
        }
    }
    let mut frames: Vec<Option<cull::Frame>> = (0..shots.len()).map(|_| None).collect();
    for (r, run) in runs.iter().enumerate() {
        let inputs: Vec<series::Frame> = run.iter().map(|&i| series::Frame { record: &records[i], time: Some(shots[i].time), burst: None, upright: None }).collect();
        let marked = series::compare_run_with_visual(&inputs, true, None, Default::default());
        for (at, &i) in run.iter().enumerate() {
            let pose = marked[at].pose;
            let mut among: Vec<usize> = (0..run.len()).filter(|&k| marked[k].pose == pose).collect();
            among.sort_by(|&a, &b| marked[b].score.unwrap_or(-1.0).total_cmp(&marked[a].score.unwrap_or(-1.0)));
            let rank = among.iter().position(|&k| k == at).unwrap() as u32 + 1;
            frames[i] = Some(cull::Frame {
                path: shots[i].path.to_string_lossy().into_owned(),
                version: String::new(),
                record: records[i].clone(),
                compared_score: marked[at].score,
                compared_eyes: marked[at].eyes.clone(),
                rank: Some(rank),
                count: Some(among.len() as u32),
                pose: Some((r * 100_000 + pose) as i64),
                rating: shots[i].rating,
                time: Some(shots[i].time),
                flag: "none".into(),
            });
        }
    }
    frames.into_iter().map(Option::unwrap).collect()
}

fn percent(part: usize, whole: usize) -> String {
    if whole == 0 { "n/a".into() } else { format!("{:.1}% ({part} of {whole})", 100.0 * part as f32 / whole as f32) }
}

fn report(label: &str, net: &cull::net::Net, groups: &[Vec<cull::Prepared>], no_people: &HashSet<String>) {
    let colours = decide::decide(net, groups);
    let stars: std::collections::HashMap<&str, i32> = groups.iter().flatten().map(|p| (p.path.as_str(), p.rating)).collect();
    let (mut low, mut low_red, mut high, mut high_kept) = (0, 0, 0, 0);
    let (mut empty, mut empty_red) = (0, 0);
    for d in &colours {
        match stars[d.path.as_str()] {
            1 | 2 => { low += 1; low_red += usize::from(d.color == "red"); }
            4 | 5 => { high += 1; high_kept += usize::from(d.color == "green" || d.color == "blue"); }
            _ => {}
        }
        if stars[d.path.as_str()] == 1 && no_people.contains(&d.path) {
            empty += 1;
            empty_red += usize::from(d.color == "red");
        }
    }
    let (first, top2) = decide::picks(net, groups);
    let bursts = groups.iter().filter(|g| g.len() > 1 && g.iter().any(|p| p.rating == 5)).count();
    let of = |share: f32| format!("{:.1}% ({} of {bursts})", share * 100.0, (share * bursts as f32).round() as usize);
    println!("{label}");
    println!("  1-2 star photos coloured red:        {}", percent(low_red, low));
    println!("  4-5 star photos coloured green/blue: {}", percent(high_kept, high));
    println!("  no-people 1★: red {}", percent(empty_red, empty));
    println!("  5-star frame is the first choice:    {}", of(first));
    println!("  5-star frame is in the top two:      {}", of(top2));
}

fn main() {
    const USAGE: &str = "usage: autocull_eval <folder> [limit] [type]\n  limit: most photos to use, default 500\n  type: events, portraits or sports, default events (after the limit)";
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(folder) = args.first() else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    // A type given in place of the limit is an error, not a silent 500 photos of events.
    let most: usize = match args.get(1) {
        None => 500,
        Some(n) => n.parse().unwrap_or_else(|_| {
            eprintln!("the limit must be a number, not {n:?}\n{USAGE}");
            std::process::exit(2);
        }),
    };
    let kind = match args.get(2).map(String::as_str) {
        None | Some("events") => cull::features::ShootKind::Events,
        Some("portraits") => cull::features::ShootKind::Portraits,
        Some("sports") => cull::features::ShootKind::Sports,
        Some(other) => {
            eprintln!("unknown shoot type {other:?}: use events, portraits or sports");
            std::process::exit(2);
        }
    };

    // Only the files directly in the folder; nothing below it is opened.
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder)
        .expect("read the folder")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map_or(false, |t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()).map_or(false, |x| x.eq_ignore_ascii_case("arw")))
        .collect();
    paths.sort();
    let mut shots: Vec<Shot> = paths
        .par_iter()
        .filter_map(|path| {
            let probe = raw::probe(path).ok()?;
            Some(Shot { path: path.clone(), time: probe.exif.capture_ms?, rating: sidecar_rating(path).or(probe.rating).unwrap_or(0) })
        })
        .collect();
    shots.sort_by(|a, b| a.time.total_cmp(&b.time).then_with(|| a.path.cmp(&b.path)));
    shots.truncate(most);
    println!("{} photos", shots.len());

    // A failed analysis keeps an empty record, so the photo still takes its place in its run.
    // Each photo's light is read twice: from a 320 px picture, as from its
    // thumbnail, and from the picture the scan decoded (as the app's scan does
    // with Advanced), timing the scan alone and the extra the second costs.
    struct Analysed { record: Option<Record>, light: Option<light::Light>, scan_light: Option<light::Light>, analyse: f64, extra: f64 }
    let analysed: Vec<Analysed> = shots
        .par_iter()
        .map(|shot| {
            let mut scan_light = None;
            let mut extra = 0.0;
            let started = std::time::Instant::now();
            let record = ai::analyze_then(&shot.path, ai::Rules::Advanced, |p| {
                let at = std::time::Instant::now();
                scan_light = Some(light::light_of_picture(&p.rgb, p.width, p.height));
                extra = at.elapsed().as_secs_f64() * 1000.0;
            })
            .as_ref()
            .ok()
            .map(Record::from);
            let analyse = started.elapsed().as_secs_f64() * 1000.0 - extra;
            let light = preview::decode_upright(&shot.path, 320).ok().map(|p| light::light_of_sized(&p.rgb, p.width as usize, p.height as usize));
            Analysed { record, light, scan_light, analyse, extra }
        })
        .collect();
    let n = analysed.len().max(1) as f64;
    let (analyse, extra) = (analysed.iter().map(|a| a.analyse).sum::<f64>() / n, analysed.iter().map(|a| a.extra).sum::<f64>() / n);
    println!("per photo, on {} threads: scan (ai::analyze) {analyse:.1} ms; light from the scan's picture {extra:.2} ms extra ({:.2}% of the scan)", rayon::current_num_threads(), 100.0 * extra / analyse.max(1e-9));
    println!("light unreadable: {} from a 320 px picture, {} from the scan's picture", analysed.iter().filter(|a| a.light.is_none()).count(), analysed.iter().filter(|a| a.scan_light.is_none()).count());
    let analysed: Vec<(Option<Record>, Option<light::Light>, Option<light::Light>)> = analysed.into_iter().map(|a| (a.record, a.light, a.scan_light)).collect();
    let failed = analysed.iter().filter(|a| a.0.is_none()).count();
    println!("analyses failed: {failed} (left out of the no-people count)");
    let records: Vec<Record> = analysed.iter().map(|a| a.0.clone().unwrap_or_default()).collect();
    let frames = frames_of(&shots, &records);
    // A photo has no people when it was analysed and its record has no subject faces.
    let no_people: HashSet<String> = frames
        .iter()
        .zip(&analysed)
        .filter(|(f, a)| a.0.is_some() && !f.record.faces.iter().any(|face| face.subject))
        .map(|(f, _)| f.path.clone())
        .collect();
    let lights: Vec<Option<light::Light>> = analysed.iter().map(|a| a.1).collect();
    let prepare = |from: usize, to: usize| cull::prepare_groups(&frames[from..to], &[], kind, |i| lights[from + i], |_| None);

    let all = prepare(0, frames.len());
    let (poses, multi) = (all.len(), all.iter().filter(|g| g.len() > 1).count());
    println!("{poses} groups, {multi} with more than one frame");
    let stars: Vec<usize> = (1..=5).map(|s| shots.iter().filter(|x| x.rating == s).count()).collect();
    println!("photos by stars 1 to 5: {stars:?}");

    let start = starter::starting_net();
    report("Starting network, every photo", &start, &all, &no_people);

    // The same verdicts whether the light came from a 320 px picture or from the scan's?
    let from_scan = cull::prepare_groups(&frames, &[], kind, |i| analysed[i].2, |_| None);
    let by_path = |groups: &[Vec<cull::Prepared>]| -> std::collections::HashMap<String, decide::Judgement> { decide::decide(&start, groups).into_iter().map(|j| (j.path.clone(), j)).collect() };
    let (thumb, scan) = (by_path(&all), by_path(&from_scan));
    let colour = thumb.iter().filter(|(p, j)| scan.get(*p).is_some_and(|k| k.color == j.color)).count();
    let verdict = thumb.iter().filter(|(p, j)| scan.get(*p).is_some_and(|k| (k.color, k.place, k.reject, &k.why) == (j.color, j.place, j.reject, &j.why))).count();
    println!("light from the scan's picture vs a 320 px picture: same colour {}, same verdict (colour, place, reject, why) {}", percent(colour, thumb.len()), percent(verdict, thumb.len()));

    // The app judges run by run as the scan goes, told of the photos shot within
    // two minutes; it must come to the same verdicts as judging the whole folder.
    let whole: std::collections::HashMap<String, (&'static str, Option<u32>, bool, String)> = decide::decide(&start, &all)
        .into_iter().map(|j| (j.path, (j.color, j.place, j.reject, j.why))).collect();
    // The app's runs: frames under 20 s apart (RUN_GAP_MS in src/lib/store.ts). Every pose lies inside one.
    const APP_RUN_GAP_MS: f64 = 20_000.0;
    let started = std::time::Instant::now();
    let (mut same, mut checked) = (0usize, 0usize);
    let mut from = 0;
    while from < frames.len() {
        let mut to = from + 1;
        while to < frames.len() && shots[to].time - shots[to - 1].time <= APP_RUN_GAP_MS {
            to += 1;
        }
        let near = |j: usize| (from..to).any(|k| (shots[j].time - shots[k].time).abs() <= 120_000.0);
        let around: Vec<(f64, light::Light)> = (0..frames.len()).filter(|&j| (j < from || j >= to) && near(j)).filter_map(|j| lights[j].map(|l| (shots[j].time, l))).collect();
        let groups = cull::prepare_groups(&frames[from..to], &around, kind, |i| lights[from + i], |_| None);
        for j in decide::decide(&start, &groups) {
            checked += 1;
            same += usize::from(whole.get(&j.path) == Some(&(j.color, j.place, j.reject, j.why.clone())));
        }
        from = to;
    }
    let per_photo = started.elapsed().as_secs_f64() * 1000.0 / checked.max(1) as f64;
    println!("judged run by run: {} the same as the whole folder; {per_photo:.3} ms per photo (light already read)", percent(same, checked));

    // Halves by run: cut at the first run boundary past the middle.
    let mut cut = shots.len() / 2;
    while cut < shots.len() && shots[cut].time - shots[cut - 1].time <= RUN_GAP_MS {
        cut += 1;
    }
    let (first, second) = (prepare(0, cut), prepare(cut, frames.len()));
    println!("first half {cut} photos, second half {} photos", frames.len() - cut);
    let shoot = lessons::Shoot { folder: String::new(), format: cull::features::FORMAT, taught_at: 0, lessons: lessons::lessons_from(&first, lessons::Source::Stars), set_aside: false, accepted: false };
    println!("lessons taught: {}", shoot.lessons.len());
    let (taught, learned) = train::learn(&start, std::slice::from_ref(&shoot));
    println!("learned: before {:.3}, after {:.3}, accepted {}, enough rated photos {}, reason {:?}", learned.before, learned.after, learned.accepted, learned.enough, learned.reason);
    report("Starting network, second half", &start, &second, &no_people);
    report("Taught network, second half", &taught, &second, &no_people);
    // What the training alone does, whether or not the check would keep it.
    let all_first: Vec<lessons::Lesson> = shoot.lessons.iter().map(|t| t.lesson.clone()).collect();
    let mut unchecked = train::fit(&start, &all_first, train::PULL, train::EPOCHS);
    train::refit(&mut unchecked, &all_first);
    report("Trained network without the check, second half", &unchecked, &second, &no_people);
}
