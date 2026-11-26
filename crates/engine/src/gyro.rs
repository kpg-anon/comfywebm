//! Stabilize with Gyroflow (Settings > Tools): Gyroflow is a program of its
//! own, so it cannot sit in the encode's filter graph. Before the encode, the
//! span from the first range's start to the last range's end is cut from the
//! source (near-lossless x264 with PCM sound, on frame boundaries), Gyroflow
//! measures the camera motion in it from the picture (optical flow; the file
//! needs no gyro data) and renders it steadied as ProRes 422 with the sound,
//! and the job is planned on that file with every time moved back by the
//! span's start. Gyroflow's CLI has no trim option, hence the cut.
//!
//! Measured on a shaken still picture (1080p, 5 s): the neighbour-frame
//! difference went from 9.3 to 2.4, 150 frames in and out, 7 s on an RTX 4070
//! laptop GPU. vid.stab 1.4 reached 0.6 on that clip, which suits vid.stab's
//! model (a flat picture moved and turned); Gyroflow models a camera turning
//! in 3D with rolling shutter, which a flat test picture does not show.

use crate::plan::{fmt_num, length_arg, resolve_segments, seek_arg, snap_to_frames};
use crate::run::{run_ffmpeg, Cancel, RunError};
use crate::spec::{EncodeSpec, MediaInfo, SubsSel};
use crate::tools::{command, output_within};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The programs Gyroflow stabilization needs besides ffmpeg.
#[derive(Debug, Clone)]
pub struct Gyro {
    pub gyroflow: PathBuf,
    pub ffprobe: PathBuf,
}

/// The job moved onto Gyroflow's file, and the temporary files to delete afterwards.
pub struct Prepared {
    pub spec: EncodeSpec,
    pub info: MediaInfo,
    pub files: Vec<PathBuf>,
}

/// Gyroflow's `-p` value: ProRes 422 (10-bit 4:2:2, the only pixel formats its
/// ProRes takes) with PCM sound when there is sound.
pub fn out_params(output: &Path, audio: bool) -> String {
    let mut m = serde_json::Map::new();
    m.insert("codec".into(), "ProRes".into());
    m.insert("codec_options".into(), "ProRes 422".into());
    m.insert("pixel_format".into(), "YUV422P10LE".into());
    m.insert("audio".into(), audio.into());
    if audio {
        m.insert("audio_codec".into(), "PCM (s16le)".into());
    }
    // forward slashes: Gyroflow reads the value as JSON, and a JSON string takes them as they are
    m.insert("output_path".into(), output.to_string_lossy().replace('\\', "/").into());
    serde_json::Value::Object(m).to_string()
}

/// Runs Gyroflow on `input`, writing `output`; `on_fraction` gets 0..1 (measuring is
/// the first third, rendering the rest).
pub fn run_gyroflow(gyroflow: &Path, input: &Path, output: &Path, audio: bool, cancel: &Cancel, mut on_fraction: impl FnMut(f64)) -> Result<(), RunError> {
    if cancel.is_canceled() {
        return Err(RunError::Canceled);
    }
    let mut cmd = command(gyroflow);
    cmd.arg(input)
        .args(["--ignore-file-motion", "--optical-correction", "100", "-f", "--stdout-progress", "-p"])
        .arg(out_params(output, audio))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| RunError::Failed(format!("Cannot run Gyroflow ({}): {e}", gyroflow.display())))?;
    let stderr = child.stderr.take().map(|e| std::thread::spawn(move || std::io::read_to_string(e).unwrap_or_default()));
    // Gyroflow colours its log: "[31m[ERROR] [0m[id] Error: ..."
    let plain = |l: &str| {
        let mut o = String::new();
        let mut it = l.chars().peekable();
        while let Some(c) = it.next() {
            if c == '' {
                for d in it.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                o.push(c);
            }
        }
        o
    };
    let error_of = |l: &str| -> Option<String> {
        let l = plain(l);
        let (_, rest) = l.split_once("[ERROR]")?;
        // drop the "[id]" in front of the message
        let rest = rest.trim();
        let rest = if rest.starts_with('[') { rest.split_once(']').map_or(rest, |(_, r)| r.trim()) } else { rest };
        (!rest.is_empty()).then(|| rest.trim_start_matches("Error: ").trim_start_matches("An error occurred: ").to_string())
    };
    // "[id] Synchronization progress: 27.1%" and "[id] Rendering progress: 75/150 frames (50.0%) ETA 1.2s"
    let mut errors: Vec<String> = Vec::new();
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if cancel.is_canceled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Canceled);
            }
            let pct = |l: &str| l.rsplit_once('%').and_then(|(a, _)| a.rsplit(|c: char| c == ' ' || c == '(').next()).and_then(|n| n.parse::<f64>().ok());
            if line.contains("Synchronization progress") {
                if let Some(p) = pct(&line) {
                    on_fraction(p / 100.0 / 3.0);
                }
            } else if line.contains("Rendering progress") {
                if let Some(p) = pct(&line) {
                    on_fraction(1.0 / 3.0 + p / 100.0 * 2.0 / 3.0);
                }
            } else if let Some(e) = error_of(&line) {
                errors.push(e);
            }
        }
    }
    let status = child.wait().map_err(|e| RunError::Failed(e.to_string()))?;
    let err_text = stderr.and_then(|h| h.join().ok()).unwrap_or_default();
    if !status.success() || !output.is_file() {
        errors.extend(err_text.lines().filter_map(error_of));
        // the first error says why ("Optical analysis failed: Not enough of the image could be tracked"); later ones only that rendering failed
        let last = errors.first().cloned().unwrap_or_else(|| "no message".into());
        return Err(RunError::Failed(format!("Gyroflow did not stabilize the clip (exit {}): {last}", status.code().unwrap_or(-1))));
    }
    Ok(())
}

