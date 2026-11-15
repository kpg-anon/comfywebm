//! Running a job: ffmpeg passes, progress parsing, size-fit retries, cancel.
//! Also the sample preview and frame export, which run ffmpeg once.

use crate::naming;
use crate::plan::{self, Opts, SegmentFilters};
use crate::spec::{Crop, EncodeSpec, JobResult, MediaInfo, Progress, Rate, Segment};
use crate::tools::{command, kill_tree};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const MAX_ATTEMPTS: u32 = 4;
const STDERR_LINES: usize = 20;
/// a higher bitrate that grows the file by less than this means the encoder is at its quality ceiling
const CEILING_GROWTH: f64 = 1.03;
const SAMPLE_SECS: f64 = 3.0;

/// Shared between the thread running a process and whoever wants to stop it.
#[derive(Default)]
pub struct Cancel {
    flag: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Kills the running process tree (if any) and makes the work stop.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        if let Some(child) = self.child.lock().unwrap().as_mut() {
            kill_tree(child);
        }
    }

    pub fn is_canceled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Stores the running child so `cancel` can kill it. Kills it at once if
    /// `cancel` already ran.
    pub fn attach(&self, child: Child) {
        let mut slot = self.child.lock().unwrap();
        *slot = Some(child);
        if self.is_canceled() {
            if let Some(c) = slot.as_mut() {
                kill_tree(c);
            }
        }
    }

    /// Takes the child back, to wait for it.
    pub fn detach(&self) -> Option<Child> {
        self.child.lock().unwrap().take()
    }
}

#[derive(Debug)]
pub enum RunError {
    Canceled,
    Failed(String),
}

impl From<String> for RunError {
    fn from(s: String) -> Self {
        RunError::Failed(s)
    }
}

/// Reads a pipe line by line. A line that is not UTF-8 must not stop the
/// reading, or the pipe fills and the child blocks.
pub fn each_line(stream: impl Read, mut f: impl FnMut(&str)) {
    let mut r = BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match r.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => f(String::from_utf8_lossy(&buf).trim()),
        }
    }
}

/// Collects the last `n` non-empty lines of a pipe on a thread.
pub fn tail_thread(stream: impl Read + Send + 'static, n: usize) -> std::thread::JoinHandle<Vec<String>> {
    std::thread::spawn(move || {
        let mut tail: VecDeque<String> = VecDeque::new();
        each_line(stream, |line| {
            if line.is_empty() {
                return;
            }
            if tail.len() == n {
                tail.pop_front();
            }
            tail.push_back(line.to_string());
        });
        tail.into_iter().collect()
    })
}

/// Progress of one ffmpeg run, from `-progress pipe:1`.
pub struct PassProgress {
    /// 0..1 of the output duration
    pub fraction: f64,
    pub speed: Option<f64>,
}

/// Runs ffmpeg once, reading `-progress` output from stdout. Returns the
/// last stderr lines as the error when ffmpeg fails.
pub fn run_ffmpeg(
    ffmpeg: &Path,
    args: &[String],
    duration: f64,
    cancel: &Cancel,
    mut on_progress: impl FnMut(PassProgress),
) -> Result<(), RunError> {
    if cancel.is_canceled() {
        return Err(RunError::Canceled);
    }
    let mut child = command(ffmpeg)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RunError::Failed(format!("Cannot run ffmpeg ({}): {e}", ffmpeg.display())))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let err_thread = tail_thread(child.stderr.take().expect("piped stderr"), STDERR_LINES);
    cancel.attach(child);

    let mut out_secs = 0.0;
    let mut speed = None;
    each_line(stdout, |line| {
        let Some((k, v)) = line.split_once('=') else { return };
        match k {
            "out_time_us" => {
                if let Ok(us) = v.parse::<f64>() {
                    out_secs = us / 1_000_000.0;
                }
            }
            "speed" => speed = v.trim_end_matches('x').trim().parse::<f64>().ok().filter(|s| *s > 0.0),
            "progress" => {
                let fraction = if duration > 0.0 { (out_secs / duration).clamp(0.0, 1.0) } else { 0.0 };
                on_progress(PassProgress { fraction, speed });
            }
            _ => {}
        }
    });

    let status = cancel.detach().map(|mut c| c.wait());
    let tail = err_thread.join().unwrap_or_default().join("\n");
    if cancel.is_canceled() {
        return Err(RunError::Canceled);
    }
    match status {
        Some(Ok(s)) if s.success() => Ok(()),
        Some(Ok(s)) => Err(RunError::Failed(if tail.is_empty() { format!("ffmpeg exited with {s}") } else { tail })),
        Some(Err(e)) => Err(RunError::Failed(format!("Waiting for ffmpeg failed: {e}"))),
        None => Err(RunError::Failed("ffmpeg process was lost".into())),
    }
}

