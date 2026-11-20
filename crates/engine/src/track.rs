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
pub fn follow(frames: &[Gray], b: (usize, usize, usize, usize), mut progress: impl FnMut(f64)) -> Vec<(f32, f32, f32)> {
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
    for (n, f) in frames[1..].iter().enumerate() {
        if n % 10 == 0 {
            progress(n as f64 / frames.len() as f64);
        }
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

/// The range `start`..`end` of the video as grey frames of `size`, and each
/// frame's time (source seconds). `progress` gets 0..1 of the reading.
pub fn read_gray(ffmpeg: &Path, info: &MediaInfo, start: f64, end: f64, size: (u32, u32), cancel: &Cancel, mut progress: impl FnMut(f64)) -> Result<(Vec<Gray>, Vec<f64>), RunError> {
    let (w, h) = size;
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
            progress((frames.len() as f64 / fps / (end - start)).min(1.0));
        }
    }
    let status = cancel.detach().map(|mut c| c.wait());
    let times = times_thread.join().unwrap_or_default();
    if cancel.is_canceled() {
        return Err(RunError::Canceled);
    }
    if !matches!(status, Some(Ok(s)) if s.success()) || frames.is_empty() {
        return Err(RunError::Failed("ffmpeg could not read the video to follow it.".into()));
    }
    let times: Vec<f64> = if times.len() == frames.len() { times.iter().map(|t| start + t).collect() } else { (0..frames.len()).map(|i| start + i as f64 / fps).collect() };
    Ok((frames, times))
}

/// Long side of the frames points are followed on: points (an eye, a corner) are small.
const POINT_SIDE: u32 = 960;
/// Side of the patch around each point, in those frames' pixels (odd).
const PATCH: usize = 33;
/// How far a point is looked for around where it should be (its last position plus its last move).
const REACH: isize = 24;

/// `g` at half size, each pixel the mean of four (the coarse level of the point search).
pub fn half_size(g: &Gray) -> Gray {
    let (w, h) = ((g.w / 2).max(1), (g.h / 2).max(1));
    let mut px = Vec::with_capacity(w * h);
    for j in 0..h {
        for i in 0..w {
            let (x, y) = ((2 * i).min(g.w - 1), (2 * j).min(g.h - 1));
            let (x1, y1) = ((x + 1).min(g.w - 1), (y + 1).min(g.h - 1));
            px.push((g.at(x, y) + g.at(x1, y) + g.at(x, y1) + g.at(x1, y1)) / 4.0);
        }
    }
    Gray { w, h, px }
}

/// `g` resized to `w` x `h` (bilinear).
fn resize(g: &Gray, w: usize, h: usize) -> Gray {
    let mut px = Vec::with_capacity(w * h);
    let (kx, ky) = (g.w as f32 / w as f32, g.h as f32 / h as f32);
    for j in 0..h {
        let sy = ((j as f32 + 0.5) * ky - 0.5).clamp(0.0, (g.h - 1) as f32);
        let (y0, fy) = (sy.floor() as usize, sy.fract());
        let y1 = (y0 + 1).min(g.h - 1);
        for i in 0..w {
            let sx = ((i as f32 + 0.5) * kx - 0.5).clamp(0.0, (g.w - 1) as f32);
            let (x0, fx) = (sx.floor() as usize, sx.fract());
            let x1 = (x0 + 1).min(g.w - 1);
            let top = g.at(x0, y0) * (1.0 - fx) + g.at(x1, y0) * fx;
            let bot = g.at(x0, y1) * (1.0 - fx) + g.at(x1, y1) * fx;
            px.push(top * (1.0 - fy) + bot * fy);
        }
    }
    Gray { w, h, px }
}

