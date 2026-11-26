//! Interpolation with RIFE (Settings > Tools > Interpolation): slow motion gets
//! new in-between frames from RIFE, a neural network for video frame
//! interpolation (Huang et al., "Real-Time Intermediate Flow Estimation",
//! ECCV 2022), run by rife-ncnn-vulkan (TNTwise's fork, MIT) on the graphics
//! card through Vulkan. It is a program of its own that reads and writes
//! folders of pictures, so it cannot sit in the encode's filter graph. Before
//! the encode, the span from the first range's start to the last range's end
//! is decoded (as Gyroflow's cut is, gyro.rs), RIFE fills it to the rate the
//! slowest part needs, and the job is planned on that denser file with every
//! time moved back by the span's start; the encode's own `fps` then takes the
//! frames it needs, as it would from a fast camera.
//!
//! RIFE's `-n` spreads its output evenly: output frame i of a folder of c
//! pictures is at input time i * c / n (in frames). The span is done in chunks,
//! so the temporary pictures stay at about a gigabyte. With p input frames per r
//! output frames (the ratio of the speed and rates), a chunk is p * (m + 1)
//! pictures turned into r * (m + 1), of which the first r * m are kept; the last p
//! pictures start the next chunk, so every output frame lands on the same grid.
//! An output frame at a whole input frame is that frame unchanged (measured:
//! PSNR inf).

use crate::gyro::Prepared;
use crate::plan::{color_tags, fmt_num, length_arg, resolve_segments, seek_arg, snap_to_frames, TONEMAP};
use crate::run::{Cancel, RunError};
use crate::spec::{EncodeSpec, InterpMethod, MediaInfo};
use crate::tools::command;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::Duration;

/// The model the installer keeps (src-tauri install.rs): RIFE 4.26, the newest
/// regular model in the release (4.25 lite is faster, 4.26 large slower).
pub const MODEL: &str = "rife-v4.26";

/// The RIFE program, its model folder, and ffprobe for reading its result.
#[derive(Debug, Clone)]
pub struct Rife {
    pub exe: PathBuf,
    pub model: PathBuf,
    pub ffprobe: PathBuf,
}

/// What a job asks of RIFE: `p` input frames become `r` output frames (p < r),
/// filling the slowest part to `target` frames a second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Need {
    pub src_fps: f64,
    pub target: f64,
    pub p: u32,
    pub r: u32,
}

impl Need {
    /// The rate of RIFE's file.
    pub fn rate(&self) -> f64 {
        self.src_fps * self.r as f64 / self.p as f64
    }
}

/// `q` as p / r with r at most 64: the closest fraction, the smallest r among equals.
pub fn ratio(q: f64) -> (u32, u32) {
    let mut best = (1u32, 1u32);
    let mut err = f64::INFINITY;
    for r in 1..=64u32 {
        let p = (q * r as f64).round().max(1.0) as u32;
        let e = (p as f64 / r as f64 - q).abs();
        if e < err - 1e-9 {
            err = e;
            best = (p, r);
        }
    }
    best
}

