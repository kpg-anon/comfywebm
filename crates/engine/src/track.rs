//! Follow a person: a box drawn on one frame is followed through the frames
//! after it, and the path becomes zoom points (zoom.rs) that keep it in the
//! picture. ffmpeg's stabilisers (vidstab, deshake) steady the whole camera;
//! they do not follow one thing in it.
//!
//! The frames are read once, small and grey (long side SIDE). Each frame,
//! the template (the box's picture) is looked for around where it was:
//! normalised cross-correlation over a coarse grid, then the best spot
//! refined to the pixel and then between pixels (a parabola through the
//! match and its neighbours). The template takes in a little of each good match
//! (it turns, the light changes); a weak match (someone in front, a blur)
//! keeps the last position instead of jumping. The path is then smoothed.

use crate::run::{Cancel, RunError};
use crate::spec::{MediaInfo, Rect};
use std::io::Read;
use std::path::Path;

/// Long side of the frames looked at.
const SIDE: u32 = 240;
/// Below this match the position is kept (0..1).
const LOST: f32 = 0.35;
/// How much of a good match goes into the template.
const LEARN: f32 = 0.08;

/// A grey picture, row by row.
#[derive(Clone)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

impl Gray {
    fn at(&self, x: usize, y: usize) -> f32 {
        self.px[y * self.w + x]
    }
}

/// Normalised cross-correlation of `t` with `f` at (x, y), -1..1.
fn ncc(f: &Gray, t: &Gray, x: usize, y: usize, step: usize) -> f32 {
    let (mut sf, mut st, mut sff, mut stt, mut sft, mut n) = (0f32, 0f32, 0f32, 0f32, 0f32, 0f32);
    let mut j = 0;
    while j < t.h {
        let mut i = 0;
        while i < t.w {
            let a = f.at(x + i, y + j);
            let b = t.at(i, j);
            sf += a;
            st += b;
            sff += a * a;
            stt += b * b;
            sft += a * b;
            n += 1.0;
            i += step;
        }
        j += step;
    }
    let cov = sft - sf * st / n;
    let vf = sff - sf * sf / n;
    let vt = stt - st * st / n;
    if vf <= 1e-6 || vt <= 1e-6 {
        return 0.0;
    }
    cov / (vf * vt).sqrt()
}

fn crop(f: &Gray, x: usize, y: usize, w: usize, h: usize) -> Gray {
    let mut px = Vec::with_capacity(w * h);
    for j in 0..h {
        px.extend_from_slice(&f.px[(y + j) * f.w + x..(y + j) * f.w + x + w]);
    }
    Gray { w, h, px }
}