/// A point followed frame by frame: a patch around it, matched at three sizes
/// (a face coming closer grows) around where it should be (its last place plus
/// its last move), between pixels at the best spot.
pub struct PointTrack {
    /// the patch as it looks now, at its size on the first frame
    patch: Gray,
    /// its size now against the first frame
    scale: f32,
    /// the point's place (frames' pixels) and its last move
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

impl PointTrack {
    pub fn new(first: &Gray, x: f32, y: f32) -> Self {
        // the point is at the patch's middle: its top-left corner plus half its side
        let half = PATCH as f32 / 2.0;
        let x0 = (x - half).round().clamp(0.0, (first.w - PATCH) as f32) as usize;
        let y0 = (y - half).round().clamp(0.0, (first.h - PATCH) as f32) as usize;
        PointTrack { patch: crop(first, x0, y0, PATCH, PATCH), scale: 1.0, x, y, vx: 0.0, vy: 0.0 }
    }

    pub fn pos(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    /// Puts the point at (x, y) without learning from this frame (it was found some other way).
    pub fn place(&mut self, x: f32, y: f32) {
        self.vx = 0.0;
        self.vy = 0.0;
        self.x = x;
        self.y = y;
    }

    /// The best match in `f` (`fh` is it at half size): (x, y, scale, match), not applied yet.
    pub fn find(&self, f: &Gray, fh: &Gray) -> (f32, f32, f32, f32) {
        let mut best = (self.x, self.y, self.scale, -2.0f32);
        for k in [0.92f32, 1.0, 1.08] {
            let sc = (self.scale * k).clamp(0.3, 4.0);
            let n = ((PATCH as f32 * sc).round() as usize).clamp(9, f.w.min(f.h) - 1);
            let t = if n == PATCH { self.patch.clone() } else { resize(&self.patch, n, n) };
            let half = n as f32 / 2.0;
            let (px, py) = ((self.x + self.vx - half).round() as isize, (self.y + self.vy - half).round() as isize);
            let (maxx, maxy) = ((f.w - n) as isize, (f.h - n) as isize);
            let reach = (REACH as f32 * sc.max(1.0)) as isize;
            // every position on the half-size copy, then around the best at full size
            let th = half_size(&t);
            let (hmx, hmy) = (fh.w as isize - th.w as isize, fh.h as isize - th.h as isize);
            let (mut sb, mut bx, mut by) = (-2f32, px.clamp(0, maxx) as usize, py.clamp(0, maxy) as usize);
            if hmx >= 0 && hmy >= 0 {
                let (hx, hy, hr) = (px / 2, py / 2, reach / 2 + 1);
                for cy in (hy - hr).clamp(0, hmy)..=(hy + hr).clamp(0, hmy) {
                    for cx in (hx - hr).clamp(0, hmx)..=(hx + hr).clamp(0, hmx) {
                        let v = ncc(fh, &th, cx as usize, cy as usize, 1);
                        if v > sb {
                            (sb, bx, by) = (v, (cx as usize * 2).min(maxx as usize), (cy as usize * 2).min(maxy as usize));
                        }
                    }
                }
            }
            let (fx, fy) = (bx, by);
            sb = -2.0;
            for cy in fy.saturating_sub(2)..=(fy + 2).min(maxy as usize) {
                for cx in fx.saturating_sub(2)..=(fx + 2).min(maxx as usize) {
                    let v = ncc(f, &t, cx, cy, 1);
                    if v > sb {
                        (sb, bx, by) = (v, cx, cy);
                    }
                }
            }
            if sb > best.3 {
                let peak = |l: f32, c: f32, r: f32| {
                    let d = l - 2.0 * c + r;
                    if d < -1e-6 { ((l - r) / (2.0 * d)).clamp(-0.5, 0.5) } else { 0.0 }
                };
                let mut sx = bx as f32;
                let mut sy = by as f32;
                if bx > 0 && bx < maxx as usize {
                    sx += peak(ncc(f, &t, bx - 1, by, 1), sb, ncc(f, &t, bx + 1, by, 1));
                }
                if by > 0 && by < maxy as usize {
                    sy += peak(ncc(f, &t, bx, by - 1, 1), sb, ncc(f, &t, bx, by + 1, 1));
                }
                best = (sx + half, sy + half, sc, sb);
            }
        }
        best
    }

    /// Takes a match from `find`: moves there and learns a little of what it looks like now.
    pub fn take(&mut self, f: &Gray, m: (f32, f32, f32, f32)) {
        let (x, y, sc, _) = m;
        self.vx = x - self.x;
        self.vy = y - self.y;
        self.x = x;
        self.y = y;
        self.scale = sc;
        let n = ((PATCH as f32 * sc).round() as usize).clamp(9, f.w.min(f.h) - 1);
        let x0 = (x - n as f32 / 2.0).round().clamp(0.0, (f.w - n) as f32) as usize;
        let y0 = (y - n as f32 / 2.0).round().clamp(0.0, (f.h - n) as f32) as usize;
        let now = crop(f, x0, y0, n, n);
        let now = if n == PATCH { now } else { resize(&now, PATCH, PATCH) };
        for (a, b) in self.patch.px.iter_mut().zip(now.px) {
            *a += (b - *a) * LEARN;
        }
    }

    /// Lost this frame (covered, blurred): it stays and stops moving.
    pub fn hold(&mut self) {
        self.vx = 0.0;
        self.vy = 0.0;
    }
}

/// Points followed together through the frames: (x, y, match) per point per
/// frame. Two points are one thing that moves, turns and comes closer as a
/// whole (two eyes): a frame where their distance changes by more than a
/// sixth or their line turns by more than 15 degrees keeps the better match
/// and puts the other where it was against it (one slid onto another feature).
pub fn follow_points(frames: &[Gray], start: &[(f32, f32)], mut progress: impl FnMut(f64)) -> Vec<Vec<(f32, f32, f32)>> {
    let Some(first) = frames.first() else { return Vec::new() };
    let mut tr: Vec<PointTrack> = start.iter().map(|&(x, y)| PointTrack::new(first, x, y)).collect();
    let mut out: Vec<Vec<(f32, f32, f32)>> = vec![start.iter().map(|&(x, y)| (x, y, 1.0)).collect()];
    for (n, f) in frames[1..].iter().enumerate() {
        if n % 10 == 0 {
            progress(n as f64 / frames.len() as f64);
        }
        let fh = half_size(f);
        let found: Vec<(f32, f32, f32, f32)> = tr.iter().map(|t| t.find(f, &fh)).collect();
        let mut row = Vec::with_capacity(tr.len());
        if tr.len() == 2 {
            let (a0, b0) = (tr[0].pos(), tr[1].pos());
            let (ma, mb) = (found[0], found[1]);
            let d0 = (b0.0 - a0.0).hypot(b0.1 - a0.1).max(1.0);
            let d1 = (mb.0 - ma.0).hypot(mb.1 - ma.1);
            let mut turn = (mb.1 - ma.1).atan2(mb.0 - ma.0) - (b0.1 - a0.1).atan2(b0.0 - a0.0);
            while turn > std::f32::consts::PI {
                turn -= 2.0 * std::f32::consts::PI;
            }
            while turn < -std::f32::consts::PI {
                turn += 2.0 * std::f32::consts::PI;
            }
            let together = (0.85..=1.18).contains(&(d1 / d0)) && turn.abs() < 15f32.to_radians();
            if together && ma.3 >= 0.5 && mb.3 >= 0.5 {
                tr[0].take(f, ma);
                tr[1].take(f, mb);
                row.push((ma.0, ma.1, ma.3));
                row.push((mb.0, mb.1, mb.3));
            } else {
                // the better match leads; the other keeps its place against it
                let (lead, other) = if ma.3 >= mb.3 { (0, 1) } else { (1, 0) };
                let m = found[lead];
                if m.3 >= 0.5 {
                    let (lx, ly) = tr[lead].pos();
                    let (ox, oy) = tr[other].pos();
                    tr[lead].take(f, m);
                    let (nx, ny) = (m.0 + (ox - lx), m.1 + (oy - ly));
                    tr[other].place(nx, ny);
                } else {
                    tr[0].hold();
                    tr[1].hold();
                }
                let (p0, p1) = (tr[0].pos(), tr[1].pos());
                row.push((p0.0, p0.1, if lead == 0 { m.3 } else { 0.0 }));
                row.push((p1.0, p1.1, if lead == 1 { m.3 } else { 0.0 }));
            }
        } else {
            for (t, m) in tr.iter_mut().zip(found) {
                if m.3 >= 0.5 {
                    t.take(f, m);
                } else {
                    t.hold();
                }
                let p = t.pos();
                row.push((p.0, p.1, m.3));
            }
        }
        out.push(row);
    }
    out
}

/// Each frame's time and the followed points there (source display pixels) with their match.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PointsFrame {
    pub t: f64,
    /// (x, y, match) per point, in the order given
    pub p: Vec<(f64, f64, f64)>,
}

/// Follows `points` (source display pixels, on the frame at `start`) to `end`.
pub fn run_points(ffmpeg: &Path, info: &MediaInfo, start: f64, end: f64, points: &[(f64, f64)], cancel: &Cancel, mut progress: impl FnMut(f64)) -> Result<Vec<PointsFrame>, RunError> {
    if info.audio_only || end <= start || points.is_empty() {
        return Err(RunError::Failed("Nothing to follow here.".into()));
    }
    let (sw, sh) = (info.video.width.max(2), info.video.height.max(2));
    let kk = (POINT_SIDE as f64 / sw.max(sh) as f64).min(1.0);
    let even = |v: f64| (((v / 2.0).round() as u32) * 2).max(PATCH as u32 + 1);
    let (w, h) = (even(sw as f64 * kk), even(sh as f64 * kk));
    let k = w as f64 / sw as f64;
    let (frames, times) = read_gray(ffmpeg, info, start, end, (w, h), cancel, |f| progress(f * 0.4))?;
    let start_px: Vec<(f32, f32)> = points.iter().map(|&(x, y)| ((x * k) as f32, (y * k) as f32)).collect();
    let rows = follow_points(&frames, &start_px, |f| progress(0.4 + f * 0.55));
    Ok(times
        .iter()
        .zip(rows)
        .map(|(&t, row)| PointsFrame { t, p: row.iter().map(|p| (p.0 as f64 / k, p.1 as f64 / k, p.2 as f64)).collect() })
        .collect())
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
    let fps = if info.video.fps > 0.0 { info.video.fps } else { 30.0 };
    // reading is the first 40%; matching the box in each frame the rest
    let (frames, times) = read_gray(ffmpeg, info, start, end, (w, h), cancel, |f| progress(f * 0.4))?;
    let b = |v: u32| (v as f64 * k).round() as usize;
    let path = follow(&frames, (b(bx.x), b(bx.y), b(bx.w), b(bx.h)), |f| progress(0.4 + f * 0.55));
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
        let path = follow(&frames, (10, 20, 16, 16), |_| {});
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
        let path = follow(&frames, (20, 20, 16, 16), |_| {});
        let last = path[path.len() - 1];
        assert_eq!((last.0, last.1), (24.0, 20.0), "kept the last position (match {})", last.2);
    }

