//! Smoke harness: probe a file and run one size-mode encode, or ask yt-dlp about a link.
//!
//! cargo run -p comfywebm-engine --example encode -- <input> <output>
//!     [--range S-E]... [--crop x,y,w,h] [--max-bytes N] [--audio-k K]
//!     [--container webm|mp4|gif|webp] [--codec vp9|vp8|av1|h264|hevc|gif|webp] [--effort fast|balanced|best] [--caption TEXT]
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
    let mut caption = None;
    let num = |v: Option<String>, name: &str| -> Result<f64, String> {
        v.and_then(|s| s.parse().ok()).ok_or(format!("{name} needs a number"))
    };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--url-info" => url = args.next(),
            "--caption" => caption = args.next().map(|text| comfywebm_engine::Caption {
                text,
                top: false,
                size: 7.0,
                font: std::env::var("CWM_CAPTION_FONT").ok(),
                color: std::env::var("CWM_CAPTION_COLOR").unwrap_or_else(|_| "#ffffff".into()),
                outline_color: "#000000".into(),
                outline: 7.0,
                custom: false,
                x: 0.5,
                y: 0.9,
            }),
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
            "--container" => container = match args.next().as_deref() {
                Some("mp4") => Container::Mp4,
                Some("gif") => Container::Gif,
                Some("webp") => Container::Webp,
                Some("audio") => Container::Audio,
                _ => Container::Webm,
            },
            "--codec" => {
                codec = match args.next().as_deref() {
                    Some("vp8") => Codec::Vp8,
                    Some("av1") => Codec::Av1,
                    Some("h264") => Codec::H264,
                    Some("hevc") => Codec::Hevc,
                    Some("gif") => Codec::Gif,
                    Some("webp") => Codec::Webp,
                    Some("opus") => Codec::Opus,
                    Some("mp3") => Codec::Mp3,
                    Some("aac") => Codec::Aac,
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

    // CWM_JOIN=<file>: append that whole file as one more range (joined)
    let join = std::env::var("CWM_JOIN").ok();
    let mut sources = std::collections::HashMap::new();
    let mut segment_sources = Vec::new();
    if let Some(j) = &join {
        let ji = probe::probe(&ffprobe, j)?;
        if segments.is_empty() {
            segments.push(Segment { start: 0.0, end: info.duration });
        }
        segment_sources = vec![None; segments.len()];
        segments.push(Segment { start: 0.0, end: ji.duration });
        segment_sources.push(Some(j.clone()));
        sources.insert(j.clone(), ji);
    }
    // CWM_COVER=<image>: the picture for a sound file (its size from probing it, as the app does)
    let cover = std::env::var("CWM_COVER").ok();
    let cover_size = match &cover {
        Some(c) => {
            let ci = probe::probe(&ffprobe, c)?;
            Some((ci.video.width, ci.video.height))
        }
        None => None,
    };
    let spec = EncodeSpec {
        input,
        segments,
        crop,
        rotate: 0,
        mirror: false,
        vflip: false,
        adjust: None,
        max_width: None,
        max_height: None,
        auto_scale: codec.is_image(),
        fps: if codec.is_image() { Some(15.0) } else { None },
        speed: std::env::var("CWM_SPEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0),
        speed_ramp: vec![],
        zoom: vec![],
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
        subs: std::env::var("CWM_SUBS").ok().map(|path| comfywebm_engine::SubsSel::File { path }),
        title: None,
        extra_args: vec![],
        output_name: None,
        stream_copy: false,
        caption,
        segment_sources,
        layout: None,
        music: std::env::var("CWM_MUSIC").ok().map(|path| comfywebm_engine::Music { path, mix: std::env::var("CWM_MIX").is_ok(), volume_db: -6.0, offset: 0.0 }),
        overlay: std::env::var("CWM_LOGO").ok().map(|path| comfywebm_engine::Overlay {
            path,
            corner: std::env::var("CWM_LOGO_PLACE").unwrap_or_else(|_| "br".into()),
            size: 20.0,
            opacity: 0.8,
            x: 0.5,
            y: 0.5,
            spacing: 0.5,
            angle: std::env::var("CWM_LOGO_ANGLE").ok().and_then(|a| a.parse().ok()).unwrap_or(0.0),
        }),
        subs_delay: std::env::var("CWM_SUBS_DELAY").ok().and_then(|s| s.parse().ok()).unwrap_or(0.0),
        subs_font_size: std::env::var("CWM_SUBS_SIZE").ok().and_then(|s| s.parse().ok()),
        stabilize: std::env::var("CWM_STAB").is_ok(),
        // CWM_STAB=vidstab: vid.stab, any other value: deshake
        stab_method: if std::env::var("CWM_STAB").as_deref() == Ok("vidstab") { comfywebm_engine::StabMethod::Vidstab } else { comfywebm_engine::StabMethod::Deshake },
        smooth_slowmo: std::env::var("CWM_SMOOTH").is_ok(),
        output_dir: None,
        cover,
        cover_size,
    };
    let zscale = st.filters.zscale;
    let shown = plan::build(&spec, &info, &plan::Opts { display: true, zscale, sources: Some(&sources), ..plan::Opts::new(&output, "passlog") })?.info();
    println!("plan: {}", serde_json::to_string_pretty(&shown).unwrap());

    let cancel = run::Cancel::new();
    let outcome = run::run_job(&ffmpeg, &spec, &info, Path::new(&output), zscale, Some(&sources), None, &cancel, |p| {
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