/// Follows the box (x, y, w, h in the frames' pixels, first frame) through
/// `frames`; returns the box's top-left corner and match in each frame.
pub fn follow(frames: &[Gray], b: (usize, usize, usize, usize)) -> Vec<(f32, f32, f32)> {
    let Some(first) = frames.first() else { return Vec::new() };
    let (w, h) = (b.2.max(4).min(first.w), b.3.max(4).min(first.h));
    let (mut x, mut y) = (b.0.min(first.w - w), b.1.min(first.h - h));
    let mut t = crop(first, x, y, w, h);
    let mut out = vec![(x as f32, y as f32, 1.0)];
    // look this far around the last position
    let r = ((w.max(h) as f32) * 0.4).max(10.0) as isize;
    // every position is tried (a coarse grid missed sharp matches); a big box
    // is compared on every second pixel, then the best spot on all of them
    let coarse = if w * h > 1200 { 2 } else { 1 };
    for f in &frames[1..] {
        let (mut best, mut bx, mut by) = (-2f32, x, y);
        let lo = |c: usize, lim: usize| (c as isize - r).clamp(0, lim as isize) as usize;
        let hi = |c: usize, lim: usize| (c as isize + r).clamp(0, lim as isize) as usize;
        let (x0, x1, y0, y1) = (lo(x, f.w - w), hi(x, f.w - w), lo(y, f.h - h), hi(y, f.h - h));
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                let s = ncc(f, &t, cx, cy, coarse);
                if s > best {
                    (best, bx, by) = (s, cx, cy);
                }
            }
        }
        if coarse > 1 {
            // around the best, on every pixel
            let (fx, fy) = (bx, by);
            best = -2.0;
            for cy in fy.saturating_sub(1)..=(fy + 1).min(f.h - h) {
                for cx in fx.saturating_sub(1)..=(fx + 1).min(f.w - w) {
                    let s = ncc(f, &t, cx, cy, 1);
                    if s > best {
                        (best, bx, by) = (s, cx, cy);
                    }
                }
            }
        }
        let (mut sx, mut sy) = (bx as f32, by as f32);
        if best >= LOST {
            // between pixels: the top of a parabola through the match and its neighbours
            let peak = |l: f32, c: f32, r: f32| {
                let d = l - 2.0 * c + r;
                if d < -1e-6 { ((l - r) / (2.0 * d)).clamp(-0.5, 0.5) } else { 0.0 }
            };
            if bx > 0 && bx < f.w - w {
                sx += peak(ncc(f, &t, bx - 1, by, 1), ncc(f, &t, bx, by, 1), ncc(f, &t, bx + 1, by, 1));
            }
            if by > 0 && by < f.h - h {
                sy += peak(ncc(f, &t, bx, by - 1, 1), ncc(f, &t, bx, by, 1), ncc(f, &t, bx, by + 1, 1));
            }
            x = bx;
            y = by;
            // the template takes in a little of what it matched
            let now = crop(f, x, y, w, h);
            for (a, b) in t.px.iter_mut().zip(now.px) {
                *a += (b - *a) * LEARN;
            }
            out.push((sx, sy, best));
        } else {
            out.push((x as f32, y as f32, best));
        }
    }
    out
}

