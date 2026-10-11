//! Checks the face analysis against photos you have labelled yourself, and
//! says how often it agrees — without the photos going anywhere.
//!
//!     cargo run --release --example evaluate -- <folder> [--misses]
//!
//! The folder holds the photos and a `labels.csv` beside them, one photo a
//! line, saying what is true of the people the photo is about:
//!
//!     file,eyes_shut,looking_away,soft
//!     DSC00412.ARW,1,0,0
//!     DSC00413.ARW,0,0,
//!
//! `1` for yes, `0` for no, nothing for "not labelled". Only totals are
//! printed: how many of each the analysis caught, and how many it raised
//! that were not there. `--misses` also lists which files it got wrong, for
//! looking at them yourself; nothing is written anywhere.
//!
//! Keep the folder outside the repository, or under `private-eval/`, which
//! git ignores. This is how the thresholds in `ai/` can be checked after a
//! change, on anyone's own photos, with none of them shared.

use std::path::Path;

use opencull_lib::ai;
use rayon::prelude::*;

/// Reads one answer out of an analysis.
type Reading = fn(&ai::Analysis) -> bool;

/// What a photo's label says, per check: yes, no, or not labelled.
type Said = [Option<bool>; 3];

/// What a label is about, and how to read the analysis's answer for it.
const CHECKS: [(&str, Reading); 3] = [
    ("eyes shut", |a| a.faces.iter().any(|f| f.subject && f.eyes_shut)),
    ("looking away", |a| a.faces.iter().any(|f| f.subject && f.looking_away)),
    // Soft as the face panel shows it: sharpness under 50 out of 100.
    ("soft focus", |a| a.faces.iter().any(|f| f.subject && f.focus_log.is_some_and(|log| ai::focus::score(10f64.powf(log)) < 50.0))),
];

#[derive(Default)]
struct Tally {
    caught: usize,
    missed: usize,
    raised_wrongly: usize,
    agreed_none: usize,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (folder, show_misses) = match args.as_slice() {
        [folder] => (Path::new(folder), false),
        [folder, flag] if flag == "--misses" => (Path::new(folder), true),
        _ => {
            eprintln!("usage: evaluate <folder with labels.csv> [--misses]");
            std::process::exit(2);
        }
    };
    let text = std::fs::read_to_string(folder.join("labels.csv")).unwrap_or_else(|e| {
        eprintln!("no labels.csv in {}: {e}", folder.display());
        std::process::exit(2);
    });

    // file, then one Option<bool> per check.
    let labels: Vec<(String, Said)> = text
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let mut cells = line.split(',').map(str::trim);
            let file = cells.next().unwrap_or_default().to_string();
            let mut said = [None; 3];
            for slot in &mut said {
                *slot = match cells.next() {
                    Some("1") => Some(true),
                    Some("0") => Some(false),
                    _ => None,
                };
            }
            (file, said)
        })
        .collect();

    let results: Vec<(String, Said, Result<ai::Analysis, ai::Failure>)> = labels
        .into_par_iter()
        .map(|(file, said)| {
            let analysed = ai::analyze(&folder.join(&file));
            (file, said, analysed)
        })
        .collect();

    let mut tallies: [Tally; 3] = Default::default();
    let mut failed = 0;
    let mut misses: Vec<String> = Vec::new();
    for (file, said, analysed) in &results {
        let Ok(analysis) = analysed else {
            failed += 1;
            continue;
        };
        for (i, (name, read)) in CHECKS.iter().enumerate() {
            let Some(truth) = said[i] else { continue };
            let found = read(analysis);
            let tally = &mut tallies[i];
            match (truth, found) {
                (true, true) => tally.caught += 1,
                (true, false) => {
                    tally.missed += 1;
                    misses.push(format!("{file}: {name} missed"));
                }
                (false, true) => {
                    tally.raised_wrongly += 1;
                    misses.push(format!("{file}: {name} raised wrongly"));
                }
                (false, false) => tally.agreed_none += 1,
            }
        }
    }

    println!("{} photos labelled, {failed} could not be analysed", results.len());
    for ((name, _), t) in CHECKS.iter().zip(&tallies) {
        let present = t.caught + t.missed;
        let absent = t.raised_wrongly + t.agreed_none;
        if present + absent == 0 {
            continue;
        }
        let rate = |n: usize, of: usize| if of == 0 { "-".to_string() } else { format!("{:.1}%", 100.0 * n as f64 / of as f64) };
        println!(
            "{name:>13}: caught {} of {present} ({}), raised on {} of {absent} without it ({})",
            t.caught,
            rate(t.caught, present),
            t.raised_wrongly,
            rate(t.raised_wrongly, absent)
        );
    }
    if show_misses {
        misses.sort();
        for m in misses {
            println!("  {m}");
        }
    }
}