/// A unique pass log prefix in the temp dir.
pub fn passlog_prefix() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("comfywebm-{}-{nanos}-{n}", std::process::id()))
}

/// Deletes the pass log files (libvpx and x264 write `<prefix>-0.log` and
/// more, x265 `<prefix>-x265.log` and `.cutree`), and the temporary outputs.
struct Cleanup {
    passlog: PathBuf,
    files: Vec<PathBuf>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let (Some(dir), Some(prefix)) = (self.passlog.parent(), self.passlog.file_name()) {
            let prefix = prefix.to_string_lossy().into_owned();
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().starts_with(&prefix) {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        }
        for p in &self.files {
            let _ = std::fs::remove_file(p);
        }
    }
}

pub struct Outcome {
    /// final path; differs from the requested one only if that appeared meanwhile
    pub output: PathBuf,
    pub result: JobResult,
    /// (video kbps, bytes) of every attempt, in order
    pub attempts: Vec<(f64, u64)>,
}

/// Which of two outputs to keep under a size limit: one under the limit
/// beats one over it; among those under, the larger; among those over, the smaller.
fn better(new: u64, old: u64, max: Option<f64>) -> bool {
    let Some(max) = max else { return true };
    let (n_ok, o_ok) = (new as f64 <= max, old as f64 <= max);
    match (n_ok, o_ok) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => new > old,
        (false, false) => new < old,
    }
}

/// Encodes `spec` to `final_path` (through `<final_path>.part`).
///
/// Size mode fits the file to the limit in at most 4 attempts. An output over
/// the limit, or under `plan::FILL_ACCEPT` of it, is encoded again at the
/// bitrate from `plan::next_video_kbps`, which aims at `plan::FILL_TARGET`.
/// The best attempt is kept (the largest under the limit, else the smallest).
/// A higher bitrate that grows the file by less than 3% stops the loop.
/// Retries repeat only pass 2 where the pass 1 log does not depend on the
/// bitrate (libvpx, x264).
pub fn run_job(
    ffmpeg: &Path,
    spec: &EncodeSpec,
    info: &MediaInfo,
    final_path: &Path,
    zscale: bool,
    cancel: &Cancel,
    mut on_progress: impl FnMut(&Progress),
) -> Result<Outcome, RunError> {
    let started = Instant::now();
    let part = naming::part_path(final_path);
    let best_path = {
        let mut s = final_path.as_os_str().to_owned();
        s.push(".best.part");
        PathBuf::from(s)
    };
    let passlog = passlog_prefix();
    let mut cleanup = Cleanup { passlog: passlog.clone(), files: vec![part.clone(), best_path.clone()] };
    let part_s = part.to_string_lossy().into_owned();
    let passlog_s = passlog.to_string_lossy().into_owned();
    let max_bytes = match spec.rate {
        Rate::Size { max_bytes } => Some(max_bytes),
        Rate::Crf { .. } => None,
    };

    let mut kbps_override = None;
    let mut attempt = 1;
    let mut pass1_done = false;
    let mut best: Option<u64> = None;
    let mut history: Vec<(f64, u64)> = Vec::new();
    loop {
        let opts = Opts { video_kbps: kbps_override, zscale, ..Opts::new(&part_s, &passlog_s) };
        let planned = plan::build(spec, info, &opts)?;
        let total_passes = planned.passes.len() as u32;
        // (pass number, args, start and length of this pass in the attempt's fraction)
        let runs: Vec<(u32, &Vec<String>, f64, f64)> = if total_passes == 2 && pass1_done && planned.reuse_pass1 {
            vec![(2, &planned.passes[1], 0.0, 1.0)]
        } else if total_passes == 2 {
            vec![(1, &planned.passes[0], 0.0, 0.3), (2, &planned.passes[1], 0.3, 0.7)]
        } else {
            vec![(1, &planned.passes[0], 0.0, 1.0)]
        };
        let attempt_start = Instant::now();
        for (pass, args, base, len) in runs {
            run_ffmpeg(ffmpeg, args, planned.duration_out, cancel, |p| {
                let fraction = (base + p.fraction * len).clamp(0.0, 1.0);
                let elapsed = attempt_start.elapsed().as_secs_f64();
                let eta_secs = (fraction >= 0.01).then(|| elapsed * (1.0 - fraction) / fraction);
                on_progress(&Progress { fraction, pass, passes: total_passes, attempt, speed: p.speed, eta_secs });
            })?;
        }
        pass1_done = total_passes == 2;
        let size = std::fs::metadata(&part).map_err(|e| format!("ffmpeg wrote no output: {e}"))?.len();
        let kbps = planned.video_kbps.unwrap_or(0.0);

        if best.is_none_or(|b| better(size, b, max_bytes)) {
            std::fs::rename(&part, &best_path).map_err(|e| format!("Cannot rename {}: {e}", part.display()))?;
            best = Some(size);
        } else {
            let _ = std::fs::remove_file(&part);
        }
        let prev = history.last().copied();
        history.push((kbps, size));

        let (Some(max), Some(k)) = (max_bytes, planned.video_kbps) else { break };
        if attempt >= MAX_ATTEMPTS {
            break;
        }
        let s = size as f64;
        if s <= max && s >= max * plan::FILL_ACCEPT {
            break;
        }
        if s <= max {
            if let Some((pk, ps)) = prev {
                if k > pk && s < ps as f64 * CEILING_GROWTH {
                    break;
                }
            }
        }
        kbps_override = Some(plan::next_video_kbps(&history, max));
        attempt += 1;
    }

    let size = best.expect("at least one attempt");
    let output = if final_path.exists() { naming::unique(final_path) } else { final_path.to_path_buf() };
    std::fs::rename(&best_path, &output)
        .map_err(|e| format!("Cannot rename {} to {}: {e}", best_path.display(), output.display()))?;
    cleanup.files.clear();
    drop(cleanup);
    Ok(Outcome {
        output,
        result: JobResult {
            size_bytes: size,
            elapsed_secs: started.elapsed().as_secs_f64(),
            attempts: attempt,
            over_limit: max_bytes.is_some_and(|m| size as f64 > m),
        },
        attempts: history,
    })
}