/// A centred moving average over `half` values on each side, fewer near the
/// ends so that it stays centred (a one-sided window pulled the first and
/// last positions towards the middle of the path).
pub fn smooth(v: &[f32], half: usize) -> Vec<f32> {
    (0..v.len())
        .map(|i| {
            let k = half.min(i).min(v.len() - 1 - i);
            let a = i - k;
            let b = i + k + 1;
            v[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// The frames' size for a picture of w x h: long side SIDE, even.
pub fn small_size(w: u32, h: u32) -> (u32, u32) {
    let k = (SIDE as f64 / w.max(h).max(1) as f64).min(1.0);
    let even = |v: f64| (((v / 2.0).round() as u32) * 2).max(2);
    (even(w as f64 * k), even(h as f64 * k))
}

/// Follows `bx` (source display pixels, on the frame at `start`) from `start`
/// to `end` and returns zoom points every `every` seconds, in source pixels:
/// (time, centre x, centre y) plus the box size, for the app to turn into
/// zoom points of its picture (it knows the crop, rotation and flips).
pub fn run(ffmpeg: &Path, info: &MediaInfo, start: f64, end: f64, bx: Rect, cancel: &Cancel, mut progress: impl FnMut(f64)) -> Result<Vec<TrackPoint>, RunError> {
    if info.audio_only || end <= start {
        return Err(RunError::Failed("Nothing to follow here.".into()));
    }
    let (sw, sh) = (info.video.width.max(2), info.video.height.max(2));
    let (w, h) = small_size(sw, sh);
    let k = w as f64 / sw as f64;
    let args: Vec<String> = [
        "-hide_banner", "-nostdin", "-nostats", "-ss", &crate::plan::fmt_num(start), "-t", &crate::plan::fmt_num(end - start),
        "-i", &info.path, "-map", &format!("0:{}", info.video.stream_index), "-an", "-sn", "-dn",
        "-fps_mode", "passthrough", "-vf", &format!("scale={w}:{h}:flags=area,format=gray,showinfo"), "-f", "rawvideo", "-",
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
    let (wu, hu) = (w as usize, h as usize);
    let mut buf = vec![0u8; wu * hu];
    let mut frames = Vec::new();
    let fps = if info.video.fps > 0.0 { info.video.fps } else { 30.0 };
    while stdout.read_exact(&mut buf).is_ok() {
        frames.push(Gray { w: wu, h: hu, px: buf.iter().map(|&v| v as f32).collect() });
        if frames.len() % 15 == 0 {
            progress((frames.len() as f64 / fps / (end - start)).min(0.9) * 0.8);
        }
    }
    let status = cancel.detach().map(|mut c| c.wait());
    let times = times_thread.join().unwrap_or_default();
    if cancel.is_canceled() {
        return Err(RunError::Canceled);
    }
    if !matches!(status, Some(Ok(s)) if s.success()) || frames.is_empty() {
        return Err(RunError::Failed("ffmpeg could not read the video to follow the box.".into()));
    }
    let times: Vec<f64> = if times.len() == frames.len() { times.iter().map(|t| start + t).collect() } else { (0..frames.len()).map(|i| start + i as f64 / fps).collect() };
    let b = |v: u32| (v as f64 * k).round() as usize;
    let path = follow(&frames, (b(bx.x), b(bx.y), b(bx.w), b(bx.h)));
    progress(0.95);
    // take out the frame-to-frame noise (about a fifth of a second); the app
    // makes a steady camera path from this (zoom.ts camera), then back to
    // source pixels (centre)
    let half = ((fps * 0.1).round() as usize).max(1);
    let xs = smooth(&path.iter().map(|p| p.0).collect::<Vec<_>>(), half);
    let ys = smooth(&path.iter().map(|p| p.1).collect::<Vec<_>>(), half);
    let (bw, bh) = (b(bx.w).max(4) as f64, b(bx.h).max(4) as f64);
    Ok(times
        .iter()
        .enumerate()
        .map(|(i, t)| TrackPoint {
            t: *t,
            x: (xs[i] as f64 + bw / 2.0) / k,
            y: (ys[i] as f64 + bh / 2.0) / k,
            w: bw / k,
            h: bh / k,
            score: path[i].2 as f64,
        })
        .collect())
}

/// Where the box is at time `t` (source display pixels, its centre and size) and how well it matched.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TrackPoint {
    pub t: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a noisy background with a bright textured square at (x, y)
    fn frame(x: usize, y: usize) -> Gray {
        let (w, h) = (120, 80);
        let mut px: Vec<f32> = (0..w * h).map(|i| ((i * 7919) % 97) as f32).collect();
        for j in 0..16 {
            for i in 0..16 {
                // an irregular texture (a slanted ramp would match itself shifted)
                let n = (i * 73 + j * 151 + i * j * 37) % 101;
                px[(y + j) * w + x + i] = 150.0 + n as f32;
            }
        }
        Gray { w, h, px }
    }

    #[test]
    fn follows_a_moving_square() {
        let frames: Vec<Gray> = (0..30).map(|k| frame(10 + k * 2, 20 + k)).collect();
        let path = follow(&frames, (10, 20, 16, 16));
        assert_eq!(path.len(), 30);
        for (k, p) in path.iter().enumerate() {
            let (ex, ey) = ((10 + k * 2) as f32, (20 + k) as f32);
            assert!((p.0 - ex).abs() <= 0.5 && (p.1 - ey).abs() <= 0.5, "frame {k}: {:?}", p);
        }
    }

    #[test]
    fn holds_when_the_box_is_lost() {
        let mut frames: Vec<Gray> = (0..5).map(|k| frame(20 + k, 20)).collect();
        // the square is gone: plain noise
        let (w, h) = (120, 80);
        frames.push(Gray { w, h, px: (0..w * h).map(|i| ((i * 31) % 13) as f32).collect() });
        let path = follow(&frames, (20, 20, 16, 16));
        let last = path[path.len() - 1];
        assert_eq!((last.0, last.1), (24.0, 20.0), "kept the last position (match {})", last.2);
    }

    #[test]
    fn smooths_jitter() {
        let v = [0.0, 10.0, 0.0, 10.0, 0.0];
        assert_eq!(smooth(&v, 1), vec![0.0, 10.0 / 3.0, 20.0 / 3.0, 10.0 / 3.0, 0.0]);
    }
}
