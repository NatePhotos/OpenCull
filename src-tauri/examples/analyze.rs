//! Runs the face analysis over photos and writes what it finds, one line a
//! face, so it can be set beside the Mac app's readings for the same photos.
//!
//!     cargo run --release --features turbo-system --example analyze -- \
//!         <list.csv: name,path per line> <out.csv> [folder for face and eye crops]
//!
//! Photographs are only read.

use std::io::Write;
use std::path::Path;
use std::time::Instant;

use opencull_lib::{ai, faces, preview};
use rayon::prelude::*;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `--basic` anywhere judges by the rules of release 0.3.
    let rules = if args.iter().any(|a| a == "--basic") { ai::Rules::Basic } else { ai::Rules::Advanced };
    args.retain(|a| a != "--basic");
    let (list, out, crops) = match args.as_slice() {
        [list, out] => (list, out, None),
        [list, out, crops] => (list, out, Some(std::path::PathBuf::from(crops))),
        _ => {
            eprintln!("usage: analyze <list.csv> <out.csv> [folder for crops]");
            std::process::exit(2);
        }
    };
    if let Some(dir) = &crops {
        std::fs::create_dir_all(dir).expect("make the folder for crops");
    }
    let text = std::fs::read_to_string(list).expect("read the list");
    let jobs: Vec<(&str, &str)> = text.lines().filter_map(|l| l.split_once(',')).collect();

    let started = Instant::now();
    let lines: Vec<String> = jobs
        .par_iter()
        .map(|(name, path)| {
            let t = Instant::now();
            let analysed = preview::decode_upright(Path::new(path), u32::MAX).and_then(|p| {
                let a = ai::analyze_pixels(&p.rgb, p.width, p.height, rules)?;
                if let Some(dir) = &crops {
                    for f in &a.faces {
                        let stem = format!("{name}_{:.4}_{:.4}", f.place.mid_x, f.place.mid_y);
                        for (end, rect, w, h) in [("face", f.place.box_rect(p.width, p.height), 256, 256), ("eyes", f.place.eyes_rect(p.width, p.height), 448, 128)] {
                            if let Ok(cut) = faces::cut(&p.rgb, p.width, p.height, rect, w, h) {
                                std::fs::write(dir.join(format!("{stem}-{end}.jpg")), preview::encode_jpeg(cut, w, h, 90)?).ok();
                            }
                        }
                    }
                }
                Ok(a)
            });
            match analysed {
                Err(e) => format!("{name},ERROR,{e}\n"),
                Ok(a) => {
                    let ms = t.elapsed().as_millis();
                    let score = a.score.map_or(String::new(), |s| format!("{s:.2}"));
                    if a.faces.is_empty() {
                        return format!("{name},0,0,0,0,0,0,,0,0,0,0,0,0,,{score},{ms}\n");
                    }
                    a.faces
                        .iter()
                        .map(|f| {
                            format!(
                                "{name},{:.4},{:.4},{:.4},{:.4},{:.3},{},{:?},{:.4},{:.4},{:.4},{:.4},{},{},{},{score},{ms},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}\n",
                                f.place.mid_x,
                                f.place.mid_y,
                                f.place.w,
                                f.place.h,
                                f.sure,
                                u8::from(f.subject),
                                f.reason,
                                f.verdict.eyes[2],
                                f.verdict.ear,
                                f.verdict.quality,
                                f.verdict.away,
                                u8::from(f.eyes_shut),
                                u8::from(f.looking_away),
                                f.focus_log.map_or(String::new(), |v| format!("{v:.4}")),
                                f.verdict.validity[0],
                                f.verdict.validity[1],
                                f.verdict.validity[2],
                                f.verdict.gaze[0],
                                f.verdict.gaze[1],
                                f.verdict.gaze[2],
                                f.verdict.gaze[3],
                                f.verdict.gaze[4],
                                f.verdict.eye_offset,
                                f.verdict.head_yaw,
                                f.verdict.eyes[0],
                                f.verdict.eyes[1],
                                f.verdict.eyes[3],
                                f.eyes_away.map_or(String::new(), |v| format!("{v:.4}")),
                                f.state.map_or("", |s| s.name()),
                                f.blink.map_or(String::new(), |v| format!("{v:.4}")),
                                f.verdict.expression[0],
                                f.verdict.expression[1],
                                f.verdict.expression[2],
                                f.verdict.expression[3],
                                f.verdict.expression[4],
                                f.verdict.profile,
                                f.verdict.down,
                            )
                        })
                        .collect()
                }
            }
        })
        .collect();

    let mut file = std::fs::File::create(out).expect("create the output file");
    writeln!(file, "file,midX,midY,w,h,sure,subject,reason,shut,ear,quality,away,eyesShut,lookingAway,focusLog,score,ms,isSubject,isBackground,notFace,atCamera,slightlyOff,gazeAway,gazeDown,profile,eyeOffset,headYaw,eyesOpen,eyesPartial,eyesHidden,eyesAway,state,blink,neutral,smiling,laughing,midWord,otherFace,profileFace,downFace").unwrap();
    for line in &lines {
        file.write_all(line.as_bytes()).unwrap();
    }
    let seconds = started.elapsed().as_secs_f64();
    println!("{} photos in {seconds:.1} s — {:.0} a second with every core, {:.0} ms each", jobs.len(), jobs.len() as f64 / seconds, seconds * 1000.0 / jobs.len() as f64);
}
