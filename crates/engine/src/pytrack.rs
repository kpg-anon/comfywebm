//! Follow > Track with a Python tracker (track.py) in the environment the app
//! installs (src-tauri install.rs): VitTrack (OpenCV's small neural tracker)
//! or SAM 2 (Meta's video segmentation model; it follows the person's
//! outline). ffmpeg decodes the range (long side 1024, BGR) straight into the
//! script's stdin; the script answers with a JSON line per analysed frame.
//! The result is the same `TrackPoint`s as the built-in tracker (track.rs).

use crate::run::{Cancel, RunError};
use crate::spec::{MediaInfo, Rect};
use crate::track::{smooth, TrackPoint};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The script, written beside the environment before each run (an app update brings a new one).
pub const SCRIPT: &str = include_str!("track.py");

/// Long side of the frames the tracker gets (SAM 2 works at 1024).
pub const SIDE: u32 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Vittrack,
    Sam2,
    /// OpenCV's YuNet face finder (run_face), in the VitTrack environment
    Face,
}

impl Method {
    pub fn arg(self) -> &'static str {
        match self {
            Method::Vittrack => "vittrack",
            Method::Sam2 => "sam2",
            Method::Face => "face",
        }
    }
}

/// An installed Python tracker: the environment's python, the script and the model file.
#[derive(Debug, Clone)]
pub struct PyTracker {
    pub method: Method,
    pub python: PathBuf,
    pub script: PathBuf,
    pub model: PathBuf,
}

fn even_size(w: u32, h: u32) -> (u32, u32) {
    let k = (SIDE as f64 / w.max(h).max(1) as f64).min(1.0);
    let even = |v: f64| (((v / 2.0).round() as u32) * 2).max(2);
    (even(w as f64 * k), even(h as f64 * k))
}

/// One line of the script's output.
#[derive(serde::Deserialize)]
struct Line {
    i: Option<usize>,
    x: Option<f64>,
    y: Option<f64>,
    w: Option<f64>,
    h: Option<f64>,
    s: Option<f64>,
    p: Option<f64>,
    device: Option<String>,
    /// face: the two eyes
    e: Option<Vec<(f64, f64)>>,
}

/// Follows `bx` (source display pixels, on the frame at `start`) to `end`.
/// `progress` gets 0..1; `device` gets "cuda", "mps" or "cpu" once the script knows.
/// What a run of the script gives: each frame's time (source seconds), the
/// frames' scale against the source, and its frame rate.
struct Ran {
    times: Vec<f64>,
    k: f64,
    fps: f64,
}

/// Runs the script over `start`..`end` with ffmpeg feeding it; `on` gets each
/// line and says whether it held a frame's result (none at all is an error).
#[allow(clippy::too_many_arguments)]
fn run_script(
    ffmpeg: &Path,
    info: &MediaInfo,
    start: f64,
    end: f64,
    bx: Rect,
    t: &PyTracker,
    cancel: &Cancel,
    mut progress: impl FnMut(f64),
    mut device: impl FnMut(&str),
    mut on: impl FnMut(&Line) -> bool,
) -> Result<Ran, RunError> {
    if info.audio_only || end <= start {
        return Err(RunError::Failed("Nothing to follow here.".into()));
    }
    std::fs::write(&t.script, SCRIPT).map_err(|e| RunError::Failed(format!("Cannot write the tracker script: {e}")))?;
    let (sw, sh) = (info.video.width.max(2), info.video.height.max(2));
    let (w, h) = even_size(sw, sh);
    let k = w as f64 / sw as f64;
    let fps = if info.video.fps > 0.0 { info.video.fps } else { 30.0 };
    let frames = ((end - start) * fps).round().max(1.0) as usize;

    let mut ff = crate::tools::command(ffmpeg);
    ff.args(["-hide_banner", "-nostdin", "-nostats", "-ss", &crate::plan::fmt_num(start), "-t", &crate::plan::fmt_num(end - start), "-i"])
        .arg(&info.path)
        .args(["-map", &format!("0:{}", info.video.stream_index), "-an", "-sn", "-dn", "-fps_mode", "passthrough"])
        .args(["-vf", &format!("scale={w}:{h}:flags=area,format=bgr24,showinfo"), "-f", "rawvideo", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut ffc = ff.spawn().map_err(|e| RunError::Failed(format!("Cannot run ffmpeg: {e}")))?;
    let ff_out = ffc.stdout.take().expect("piped stdout");
    let ff_err = ffc.stderr.take().expect("piped stderr");
    let times_thread = std::thread::spawn(move || {
        let mut times = Vec::new();
        crate::run::each_line(ff_err, |line| {
            if line.contains("Parsed_showinfo") {
                if let Some(t) = line.split("pts_time:").nth(1).and_then(|r| r.split_whitespace().next()).and_then(|v| v.parse::<f64>().ok()) {
                    times.push(t);
                }
            }
        });
        times
    });

    let r = |v: u32| v as f64 * k;
    let mut py = crate::tools::command(&t.python);
    py.arg("-I")
        .arg(&t.script)
        .args(["--method", t.method.arg(), "--model"])
        .arg(&t.model)
        .args(["--w", &w.to_string(), "--h", &h.to_string()])
        .args(["--box", &format!("{:.2},{:.2},{:.2},{:.2}", r(bx.x), r(bx.y), r(bx.w).max(4.0), r(bx.h).max(4.0))])
        .args(["--fps", &crate::plan::fmt_num(fps), "--frames", &frames.to_string()])
        // the script's own output unbuffered, and no user site-packages
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONNOUSERSITE", "1")
        .stdin(Stdio::from(ff_out))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut pyc = match py.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = ffc.kill();
            return Err(RunError::Failed(format!("Cannot run the tracker's Python: {e}")));
        }
    };
    let out = pyc.stdout.take().expect("piped stdout");
    let err_tail = crate::run::tail_thread(pyc.stderr.take().expect("piped stderr"), 6);
    cancel.attach(pyc);

    let mut got = 0usize;
    crate::run::each_line(out, |line| {
        let Ok(l) = serde_json::from_str::<Line>(line) else { return };
        if let Some(d) = &l.device {
            device(d);
        }
        if let Some(p) = l.p {
            progress(p.clamp(0.0, 1.0) * 0.95);
        }
        if on(&l) {
            got += 1;
        }
    });
    let status = cancel.detach().map(|mut c| c.wait());
    // the tracker stopped (or was stopped): ffmpeg may still be writing into the closed pipe
    let _ = ffc.kill();
    let _ = ffc.wait();
    let times = times_thread.join().unwrap_or_default();
    let tail = err_tail.join().unwrap_or_default();
    if cancel.is_canceled() {
        return Err(RunError::Canceled);
    }
    if !matches!(status, Some(Ok(s)) if s.success()) || got == 0 {
        // uv's launcher could not find the Python the environment was made with
        if tail.iter().any(|l| l.contains("trampoline")) {
            return Err(RunError::Failed(format!(
                "The {} Python environment is damaged (its Python is missing). Settings > Tools > Tracking > Update installs it again.",
                t.method.arg()
            )));
        }
        let why = tail.iter().rev().find(|l| !l.trim().is_empty() && !l.contains("Warning") && !l.contains("warn(")).cloned().unwrap_or_else(|| "no message".into());
        if t.method == Method::Face && got == 0 && matches!(status, Some(Ok(s)) if s.success()) {
            return Err(RunError::Failed("No face was found near where you clicked.".into()));
        }
        return Err(RunError::Failed(format!("The {} tracker stopped: {why}", t.method.arg())));
    }
    Ok(Ran { times, k, fps })
}

