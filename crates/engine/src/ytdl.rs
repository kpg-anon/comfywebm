//! yt-dlp: link info and downloads.
//!
//! yt-dlp runs with `--ignore-config`, so the user's own yt-dlp config
//! (audio only, archives, output templates) cannot change what comfywebm
//! gets; `Settings.ytdlpArgs` is the place for extra options such as cookies.

use crate::run::{each_line, tail_thread, Cancel, RunError};
use crate::spec::{Segment, UrlInfo};
use crate::tools::command;
use serde_json::Value;
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

/// `url_info` gives up after this long
const INFO_LIMIT: Duration = Duration::from_secs(90);

/// Only http(s) links reach yt-dlp, so nothing from the UI is read as an option.
pub fn check_url(url: &str) -> Result<(), String> {
    let lower = url.trim().to_ascii_lowercase();
    if (lower.starts_with("https://") || lower.starts_with("http://")) && !url.trim().chars().any(char::is_whitespace) {
        Ok(())
    } else {
        Err("Not a web link. It has to start with http:// or https://.".into())
    }
}

fn last_error(lines: &[String], fallback: String) -> String {
    lines
        .iter()
        .rev()
        .find(|l| l.starts_with("ERROR"))
        .or(lines.last())
        .map(|l| l.trim_start_matches("ERROR: ").to_string())
        .unwrap_or(fallback)
}

/// `yt-dlp -J --no-playlist <url>`.
pub fn url_info(ytdlp: &Path, url: &str, extra_args: &[String]) -> Result<UrlInfo, String> {
    check_url(url)?;
    let mut args: Vec<String> =
        ["--ignore-config", "--no-playlist", "--no-warnings", "--encoding", "utf-8", "-J"].iter().map(|s| s.to_string()).collect();
    args.extend(extra_args.iter().cloned());
    args.extend(["--".into(), url.trim().to_string()]);
    let mut child = command(ytdlp)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot run yt-dlp ({}): {e}", ytdlp.display()))?;
    let mut out = child.stdout.take().expect("piped stdout");
    let out_thread = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = out.read_to_end(&mut b);
        b
    });
    let err_thread = tail_thread(child.stderr.take().expect("piped stderr"), 20);
    let started = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if started.elapsed() > INFO_LIMIT {
            crate::tools::kill_tree(&mut child);
            let _ = child.wait();
            return Err("yt-dlp did not answer within 90 seconds.".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    if !status.success() {
        return Err(last_error(&stderr, format!("yt-dlp exited with {status}")));
    }
    let j: Value = serde_json::from_slice(&stdout).map_err(|e| format!("yt-dlp gave no readable answer: {e}"))?;
    Ok(parse_info(url, &j))
}


pub fn parse_info(url: &str, j: &Value) -> UrlInfo {
    let s = |k: &str| j.get(k).and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_string);
    let mut heights: Vec<u32> = j
        .get("formats")
        .and_then(Value::as_array)
        .map(|fs| {
            fs.iter()
                .filter(|f| f.get("vcodec").and_then(Value::as_str) != Some("none"))
                .filter_map(|f| f.get("height").and_then(Value::as_u64))
                .filter(|h| *h > 0)
                .map(|h| h as u32)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect()
        })
        .unwrap_or_default();
    if heights.is_empty() {
        if let Some(h) = j.get("height").and_then(Value::as_u64).filter(|h| *h > 0) {
            heights.push(h as u32);
        }
    }
    heights.sort_unstable_by(|a, b| b.cmp(a));
    UrlInfo {
        url: url.to_string(),
        id: s("id").unwrap_or_default(),
        title: s("title").unwrap_or_else(|| "video".into()),
        uploader: s("uploader").or_else(|| s("channel")).or_else(|| s("uploader_id")),
        duration: j.get("duration").and_then(Value::as_f64).filter(|d| *d > 0.0),
        is_live: j.get("is_live").and_then(Value::as_bool).unwrap_or(false),
        thumbnail: s("thumbnail"),
        heights,
    }
}

/// Download progress as the `download-progress` event carries it.
pub struct Progress {
    pub fraction: f64,
    pub speed: Option<String>,
    pub eta: Option<String>,
    /// "download" or "merge"
    pub stage: &'static str,
}

fn human_speed(bps: f64) -> String {
    let units = ["B/s", "KiB/s", "MiB/s", "GiB/s"];
    let mut v = bps;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", units[i])
}