/// What the job needs from RIFE, or None when it does not use RIFE: Interpolation off
/// or set to the built-in method, no slow part, a picture or GIF output, or ranges
/// from several files (those use minterpolate, plan.rs).
pub fn need(spec: &EncodeSpec, info: &MediaInfo) -> Option<Need> {
    if !spec.smooth_slowmo || spec.interp_method != InterpMethod::Rife || info.audio_only || spec.codec.is_image() {
        return None;
    }
    if spec.layout.is_some() || spec.segment_sources.iter().any(|s| s.is_some()) {
        return None;
    }
    let src = info.video.fps;
    if !src.is_finite() || src <= 0.0 {
        return None;
    }
    let segs = resolve_segments(&spec.segments, info.duration).ok()?;
    let ramp = if spec.speed_ramp.is_empty() { None } else { crate::ramp::Ramp::new(&spec.speed_ramp, &segs) };
    let speed = if ramp.is_some() { 1.0 } else { spec.speed };
    let slowest = ramp.as_ref().map_or(speed, |r| r.min_speed());
    if !slowest.is_finite() || slowest <= 0.0 || slowest >= 1.0 - 1e-9 {
        return None;
    }
    // the rate the slow parts are filled to, as minterpolate's (plan.rs): the source's,
    // or a frame rate cap below the rate the speed gives
    let cap = spec.fps.filter(|f| f.is_finite() && *f > 0.0).filter(|f| *f < src * speed - 0.01);
    let target = cap.map_or(src, |c| c.min(src));
    let q = src * slowest / target;
    if q >= 1.0 - 1e-6 {
        return None;
    }
    let (p, r) = ratio(q);
    (p < r).then_some(Need { src_fps: src, target, p, r })
}

/// ffmpeg's `scale` matrix name for a colorspace tag.
fn matrix(space: &str) -> &'static str {
    match space {
        "smpte170m" | "bt470bg" => "bt601",
        "bt2020nc" | "bt2020c" => "bt2020",
        "smpte240m" => "smpte240m",
        "fcc" => "fcc",
        _ => "bt709",
    }
}

/// One BMP from a stream of them (ffmpeg's image2pipe), or None at its end.
fn read_bmp(r: &mut impl Read) -> std::io::Result<Option<Vec<u8>>> {
    let mut head = [0u8; 6];
    let mut got = 0;
    while got < head.len() {
        let n = r.read(&mut head[got..])?;
        if n == 0 {
            return if got == 0 { Ok(None) } else { Err(std::io::ErrorKind::UnexpectedEof.into()) };
        }
        got += n;
    }
    if &head[..2] != b"BM" {
        return Err(std::io::Error::other("not a BMP picture"));
    }
    let size = u32::from_le_bytes([head[2], head[3], head[4], head[5]]) as usize;
    if size < 54 {
        return Err(std::io::Error::other("a BMP picture too short"));
    }
    let mut buf = vec![0u8; size];
    buf[..6].copy_from_slice(&head);
    r.read_exact(&mut buf[6..])?;
    Ok(Some(buf))
}

/// A BMP's width and height.
fn bmp_size(b: &[u8]) -> (u32, u32) {
    let w = i32::from_le_bytes([b[18], b[19], b[20], b[21]]).unsigned_abs();
    let h = i32::from_le_bytes([b[22], b[23], b[24], b[25]]).unsigned_abs();
    (w, h)
}

/// Kills the processes it holds when dropped (an error or a cancel on the way).
struct Procs(Vec<Child>);
impl Drop for Procs {
    fn drop(&mut self) {
        for c in &mut self.0 {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// The last non-empty line of a program's error output.
fn last_line(s: &str) -> String {
    s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message").trim().to_string()
}

/// Runs RIFE on the pictures in `input`, writing `n` pictures to `output`; `on_made` gets
/// how many it has written so far (counted about 4 times a second).
fn run_rife(rife: &Rife, input: &Path, output: &Path, n: u32, uhd: bool, cancel: &Cancel, mut on_made: impl FnMut(usize)) -> Result<(), RunError> {
    let mut cmd = command(&rife.exe);
    cmd.arg("-i").arg(input).arg("-o").arg(output).arg("-n").arg(n.to_string()).arg("-m").arg(&rife.model).args(["-f", "%08d.png"]);
    if uhd {
        cmd.arg("-u");
    }
    // the program finds its own files (vcomp140.dll beside it) from its folder
    if let Some(dir) = rife.exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| RunError::Failed(format!("Cannot run RIFE ({}): {e}", rife.exe.display())))?;
    let err = child.stderr.take().map(|e| std::thread::spawn(move || std::io::read_to_string(e).unwrap_or_default()));
    let mut last = std::time::Instant::now();
    let status = loop {
        if cancel.is_canceled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::Canceled);
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(RunError::Failed(e.to_string())),
        }
        if last.elapsed() > Duration::from_millis(250) {
            last = std::time::Instant::now();
            on_made(std::fs::read_dir(output).map(|d| d.count()).unwrap_or(0));
        }
    };
    let text = err.and_then(|h| h.join().ok()).unwrap_or_default();
    let made = std::fs::read_dir(output).map(|d| d.count()).unwrap_or(0);
    if !status.success() || made < n as usize {
        // it lists the graphics cards first; the error is at the end
        return Err(RunError::Failed(format!("RIFE did not make the in-between frames (exit {}, {made} of {n} pictures): {}", status.code().unwrap_or(-1), last_line(&text))));
    }
    Ok(())
}