/// Cuts the job's span, stabilizes it with Gyroflow and returns the job moved
/// onto the result. `work` is a file name prefix for the temporary files.
/// `on_fraction` gets 0..1 over the whole preparation.
pub fn prepare(ffmpeg: &Path, gyro: &Gyro, spec: &EncodeSpec, info: &MediaInfo, work: &Path, cancel: &Cancel, mut on_fraction: impl FnMut(f64)) -> Result<Prepared, RunError> {
    if spec.segment_sources.iter().any(|s| s.is_some()) || spec.layout.is_some() {
        return Err(RunError::Failed("Gyroflow stabilizes one file; ranges from other files (Splits, joined files) need vid.stab or deshake.".into()));
    }
    let v = &info.video;
    let mut segs = resolve_segments(&spec.segments, info.duration)?;
    snap_to_frames(&mut segs, v.fps, v.vfr);
    let s0 = segs.iter().map(|g| g.0).fold(f64::INFINITY, f64::min);
    let e0 = segs.iter().map(|g| g.1).fold(0.0, f64::max);
    let span = (e0 - s0).max(0.0);
    let name = |ext: &str| PathBuf::from(format!("{}{ext}", work.to_string_lossy()));
    let cut = name("-gyro-in.mkv");
    let steady = name("-gyro.mov");
    let subs_file = name("-gyro-subs.ass");
    let mut files = vec![cut.clone(), steady.clone()];

    // 1. the span on frame boundaries, as the encode would read it; x264 at CRF 8 is
    //    near-lossless and far smaller than FFV1 for 4K. Deinterlaced here when asked:
    //    Gyroflow should see whole frames. Rotation metadata is applied by the re-encode.
    let audio = spec.audio.filter(|a| info.audio.iter().any(|t| t.track == a.track));
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-progress", "pipe:1", "-nostats", "-y"].iter().map(|s| s.to_string()).collect();
    if s0 > 0.0 {
        a.extend(["-ss".into(), fmt_num(seek_arg(s0, v.fps, v.vfr))]);
    }
    a.extend(["-t".into(), fmt_num(length_arg(s0, e0, v.fps, v.vfr)), "-i".into(), spec.input.clone()]);
    a.extend(["-map".into(), format!("0:{}", v.stream_index)]);
    if let Some(au) = audio {
        a.extend(["-map".into(), format!("0:a:{}", au.track), "-c:a".into(), "pcm_s16le".into()]);
    }
    if spec.deinterlace {
        a.extend(["-vf".into(), "bwdif=mode=send_frame".into()]);
    }
    a.extend(["-c:v", "libx264", "-preset", "veryfast", "-crf", "8", "-sn", "-dn", "-map_metadata", "-1"].iter().map(|s| s.to_string()));
    a.push(cut.to_string_lossy().into_owned());
    let cleanup = |files: &[PathBuf]| files.iter().for_each(|f| drop(std::fs::remove_file(f)));
    if let Err(e) = run_ffmpeg(ffmpeg, &a, span, cancel, |p| on_fraction(p.fraction * 0.2)) {
        cleanup(&files);
        return Err(e);
    }

    // embedded subtitles do not survive Gyroflow: take the chosen track out as a file first
    let subs = subs_for_cut(ffmpeg, spec, &subs_file, &mut files);

    // 2. Gyroflow
    if let Err(e) = run_gyroflow(&gyro.gyroflow, &cut, &steady, audio.is_some(), cancel, |f| on_fraction(0.2 + f * 0.8)) {
        cleanup(&files);
        return Err(e);
    }
    let _ = std::fs::remove_file(&cut);
    files.retain(|f| f != &cut);
    let steady_s = steady.to_string_lossy().into_owned();
    let new_info = match crate::probe::probe(&gyro.ffprobe, &steady_s) {
        Ok(i) => i,
        Err(e) => {
            cleanup(&files);
            return Err(RunError::Failed(format!("Gyroflow's file could not be read: {e}")));
        }
    };

    // 3. the job on Gyroflow's file: every source time moves back by the span's start
    let mut s = shifted(spec, &segs, s0, subs, audio);
    s.input = steady_s;
    s.deinterlace = false;
    s.stabilize = false;
    Ok(Prepared { spec: s, info: new_info, files })
}

