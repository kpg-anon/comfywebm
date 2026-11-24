//! White balance methods (wb.rs) measured on real clips with known casts:
//! cargo run --release -p comfywebm-engine --example wb_bench -- <file>[@start-end] ...
//!
//! Each frame is taken as correctly balanced, given a cast in linear light (clipped and
//! rounded to 8-bit sRGB again, as a camera would), and each method's estimate of the
//! light is compared with the cast: the angular error in degrees, as colour constancy
//! papers report it. "neutral" is the estimate on the frame as it is (how far a method
//! would move a frame that needs nothing).
use comfywebm_engine::wb::{self, angle, Frame, WbMethod};
use comfywebm_engine::{autocolor, probe, tools};

/// residual casts: warm, cool, green, magenta and mixes (red, green, blue gains)
const CASTS: [[f64; 3]; 8] = [
    [1.0, 0.92, 0.70],
    [1.0, 0.85, 0.55],
    [0.78, 0.90, 1.0],
    [0.65, 0.82, 1.0],
    [0.92, 1.0, 0.92],
    [1.0, 0.88, 1.0],
    [1.0, 0.95, 0.85],
    [0.88, 1.0, 0.80],
];

fn cast(f: &Frame, c: [f64; 3]) -> Frame {
    let px = f
        .px
        .iter()
        .map(|p| {
            [0, 1, 2].map(|i| {
                // 8-bit again after the cast, as in a file
                let v = wb::to_srgb((p[i] * c[i]).min(1.0));
                wb::to_linear((v * 255.0).round() / 255.0)
            })
        })
        .collect();
    Frame { w: f.w, h: f.h, px }
}

fn main() {
    let t = tools::find(None, None);
    let (ffmpeg, ffprobe) = (t.ffmpeg.expect("ffmpeg"), t.ffprobe.expect("ffprobe"));
    let mut err: Vec<Vec<f64>> = vec![Vec::new(); WbMethod::ALL.len()];
    let mut neutral: Vec<Vec<f64>> = vec![Vec::new(); WbMethod::ALL.len()];
    for arg in std::env::args().skip(1) {
        let (path, segs) = match arg.rsplit_once('@') {
            Some((p, r)) => {
                let (s, e) = r.split_once('-').expect("start-end");
                (p.to_string(), vec![(s.parse().unwrap(), e.parse().unwrap())])
            }
            None => (arg.clone(), vec![]),
        };
        let info = probe::probe(&ffprobe, &path).expect("probe");
        let frames = autocolor::read_frames(&ffmpeg, &info, &autocolor::sample_times(&segs, info.duration)).expect("frames");
        let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().chars().take(48).collect::<String>()).unwrap_or_default();
        let mut line = format!("{name:<50}");
        for (k, m) in WbMethod::ALL.into_iter().enumerate() {
            // per clip, as the app does (all its frames together)
            let n = angle(wb::estimate(m, &frames), [1.0, 1.0, 1.0]);
            neutral[k].push(n);
            let mut e_clip = Vec::new();
            for c in CASTS {
                let cf: Vec<Frame> = frames.iter().map(|f| cast(f, c)).collect();
                e_clip.push(angle(wb::estimate(m, &cf), c));
            }
            line.push_str(&format!(" {:>6.2}", e_clip.iter().sum::<f64>() / e_clip.len() as f64));
            err[k].extend(e_clip);
        }
        println!("{line}");
    }
    let stats = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
        let med = v[v.len() / 2];
        let worst = v[v.len() * 3 / 4..].iter().sum::<f64>() / (v.len() - v.len() * 3 / 4).max(1) as f64;
        (mean, med, worst)
    };
    println!("\n{:<16} {:>7} {:>7} {:>9} {:>14}", "method", "mean", "median", "worst 25%", "neutral mean");
    for (k, m) in WbMethod::ALL.into_iter().enumerate() {
        let (a, b, c) = stats(&mut err[k]);
        let (n, _, _) = stats(&mut neutral[k]);
        println!("{:<16} {:>7.2} {:>7.2} {:>9.2} {:>14.2}", m.name(), a, b, c, n);
    }
}
