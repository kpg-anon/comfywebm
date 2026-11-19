//! Quality check: how close the encode will look to the source.
//!
//! The bits per pixel of a job do not say that: on a 4K fancam with a lot of
//! motion, 4 MiB for 10 s scored VMAF ~82 (visible compression) at 1152x2048
//! (0.047 bpp) and at 648x1152 (0.148 bpp) alike, while the app called both
//! "good". So a short piece is encoded with the job's settings and measured:
//!
//! 1. A window of the source (about 2 s of output, from the middle of the
//!    longest range) is encoded as the job would, at the bitrate and picture
//!    size the full job gets, in one pass.
//! 2. The same window goes through the same filters (crop, rotation, speed,
//!    adjustments, caption, watermark) into a practically lossless H.264
//!    reference (CRF 1) at the
//!    picture's own size, at most 1920 pixels long, so only the encode and its
//!    smaller size count against it.
//! 3. ffmpeg's libvmaf compares the two (the encode scaled up to the
//!    reference): 0..100, where about 93 and up is hard to tell from the
//!    source and 80..85 shows visible compression.

use crate::plan::{self, Opts};
use crate::run::{Cancel, RunError};
use crate::spec::{Codec, Container, Effort, EncodeSpec, MediaInfo, Rate, Segment};
use std::path::Path;

/// Output seconds measured.
const PROBE_OUT_SECS: f64 = 2.0;
/// The reference's long side.
const REF_LONG: u32 = 1920;

/// The source window measured: about PROBE_OUT_SECS of output from the middle of the longest range.
pub fn window(spec: &EncodeSpec, info: &MediaInfo) -> (f64, f64) {
    let ranges: Vec<(f64, f64)> = if spec.segments.is_empty() {
        vec![(0.0, info.duration)]
    } else {
        spec.segments.iter().map(|s| (s.start, s.end.min(info.duration.max(s.end)))).collect()
    };
    let (s, e) = ranges.into_iter().max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0))).unwrap_or((0.0, info.duration));
    // a slower speed stretches the source: 2 s of output at 0.5x is 1 s of source
    let speed = if spec.speed_ramp.is_empty() && spec.speed.is_finite() && spec.speed > 0.0 { spec.speed.min(4.0) } else { 1.0 };
    let len = (PROBE_OUT_SECS * speed).clamp(0.5, 8.0).min(e - s);
    let start = s + ((e - s) - len) / 2.0;
    (start, start + len)
}

/// The reference size: the picture before any downscale, at most REF_LONG on the long side, even.
pub fn reference_size(w: u32, h: u32) -> (u32, u32) {
    let k = (REF_LONG as f64 / w.max(h).max(1) as f64).min(1.0);
    let even = |v: f64| (((v / 2.0).round() as u32) * 2).max(2);
    (even(w as f64 * k), even(h as f64 * k))
}

