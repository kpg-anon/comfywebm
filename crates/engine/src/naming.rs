//! Output file names: the name template and collision-safe paths.

use crate::spec::{EncodeSpec, MediaInfo};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Time for file names: `5.3s`, `2m05.3s`, `1h02m05s`. Tenths only when nonzero.
pub fn fmt_time(secs: f64) -> String {
    let tenths = (secs.max(0.0) * 10.0).round() as u64;
    let (whole, frac) = (tenths / 10, tenths % 10);
    let (h, m, s) = (whole / 3600, (whole / 60) % 60, whole % 60);
    let sec = if frac > 0 { format!("{s:02}.{frac}") } else { format!("{s:02}") };
    if h > 0 {
        format!("{h}h{m:02}m{sec}s")
    } else if m > 0 {
        format!("{m}m{sec}s")
    } else {
        let sec = sec.trim_start_matches('0');
        let sec = if sec.is_empty() || sec.starts_with('.') { format!("0{sec}") } else { sec.to_string() };
        format!("{sec}s")
    }
}

/// Today's date as YYYY-MM-DD (UTC; std has no time zone support).
pub fn today() -> String {
    let days = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() / 86400).unwrap_or(0) as i64;
    // civil-from-days, Howard Hinnant's algorithm
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Replaces characters that are not allowed in file names on Windows.
pub fn sanitize(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    let s = s.trim_start().to_string();
    if s.is_empty() {
        return "clip".into();
    }
    let upper = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.len() == 4
            && upper.as_bytes()[3].is_ascii_digit());
    if reserved { format!("_{s}") } else { s }
}

/// Fills `{name} {start} {end} {codec} {height} {date}`.
pub fn render(template: &str, spec: &EncodeSpec, info: &MediaInfo, out_height: u32) -> String {
    let stem = Path::new(&spec.input)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "clip".into());
    // first range's start and last range's end; no ranges = the whole file
    let start = spec.segments.first().map(|s| s.start).unwrap_or(0.0);
    let end = spec.segments.last().map(|s| s.end).unwrap_or(info.duration);
    let t = if template.trim().is_empty() { crate::spec::DEFAULT_NAME_TEMPLATE } else { template };
    sanitize(
        &t.replace("{name}", &stem)
            .replace("{start}", &fmt_time(start))
            .replace("{end}", &fmt_time(end))
            .replace("{codec}", spec.codec.name())
            .replace("{height}", &out_height.to_string())
            .replace("{date}", &today()),
    )
}

/// `<path>.part`, the file ffmpeg writes before the final rename.
pub fn part_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// First of `x.webm`, `x (1).webm`, `x (2).webm`, ... for which neither the
/// file nor its `.part` exists.
pub fn unique(path: &Path) -> PathBuf {
    let taken = |p: &Path| p.exists() || part_path(p).exists();
    if !taken(path) {
        return path.to_path_buf();
    }
    let dir = path.parent().unwrap_or(Path::new(""));
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !taken(p))
        .expect("unbounded range")
}

/// Moves `src` to `want`, or to the first free `want (n)` name, without ever
/// replacing an existing file. The name is reserved by creating it with
/// `create_new` (atomic on every OS, also between two app instances), then
/// `src` is renamed over the empty placeholder.
pub fn publish(src: &Path, want: &Path) -> Result<PathBuf, String> {
    let dir = want.parent().unwrap_or(Path::new(""));
    let stem = want.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = want.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for n in 0..10_000u32 {
        let cand = if n == 0 { want.to_path_buf() } else { dir.join(format!("{stem} ({n}){ext}")) };
        if n > 0 && part_path(&cand).exists() {
            continue;
        }
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&cand) {
            Ok(f) => {
                drop(f);
                return match std::fs::rename(src, &cand) {
                    Ok(()) => Ok(cand),
                    Err(e) => {
                        let _ = std::fs::remove_file(&cand);
                        Err(format!("Cannot rename {} to {}: {e}", src.display(), cand.display()))
                    }
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot create {}: {e}", cand.display())),
        }
    }
    Err(format!("No free file name for {}", want.display()))
}

/// `output_dir`, or the folder of `input` when it is not set.
pub fn output_dir(input: &str, output_dir: Option<&str>) -> PathBuf {
    output_dir
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| Path::new(input).parent().map(Path::to_path_buf))
        .unwrap_or_default()
}

/// Final output path: `<dir>/<stem>.<webm|mp4>`, where dir is `output_dir`
/// or the source's folder and stem is `spec.outputName` or the rendered template.
pub fn output_path(spec: &EncodeSpec, info: &MediaInfo, out_height: u32, out_dir: Option<&str>, template: &str) -> PathBuf {
    let dir = output_dir(&spec.input, out_dir);
    let ext = spec.container.ext();
    let stem = match spec.output_name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => {
            let lower = n.to_ascii_lowercase();
            let n = if lower.ends_with(".webm") || lower.ends_with(".mp4") || lower.ends_with(".gif") || lower.ends_with(".webp") { &n[..n.rfind('.').unwrap()] } else { n };
            sanitize(n)
        }
        None => render(template, spec, info, out_height),
    };
    unique(&dir.join(format!("{stem}.{ext}")))
}

/// Frame export path: `<dir>/{name}_{time}.png`, collision-safe.
pub fn frame_path(input: &str, time: f64, out_dir: Option<&str>) -> PathBuf {
    let stem = Path::new(input).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "frame".into());
    let name = sanitize(&format!("{stem}_{}", fmt_time(time)));
    unique(&output_dir(input, out_dir).join(format!("{name}.png")))
}