/// Picture file `i` (from 1) in `dir`.
fn pic(dir: &Path, i: usize, ext: &str) -> PathBuf {
    dir.join(format!("{i:08}.{ext}"))
}

/// Decodes the job's span, fills it with RIFE and returns the job moved onto the
/// result. `work` is a file name prefix for the temporary files; `on_fraction`
/// gets 0..1.
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    ffmpeg: &Path,
    rife: &Rife,
    spec: &EncodeSpec,
    info: &MediaInfo,
    need: &Need,
    zscale: bool,
    work: &Path,
    cancel: &Cancel,
    mut on_fraction: impl FnMut(f64),
) -> Result<Prepared, RunError> {
    let v = &info.video;
    let tonemap = v.hdr && spec.tonemap && zscale;
    if v.hdr && !tonemap {
        return Err(RunError::Failed(
            "RIFE works on SDR pictures. Turn on Tone map (Advanced) for this HDR clip, or choose the built-in interpolation in Settings > Tools.".into(),
        ));
    }
    let mut segs = resolve_segments(&spec.segments, info.duration)?;
    snap_to_frames(&mut segs, v.fps, v.vfr);
    let s0 = segs.iter().map(|g| g.0).fold(f64::INFINITY, f64::min);
    let e0 = segs.iter().map(|g| g.1).fold(0.0, f64::max);

    let base = work.to_string_lossy().into_owned();
    let dir = PathBuf::from(format!("{base}-rife"));
    let dense = PathBuf::from(format!("{base}-rife.mkv"));
    let subs_file = PathBuf::from(format!("{base}-rife-subs.ass"));
    let _ = std::fs::remove_dir_all(&dir);
    let in_dir = dir.join("in");
    std::fs::create_dir_all(&in_dir).map_err(|e| RunError::Failed(format!("Cannot create {}: {e}", in_dir.display())))?;
    let res = fill(ffmpeg, rife, spec, info, need, tonemap, (s0, e0), &dir, &dense, cancel, &mut on_fraction);
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = res {
        let _ = std::fs::remove_file(&dense);
        return Err(e);
    }
    let mut files = vec![dense.clone()];
    let dense_s = dense.to_string_lossy().into_owned();
    let new_info = match crate::probe::probe(&rife.ffprobe, &dense_s) {
        Ok(i) => i,
        Err(e) => {
            let _ = std::fs::remove_file(&dense);
            return Err(RunError::Failed(format!("RIFE's file could not be read: {e}")));
        }
    };
    let subs = crate::gyro::subs_for_cut(ffmpeg, spec, &subs_file, &mut files);
    let audio = spec.audio.filter(|a| info.audio.iter().any(|t| t.track == a.track));

    // the job on RIFE's file: every source time moves back by the span's start, as for Gyroflow
    let mut s = crate::gyro::shifted(spec, &segs, s0, subs, audio);
    s.input = dense_s;
    s.deinterlace = false;
    s.tonemap = false;
    // the frames are made: the encode only takes them, at the rate interpolation fills to
    s.smooth_slowmo = false;
    s.fps = Some(need.target);
    Ok(Prepared { spec: s, info: new_info, files })
}

