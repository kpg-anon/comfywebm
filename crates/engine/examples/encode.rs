//! Smoke harness: probe a file and run one size-mode encode, or ask yt-dlp about a link.
//!
//! cargo run -p comfywebm-engine --example encode -- <input> <output>
//!     [--range S-E]... [--crop x,y,w,h] [--max-bytes N] [--audio-k K]
//!     [--container webm|mp4] [--codec vp9|vp8|av1|h264|hevc] [--effort fast|balanced|best]
//! cargo run -p comfywebm-engine --example encode -- --url-info <url>

use comfywebm_engine::{
    plan, probe, run, tools, ytdl, AudioSel, Codec, Container, Crop, Effort, EncodeSpec, Playback, Rate, Segment,
};
use std::io::Write;
use std::path::Path;

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut pos = Vec::new();
    let (mut segments, mut crop, mut max_bytes, mut audio_k) = (Vec::new(), None, 4_194_304.0, None);
    let (mut container, mut codec, mut effort) = (Container::Webm, Codec::Vp9, Effort::Fast);
    let mut url = None;
    let num = |v: Option<String>, name: &str| -> Result<f64, String> {
        v.and_then(|s| s.parse().ok()).ok_or(format!("{name} needs a number"))
    };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--url-info" => url = args.next(),
            "--range" => {
                let r = args.next().unwrap_or_default();
                let (s, e) = r.split_once('-').ok_or("--range needs S-E")?;
                segments.push(Segment {
                    start: s.parse().map_err(|_| "bad range start")?,
                    end: e.parse().map_err(|_| "bad range end")?,
                });
            }
            "--max-bytes" => max_bytes = num(args.next(), "--max-bytes")?,
            "--audio-k" => audio_k = Some(num(args.next(), "--audio-k")?),
            "--crop" => {
                let v: Vec<f64> = args.next().unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect();
                if v.len() != 4 {
                    return Err("--crop needs x,y,w,h".into());
                }
                crop = Some(Crop { x: v[0], y: v[1], w: v[2], h: v[3] });
            }
            "--container" => container = if args.next().as_deref() == Some("mp4") { Container::Mp4 } else { Container::Webm },
            "--codec" => {
                codec = match args.next().as_deref() {
                    Some("vp8") => Codec::Vp8,
                    Some("av1") => Codec::Av1,
                    Some("h264") => Codec::H264,
                    Some("hevc") => Codec::Hevc,
                    _ => Codec::Vp9,
                }
            }
            "--effort" => {
                effort = match args.next().as_deref() {
                    Some("balanced") => Effort::Balanced,
                    Some("best") => Effort::Best,
                    _ => Effort::Fast,
                }
            }
            _ => pos.push(a),
        }
    }

    let t = tools::find(None, None);
    let st = tools::status(&t);
    println!("tools: {}", serde_json::to_string(&st).unwrap());

    if let Some(url) = url {
        let ytdlp = t.ytdlp.ok_or("yt-dlp not found")?;
        let info = ytdl::url_info(&ytdlp, &url, &[])?;
        println!("url_info: {}", serde_json::to_string_pretty(&info).unwrap());
        return Ok(());
    }

    let [input, output] = <[String; 2]>::try_from(pos).map_err(|_| "usage: encode <input> <output> [options]")?;
    let ffmpeg = t.ffmpeg.ok_or("ffmpeg not found")?;
    let ffprobe = t.ffprobe.ok_or("ffprobe not found")?;

    let info = probe::probe(&ffprobe, &input)?;
    println!("probe: {}", serde_json::to_string(&info).unwrap());

    let spec = EncodeSpec {
        input,
        segments,
        crop,
        rotate: 0,
        mirror: false,
        max_width: None,
        max_height: None,
        auto_scale: false,
        fps: None,
        speed: 1.0,
        playback: Playback::Forward,
        deinterlace: info.video.interlaced,
        denoise: false,
        tonemap: info.video.hdr,
        fade_in: 0.0,
        fade_out: 0.0,
        container,
        codec,
        rate: Rate::Size { max_bytes },
        two_pass: true,
        effort,
        audio: audio_k.map(|k| AudioSel { track: 0, bitrate_k: k, volume_db: 0.0, normalize: false }),
        subs: None,
        title: None,
        extra_args: vec![],
        output_name: None,
    };
    let zscale = st.filters.zscale;
    let shown = plan::build(&spec, &info, &plan::Opts { display: true, zscale, ..plan::Opts::new(&output, "passlog") })?.info();
    println!("plan: {}", serde_json::to_string_pretty(&shown).unwrap());

    let cancel = run::Cancel::new();
    let outcome = run::run_job(&ffmpeg, &spec, &info, Path::new(&output), zscale, &cancel, |p| {
        print!(
            "\rattempt {} pass {}/{} {:5.1}% speed {:?} eta {:?}      ",
            p.attempt,
            p.pass,
            p.passes,
            p.fraction * 100.0,
            p.speed,
            p.eta_secs.map(|e| e.round())
        );
        let _ = std::io::stdout().flush();
    })
    .map_err(|e| format!("{e:?}"))?;
    println!();
    for (i, (k, b)) in outcome.attempts.iter().enumerate() {
        println!("attempt {}: {} kbps -> {} bytes", i + 1, k.round(), b);
    }
    println!("output: {}", outcome.output.display());
    println!("result: {}", serde_json::to_string(&outcome.result).unwrap());
    Ok(())
}