/// The chosen subtitles for a job moved onto a cut of its span (Gyroflow, RIFE): embedded
/// ones do not survive the cut, so the track is taken out to `file` first (added to `files`).
pub(crate) fn subs_for_cut(ffmpeg: &Path, spec: &EncodeSpec, file: &Path, files: &mut Vec<PathBuf>) -> Option<SubsSel> {
    let Some(SubsSel::Embedded { track }) = &spec.subs else { return spec.subs.clone() };
    let mut cmd = command(ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-i", &spec.input, "-map", &format!("0:s:{track}")]).arg(file);
    match output_within(cmd, std::time::Duration::from_secs(120), "Extracting the subtitles") {
        Ok(o) if o.status.success() && file.is_file() => {
            files.push(file.to_path_buf());
            Some(SubsSel::File { path: file.to_string_lossy().into_owned() })
        }
        _ => None,
    }
}

/// `spec` on a file cut from its span: every source time moves back by `s0` (the span's
/// start), the sound is the cut's only track. The caller sets the input.
pub(crate) fn shifted(spec: &EncodeSpec, segs: &[(f64, f64)], s0: f64, subs: Option<SubsSel>, audio: Option<crate::spec::AudioSel>) -> EncodeSpec {
    let mut s = spec.clone();
    s.segments = segs.iter().map(|(a, b)| crate::spec::Segment { start: (a - s0).max(0.0), end: (b - s0).max(0.0) }).collect();
    s.speed_ramp = spec.speed_ramp.iter().map(|p| crate::spec::SpeedPoint { t: p.t - s0, ..*p }).collect();
    s.zoom = spec.zoom.iter().map(|p| crate::spec::ZoomPoint { t: p.t - s0, ..*p }).collect();
    // rotation keyframes before the span keep their (negative) times: they set the angle at its start
    s.rotate_points = spec.rotate_points.iter().map(|p| crate::spec::RotatePoint { t: p.t - s0, ..*p }).collect();
    s.rotate_from = Some(spec.rotate_from.unwrap_or(spec.segments.first().map_or(0.0, |g| g.start)) - s0);
    // the subtitles keep the source's times; the filter shows them `delay` later
    s.subs = subs;
    s.subs_delay = spec.subs_delay - s0;
    s.audio = audio.map(|au| crate::spec::AudioSel { track: 0, ..au });
    s.segment_sources.clear();
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_parameters_are_json() {
        let p = out_params(Path::new(r"C:\a b\x.mov"), true);
        let v: serde_json::Value = serde_json::from_str(&p).unwrap();
        assert_eq!(v["output_path"], "C:/a b/x.mov");
        assert_eq!(v["pixel_format"], "YUV422P10LE");
        assert_eq!(v["audio_codec"], "PCM (s16le)");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&out_params(Path::new("/t/x.mov"), false)).unwrap()["audio"], false);
    }
}