    #[test]
    fn follows_a_point() {
        // a textured 48 px square moving 1.5 px a frame across and 0.5 down, a point in it followed
        let big = |x: usize, y: usize| {
            let (w, h) = (160, 120);
            let mut px: Vec<f32> = (0..w * h).map(|i| ((i * 7919) % 97) as f32).collect();
            for j in 0..48 {
                for i in 0..48 {
                    px[(y + j) * w + x + i] = 120.0 + ((i * 73 + j * 151 + i * j * 37) % 101) as f32;
                }
            }
            Gray { w, h, px }
        };
        let frames: Vec<Gray> = (0..20).map(|k| big(20 + (k * 3) / 2, 20 + k / 2)).collect();
        let rows = follow_points(&frames, &[(44.0, 44.0)], |_| {});
        assert_eq!(rows.len(), 20);
        for (k, p) in rows.iter().map(|r| r[0]).enumerate() {
            let (ex, ey) = ((44 + (k * 3) / 2) as f32, (44 + k / 2) as f32);
            assert!((p.0 - ex).abs() <= 1.0 && (p.1 - ey).abs() <= 1.0, "frame {k}: {:?} want {ex} {ey}", p);
        }
    }

    #[test]
    fn smooths_jitter() {
        let v = [0.0, 10.0, 0.0, 10.0, 0.0];
        assert_eq!(smooth(&v, 1), vec![0.0, 10.0 / 3.0, 20.0 / 3.0, 10.0 / 3.0, 0.0]);
    }
}