/// The probe's VMAF, 0..100. `dir` holds the two temporary files (deleted after).
pub fn check(ffmpeg: &Path, spec: &EncodeSpec, info: &MediaInfo, zscale: bool, dir: &Path, cancel: &Cancel) -> Result<f64, RunError> {
    if spec.layout.is_some() || spec.stream_copy || spec.codec.is_image() || spec.codec.is_audio() || spec.container == Container::Audio || info.audio_only {
        return Err(RunError::Failed("No quality check for this kind of output.".into()));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    // the full job: its bitrate and picture size
    let mut own = spec.clone();
    own.segment_sources.clear();
    let full = plan::build(&own, info, &Opts { display: true, zscale, ..Opts::new("", "") })?;
    let (ws, we) = window(&own, info);
    let mut probe = own.clone();
    probe.segments = vec![Segment { start: ws, end: we }];
    probe.speed_ramp.clear();
    probe.fade_in = 0.0;
    probe.fade_out = 0.0;
    probe.audio = None;
    probe.music = None;
    probe.auto_scale = false;
    probe.max_width = Some(full.out_width);
    probe.max_height = Some(full.out_height);
    // the reference: the same filters, no downscale past REF_LONG, practically lossless
    let unscaled = plan::build(&EncodeSpec { max_width: None, max_height: None, auto_scale: false, ..probe.clone() }, info, &Opts { display: true, zscale, ..Opts::new("", "") })?;
    let (rw, rh) = reference_size(unscaled.out_width, unscaled.out_height);
    let reference = EncodeSpec {
        container: Container::Mp4,
        codec: Codec::H264,
        // CRF 1: practically lossless (the engine's High profile does not allow 0, true lossless)
        rate: Rate::Crf { crf: 1.0, max_bitrate_k: None },
        two_pass: false,
        // the reference is practically lossless at any effort; Best would only make the check slower
        effort: Effort::Balanced,
        max_width: Some(rw),
        max_height: Some(rh),
        extra_args: Vec::new(),
        title: None,
        ..probe.clone()
    };
    let pid = std::process::id();
    let probe_out = dir.join(format!("quality-{pid}.{}", spec.ext()));
    // the reference goes into Matroska: read beside the probe in one ffmpeg run, an MP4 got its
    // frames paired wrongly (about 1 in 3 against the next frame), which scored a matching pair 35 instead of 73
    let ref_out = dir.join(format!("quality-{pid}-ref.mkv"));
    let passlog = dir.join(format!("quality-{pid}-pass"));
    let cleanup = || {
        // CWM_QUALITY_KEEP: leave the files for a look (examples/quality.rs)
        if std::env::var_os("CWM_QUALITY_KEEP").is_some() {
            return;
        }
        let _ = std::fs::remove_file(&probe_out);
        let _ = std::fs::remove_file(&ref_out);
        for ext in ["-0.log", "-0.log.mbtree", ".log", ".log.mbtree", "-stab0.trf"] {
            let _ = std::fs::remove_file(format!("{}{ext}", passlog.display()));
        }
    };
    let res = (|| {
        let probe_s = probe_out.to_string_lossy().into_owned();
        let log_s = passlog.to_string_lossy().into_owned();
        // two passes when the job has two: one pass scored 9 points under the full encode on the same frames, two passes 4
        let p = plan::build(&probe, info, &Opts { video_kbps: full.video_kbps, zscale, ..Opts::new(&probe_s, &log_s) })?;
        // the stabilizer's analysis (vid.stab), which the reference reads too
        crate::run::run_prepasses(ffmpeg, &p, cancel, |_, _| {})?;
        for pass in &p.passes {
            crate::run::run_ffmpeg(ffmpeg, pass, p.duration_out, cancel, |_| {})?;
        }
        let ref_s = ref_out.to_string_lossy().into_owned();
        // the same pass log prefix, so the reference is stabilized from the same analysis
        let r = plan::build(&reference, info, &Opts { zscale, single_pass: true, ..Opts::new(&ref_s, &log_s) })?;
        let mut ref_args = r.passes[0].clone();
        to_matroska(&mut ref_args);
        if std::env::var_os("CWM_QUALITY_KEEP").is_some() {
            eprintln!("probe: {}", p.passes.last().map(|a| a.join(" ")).unwrap_or_default());
            eprintln!("ref: {}", ref_args.join(" "));
        }
        crate::run::run_ffmpeg(ffmpeg, &ref_args, r.duration_out, cancel, |_| {})?;
        vmaf(ffmpeg, &probe_out, &ref_out, (r.out_width, r.out_height), cancel)
    })();
    cleanup();
    res
}

/// The MP4 command writes Matroska instead (the output path is already .mkv).
fn to_matroska(args: &mut Vec<String>) {
    if let Some(i) = args.iter().position(|a| a == "-movflags") {
        args.drain(i..i + 2);
    }
    if let Some(i) = args.iter().position(|a| a == "-f") {
        if args.get(i + 1).is_some_and(|f| f == "mp4") {
            args[i + 1] = "matroska".into();
        }
    }
}

/// VMAF of `dist` against `reference` (`dist` scaled up to the reference's size).
fn vmaf(ffmpeg: &Path, dist: &Path, reference: &Path, (w, h): (u32, u32), cancel: &Cancel) -> Result<f64, RunError> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    // frames are paired by their number, not their timestamps: both files come from the same
    // filters and have the same frames, but an MP4 probe (exact 1/30 s steps) beside the Matroska
    // reference (rounded to ms) paired every third frame with its neighbour and scored 68 for 99.7
    let graph = format!(
        "[0:v]settb=1/1000,setpts=N*40,scale={w}:{h}:flags=bicubic,format=yuv420p[d];[1:v]settb=1/1000,setpts=N*40,format=yuv420p[r];[d][r]libvmaf=n_threads={threads}"
    );
    let mut cmd = crate::tools::command(ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-i"])
        .arg(dist)
        .arg("-i")
        .arg(reference)
        .args(["-lavfi", &graph, "-f", "null", "-"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| RunError::Failed(format!("Cannot run ffmpeg: {e}")))?;
    let stderr = child.stderr.take().expect("piped stderr");
    cancel.attach(child);
    let mut score = None;
    crate::run::each_line(stderr, |line| {
        if let Some(v) = line.split("VMAF score:").nth(1).and_then(|r| r.trim().parse::<f64>().ok()) {
            score = Some(v);
        }
    });
    let _ = cancel.detach().map(|mut c| c.wait());
    if cancel.is_canceled() {
        return Err(RunError::Canceled);
    }
    score.ok_or_else(|| RunError::Failed("ffmpeg gave no VMAF score (it needs the libvmaf filter).".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp4_args_become_matroska() {
        let mut a: Vec<String> = ["-c:v", "libx264", "-movflags", "+faststart", "-f", "mp4", "x.mkv"].iter().map(|s| s.to_string()).collect();
        to_matroska(&mut a);
        assert_eq!(a, ["-c:v", "libx264", "-f", "matroska", "x.mkv"]);
    }

    #[test]
    fn reference_is_at_most_1920_long() {
        assert_eq!(reference_size(1152, 2048), (1080, 1920));
        assert_eq!(reference_size(2160, 1215), (1920, 1080));
        assert_eq!(reference_size(640, 360), (640, 360));
    }

    #[test]
    fn window_is_two_output_seconds_from_the_middle_of_the_longest_range() {
        let info = MediaInfo { duration: 60.0, ..Default::default() };
        let mut spec: EncodeSpec = serde_json::from_value(serde_json::json!({
            "input": "a.mp4", "segments": [{"start": 0.0, "end": 4.0}, {"start": 10.0, "end": 20.0}],
            "container": "webm", "codec": "vp9", "rate": {"mode": "size", "maxBytes": 4194304.0},
            "effort": "balanced", "twoPass": true, "speed": 1.0, "playback": "forward", "rotate": 0, "mirror": false,
            "fadeIn": 0.0, "fadeOut": 0.0, "deinterlace": false, "denoise": false, "tonemap": false, "autoScale": true
        }))
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(window(&spec, &info), (14.0, 16.0));
        // half speed: 2 s of output is 1 s of source
        spec.speed = 0.5;
        assert_eq!(window(&spec, &info), (14.5, 15.5));
        // a range shorter than the window is used whole
        spec.speed = 1.0;
        spec.segments = vec![Segment { start: 3.0, end: 4.2 }];
        let (s, e) = window(&spec, &info);
        assert!((s - 3.0).abs() < 1e-9 && (e - 4.2).abs() < 1e-9);
    }
}
