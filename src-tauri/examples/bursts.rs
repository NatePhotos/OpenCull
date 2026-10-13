//! Analyses a folder's photos, groups them into bursts the way the app does,
//! and marks the frames of each burst against one another — to see what
//! following people from frame to frame changes.
//!
//!     cargo run --release --features turbo-system --example bursts -- \
//!         <list.csv: name,path per line> <out.csv>
//!
//! Photographs are only read.

use std::io::Write;
use std::path::Path;

use opencull_lib::ai::{self, series, store::Record};
use opencull_lib::{burst, raw};
use rayon::prelude::*;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `--basic` anywhere judges by the rules of release 0.3.
    let rules = if args.iter().any(|a| a == "--basic") { ai::Rules::Basic } else { ai::Rules::Advanced };
    args.retain(|a| a != "--basic");
    let [list, out] = args.as_slice() else {
        eprintln!("usage: bursts <list.csv> <out.csv>");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(list).expect("read the list");
    let jobs: Vec<(&str, &str)> = text.lines().filter_map(|l| l.split_once(',')).collect();

    let found: Vec<(Option<f64>, Record)> = jobs
        .par_iter()
        .map(|(_, path)| {
            let path = Path::new(path);
            (raw::capture_time(path), ai::analyze(path, rules).map(|a| Record::from(&a)).unwrap_or_default())
        })
        .collect();
    let groups = burst::group(&found.iter().map(|f| f.0).collect::<Vec<_>>());

    // The whole folder as one run, in the order it was shot: the gaps
    // between sittings split it as surely as sending them apart would.
    let mut order: Vec<usize> = (0..found.len()).filter(|&i| found[i].0.is_some()).collect();
    order.sort_by(|a, b| found[*a].0.partial_cmp(&found[*b].0).unwrap().then(a.cmp(b)));
    let frames: Vec<series::Frame> = order.iter().map(|&i| series::Frame { record: &found[i].1, time: found[i].0, burst: groups[i], upright: None }).collect();
    let marked = series::compare_run(&frames, true);

    let mut poses: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for (at, m) in marked.iter().enumerate() {
        poses.entry(m.pose).or_default().push(at);
    }

    let mut file = std::fs::File::create(out).expect("create the output file");
    writeln!(file, "pose,file,burst,faces,alone,together,rank,of,eyes").unwrap();
    if let Ok(mut times) = std::fs::File::create(format!("{out}.times")) {
        for (job, f) in jobs.iter().zip(&found) {
            writeln!(times, "{},{}", job.0, f.0.map_or(String::new(), |t| format!("{t:.3}"))).ok();
        }
    }
    let (mut several, mut held, mut largest, mut followed, mut blinks) = (0, 0, 0, 0, 0);
    let bursts = groups.iter().flatten().collect::<std::collections::BTreeSet<_>>().len();
    for (id, members) in &poses {
        let ranks = ai::score::ranks(&members.iter().map(|&at| marked[at].score.map(f64::from)).collect::<Vec<_>>());
        if members.len() > 1 {
            several += 1;
            held += members.len();
            largest = largest.max(members.len());
        }
        for (place, &at) in members.iter().enumerate() {
            let (i, record) = (order[at], frames[at].record);
            let eyes: Vec<String> = marked[at].eyes.iter().map(|e| e.map_or("-".to_string(), |e| format!("{:.0}", e * 100.0))).collect();
            followed += marked[at].eyes.iter().flatten().count();
            blinks += marked[at].eyes.iter().zip(&record.faces).filter(|(e, f)| f.subject && !f.eyes_shut && e.is_some_and(|e| e < 0.6)).count();
            let show = |s: Option<f32>| s.map_or(String::new(), |s| format!("{s:.1}"));
            let burst = groups[i].map_or(String::new(), |g| g.to_string());
            writeln!(file, "{id},{},{burst},{},{},{},{},{},{}", jobs[i].0, record.faces.len(), show(record.score), show(marked[at].score), ranks[place], members.len(), eyes.join(" ")).unwrap();
        }
    }
    println!(
        "{} photos, {bursts} bursts by the clock; {several} poses of two or more frames holding {held} photos, the largest {largest}; {followed} faces followed from frame to frame; {blinks} subjects caught blinking that were not called shut",
        jobs.len()
    );
}
