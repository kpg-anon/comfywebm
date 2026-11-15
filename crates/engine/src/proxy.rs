//! Preview proxy: an H.264/AAC MP4 copy the webview can play, for sources it cannot.

use crate::run::{run_ffmpeg, Cancel, RunError};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// FNV-1a 64; stable across Rust versions, unlike `DefaultHasher`.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Cache file for `path`, keyed by path + size + mtime.
pub fn proxy_path(cache_dir: &Path, path: &str) -> Result<PathBuf, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let key = format!("{path}\n{}\n{mtime}", meta.len());
    Ok(cache_dir.join(format!("proxy-{:016x}.mp4", fnv1a(key.as_bytes()))))
}

/// Returns the cached proxy, or transcodes one. `duration` (seconds, from
/// probe) is used for progress; `on_progress` gets 0..1.
pub fn make_proxy(
    ffmpeg: &Path,
    path: &str,
    cache_dir: &Path,
    duration: f64,
    mut on_progress: impl FnMut(f64),
) -> Result<PathBuf, String> {
    let out = proxy_path(cache_dir, path)?;
    if out.is_file() {
        return Ok(out);
    }
    std::fs::create_dir_all(cache_dir).map_err(|e| format!("Cannot create {}: {e}", cache_dir.display()))?;
    let part = out.with_extension(format!("{}.part", std::process::id()));
    let args: Vec<String> = [
        "-hide_banner", "-nostdin", "-loglevel", "error", "-progress", "pipe:1", "-nostats", "-y",
        "-i", path,
        "-map", "0:v:0", "-map", "0:a:0?",
        "-vf", "scale=-2:'min(480,ih)'",
        "-c:v", "libx264", "-preset", "ultrafast", "-crf", "30", "-g", "15", "-pix_fmt", "yuv420p",
        "-c:a", "aac", "-b:a", "96k", "-ac", "2",
        "-sn", "-dn", "-movflags", "+faststart", "-f", "mp4",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain(std::iter::once(part.to_string_lossy().into_owned()))
    .collect();
    let res = run_ffmpeg(ffmpeg, &args, duration, &Cancel::new(), |p| on_progress(p.fraction));
    if let Err(e) = res {
        let _ = std::fs::remove_file(&part);
        return Err(match e {
            RunError::Canceled => "Canceled".into(),
            RunError::Failed(s) => s,
        });
    }
    std::fs::rename(&part, &out).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("Cannot rename {}: {e}", part.display())
    })?;
    on_progress(1.0);
    Ok(out)
}
