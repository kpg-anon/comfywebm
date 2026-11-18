//! Find cuts: where the picture changes to a new shot.
//!
//! The method follows PySceneDetect's AdaptiveDetector and ThresholdDetector
//! (https://github.com/Breakthrough/PySceneDetect, BSD 3-Clause; reimplemented
//! here, no code taken):
//!
//! 1. Every frame, made small (the long side 256 pixels), is compared with the
//!    one before: the mean difference of hue, saturation and value (HSV, as
//!    OpenCV stores them in 8 bits), averaged over the three. That is the
//!    frame's change score, 0..255.
//! 2. A frame is a cut when its score is at least `ratio` times the average of
//!    the two frames before and the two after, and at least `min_change`. A
//!    camera move or a light show raises the neighbours too, so it is not cut;
//!    a cut is one frame that stands out. (ffmpeg's `scene` score, used before,
//!    compares a frame only with the one before.)
//! 3. A flash (a strobe, a camera flash, stage lights) changes the picture and
//!    then changes it back, and each jump stands out from its neighbours. So a
//!    candidate is dropped when, within 0.4 s around it, a frame after the
//!    jump looks like a frame before it again (16x9 colour signatures). A real
//!    cut never goes back. (PySceneDetect's flash filter instead merges cuts
//!    that come close together, which also loses real quick cuts.)
//! 4. A fade through black (the mean of the picture under 12 of 255) is a cut
//!    in the middle of the black frames.
//! 5. Cuts closer than `min_shot` seconds to the previous one are dropped.
//!
//! [`analyze`] decodes the video once and keeps the per-frame numbers (cached
//! as text); [`detect`] finds the cuts in them, so another sensitivity needs
//! no second decode.

use crate::run::{Cancel, RunError};
use crate::spec::MediaInfo;
use std::io::Read;
use std::path::Path;

/// The long side of the frames that are compared.
const SIDE: u32 = 256;
/// Mean brightness (0..255) under which a frame is black (a fade).
const BLACK: f32 = 12.0;
/// Frames on each side of a frame that its score is compared with.
const WINDOW: usize = 2;
/// How long a flash may last before the picture comes back, seconds.
const FLASH_SECS: f64 = 0.4;
/// The colour signature of a frame: the mean HSV of each cell of this grid.
const SIG_W: usize = 16;
const SIG_H: usize = 9;

/// One number per decoded frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Analysis {
    /// the frame's time in the file, seconds
    pub times: Vec<f64>,
    /// how much it differs from the frame before (HSV mean difference, 0..255; 0 for the first)
    pub change: Vec<f32>,
    /// its mean brightness (the mean of R, G and B, 0..255)
    pub luma: Vec<f32>,
    /// How much the picture comes back across this frame: the closest any
    /// frame from here to 0.4 s later gets to any frame up to 0.4 s before,
    /// as a share of this frame's own jump (signatures). Near 0: it came back
    /// (a flash); near 1: it did not (a cut).
    pub back: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sensitivity {
    /// only clear cuts
    Low,
    Normal,
    /// also cuts between similar shots
    High,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// how many times the average of its neighbours a frame's change must be
    pub ratio: f32,
    /// and the least change it must have (0..255)
    pub min_change: f32,
    /// the shortest shot, seconds
    pub min_shot: f64,
    /// cuts in fades through black
    pub fades: bool,
    /// a jump whose picture comes back closer than this share of the jump is a flash (see `Analysis::back`)
    pub flash: f32,
}

impl Sensitivity {
    /// Normal is PySceneDetect's default (ratio 3, change 15, 15 frames ~ 0.5 s).
    pub fn params(self) -> Params {
        let (ratio, min_change) = match self {
            Sensitivity::Low => (4.0, 20.0),
            Sensitivity::Normal => (3.0, 15.0),
            Sensitivity::High => (2.2, 9.0),
        };
        Params { ratio, min_change, min_shot: 0.5, fades: true, flash: 0.5 }
    }
}

