//! Follows a box through a file and prints where it is at some times.
//!
//! cargo run --release -p comfywebm-engine --example track -- <video> <start> <end> <x> <y> <w> <h> [times...]

use comfywebm_engine::run::Cancel;
use comfywebm_engine::{probe, tools, track, Rect};
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 7 {
        eprintln!("usage: track <video> <start> <end> <x> <y> <w> <h> [times...]");
        std::process::exit(2);
    }
    let n = |i: usize| a[i].parse::<f64>().expect("number");
    let t = tools::find(None, None);
    let (ffmpeg, ffprobe) = (t.ffmpeg.expect("ffmpeg"), t.ffprobe.expect("ffprobe"));
    let info = probe::probe(&ffprobe, &a[0]).expect("probe");
    let bx = Rect { x: n(3) as u32, y: n(4) as u32, w: n(5) as u32, h: n(6) as u32 };
    let t0 = Instant::now();
    let pts = track::run(&ffmpeg, &info, n(1), n(2), bx, &Cancel::new(), |_| {}).map_err(|e| format!("{e:?}")).expect("track");
    eprintln!("{} frames in {:.1} s", pts.len(), t0.elapsed().as_secs_f64());
    let lost = pts.iter().filter(|p| p.score < 0.35).count();
    eprintln!("weak matches: {lost}");
    for want in a[7..].iter().map(|s| s.parse::<f64>().expect("time")) {
        let p = pts.iter().min_by(|x, y| (x.t - want).abs().total_cmp(&(y.t - want).abs())).expect("points");
        println!("{:.2} {:.0} {:.0} {:.0} {:.0} {:.2}", p.t, p.x, p.y, p.w, p.h, p.score);
    }
}