/// The decoding, RIFE and the writing of the denser file.
#[allow(clippy::too_many_arguments)]
fn fill(
    ffmpeg: &Path,
    rife: &Rife,
    spec: &EncodeSpec,
    info: &MediaInfo,
    need: &Need,
    tonemap: bool,
    (s0, e0): (f64, f64),
    dir: &Path,
    dense: &Path,
    cancel: &Cancel,
    on_fraction: &mut impl FnMut(f64),
) -> Result<(), RunError> {
    let v = &info.video;
    let (p, r) = (need.p as usize, need.r as usize);
    let (space, prim, trc) = color_tags(v, tonemap);
    let m_name = matrix(&space);

    // 1. the decoder: the span as BMP pictures (no compression to spend time on), with the
    //    steps that come before the picture is changed: deinterlace, tone map, square pixels
    let mut vf: Vec<String> = Vec::new();
    if spec.deinterlace {
        vf.push("bwdif=mode=send_frame".into());
    }
    if tonemap {
        vf.push(TONEMAP.into());
    }
    if v.anamorphic {
        vf.push("scale=trunc(iw*sar/2)*2:ih".into());
        vf.push("setsar=1".into());
    }
    if v.vfr {
        vf.push(format!("fps={}", fmt_num(need.src_fps)));
    }
    vf.push(format!("scale=in_color_matrix={m_name},format=bgr24"));
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error"].iter().map(|s| s.to_string()).collect();
    if s0 > 0.0 {
        a.extend(["-ss".into(), fmt_num(seek_arg(s0, v.fps, v.vfr))]);
    }
    a.extend(["-t".into(), fmt_num(length_arg(s0, e0, v.fps, v.vfr)), "-i".into(), spec.input.clone()]);
    a.extend(["-map".into(), format!("0:{}", v.stream_index), "-vf".into(), vf.join(","), "-f".into(), "image2pipe".into(), "-c:v".into(), "bmp".into(), "-".into()]);
    let mut dec_cmd = command(ffmpeg);
    dec_cmd.args(&a).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut dec = dec_cmd.spawn().map_err(|e| RunError::Failed(format!("Cannot run ffmpeg: {e}")))?;
    let dec_err = dec.stderr.take().map(|e| std::thread::spawn(move || std::io::read_to_string(e).unwrap_or_default()));
    let mut frames = BufReader::with_capacity(1 << 20, dec.stdout.take().ok_or_else(|| RunError::Failed("no ffmpeg output".into()))?);
    let mut procs = Procs(vec![dec]);

    let read = |frames: &mut BufReader<std::process::ChildStdout>| read_bmp(frames).map_err(|e| RunError::Failed(format!("Reading the decoded frames failed: {e}")));
    let Some(first) = read(&mut frames)? else {
        let e = dec_err.and_then(|h| h.join().ok()).unwrap_or_default();
        return Err(RunError::Failed(format!("ffmpeg gave no frames for RIFE: {}", last_line(&e))));
    };
    let (w, h) = bmp_size(&first);

    // 2. the writer: the pictures RIFE makes, in order, into a near-lossless file at the
    //    denser rate, with the span's sound (as Gyroflow's cut)
    let audio = spec.audio.filter(|au| info.audio.iter().any(|t| t.track == au.track));
    let pix = if w % 2 == 0 && h % 2 == 0 { "yuv420p" } else { "yuv444p" };
    let mut b: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-f", "image2pipe", "-framerate"].iter().map(|s| s.to_string()).collect();
    b.extend([fmt_num(need.rate()), "-c:v".into(), "png".into(), "-i".into(), "-".into()]);
    if let Some(au) = audio {
        if s0 > 0.0 {
            b.extend(["-ss".into(), fmt_num(seek_arg(s0, v.fps, v.vfr))]);
        }
        b.extend(["-t".into(), fmt_num(length_arg(s0, e0, v.fps, v.vfr)), "-i".into(), spec.input.clone()]);
        b.extend(["-map".into(), "0:v".into(), "-map".into(), format!("1:a:{}", au.track), "-c:a".into(), "pcm_s16le".into()]);
    } else {
        b.extend(["-map".into(), "0:v".into()]);
    }
    b.extend(["-vf".into(), format!("scale=out_color_matrix={m_name}:out_range=tv,format={pix}")]);
    b.extend(["-c:v", "libx264", "-preset", "veryfast", "-crf", "8"].iter().map(|s| s.to_string()));
    b.extend(["-colorspace".into(), space.clone(), "-color_primaries".into(), prim, "-color_trc".into(), trc, "-color_range".into(), "tv".into()]);
    b.extend(["-sn".into(), "-dn".into(), "-map_metadata".into(), "-1".into(), dense.to_string_lossy().into_owned()]);
    let mut enc_cmd = command(ffmpeg);
    enc_cmd.args(&b).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut enc = enc_cmd.spawn().map_err(|e| RunError::Failed(format!("Cannot run ffmpeg: {e}")))?;
    let enc_err = enc.stderr.take().map(|e| std::thread::spawn(move || std::io::read_to_string(e).unwrap_or_default()));
    let mut stdin = enc.stdin.take().ok_or_else(|| RunError::Failed("no ffmpeg input".into()))?;
    // each chunk's pictures go to the writer while RIFE does the next chunk
    let (tx, rx) = std::sync::mpsc::sync_channel::<(PathBuf, usize)>(1);
    let writer = std::thread::spawn(move || -> Result<(), String> {
        for (out, keep) in rx {
            for i in 1..=keep {
                let bytes = std::fs::read(pic(&out, i, "png")).map_err(|e| format!("RIFE's picture {i} could not be read: {e}"))?;
                stdin.write_all(&bytes).map_err(|e| format!("Writing RIFE's frames failed: {e}"))?;
            }
            let _ = std::fs::remove_dir_all(&out);
        }
        Ok(())
    });
    procs.0.push(enc);

    // 3. the chunks: about 600 MB of input pictures each (24 to 240 of them)
    let frame_bytes = (w as usize * h as usize * 3).max(1);
    let per = (600_000_000 / frame_bytes).clamp(24, 240);
    let m = (per / p).max(1);
    let c = p * (m + 1);
    let uhd = (w as u64) * (h as u64) > 2560 * 1440;
    let in_dir = dir.join("in");
    // frames expected, for the progress; the real count is known at the end
    let expect_in = ((e0 - s0) * need.src_fps).round().max(1.0);
    let expect_out = (expect_in * r as f64 / p as f64).ceil();
    let mut have = 0usize; // pictures in in_dir, 1..=have
    let mut total_in = 0usize;
    let mut last = first.clone();
    let mut pending = Some(first);
    let mut eof = false;
    let mut done_out = 0usize;
    let mut chunk = 0usize;
    let fail_writer = |writer: std::thread::JoinHandle<Result<(), String>>| match writer.join() {
        Ok(Err(e)) => RunError::Failed(e),
        _ => RunError::Failed("Writing RIFE's frames stopped.".into()),
    };
    loop {
        if cancel.is_canceled() {
            return Err(RunError::Canceled);
        }
        while have < c && !eof {
            let next = match pending.take() {
                Some(f) => Some(f),
                None => read(&mut frames)?,
            };
            match next {
                Some(f) => {
                    have += 1;
                    total_in += 1;
                    std::fs::write(pic(&in_dir, have, "bmp"), &f).map_err(|e| RunError::Failed(format!("Cannot write a frame for RIFE: {e}")))?;
                    last = f;
                }
                None => eof = true,
            }
        }
        // past the end: the last frame again (RIFE then makes that frame)
        while have < c {
            have += 1;
            std::fs::write(pic(&in_dir, have, "bmp"), &last).map_err(|e| RunError::Failed(format!("Cannot write a frame for RIFE: {e}")))?;
        }
        let total_out = if eof { (total_in * r).div_ceil(p) } else { usize::MAX };
        let keep = (r * m).min(total_out.saturating_sub(done_out));
        if keep == 0 {
            break;
        }
        let out = dir.join(format!("out{chunk}"));
        std::fs::create_dir_all(&out).map_err(|e| RunError::Failed(format!("Cannot create {}: {e}", out.display())))?;
        let n = r * (m + 1);
        run_rife(rife, &in_dir, &out, n as u32, uhd, cancel, |made| {
            // the pictures this chunk keeps, of those written so far
            let part = (made * keep / n).min(keep);
            on_fraction(((done_out + part) as f64 / expect_out.max((done_out + keep) as f64 + 1.0)).min(0.99));
        })?;
        if tx.send((out, keep)).is_err() {
            return Err(fail_writer(writer));
        }
        done_out += keep;
        chunk += 1;
        on_fraction((done_out as f64 / expect_out.max(done_out as f64 + 1.0)).min(0.99));
        if eof && done_out >= total_out {
            break;
        }
        // the last p pictures start the next chunk
        for i in 1..=p {
            let from = pic(&in_dir, p * m + i, "bmp");
            let to = pic(&in_dir, i, "bmp");
            std::fs::rename(&from, &to).map_err(|e| RunError::Failed(format!("Cannot move a frame for RIFE: {e}")))?;
        }
        for i in p + 1..=have.min(p * m) {
            let _ = std::fs::remove_file(pic(&in_dir, i, "bmp"));
        }
        have = p;
    }
    drop(tx);
    let wrote = writer.join().unwrap_or_else(|_| Err("Writing RIFE's frames stopped.".into()));
    // the writer's end closed ffmpeg's input; wait for both programs
    let mut ok = true;
    for ch in &mut procs.0 {
        ok &= ch.wait().map(|s| s.success()).unwrap_or(false);
    }
    procs.0.clear();
    let enc_text = enc_err.and_then(|h| h.join().ok()).unwrap_or_default();
    let _ = dec_err.and_then(|h| h.join().ok());
    wrote.map_err(RunError::Failed)?;
    if !ok || !dense.is_file() {
        return Err(RunError::Failed(format!("Writing RIFE's file failed: {}", last_line(&enc_text))));
    }
    on_fraction(1.0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios() {
        assert_eq!(ratio(0.5), (1, 2));
        assert_eq!(ratio(0.25), (1, 4));
        assert_eq!(ratio(0.4), (2, 5));
        assert_eq!(ratio(0.75), (3, 4));
        // 30 fps at 0.5x filled to 24 fps
        assert_eq!(ratio(30.0 * 0.5 / 24.0), (5, 8));
        let (p, r) = ratio(0.437);
        assert!((p as f64 / r as f64 - 0.437).abs() < 0.005, "{p}/{r}");
    }

    #[test]
    fn bmp_stream() {
        // two BMPs back to back: 'BM', the size, then the rest
        let mut one = vec![0u8; 60];
        one[..2].copy_from_slice(b"BM");
        one[2..6].copy_from_slice(&60u32.to_le_bytes());
        one[18..22].copy_from_slice(&4i32.to_le_bytes());
        one[22..26].copy_from_slice(&(-2i32).to_le_bytes());
        let stream = [one.clone(), one.clone()].concat();
        let mut r = &stream[..];
        assert_eq!(read_bmp(&mut r).unwrap().unwrap(), one);
        assert_eq!(bmp_size(&one), (4, 2));
        assert!(read_bmp(&mut r).unwrap().is_some());
        assert!(read_bmp(&mut r).unwrap().is_none());
    }
}
