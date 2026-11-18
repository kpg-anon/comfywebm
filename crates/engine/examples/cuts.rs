//! Find cuts in one file at each sensitivity, and ffmpeg's old `scene > 0.3`
//! for comparison.
//!
//! cargo run -p comfywebm-engine --example cuts -- <video> [cache dir]

use comfywebm_engine::run::Cancel;
use comfywebm_engine::scenes::{analyze, detect, Sensitivity};
use comfywebm_engine::{probe, tools};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: cuts <video> [cache dir]");
        std::process::exit(2);
    };
    let cache = args.next().map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let t = tools::find(None, None);
    let (ffmpeg, ffprobe) = (t.ffmpeg.expect("ffmpeg"), t.ffprobe.expect("ffprobe"));
    let info = probe::probe(&ffprobe, &path).expect("probe");
    let t0 = Instant::now();
    let a = analyze(&ffmpeg, &info, &cache, &Cancel::new(), |_| {}).map_err(|e| format!("{e:?}")).expect("analyze");
    println!("{} frames in {:.1} s", a.times.len(), t0.elapsed().as_secs_f64());
    for s in [Sensitivity::Low, Sensitivity::Normal, Sensitivity::High] {
        let c = detect(&a, &s.params());
        let list: Vec<String> = c.iter().take(40).map(|t| format!("{t:.2}")).collect();
        println!("{s:?}: {} cuts  {}", c.len(), list.join(" "));
    }
    // the old method, for comparison
    let t0 = Instant::now();
    let out = std::process::Command::new(&ffmpeg)
        .args(["-hide_banner", "-nostdin", "-i", &path, "-an", "-vf", "scale=160:-2,select='gt(scene,0.3)',showinfo", "-f", "null", "-"])
        .output()
        .expect("ffmpeg");
    let old: Vec<String> = String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter_map(|l| l.split("pts_time:").nth(1)?.split_whitespace().next().map(str::to_string))
        .collect();
    println!("ffmpeg scene>0.3: {} cuts ({:.1} s)  {}", old.len(), t0.elapsed().as_secs_f64(), old.iter().take(40).cloned().collect::<Vec<_>>().join(" "));
}