fn human_eta(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Arguments for a download into `dir`.
pub fn download_args(url: &str, ffmpeg: &Path, dir: &Path, max_height: Option<u32>, section: Option<Segment>, extra: &[String]) -> Vec<String> {
    let format = match max_height.filter(|h| *h > 0) {
        Some(h) => format!("bv*[height<={h}]+ba/b[height<={h}]"),
        None => "bv*+ba/b".into(),
    };
    // The height limit and the section are part of the name: yt-dlp skips a
    // download whose file already exists, so without them asking for 144p
    // after 240p returned the 240p file.
    let mut name = String::from("%(title).80B [%(id)s]");
    if let Some(h) = max_height.filter(|h| *h > 0) {
        name.push_str(&format!(" {h}p"));
    }
    if let Some(s) = section {
        name.push_str(&format!(" {}-{}", s.start.round() as u64, s.end.round() as u64));
    }
    name.push_str(".%(ext)s");
    let mut a: Vec<String> = vec![
        "--ignore-config".into(),
        "--no-playlist".into(),
        "--newline".into(),
        // --print implies --quiet; --progress keeps the progress lines
        "--progress".into(),
        "--no-mtime".into(),
        "--encoding".into(),
        "utf-8".into(),
        "-f".into(),
        format,
        "--merge-output-format".into(),
        "mkv".into(),
        "--ffmpeg-location".into(),
        ffmpeg.to_string_lossy().into_owned(),
        "-P".into(),
        dir.to_string_lossy().into_owned(),
        "-o".into(),
        name,
        "--print".into(),
        "before_dl:CWID %(id)s".into(),
        "--print".into(),
        "after_move:filepath".into(),
        "--progress-template".into(),
        "download:CWP %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s".into(),
        "--progress-template".into(),
        "postprocess:CWS %(progress.postprocessor)s".into(),
    ];
    if let Some(s) = section {
        a.push("--download-sections".into());
        a.push(format!("*{}-{}", crate::plan::fmt_num(s.start.max(0.0)), crate::plan::fmt_num(s.end)));
    }
    a.extend(extra.iter().cloned());
    a.push("--".into());
    a.push(url.trim().to_string());
    a
}

fn num(s: Option<&&str>) -> Option<f64> {
    s.and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite())
}

/// Downloads `url` into `dir` and returns the file. On cancel or failure the
/// files this download created are deleted.
#[allow(clippy::too_many_arguments)]
pub fn download(
    ytdlp: &Path,
    ffmpeg: &Path,
    url: &str,
    dir: &Path,
    max_height: Option<u32>,
    section: Option<Segment>,
    extra: &[String],
    cancel: &Cancel,
    mut on_progress: impl FnMut(Progress),
) -> Result<PathBuf, RunError> {
    check_url(url)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let before: HashSet<PathBuf> = std::fs::read_dir(dir).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    if cancel.is_canceled() {
        return Err(RunError::Canceled);
    }
    let mut child = command(ytdlp)
        .args(download_args(url, ffmpeg, dir, max_height, section, extra))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot run yt-dlp ({}): {e}", ytdlp.display()))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let err_thread = tail_thread(child.stderr.take().expect("piped stderr"), 20);
    cancel.attach(child);

    let mut id = None;
    let mut printed = Vec::new();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    each_line(stdout, |line| {
        if let Some(rest) = line.strip_prefix("CWP ") {
            let f: Vec<&str> = rest.split_whitespace().collect();
            let done = num(f.first());
            let total = num(f.get(1)).or_else(|| num(f.get(2)));
            let fraction = match (done, total) {
                (Some(d), Some(t)) if t > 0.0 => (d / t).clamp(0.0, 1.0),
                _ => 0.0,
            };
            if last_emit.elapsed() >= Duration::from_millis(200) {
                last_emit = Instant::now();
                on_progress(Progress {
                    fraction,
                    speed: num(f.get(3)).map(human_speed),
                    eta: num(f.get(4)).map(human_eta),
                    stage: "download",
                });
            }
        } else if line.starts_with("CWS ") {
            on_progress(Progress { fraction: 1.0, speed: None, eta: None, stage: "merge" });
        } else if let Some(rest) = line.strip_prefix("CWID ") {
            id = Some(rest.trim().to_string());
        } else if !line.is_empty() {
            printed.push(line.to_string());
        }
    });
    let status = cancel.detach().map(|mut c| c.wait());
    let stderr = err_thread.join().unwrap_or_default();

    let file = printed.iter().rev().map(PathBuf::from).find(|p| p.is_file());
    let ok = !cancel.is_canceled() && matches!(status, Some(Ok(s)) if s.success()) && file.is_some();
    if !ok {
        // files this download created: new since the start, and named with the
        // video id when it is known (another download may share the folder)
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                let ours = match &id {
                    Some(id) => name.contains(&format!("[{id}]")),
                    None => name.ends_with(".part") || name.ends_with(".ytdl") || name.contains(".part-Frag") || name.contains(".temp."),
                };
                if ours && !before.contains(&p) {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
        if cancel.is_canceled() {
            return Err(RunError::Canceled);
        }
        return Err(RunError::Failed(match status {
            Some(Ok(s)) if s.success() => "yt-dlp finished but did not say where the file is.".into(),
            Some(Ok(s)) => last_error(&stderr, format!("yt-dlp exited with {s}")),
            Some(Err(e)) => format!("Waiting for yt-dlp failed: {e}"),
            None => "yt-dlp process was lost".into(),
        }));
    }
    Ok(file.expect("checked above"))
}