/// The cut times (the first frame of each new shot), in order.
pub fn detect(a: &Analysis, p: &Params) -> Vec<f64> {
    let n = a.times.len().min(a.change.len()).min(a.luma.len()).min(a.back.len());
    let mut cuts: Vec<f64> = Vec::new();
    // changes that stand out from their neighbours
    if n > 2 * WINDOW {
        for i in WINDOW..n - WINDOW {
            let s = a.change[i];
            if s < p.min_change {
                continue;
            }
            let around: f32 = (i - WINDOW..=i + WINDOW).filter(|&k| k != i).map(|k| a.change[k]).sum::<f32>() / (2 * WINDOW) as f32;
            let ratio = if around.abs() >= 1e-5 { (s / around).min(255.0) } else { 255.0 };
            if ratio >= p.ratio && a.back[i] >= p.flash {
                cuts.push(a.times[i]);
            }
        }
    }
    // fades through black: the middle of each run of black frames that has picture on both sides
    if p.fades {
        let mut i = 0;
        while i < n {
            if a.luma[i] < BLACK {
                let start = i;
                while i < n && a.luma[i] < BLACK {
                    i += 1;
                }
                if start > 0 && i < n {
                    cuts.push(a.times[(start + i) / 2]);
                }
            } else {
                i += 1;
            }
        }
    }
    cuts.sort_by(|x, y| x.total_cmp(y));
    // the shortest shot: from the start, then from each cut kept
    let first = a.times.first().copied().unwrap_or(0.0);
    let mut out: Vec<f64> = Vec::new();
    let mut last = first;
    for c in cuts {
        if c - last >= p.min_shot - 1e-9 {
            out.push(c);
            last = c;
        }
    }
    out
}

/// OpenCV's 8-bit HSV of one pixel: H 0..179 (degrees / 2), S and V 0..255.
fn hsv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (r as f32, g as f32, b as f32);
    let v = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = v - min;
    let s = if v > 0.0 { 255.0 * d / v } else { 0.0 };
    let mut h = if d == 0.0 {
        0.0
    } else if v == r {
        60.0 * (g - b) / d
    } else if v == g {
        120.0 + 60.0 * (b - r) / d
    } else {
        240.0 + 60.0 * (r - g) / d
    };
    if h < 0.0 {
        h += 360.0;
    }
    ((h / 2.0).round().min(179.0) as u8, s.round() as u8, v as u8)
}

/// Per-frame numbers of one RGB24 frame against the previous frame's HSV (replaced by this frame's).
fn measure(rgb: &[u8], prev: &mut Vec<[u8; 3]>) -> (f32, f32) {
    let px = rgb.len() / 3;
    let mut sum = [0u64; 3];
    let mut bright = 0u64;
    let first = prev.len() != px;
    if first {
        prev.clear();
        prev.resize(px, [0; 3]);
    }
    for (k, p) in rgb.chunks_exact(3).enumerate() {
        bright += p[0] as u64 + p[1] as u64 + p[2] as u64;
        let (h, s, v) = hsv(p[0], p[1], p[2]);
        let q = prev[k];
        sum[0] += (h as i32 - q[0] as i32).unsigned_abs() as u64;
        sum[1] += (s as i32 - q[1] as i32).unsigned_abs() as u64;
        sum[2] += (v as i32 - q[2] as i32).unsigned_abs() as u64;
        prev[k] = [h, s, v];
    }
    let px = px.max(1) as f32;
    let change = if first { 0.0 } else { (sum[0] + sum[1] + sum[2]) as f32 / (3.0 * px) };
    (change, bright as f32 / (3.0 * px))
}

/// Mean H, S and V of each cell of a SIG_W x SIG_H grid over the frame.
fn signature(rgb: &[u8], w: usize, h: usize) -> Vec<f32> {
    let mut sum = vec![0f32; SIG_W * SIG_H * 3];
    let mut count = vec![0u32; SIG_W * SIG_H];
    for y in 0..h {
        let cy = y * SIG_H / h;
        for x in 0..w {
            let k = (y * w + x) * 3;
            let (hh, ss, vv) = hsv(rgb[k], rgb[k + 1], rgb[k + 2]);
            let c = cy * SIG_W + x * SIG_W / w;
            sum[c * 3] += hh as f32;
            sum[c * 3 + 1] += ss as f32;
            sum[c * 3 + 2] += vv as f32;
            count[c] += 1;
        }
    }
    for (c, n) in count.iter().enumerate() {
        let n = (*n).max(1) as f32;
        for v in &mut sum[c * 3..c * 3 + 3] {
            *v /= n;
        }
    }
    sum
}

