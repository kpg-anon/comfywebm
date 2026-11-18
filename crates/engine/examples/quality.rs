//! The quality check for one job, as the app runs it.
//!
//! cargo run --release -p comfywebm-engine --example quality -- <spec.json>

use comfywebm_engine::run::Cancel;
use comfywebm_engine::{probe, quality, tools, EncodeSpec};
use std::time::Instant;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: quality <spec.json>");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(&path).expect("read spec");
    let spec: EncodeSpec = serde_json::from_str(&text).expect("parse spec");
    let t = tools::find(None, None);
    let (ffmpeg, ffprobe) = (t.ffmpeg.expect("ffmpeg"), t.ffprobe.expect("ffprobe"));
    let info = probe::probe(&ffprobe, &spec.input).expect("probe");
    let t0 = Instant::now();
    match quality::check(&ffmpeg, &spec, &info, true, &std::env::temp_dir(), &Cancel::new()) {
        Ok(v) => println!("VMAF {v:.1} in {:.1} s (window {:?})", t0.elapsed().as_secs_f64(), quality::window(&spec, &info)),
        Err(e) => println!("error: {e:?}"),
    }
}