/// Follows `bx` (source display pixels, on the frame at `start`) to `end`.
/// `progress` gets 0..1; `device` gets "cuda", "mps" or "cpu" once the script knows.
#[allow(clippy::too_many_arguments)]
pub fn run(
    ffmpeg: &Path,
    info: &MediaInfo,
    start: f64,
    end: f64,
    bx: Rect,
    t: &PyTracker,
    cancel: &Cancel,
    progress: impl FnMut(f64),
    device: impl FnMut(&str),
) -> Result<Vec<TrackPoint>, RunError> {
    let mut found: Vec<(usize, f64, f64, f64, f64, f64)> = Vec::new();
    let Ran { times, k, fps } = run_script(ffmpeg, info, start, end, bx, t, cancel, progress, device, |l| {
        if let (Some(i), Some(x), Some(y), Some(w), Some(h)) = (l.i, l.x, l.y, l.w, l.h) {
            found.push((i, x, y, w, h, l.s.unwrap_or(1.0)));
            return true;
        }
        false
    })?;
    found.sort_by_key(|f| f.0);
    found.dedup_by_key(|f| f.0);
    // take out frame-to-frame noise over about a fifth of a second of analysed frames
    let rate = if found.len() > 1 { (found.len() - 1) as f64 / ((found[found.len() - 1].0 - found[0].0).max(1) as f64 / fps) } else { fps };
    let half = ((rate * 0.1).round() as usize).max(1);
    let xs = smooth(&found.iter().map(|f| f.1 as f32).collect::<Vec<_>>(), half);
    let ys = smooth(&found.iter().map(|f| f.2 as f32).collect::<Vec<_>>(), half);
    Ok(found
        .iter()
        .enumerate()
        .map(|(n, f)| TrackPoint {
            t: start + times.get(f.0).copied().unwrap_or(f.0 as f64 / fps),
            x: xs[n] as f64 / k,
            y: ys[n] as f64 / k,
            w: f.3 / k,
            h: f.4 / k,
            score: f.5,
        })
        .collect())
}

/// Follows the face nearest `near` (source display pixels, on the frame at `start`)
/// to `end` with the face finder: its two eyes in each frame (left one in the picture
/// first, source display pixels) and how sure the finder was (0 = not found, held).
#[allow(clippy::too_many_arguments)]
pub fn run_face(
    ffmpeg: &Path,
    info: &MediaInfo,
    start: f64,
    end: f64,
    near: (f64, f64),
    t: &PyTracker,
    cancel: &Cancel,
    progress: impl FnMut(f64),
) -> Result<Vec<crate::track::PointsFrame>, RunError> {
    let mut found: Vec<(usize, Vec<(f64, f64)>, f64)> = Vec::new();
    let bx = Rect { x: near.0.max(0.0) as u32, y: near.1.max(0.0) as u32, w: 1, h: 1 };
    let Ran { times, k, fps } = run_script(ffmpeg, info, start, end, bx, t, cancel, progress, |_| {}, |l| {
        if let (Some(i), Some(e)) = (l.i, l.e.as_ref()) {
            if e.len() == 2 {
                found.push((i, e.clone(), l.s.unwrap_or(1.0)));
                return true;
            }
        }
        false
    })?;
    found.sort_by_key(|f| f.0);
    found.dedup_by_key(|f| f.0);
    Ok(found
        .into_iter()
        .map(|(i, e, s)| crate::track::PointsFrame {
            t: start + times.get(i).copied().unwrap_or(i as f64 / fps),
            p: e.iter().map(|&(x, y)| (x / k, y / k, s)).collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_size_keeps_the_shape() {
        assert_eq!(even_size(3840, 2160), (1024, 576));
        assert_eq!(even_size(2160, 3840), (576, 1024));
        assert_eq!(even_size(640, 360), (640, 360));
    }
}