fn sig_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / a.len().max(1) as f32
}

/// Works out `Analysis::back` as the frames come: each frame's signature is
/// compared with the 2k+1 before it once, and a frame's value is read from
/// those differences when the k frames after it are in.
struct Flash {
    k: usize,
    /// the last 2k+2 signatures
    sigs: std::collections::VecDeque<Vec<f32>>,
    /// for the same frames: the difference to each of the 2k+1 frames before (d[j - 1] = to the frame j back)
    diffs: std::collections::VecDeque<Vec<f32>>,
    /// index of the first frame in the deques
    base: usize,
    back: Vec<f32>,
}

impl Flash {
    fn new(fps: f64) -> Self {
        let k = ((FLASH_SECS * fps).round() as usize).clamp(2, 24);
        Flash { k, sigs: Default::default(), diffs: Default::default(), base: 0, back: Vec::new() }
    }

    fn push(&mut self, sig: Vec<f32>) {
        let d: Vec<f32> = (1..=2 * self.k + 1).map(|j| self.sigs.len().checked_sub(j).map_or(f32::INFINITY, |at| sig_diff(&self.sigs[at], &sig))).collect();
        self.sigs.push_back(sig);
        self.diffs.push_back(d);
        let n = self.base + self.sigs.len();
        // frame i is done when frame i + k is in
        if n > self.k {
            self.finish(n - 1 - self.k, n);
        }
        while self.sigs.len() > 2 * self.k + 2 {
            self.sigs.pop_front();
            self.diffs.pop_front();
            self.base += 1;
        }
    }

    /// back for frame i, with the frames before `end` known
    fn finish(&mut self, i: usize, end: usize) {
        if i == 0 {
            self.back.push(1.0);
            return;
        }
        let at = |f: usize| f - self.base;
        let jump = self.diffs[at(i)][0];
        let mut close = jump;
        // pairs (a before i, b from i on), both within k of i
        for b in i..end.min(i + self.k + 1) {
            // a = b - j runs from i - 1 back to i - k - 1 (not before frame 0)
            for j in (b - i + 1)..=(b - i + self.k + 1).min(b) {
                close = close.min(self.diffs[at(b)][j - 1]);
            }
        }
        self.back.push(if jump > 0.5 { (close / jump).min(1.0) } else { 1.0 });
    }

    /// the frames at the end, whose k frames after never came
    fn end(mut self) -> Vec<f32> {
        let n = self.base + self.sigs.len();
        for i in self.back.len()..n {
            self.finish(i, n);
        }
        self.back
    }
}

/// The size the frames are compared at: the long side 256 (or the picture's own, if smaller), even.
fn small_size(w: u32, h: u32) -> (u32, u32) {
    let (w, h) = (w.max(2), h.max(2));
    let k = (SIDE as f64 / w.max(h) as f64).min(1.0);
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    (even(w as f64 * k), even(h as f64 * k))
}