/// A 3-second encode from `at` with the job's filters, in one pass at the
/// bitrate the full job would get. Written to `out`.
pub fn sample(
    ffmpeg: &Path,
    spec: &EncodeSpec,
    info: &MediaInfo,
    at: f64,
    out: &Path,
    zscale: bool,
    cancel: &Cancel,
) -> Result<(), RunError> {
    let full = plan::build(spec, info, &Opts { display: true, zscale, ..Opts::new("", "") })?;
    let mut s = spec.clone();
    let mut start = at.max(0.0);
    if info.duration > 0.0 {
        start = start.min((info.duration - SAMPLE_SECS).max(0.0));
    }
    let end = if info.duration > 0.0 { (start + SAMPLE_SECS).min(info.duration) } else { start + SAMPLE_SECS };
    s.segments = vec![Segment { start, end }];
    s.fade_in = 0.0;
    s.fade_out = 0.0;
    let out_s = out.to_string_lossy().into_owned();
    let opts = Opts { video_kbps: full.video_kbps, zscale, single_pass: true, ..Opts::new(&out_s, "") };
    let planned = plan::build(&s, info, &opts)?;
    let res = run_ffmpeg(ffmpeg, &planned.passes[0], planned.duration_out, cancel, |_| {});
    if res.is_err() {
        let _ = std::fs::remove_file(out);
    }
    res
}

/// One PNG at `time` from the source file, at source resolution, with the
/// crop applied (and tone mapping for HDR sources when zscale exists).
pub fn export_frame(ffmpeg: &Path, info: &MediaInfo, time: f64, crop: Option<Crop>, out: &Path, zscale: bool) -> Result<(), String> {
    let v = &info.video;
    let g = plan::geometry(v.width, v.height, crop, 0, None, None);
    let chain = plan::segment_chain(
        &SegmentFilters {
            deinterlace: false,
            denoise: false,
            tonemap: v.hdr && zscale,
            anamorphic: v.anamorphic,
            crop: g.crop,
            rotate: 0,
            mirror: false,
            subtitles: None,
        },
        0.0,
    )
    .join(",");
    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-y"].iter().map(|s| s.to_string()).collect();
    if time > 0.0 {
        args.extend(["-ss".into(), plan::fmt_num(time)]);
    }
    args.extend(["-i".into(), info.path.clone(), "-map".into(), format!("0:{}", v.stream_index)]);
    if !chain.is_empty() {
        args.extend(["-vf".into(), chain]);
    }
    args.extend(["-frames:v", "1", "-update", "1", "-c:v", "png", "-f", "image2"].iter().map(|s| s.to_string()));
    args.push(out.to_string_lossy().into_owned());
    let res = command(ffmpeg).args(&args).stdout(Stdio::null()).output().map_err(|e| format!("Cannot run ffmpeg: {e}"))?;
    if !res.status.success() || !out.is_file() {
        let _ = std::fs::remove_file(out);
        let err = String::from_utf8_lossy(&res.stderr).trim().to_string();
        return Err(if err.is_empty() { "ffmpeg wrote no frame. The time may be past the end.".into() } else { err });
    }
    Ok(())
}
