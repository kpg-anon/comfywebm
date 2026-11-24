//! Auto color on files, for tuning autocolor.rs:
//! cargo run -p comfywebm-engine --example autocolor -- <file>[@start-end] ...
use comfywebm_engine::{autocolor, probe, tools};

fn main() {
    let t = tools::find(None, None);
    let (ffmpeg, ffprobe) = (t.ffmpeg.expect("ffmpeg"), t.ffprobe.expect("ffprobe"));
    for arg in std::env::args().skip(1) {
        let (path, segs) = match arg.rsplit_once('@') {
            Some((p, r)) => {
                let (s, e) = r.split_once('-').expect("start-end");
                (p.to_string(), vec![(s.parse().unwrap(), e.parse().unwrap())])
            }
            None => (arg.clone(), vec![]),
        };
        let info = probe::probe(&ffprobe, &path).expect("probe");
        match autocolor::auto_color(&ffmpeg, &info, &segs, comfywebm_engine::wb::WbMethod::Combined) {
            Ok(r) => println!(
                "{}\n  temp {:+.2} tint {:+.2} bright {:+.2} contrast {:.2} gamma {:.2}\n  {}",
                path,
                r.adjust.temperature,
                r.adjust.tint,
                r.adjust.brightness,
                r.adjust.contrast,
                r.adjust.gamma,
                r.notes.join("\n  ")
            ),
            Err(e) => println!("{path}: {e}"),
        }
    }
}