/// Decode the video once and measure every frame. `progress` gets 0..1.
/// Cached in `cache_dir/strips` by the file's path, size and time.
pub fn analyze(ffmpeg: &Path, info: &MediaInfo, cache_dir: &Path, cancel: &Cancel, mut progress: impl FnMut(f64)) -> Result<Analysis, RunError> {
    if info.audio_only {
        return Err(RunError::Failed("The file has no video stream.".into()));
    }
    let dir = cache_dir.join("strips");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let out = dir.join(format!("shots-{}.txt", crate::proxy::source_key(&info.path)?));
    if let Some(a) = std::fs::read_to_string(&out).ok().and_then(|t| parse(&t)) {
        return Ok(a);
    }
    let (w, h) = small_size(info.video.width, info.video.height);
    let args: Vec<String> = [
        "-hide_banner", "-nostdin", "-nostats", "-i", &info.path, "-map", &format!("0:{}", info.video.stream_index), "-an", "-sn", "-dn",
        // every frame as it comes (no frame rate change), with its time on stderr
        "-fps_mode", "passthrough",
        "-vf", &format!("scale={w}:{h}:flags=bilinear,format=rgb24,showinfo"),
        "-f", "rawvideo", "-",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut cmd = crate::tools::command(ffmpeg);
    cmd.args(&args).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| RunError::Failed(format!("Cannot run ffmpeg: {e}")))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    cancel.attach(child);
    // the frame times, from showinfo on stderr
    let times_thread = std::thread::spawn(move || {
        let mut times = Vec::new();
        crate::run::each_line(stderr, |line| {
            if line.contains("Parsed_showinfo") {
                if let Some(t) = line.split("pts_time:").nth(1).and_then(|r| r.split_whitespace().next()).and_then(|v| v.parse::<f64>().ok()) {
                    times.push(t);
                }
            }
        });
        times
    });
    let frame = (w * h * 3) as usize;
    let mut buf = vec![0u8; frame];
    let mut prev = Vec::new();
    let mut a = Analysis::default();
    let fps = if info.video.fps > 0.0 { info.video.fps } else { 30.0 };
    let mut flash = Flash::new(fps);
    while stdout.read_exact(&mut buf).is_ok() {
        let (c, l) = measure(&buf, &mut prev);
        a.change.push(c);
        a.luma.push(l);
        flash.push(signature(&buf, w as usize, h as usize));
        if info.duration > 0.0 && a.change.len() % 30 == 0 {
            progress((a.change.len() as f64 / fps / info.duration).min(0.99));
        }
    }
    let status = cancel.detach().map(|mut c| c.wait());
    let times = times_thread.join().unwrap_or_default();
    a.back = flash.end();
    if cancel.is_canceled() {
        return Err(RunError::Canceled);
    }
    if !matches!(status, Some(Ok(s)) if s.success()) || a.change.is_empty() {
        return Err(RunError::Failed("ffmpeg could not read the video for scene cuts.".into()));
    }
    // showinfo gives one time per frame; if any went missing, frames are spaced by the frame rate
    a.times = if times.len() == a.change.len() { times } else { (0..a.change.len()).map(|i| i as f64 / fps).collect() };
    let _ = std::fs::write(&out, text(&a));
    progress(1.0);
    Ok(a)
}

fn text(a: &Analysis) -> String {
    let mut s = String::with_capacity(a.times.len() * 24);
    for i in 0..a.times.len() {
        s.push_str(&format!("{:.4} {:.2} {:.1} {:.3}\n", a.times[i], a.change[i], a.luma[i], a.back[i]));
    }
    s
}

