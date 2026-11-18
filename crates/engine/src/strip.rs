//! Timeline pictures: a filmstrip of thumbnails and an audio waveform, each one
//! image made by a single ffmpeg run and cached by source path + size + mtime.

use crate::proxy::source_key;
use crate::run::{run_ffmpeg, Cancel, RunError};
use crate::spec::MediaInfo;
use std::path::{Path, PathBuf};

/// Thumbnails in the filmstrip.
pub const THUMBS: u32 = 60;
/// Height of one thumbnail in pixels.
const THUMB_H: u32 = 54;
const WAVE_W: u32 = 2400;
const WAVE_H: u32 = 64;

fn publish(part: &Path, out: &Path, res: Result<(), RunError>) -> Result<PathBuf, RunError> {
    if let Err(e) = res {
        let _ = std::fs::remove_file(part);
        return Err(e);
    }
    if !part.is_file() {
        return Err(RunError::Failed("ffmpeg wrote no picture.".into()));
    }
    std::fs::rename(part, out).map_err(|e| {
        let _ = std::fs::remove_file(part);
        RunError::Failed(format!("Cannot rename {}: {e}", part.display()))
    })?;
    Ok(out.to_path_buf())
}

/// A JPEG of `THUMBS` frames side by side, evenly spaced over the file. Only
/// key frames are decoded, so a long file takes seconds, not minutes; a file
/// with few key frames shows the nearest one.
pub fn thumbs(ffmpeg: &Path, info: &MediaInfo, cache_dir: &Path, cancel: &Cancel) -> Result<PathBuf, RunError> {
    if info.audio_only {
        return Err(RunError::Failed("The file has no video stream.".into()));
    }
    if info.duration <= 0.0 {
        return Err(RunError::Failed("The file has no duration.".into()));
    }
    let dir = cache_dir.join("strips");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let out = dir.join(format!("thumbs2-{}.jpg", source_key(&info.path)?));
    if out.is_file() {
        return Ok(out);
    }
    let part = out.with_extension(format!("{}.part.jpg", std::process::id()));
    let v = &info.video;
    let w = if v.height > 0 { ((THUMB_H as f64 * v.width as f64 / v.height as f64 / 2.0).round() as u32 * 2).clamp(16, 256) } else { 96 };
    let rate = THUMBS as f64 / info.duration;
    // the last key frame is often seconds before the end: repeat it so the
    // last tiles are filled (the stream is endless; -frames:v 1 stops it)
    let vf = format!("tpad=stop=-1:stop_mode=clone,fps={rate:.6},scale={w}:{THUMB_H},setsar=1,tile={THUMBS}x1");
    let args: Vec<String> = [
        "-hide_banner", "-nostdin", "-loglevel", "error", "-progress", "pipe:1", "-nostats", "-y",
        "-skip_frame", "nokey", "-i", &info.path,
        "-map", &format!("0:{}", v.stream_index), "-an", "-sn", "-dn",
        "-vf", &vf, "-frames:v", "1", "-q:v", "5", "-f", "image2",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain(std::iter::once(part.to_string_lossy().into_owned()))
    .collect();
    let res = run_ffmpeg(ffmpeg, &args, 0.0, cancel, |_| {});
    publish(&part, &out, res)
}

/// One small JPEG (160 px wide) from 10% into the file, for lists. Cached.
pub fn poster(ffmpeg: &Path, info: &MediaInfo, cache_dir: &Path) -> Result<PathBuf, String> {
    let dir = cache_dir.join("strips");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let out = dir.join(format!("poster-{}.jpg", source_key(&info.path)?));
    if out.is_file() {
        return Ok(out);
    }
    let part = out.with_extension(format!("{}.part.jpg", std::process::id()));
    // a sound file: its embedded cover, which is one frame at the start
    let at = if info.audio_only { "0".into() } else { crate::plan::fmt_num((info.duration * 0.1).max(0.0)) };
    if info.audio_only && info.video.width == 0 {
        return Err("The file has no picture.".into());
    }
    let mut cmd = crate::tools::command(ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-ss", &at, "-i", &info.path]);
    cmd.args(["-map", &format!("0:{}", info.video.stream_index), "-frames:v", "1", "-vf", "scale=160:-2", "-q:v", "5", "-f", "image2"]);
    cmd.arg(&part);
    let res = crate::tools::output_within(cmd, std::time::Duration::from_secs(30), "ffmpeg (poster)")?;
    if !res.status.success() || !part.is_file() {
        let _ = std::fs::remove_file(&part);
        return Err("ffmpeg made no picture.".into());
    }
    std::fs::rename(&part, &out).map_err(|e| format!("Cannot rename {}: {e}", part.display()))?;
    Ok(out)
}

/// The embedded cover picture of a sound file as a PNG at full size, for the
/// player. Cached by source path + size + mtime.
pub fn cover(ffmpeg: &Path, info: &MediaInfo, cache_dir: &Path) -> Result<PathBuf, String> {
    if !info.audio_only || info.video.width == 0 {
        return Err("The file has no embedded picture.".into());
    }
    let dir = cache_dir.join("strips");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let out = dir.join(format!("cover-{}.png", source_key(&info.path)?));
    if out.is_file() {
        return Ok(out);
    }
    let part = out.with_extension(format!("{}.part.png", std::process::id()));
    let mut cmd = crate::tools::command(ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-i", &info.path]);
    cmd.args(["-map", &format!("0:{}", info.video.stream_index), "-frames:v", "1", "-c:v", "png", "-f", "image2"]);
    cmd.arg(&part);
    let res = crate::tools::output_within(cmd, std::time::Duration::from_secs(30), "ffmpeg (cover)")?;
    if !res.status.success() || !part.is_file() {
        let _ = std::fs::remove_file(&part);
        return Err("ffmpeg could not read the embedded picture.".into());
    }
    std::fs::rename(&part, &out).map_err(|e| format!("Cannot rename {}: {e}", part.display()))?;
    Ok(out)
}

/// A PNG of the waveform of audio track `track` (white on transparent, used
/// as a mask by the page so it takes the theme's color).
pub fn waveform(ffmpeg: &Path, info: &MediaInfo, track: u32, cache_dir: &Path, cancel: &Cancel) -> Result<PathBuf, RunError> {
    if !info.audio.iter().any(|a| a.track == track) {
        return Err(RunError::Failed("The file has no such audio track.".into()));
    }
    let dir = cache_dir.join("strips");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let out = dir.join(format!("wave-{}-{track}.png", source_key(&info.path)?));
    if out.is_file() {
        return Ok(out);
    }
    let part = out.with_extension(format!("{}.part.png", std::process::id()));
    let filter = format!(
        "[0:a:{track}]aresample=8000,aformat=channel_layouts=mono,showwavespic=s={WAVE_W}x{WAVE_H}:colors=white:scale=sqrt[w]"
    );
    let args: Vec<String> = [
        "-hide_banner", "-nostdin", "-loglevel", "error", "-progress", "pipe:1", "-nostats", "-y",
        "-vn", "-sn", "-dn", "-i", &info.path,
        "-filter_complex", &filter, "-map", "[w]", "-frames:v", "1", "-f", "image2",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain(std::iter::once(part.to_string_lossy().into_owned()))
    .collect();
    let res = run_ffmpeg(ffmpeg, &args, 0.0, cancel, |_| {});
    publish(&part, &out, res)
}