fn parse(t: &str) -> Option<Analysis> {
    let mut a = Analysis::default();
    for line in t.lines() {
        let mut it = line.split_whitespace().map(|v| v.parse::<f64>());
        let (Some(Ok(time)), Some(Ok(c)), Some(Ok(l)), Some(Ok(b))) = (it.next(), it.next(), it.next(), it.next()) else { return None };
        a.times.push(time);
        a.change.push(c as f32);
        a.luma.push(l as f32);
        a.back.push(b as f32);
    }
    (!a.times.is_empty()).then_some(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// frames at 30 fps with the given change scores, all bright
    fn frames(change: &[f32]) -> Analysis {
        Analysis { times: (0..change.len()).map(|i| i as f64 / 30.0).collect(), change: change.to_vec(), luma: vec![100.0; change.len()], back: vec![1.0; change.len()] }
    }

    #[test]
    fn hsv_matches_opencv() {
        assert_eq!(hsv(255, 0, 0), (0, 255, 255));
        assert_eq!(hsv(0, 255, 0), (60, 255, 255));
        assert_eq!(hsv(0, 0, 255), (120, 255, 255));
        assert_eq!(hsv(128, 128, 128), (0, 0, 128));
        assert_eq!(hsv(0, 0, 0), (0, 0, 0));
    }

    #[test]
    fn a_cut_stands_out_from_its_neighbours() {
        let mut c = vec![2.0; 60];
        c[30] = 40.0;
        assert_eq!(detect(&frames(&c), &Sensitivity::Normal.params()), vec![1.0]);
    }

    #[test]
    fn steady_motion_is_not_a_cut() {
        // a pan: every frame changes a lot, none stands out
        let c: Vec<f32> = (0..60).map(|i| 30.0 + (i % 3) as f32).collect();
        assert!(detect(&frames(&c), &Sensitivity::Normal.params()).is_empty());
        // a small change that stands out is still under the least change
        let mut c = vec![0.5; 60];
        c[30] = 8.0;
        assert!(detect(&frames(&c), &Sensitivity::Normal.params()).is_empty());
        assert_eq!(detect(&frames(&c), &Sensitivity::High.params()), Vec::<f64>::new(), "8 < 9");
    }

    #[test]
    fn a_jump_that_comes_back_is_a_flash() {
        let mut a = frames(&vec![2.0; 60]);
        a.change[30] = 40.0;
        a.back[30] = 0.1;
        assert!(detect(&a, &Sensitivity::Normal.params()).is_empty());
    }

    #[test]
    fn flash_tracking_finds_the_return() {
        // signatures: picture A, a 2-frame flash F, A again; then a cut to B that stays
        let (sa, sf, sb) = (vec![10.0; 4], vec![200.0; 4], vec![120.0; 4]);
        let mut f = Flash::new(30.0); // k = 12
        let seq: Vec<&Vec<f32>> = (0..60).map(|i| if i < 20 { &sa } else if i < 22 { &sf } else if i < 40 { &sa } else { &sb }).collect();
        for s in &seq {
            f.push((*s).clone());
        }
        let back = f.end();
        assert_eq!(back.len(), 60);
        assert!(back[20] < 0.05, "into the flash: {}", back[20]);
        assert!(back[22] < 0.05, "out of the flash: {}", back[22]);
        assert!(back[40] > 0.95, "the cut: {}", back[40]);
    }

    #[test]
    fn shots_shorter_than_half_a_second_are_dropped() {
        let mut c = vec![2.0; 90];
        c[30] = 40.0;
        c[39] = 40.0; // 0.3 s after the first: dropped
        c[60] = 40.0;
        assert_eq!(detect(&frames(&c), &Sensitivity::Normal.params()), vec![1.0, 2.0]);
        // nor a cut in the first half second
        let mut c = vec![2.0; 60];
        c[6] = 40.0;
        assert!(detect(&frames(&c), &Sensitivity::Normal.params()).is_empty());
    }

    #[test]
    fn a_fade_through_black_is_cut_in_its_middle() {
        let mut a = frames(&vec![2.0; 90]);
        for l in &mut a.luma[40..50] {
            *l = 3.0;
        }
        assert_eq!(detect(&a, &Sensitivity::Normal.params()), vec![45.0 / 30.0]);
        // black at the start or the end is no cut
        let mut a = frames(&vec![2.0; 90]);
        for (i, l) in a.luma.iter_mut().enumerate() {
            if i < 20 || i >= 80 {
                *l = 3.0;
            }
        }
        assert!(detect(&a, &Sensitivity::Normal.params()).is_empty());
    }

    #[test]
    fn cache_text_round_trip() {
        let a = Analysis { times: vec![0.0, 0.0333], change: vec![0.0, 12.5], luma: vec![100.0, 3.5], back: vec![1.0, 0.25] };
        assert_eq!(parse(&text(&a)), Some(a));
        assert_eq!(parse("garbage"), None);
    }

    #[test]
    fn small_size_keeps_the_shape() {
        assert_eq!(small_size(1920, 1080), (256, 144));
        assert_eq!(small_size(1080, 1920), (144, 256));
        assert_eq!(small_size(160, 90), (160, 90));
    }
}
