//! Pure planning: bitrate math, output size, filter graph, ffmpeg arguments
//! and warnings. Nothing here touches the file system or spawns processes.

use crate::spec::{
    Adjust, Caption,
    Codec, Container, Crop, Effort, EncodeSpec, MediaInfo, PlanInfo, Playback, Quality, Rate, Segment, StabMethod, SubsSel,
    VideoInfo,
};

/// Paths and overrides that differ between the displayed command and a real run.
#[derive(Debug, Clone)]
pub struct Opts<'a> {
    /// output file passed to ffmpeg (the `.part` file during a run)
    pub output: &'a str,
    /// pass log prefix (`-passlogfile`, and `<prefix>-x265.log` for x265)
    pub passlog: &'a str,
    /// replaces the computed size-mode video bitrate (size-fit retries, sample preview)
    pub video_kbps: Option<f64>,
    /// value for `-threads`
    pub threads: usize,
    /// true: leave out the flags only the app needs (`-progress pipe:1` etc.)
    pub display: bool,
    /// ffmpeg has the zscale filter (needed for tonemap)
    pub zscale: bool,
    /// one pass even where the job would use two (sample preview)
    pub single_pass: bool,
    /// write the first output frame as a PNG instead of encoding (processed frame export)
    pub frame: bool,
    /// probe results of the other files named in `EncodeSpec.segment_sources`
    pub sources: Option<&'a std::collections::HashMap<String, MediaInfo>>,
}

impl<'a> Opts<'a> {
    pub fn new(output: &'a str, passlog: &'a str) -> Self {
        Opts { output, passlog, video_kbps: None, threads: cpu_threads(), display: false, zscale: true, single_pass: false, frame: false, sources: None }
    }
}

#[derive(Debug, Clone)]
pub struct Planned {
    pub out_width: u32,
    pub out_height: u32,
    pub duration_out: f64,
    /// frames per second of the output (0 for sound only)
    pub out_fps: f64,
    /// video bitrate passed to the encoder (size mode), or the cap (crf mode)
    pub video_kbps: Option<f64>,
    pub audio_kbps: f64,
    pub bits_per_pixel: Option<f64>,
    pub quality: Option<Quality>,
    pub auto_scaled: bool,
    /// one argument list per ffmpeg run, program name not included
    pub passes: Vec<Vec<String>>,
    /// runs before the passes, once per job (not again on a size-fit retry):
    /// the vid.stab analysis of each range, with the range's length for progress
    pub prepasses: Vec<(Vec<String>, f64)>,
    /// a retry may run only pass 2 and reuse the pass 1 log (libvpx, x264)
    pub reuse_pass1: bool,
    pub warnings: Vec<String>,
    /// splits, split screen: each clip's box on the canvas
    pub cells: Vec<crate::spec::Rect>,
}

impl Planned {
    pub fn info(&self) -> PlanInfo {
        PlanInfo {
            out_width: self.out_width,
            out_height: self.out_height,
            duration_out: self.duration_out,
            out_fps: (self.out_fps * 1000.0).round() / 1000.0,
            video_kbps: self.video_kbps.map(|k| k.round()),
            audio_kbps: self.audio_kbps,
            bits_per_pixel: self.bits_per_pixel.map(|b| (b * 10000.0).round() / 10000.0),
            quality: self.quality,
            auto_scaled: self.auto_scaled,
            command: self.prepasses.iter().map(|(a, _)| a).chain(&self.passes).map(|a| shell_join("ffmpeg", a)).collect::<Vec<_>>().join("\n"),
            warnings: self.warnings.clone(),
            output: None,
            cells: self.cells.clone(),
        }
    }
}

/// Number of logical CPUs, for `-threads`.
pub fn cpu_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

/// Reverse and boomerang warn above this estimate of the frames held in memory.
const REVERSE_WARN_BYTES: f64 = 1.0e9;
/// Lowest video bitrate ever passed to the encoder.
const MIN_VIDEO_KBPS: f64 = 8.0;
/// Bytes kept for a still picture (one keyframe in size mode) per pixel of the picture,
/// when the sound would otherwise fill the limit. Measured with VP9 at its lowest
/// quality: a 1920x1080 picture of pure noise took 0.82 bytes per pixel (1,651 KiB), a
/// plain 640x360 test pattern 0.19 (42 KiB). Real pictures fall between.
const STILL_BYTES_PER_PIXEL: f64 = 0.9;
/// Plus this for the empty frames after the keyframe and the container.
const STILL_MARGIN_BYTES: f64 = 32768.0;
/// The sound bitrate is not lowered below this for a still picture.
const MIN_STILL_AUDIO_KBPS: f64 = 24.0;
/// The same for Vorbis (VP8), which fails to open at 42 kbps stereo.
const MIN_VORBIS_KBPS: f64 = 48.0;

/// Size mode, first attempt: `(maxBytes * 8 / 1000 / durationOut - audioKbps) * 0.985`.
/// The 1.5% margin covers container overhead and encoder overshoot; the
/// retries in run.rs fill the rest of the limit.
pub fn size_video_kbps(max_bytes: f64, duration_out: f64, audio_kbps: f64) -> f64 {
    let total_kbps = max_bytes * 8.0 / 1000.0 / duration_out;
    (total_kbps - audio_kbps) * 0.985
}

/// Size mode retries aim the file at this share of the limit.
pub const FILL_TARGET: f64 = 0.99;
/// An output at or above this share of the limit (and not over it) ends the retries.
pub const FILL_ACCEPT: f64 = 0.98;

/// Video bitrate for the next size-mode attempt, from every earlier attempt
/// as (video kbps, output bytes), so the file lands at `FILL_TARGET` of
/// `max_bytes`.
///
/// When one earlier attempt is under the limit and one is over, the bitrate is
/// interpolated between the closest pair (file size is close to linear in the
/// bitrate over a small range). Otherwise the last attempt is scaled by
/// target/actual. Never more than twice, or less than half, the last bitrate,
/// and never outside a bracketing pair.
pub fn next_video_kbps(history: &[(f64, u64)], max_bytes: f64) -> f64 {
    let target = max_bytes * FILL_TARGET;
    let &(k, s) = history.last().expect("at least one attempt");
    let s = s as f64;
    let under = history.iter().filter(|(_, b)| (*b as f64) <= max_bytes).max_by(|a, b| a.1.cmp(&b.1)).copied();
    let over = history.iter().filter(|(_, b)| (*b as f64) > max_bytes).min_by(|a, b| a.1.cmp(&b.1)).copied();
    if let (Some((ku, su)), Some((ko, so))) = (under, over) {
        let (su, so) = (su as f64, so as f64);
        if so > su && ko > ku {
            let t = ((target - su) / (so - su)).clamp(0.0, 1.0);
            return ku + (ko - ku) * t;
        }
    }
    (k * target / s).clamp(k * 0.5, k * 2.0)
}

/// Bits per pixel below which a codec's picture visibly breaks down.
pub fn bpp_floor(codec: Codec) -> f64 {
    match codec {
        Codec::Av1 => 0.022,
        Codec::Vp9 | Codec::Hevc => 0.030,
        Codec::H264 => 0.045,
        Codec::Vp8 => 0.055,
        // GIF is palette-coded and compresses far worse than video codecs
        Codec::Gif => 1.0,
        // lossy WebP codes each frame (or changed rectangle) as a still picture
        Codec::Webp => 0.35,
        Codec::Opus | Codec::Mp3 | Codec::Aac => 1.0,
    }
}

pub fn bits_per_pixel(kbps: f64, w: u32, h: u32, fps: f64) -> f64 {
    kbps * 1000.0 / (w as f64 * h as f64 * fps)
}

/// good at 1.5x the floor or more, fair at the floor or more, poor below.
pub fn quality(bpp: f64, codec: Codec) -> Quality {
    let floor = bpp_floor(codec);
    if bpp >= floor * 1.5 {
        Quality::Good
    } else if bpp >= floor {
        Quality::Fair
    } else {
        Quality::Poor
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// crop rectangle (x, y, w, h) in source display pixels, None when it is the full frame
    pub crop: Option<(u32, u32, u32, u32)>,
    /// size after crop and rotate
    pub rot_w: u32,
    pub rot_h: u32,
    /// final, even size
    pub out_w: u32,
    pub out_h: u32,
}

fn even_down(v: f64) -> u32 {
    let v = v.round().max(2.0) as u32;
    v - v % 2
}

fn fit_factor(w: u32, h: u32, max_w: Option<u32>, max_h: Option<u32>) -> f64 {
    let mut f = 1.0f64;
    if let Some(mw) = max_w.filter(|v| *v > 0) {
        f = f.min(mw as f64 / w as f64);
    }
    if let Some(mh) = max_h.filter(|v| *v > 0) {
        f = f.min(mh as f64 / h as f64);
    }
    f
}

/// Crop clamped to the frame, rotated, then downscaled to fit max_w x max_h
/// keeping the aspect ratio (never upscaled), then rounded down to even numbers.
pub fn geometry(
    src_w: u32,
    src_h: u32,
    crop: Option<Crop>,
    rotate: u32,
    max_w: Option<u32>,
    max_h: Option<u32>,
) -> Geometry {
    let (sw, sh) = (src_w.max(2), src_h.max(2));
    let mut rect = None;
    let (mut cw, mut ch) = (sw, sh);
    if let Some(c) = crop {
        let x = (c.x.round().max(0.0) as u32).min(sw - 2);
        let y = (c.y.round().max(0.0) as u32).min(sh - 2);
        let w = (c.w.round().max(2.0) as u32).min(sw - x);
        let h = (c.h.round().max(2.0) as u32).min(sh - y);
        if (x, y, w, h) != (0, 0, sw, sh) {
            rect = Some((x, y, w, h));
            cw = w;
            ch = h;
        }
    }
    let (rw, rh) = if rotate % 180 == 90 { (ch, cw) } else { (cw, ch) };
    let f = fit_factor(rw, rh, max_w, max_h);
    Geometry { crop: rect, rot_w: rw, rot_h: rh, out_w: even_down(rw as f64 * f), out_h: even_down(rh as f64 * f) }
}

/// Heights autoScale tries below the source, largest first.
pub const LADDER: [u32; 11] = [1440, 1080, 900, 720, 600, 540, 480, 432, 360, 288, 240];

/// Auto picks a size with at least this many times the codec's floor of bits
/// per pixel (VP9: 0.03 x 3.3 = 0.10). Measured on a 4K fancam with LED
/// walls (VMAF against the source, scored at 1080x1920): at 4 MiB for 10 s the
/// sizes from 648x1152 to 1152x2048 scored 81.7 to 83.2 and all showed
/// compression, where the floor alone had called 1152x2048 (0.047) good; at
/// 0.10 the picture is a size where the blocking shows less.
pub const AUTO_TARGET: f64 = 3.3;
/// When even the smallest size does not reach the target, Auto goes no lower than
/// this short side: at 1, 2 and 4 MiB every size under it scored worse (540x960
/// 49.5, 480x854 48.4, 360x640 44.1 at 1 MiB).
pub const AUTO_MIN_SHORT: f64 = 540.0;

/// autoScale, from {source, LADDER} within max_w/max_h:
/// 1. the largest size at least AUTO_MIN_SHORT on its short side whose bits
///    per pixel reach AUTO_TARGET x the codec's floor;
/// 2. else the AUTO_MIN_SHORT size, if it reaches the floor;
/// 3. else (a very small limit) the largest size of any kind that reaches the
///    floor, or the smallest with the flag false.
/// GIF and WebP have no bitrate (the size is what meets the limit): the largest
/// that reaches the floor. The ladder applies to the shorter side, so a
/// portrait clip at "720" is 720 pixels wide.
pub fn auto_scale(g: Geometry, max_w: Option<u32>, max_h: Option<u32>, kbps: f64, fps: f64, codec: Codec) -> (Geometry, bool) {
    let (rw, rh) = (g.rot_w as f64, g.rot_h as f64);
    let short = rw.min(rh);
    let limit = fit_factor(g.rot_w, g.rot_h, max_w, max_h);
    let mut factors = vec![1.0f64];
    factors.extend(LADDER.iter().map(|h| *h as f64).filter(|h| *h < short).map(|h| h / short));
    let cands: Vec<Geometry> = factors
        .into_iter()
        .map(|f| {
            let f = f.min(limit);
            Geometry { out_w: even_down(rw * f), out_h: even_down(rh * f), ..g }
        })
        .collect();
    let bpp = |c: &Geometry| bits_per_pixel(kbps, c.out_w, c.out_h, fps);
    let floor = bpp_floor(codec);
    let last = *cands.last().unwrap_or(&g);
    if codec.is_image() {
        return cands.iter().find(|c| bpp(c) >= floor).map_or((last, false), |c| (*c, true));
    }
    // sizes at least AUTO_MIN_SHORT on the short side (all of them for a smaller source)
    let big = |c: &&Geometry| c.out_w.min(c.out_h) as f64 >= AUTO_MIN_SHORT.min(short) - 1.0;
    if let Some(c) = cands.iter().filter(big).find(|c| bpp(c) >= floor * AUTO_TARGET) {
        return (*c, true);
    }
    if let Some(c) = cands.iter().filter(big).last().filter(|c| bpp(c) >= floor) {
        return (*c, true);
    }
    // a very small limit: as small as the floor needs, as before
    cands.iter().find(|c| bpp(c) >= floor).map_or((last, false), |c| (*c, true))
}

/// Number formatting for arguments: at most 3 decimals, no trailing zeros.
pub fn fmt_num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.to_string() }
}

/// Escapes a value (a file path) for a filter option inside `-vf` or
/// `-filter_complex`. Two levels apply: the filter's own option parser
/// (`\ ' :` are special) and then the filtergraph parser (`\ ' [ ] , ;`).
/// Backslashes in Windows paths become `/` first, so `C:\a b\x.srt` becomes
/// `C\\:/a b/x.srt`.
pub fn escape_filter_value(path: &str) -> String {
    let p = if cfg!(windows) { path.replace('\\', "/") } else { path.to_string() };
    let mut lvl1 = String::new();
    for ch in p.chars() {
        if matches!(ch, '\\' | '\'' | ':') {
            lvl1.push('\\');
        }
        lvl1.push(ch);
    }
    let mut lvl2 = String::new();
    for ch in lvl1.chars() {
        if matches!(ch, '\\' | '\'' | '[' | ']' | ',' | ';') {
            lvl2.push('\\');
        }
        lvl2.push(ch);
    }
    lvl2
}

/// Escapes text for a filter option value inside a filter graph (the same
/// two levels as `escape_filter_value`, without the path slash change).
pub fn escape_filter_text(text: &str) -> String {
    let mut lvl1 = String::new();
    for ch in text.chars() {
        if matches!(ch, '\\' | '\'' | ':') {
            lvl1.push('\\');
        }
        lvl1.push(ch);
    }
    let mut lvl2 = String::new();
    for ch in lvl1.chars() {
        if matches!(ch, '\\' | '\'' | '[' | ']' | ',' | ';') {
            lvl2.push('\\');
        }
        lvl2.push(ch);
    }
    lvl2
}

/// A bold sans font file that exists on this system, for drawtext.
fn caption_font() -> Option<String> {
    [
        "C:/Windows/Fonts/arialbd.ttf",
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
    ]
    .iter()
    .find(|p| std::path::Path::new(p).is_file())
    .map(|p| escape_filter_value(p))
}

/// The filter for the watermark input and the overlay that places it, on an
/// output `out_w` x `out_h`.
/// - a corner ("tl", "tr", "bl", "br"): inset by 3% of the shorter side
/// - "custom": `x`, `y` (0..1) of the room left beside and below the image
/// - "tile": the image fitted into square cells (width x width), with `spacing`
///   times the width between them, repeated over the whole picture and centered
///
/// `angle` (degrees, clockwise) turns the image; for "tile" it turns the whole
/// grid, so the rows run diagonally, and the grid is made large enough to cover
/// the picture's diagonal.
pub fn overlay_chain(o: &crate::spec::Overlay, out_w: u32, out_h: u32) -> (String, String) {
    let w = ((out_w as f64 * o.size.clamp(2.0, 100.0) / 100.0 / 2.0).round() as u32 * 2).max(2);
    let a = o.opacity.clamp(0.0, 1.0);
    let fade = if a < 1.0 { format!(",colorchannelmixer=aa={}", fmt_num((a * 100.0).round() / 100.0)) } else { String::new() };
    let deg = if o.angle.is_finite() { (o.angle % 360.0 + 540.0) % 360.0 - 180.0 } else { 0.0 };
    let turned = deg.abs() > 0.05;
    let rad = (deg.to_radians() * 1e6).round() / 1e6;
    match o.corner.as_str() {
        "tile" => {
            let cell = ((w as f64 * (1.0 + o.spacing.clamp(0.0, 3.0))).round() as u32).max(w).max(2);
            // a turned grid has to cover the diagonal, so it is a square that wide
            let (cover_w, cover_h) = if turned {
                let d = ((out_w as f64).hypot(out_h as f64)).ceil() as u32;
                (d, d)
            } else {
                (out_w, out_h)
            };
            let cols = cover_w.div_ceil(cell) + 1;
            let rows = cover_h.div_ceil(cell) + 1;
            let x = (out_w as i64 - (cols * cell) as i64) / 2;
            let y = (out_h as i64 - (rows * cell) as i64) / 2;
            let turn = if turned { format!(",rotate=a={rad}:ow=iw:oh=ih:c=none") } else { String::new() };
            // one picture made of cols x rows copies: loop repeats the single frame, tile lays them out
            (
                format!(
                    "scale={w}:{w}:force_original_aspect_ratio=decrease,format=rgba{fade},pad={cell}:{cell}:(ow-iw)/2:(oh-ih)/2:color=black@0,loop=loop={}:size=1,tile=layout={cols}x{rows},setpts=0{turn}",
                    cols * rows - 1
                ),
                format!("overlay=x={x}:y={y}:format=auto"),
            )
        }
        c => {
            // the turned image's bounding box is what gets placed
            let turn = if turned { format!(",rotate=a={rad}:ow=rotw({rad}):oh=roth({rad}):c=none") } else { String::new() };
            let lg = format!("scale={w}:-2,format=rgba{fade}{turn}");
            if c == "custom" {
                let fx = fmt_num((o.x.clamp(0.0, 1.0) * 1000.0).round() / 1000.0);
                let fy = fmt_num((o.y.clamp(0.0, 1.0) * 1000.0).round() / 1000.0);
                return (lg, format!("overlay=x=(W-w)*{fx}:y=(H-h)*{fy}:format=auto"));
            }
            let m = (out_w.min(out_h) as f64 * 0.03).round();
            let x = if c.ends_with('l') { format!("{m}") } else { format!("W-w-{m}") };
            let y = if c.starts_with('t') { format!("{m}") } else { format!("H-h-{m}") };
            (lg, format!("overlay=x={x}:y={y}:format=auto"))
        }
    }
}

/// Splits: for each of `n` clips of a split screen, its tile (the tiles cover
/// the canvas exactly) and the box its picture goes in (the tile less half the
/// gap on each side that meets another tile). A grid has `cols` columns; tiles
/// past `n` are left empty (returned after the clips' tiles, with no box).
/// Every size is even, for 4:2:0 video.
pub fn layout_cells(l: &crate::spec::Layout, n: usize) -> Vec<(crate::spec::Rect, Option<crate::spec::Rect>)> {
    use crate::spec::{LayoutMode, Rect};
    let (cw, ch) = (l.width / 2 * 2, l.height / 2 * 2);
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    // split `total` into parts by weight; the last part takes the remainder
    let split = |total: u32, weights: &[f64]| -> Vec<(u32, u32)> {
        let sum: f64 = weights.iter().sum();
        let mut out = Vec::new();
        let mut at = 0u32;
        for (i, w) in weights.iter().enumerate() {
            let len = if i + 1 == weights.len() { total - at } else { even(total as f64 * w / sum).min(total - at) };
            out.push((at, len));
            at += len;
        }
        out
    };
    let weights: Vec<f64> = if l.weights.len() == n && l.weights.iter().all(|w| w.is_finite() && *w > 0.0) {
        l.weights.clone()
    } else {
        vec![1.0; n]
    };
    let tiles: Vec<Rect> = match l.mode {
        LayoutMode::Columns => split(cw, &weights).into_iter().map(|(x, w)| Rect { x, y: 0, w, h: ch }).collect(),
        LayoutMode::Rows => split(ch, &weights).into_iter().map(|(y, h)| Rect { x: 0, y, w: cw, h }).collect(),
        LayoutMode::Grid => {
            let cols = if l.columns > 0 { l.columns as usize } else { (n as f64).sqrt().ceil() as usize }.max(1);
            let rows = n.div_ceil(cols).max(1);
            let xs = split(cw, &vec![1.0; cols]);
            let ys = split(ch, &vec![1.0; rows]);
            (0..cols * rows).map(|k| Rect { x: xs[k % cols].0, y: ys[k / cols].0, w: xs[k % cols].1, h: ys[k / cols].1 }).collect()
        }
        LayoutMode::Sequence => vec![Rect { x: 0, y: 0, w: cw, h: ch }],
    };
    let half = (l.gap / 4) * 2; // half the gap, even
    tiles
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            let l0 = if t.x > 0 { half } else { 0 };
            let r0 = if t.x + t.w < cw { half } else { 0 };
            let t0 = if t.y > 0 { half } else { 0 };
            let b0 = if t.y + t.h < ch { half } else { 0 };
            let inner = Rect { x: t.x + l0, y: t.y + t0, w: t.w.saturating_sub(l0 + r0).max(2), h: t.h.saturating_sub(t0 + b0).max(2) };
            (t, (i < n || l.mode == LayoutMode::Sequence).then_some(inner))
        })
        .collect()
}

/// "#rrggbb" for a filter's color option ("0xRRGGBB"); anything else is black.
fn filter_color(c: &str) -> String {
    let h = c.trim().trim_start_matches('#');
    if h.len() == 6 && h.chars().all(|ch| ch.is_ascii_hexdigit()) {
        format!("0x{}", h.to_ascii_uppercase())
    } else {
        "black".into()
    }
}

/// Split screen, length Loop: how a clip shorter than the output is repeated.
/// Either way one pass through the clip is exactly `frames` frames at the
/// layout's rate, and its sound is cut or padded to the same length, so
/// picture and sound stay together over any number of repeats.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ClipLoop {
    /// the frames are kept in memory (at the size of the clip's part) by the `loop` filter
    Frames { frames: u32 },
    /// the range is read `copies` times from the file (extra inputs) and joined:
    /// for a clip too large to keep in memory
    Inputs { frames: u32, copies: usize },
}

impl ClipLoop {
    fn frames(self) -> u32 {
        match self {
            ClipLoop::Frames { frames } | ClipLoop::Inputs { frames, .. } => frames,
        }
    }
}

/// A looping clip whose frames take more memory than this is read again from the file instead.
const LOOP_MEMORY: f64 = 512.0 * 1024.0 * 1024.0;
/// More copies than this would make the command line too long (Windows: 32767 characters).
const LOOP_MAX_COPIES: usize = 60;

/// Splits: one clip's graph fragment, `[src]` (after its own source filters,
/// in `pre`) fitted into `bx` x `by` with `fill`, at `fps`, repeated every
/// `repeat` frames, placed in its tile (`pad`: tile size, offset, color), held
/// on its last frame up to `hold` seconds and cut to `len` seconds, ending in
/// `[out]`. Labels made here start with `tag`.
#[allow(clippy::too_many_arguments)]
pub fn fit_fragment(
    src: &str,
    pre: &str,
    out: &str,
    tag: &str,
    fill: crate::spec::Fill,
    bx: u32,
    by: u32,
    fps: f64,
    repeat: Option<u32>,
    pad: Option<(crate::spec::Rect, crate::spec::Rect, &str)>,
    hold: Option<f64>,
    len: Option<f64>,
) -> String {
    use crate::spec::Fill;
    let pre = if pre.is_empty() { "null".to_string() } else { pre.to_string() };
    let mut tail = vec![format!("fps={}", fmt_num(fps)), "format=yuv420p".to_string()];
    if let Some(n) = repeat {
        // exactly n frames (one more copy of the last one covers a frame lost to rounding), then again and again
        tail.push(format!("tpad=stop_mode=clone:stop=1,trim=end_frame={n},loop=loop=-1:size={n}"));
    }
    if let Some((tile, inner, color)) = pad {
        tail.push(format!("pad={}:{}:{}:{}:color={color}", tile.w, tile.h, inner.x - tile.x, inner.y - tile.y));
    }
    if let Some(h) = hold.filter(|h| *h > 0.0) {
        tail.push(format!("tpad=stop_mode=clone:stop_duration={}", fmt_num(h)));
    }
    if let Some(l) = len {
        tail.push(format!("trim=duration={}", fmt_num(l)));
    }
    tail.push("setpts=PTS-STARTPTS".into());
    let tail = tail.join(",");
    match fill {
        Fill::Bars => format!(
            "{src}{pre},scale={bx}:{by}:force_original_aspect_ratio=decrease:force_divisible_by=2,pad={bx}:{by}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,{tail}{out};"
        ),
        Fill::Crop => format!(
            "{src}{pre},scale={bx}:{by}:force_original_aspect_ratio=increase:force_divisible_by=2,crop={bx}:{by},setsar=1,{tail}{out};"
        ),
        // the picture whole, over a blurred copy that fills the space
        Fill::Blur => format!(
            "{src}{pre},setsar=1,split[{tag}a][{tag}b];\
             [{tag}a]scale={bx}:{by}:force_original_aspect_ratio=increase:force_divisible_by=2,crop={bx}:{by},gblur=sigma=30,eq=brightness=-0.08[{tag}bg];\
             [{tag}b]scale={bx}:{by}:force_original_aspect_ratio=decrease:force_divisible_by=2[{tag}fg];\
             [{tag}bg][{tag}fg]overlay=(W-w)/2:(H-h)/2,setsar=1,{tail}{out};"
        ),
    }
}

/// The caption's own font file when it is a font file that exists.
fn caption_own_font(c: &Caption) -> Option<&str> {
    c.font
        .as_deref()
        .filter(|f| !f.trim().is_empty())
        .filter(|f| crate::fonts::is_font_file(std::path::Path::new(f)) && std::path::Path::new(f).is_file())
}

/// "#rrggbb" as drawtext's "0xRRGGBB"; anything else gives `fallback`.
fn drawtext_color(c: &str, fallback: &str) -> String {
    let h = c.trim().trim_start_matches('#');
    if h.len() == 6 && h.chars().all(|ch| ch.is_ascii_hexdigit()) {
        format!("0x{}", h.to_ascii_uppercase())
    } else {
        fallback.to_string()
    }
}

/// The drawtext filter for a caption on a picture `out_h` pixels high.
pub fn caption_filter(c: &Caption, out_h: u32) -> Option<String> {
    let text = c.text.trim_end();
    if text.trim().is_empty() {
        return None;
    }
    let size = ((out_h as f64) * c.size.clamp(2.0, 30.0) / 100.0).round().max(8.0);
    let outline = c.outline.clamp(0.0, 25.0);
    let border = if outline > 0.0 {
        let b = (size * outline / 100.0).round().max(1.0);
        format!(":borderw={b}:bordercolor={}", drawtext_color(&c.outline_color, "0x000000"))
    } else {
        String::new()
    };
    let margin = (out_h as f64 * 0.04).round();
    let frac = |v: f64| fmt_num((v.clamp(0.0, 1.0) * 1000.0).round() / 1000.0);
    let (x, y) = if c.custom {
        (format!("(w-text_w)*{}", frac(c.x)), format!("(h-text_h)*{}", frac(c.y)))
    } else if c.top {
        ("(w-text_w)/2".into(), format!("{margin}"))
    } else {
        ("(w-text_w)/2".into(), format!("h-text_h-{margin}"))
    };
    let font = caption_own_font(c)
        .map(escape_filter_value)
        .or_else(caption_font)
        .map(|f| format!("fontfile={f}:"))
        .unwrap_or_default();
    // centered lines (text_align needs ffmpeg 6.1 or newer, so only when there are several)
    let align = if text.contains('\n') { "text_align=C:" } else { "" };
    Some(format!(
        "drawtext={font}{align}text={}:expansion=none:fontsize={size}:fontcolor={}{border}:line_spacing={}:x={x}:y={y}",
        escape_filter_text(text),
        drawtext_color(&c.color, "0xFFFFFF"),
        (size * 0.15).round()
    ))
}

/// Quotes a value inside `-x265-params` (parsed with `:` between pairs).
fn quote_param(path: &str) -> String {
    let p = if cfg!(windows) { path.replace('\\', "/") } else { path.to_string() };
    format!("'{}'", p.replace('\'', "'\\''"))
}

/// `atempo` accepts 0.5..100, so slower speeds are chained.
fn atempo_chain(speed: f64) -> Vec<String> {
    let mut out = Vec::new();
    let mut s = speed;
    while s < 0.5 {
        out.push("atempo=0.5".to_string());
        s /= 0.5;
    }
    while s > 100.0 {
        out.push("atempo=100".to_string());
        s /= 100.0;
    }
    if (s - 1.0).abs() > 1e-9 {
        out.push(format!("atempo={}", fmt_num(s)));
    }
    out
}

/// Ranges in source seconds, clamped to the file. Empty = the whole file.
pub fn resolve_segments(segments: &[Segment], duration: f64) -> Result<Vec<(f64, f64)>, String> {
    if segments.is_empty() {
        return if duration > 0.0 {
            Ok(vec![(0.0, duration)])
        } else {
            Err("The file's duration is unknown. Set a range.".into())
        };
    }
    let mut out = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        if !s.start.is_finite() || !s.end.is_finite() {
            return Err(format!("Range {} has no valid start or end.", i + 1));
        }
        let start = s.start.max(0.0);
        let end = if duration > 0.0 { s.end.min(duration) } else { s.end };
        if end - start < 0.01 {
            return Err(format!("Range {}: the end must be after the start.", i + 1));
        }
        out.push((start, end));
    }
    Ok(out)
}

/// Moves range edges onto frame start times, the frames the player shows.
/// The player shows the frame whose time span holds the playhead, so a start
/// inside frame k means frame k; ffmpeg's `-ss` starts at the first frame at
/// or after the time, which began such a cut at frame k + 1. The end includes
/// a frame that starts before it (as before). Pass the start to `-ss` through
/// `seek_arg`. Variable frame rate files have no frame grid and are left as
/// they are.
pub fn snap_to_frames(segs: &mut [(f64, f64)], fps: f64, vfr: bool) {
    if vfr || !(fps.is_finite() && fps > 0.0) {
        return;
    }
    for (s, e) in segs.iter_mut() {
        let first = (*s * fps + 1e-4).floor();
        let end = (*e * fps - 1e-4).ceil().max(first + 1.0);
        *s = first / fps;
        *e = end / fps;
    }
}

/// The `-ss` value for a range start made by `snap_to_frames`: a quarter of a
/// frame early, so neither the 3-decimal argument nor a container's rounded
/// timestamps (Matroska and WebM keep milliseconds) can put the frame before it.
pub fn seek_arg(start: f64, fps: f64, vfr: bool) -> f64 {
    if vfr || !(fps.is_finite() && fps > 0.0) {
        return start;
    }
    (start - 0.25 / fps).max(0.0)
}

/// The input `-t` value for a range made by `snap_to_frames`: its length less a
/// quarter of a frame. ffmpeg counts `-t` from the first frame it keeps, not
/// from `-ss`; with millisecond timestamps that frame can be stored up to
/// 0.5 ms late and the length rounds up, which let the frame after the range
/// in (a 60 fps WebM: first frame at 13.567, -t 4.617, next frame at 18.183).
pub fn length_arg(start: f64, end: f64, fps: f64, vfr: bool) -> f64 {
    if vfr || !(fps.is_finite() && fps > 0.0) {
        return end - start;
    }
    end - start - 0.25 / fps
}

/// `eq`, `hue` and `curves` for the color adjustments; empty when all are neutral.
pub fn adjust_filters(a: &Adjust) -> Vec<String> {
    let mut f = Vec::new();
    let near = |v: f64, n: f64| (v - n).abs() < 1e-6;
    let (b, c, s, g) = (a.brightness.clamp(-1.0, 1.0), a.contrast.clamp(0.0, 3.0), a.saturation.clamp(0.0, 3.0), a.gamma.clamp(0.1, 10.0));
    if !(near(b, 0.0) && near(c, 1.0) && near(s, 1.0) && near(g, 1.0)) {
        f.push(format!("eq=brightness={}:contrast={}:saturation={}:gamma={}", fmt_num(b), fmt_num(c), fmt_num(s), fmt_num(g)));
    }
    if !near(a.hue, 0.0) {
        f.push(format!("hue=h={}", fmt_num(a.hue.clamp(-180.0, 180.0))));
    }
    let mut pts: Vec<(f64, f64)> =
        a.curve.iter().filter(|(x, y)| x.is_finite() && y.is_finite()).map(|(x, y)| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))).collect();
    pts.sort_by(|p, q| p.0.total_cmp(&q.0));
    pts.dedup_by(|p, q| (p.0 - q.0).abs() < 1e-4);
    let identity = pts.iter().all(|(x, y)| (x - y).abs() < 1e-4);
    if pts.len() >= 2 && !identity {
        let list: Vec<String> = pts.iter().map(|(x, y)| format!("{}/{}", fmt_num(*x), fmt_num(*y))).collect();
        f.push(format!("curves=master='{}'", list.join(" ")));
    }
    f
}

pub const TONEMAP: &str =
    "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,format=yuv420p";

/// Per-segment filters, in order: deinterlace, denoise, tonemap, (square
/// pixels), color adjustments, crop, rotate, mirror, vertical flip, subtitles.
pub struct SegmentFilters<'a> {
    pub deinterlace: bool,
    pub denoise: bool,
    /// the stabilizing filter of this range (`deshake`, or vidstabtransform with its analysis file)
    pub stabilize: Option<&'a str>,
    pub tonemap: bool,
    pub anamorphic: bool,
    pub crop: Option<(u32, u32, u32, u32)>,
    pub rotate: u32,
    pub mirror: bool,
    pub vflip: bool,
    pub adjust: Option<&'a Adjust>,
    /// (escaped value for `filename=`, optional `si=` index)
    pub subtitles: Option<&'a (String, Option<u32>)>,
    /// seconds the subtitles are shown later
    pub subs_delay: f64,
    /// `force_style` FontSize
    pub subs_font_size: Option<f64>,
}

pub fn segment_chain(sf: &SegmentFilters, start: f64) -> Vec<String> {
    // Input seeking does not always start at 0: an MPEG-2 .ts cut with
    // `-ss 1` started at 0.2 s, which froze the first frame for 0.2 s and
    // would shift burned-in subtitles by as much. Rebase every segment to 0.
    let mut f: Vec<String> = vec!["setpts=PTS-STARTPTS".into()];
    if sf.deinterlace {
        f.push("bwdif=mode=send_frame".into());
    }
    if let Some(stab) = sf.stabilize {
        // before the crop, so the borders it pulls in can be cropped away
        f.push(stab.into());
    }
    if sf.denoise {
        f.push("hqdn3d".into());
    }
    if sf.tonemap {
        f.push(TONEMAP.into());
    }
    if sf.anamorphic {
        // crop coordinates are in display pixels, so resample to square pixels first
        f.push("scale=trunc(iw*sar/2)*2:ih".into());
        f.push("setsar=1".into());
    }
    if let Some(a) = sf.adjust {
        f.extend(adjust_filters(a));
    }
    if let Some((x, y, w, h)) = sf.crop {
        f.push(format!("crop={w}:{h}:{x}:{y}"));
    }
    match (sf.rotate, sf.mirror) {
        (90, m) => {
            f.push("transpose=clock".into());
            if m {
                f.push("hflip".into());
            }
        }
        (270, m) => {
            f.push("transpose=cclock".into());
            if m {
                f.push("hflip".into());
            }
        }
        // 180 then a horizontal flip is a vertical flip
        (180, true) => f.push("vflip".into()),
        (180, false) => {
            f.push("hflip".into());
            f.push("vflip".into());
        }
        (_, true) => f.push("hflip".into()),
        _ => {}
    }
    if sf.vflip {
        f.push("vflip".into());
    }
    if let Some((file, si)) = sf.subtitles {
        // Input seeking makes each segment start at 0. The subtitles filter
        // needs the source times, so shift up, render, and shift back.
        // a delay moves the frames' times back, so a later cue meets them
        let at = start - sf.subs_delay;
        let shift = at.abs() > 1e-9;
        if shift {
            f.push(format!("setpts=PTS{}{}/TB", if at < 0.0 { "-" } else { "+" }, fmt_num(at.abs())));
        }
        let mut s = format!("subtitles=filename={file}");
        if let Some(si) = si {
            s.push_str(&format!(":si={si}"));
        }
        if let Some(fs) = sf.subs_font_size.filter(|v| v.is_finite() && *v > 0.0) {
            // the value is quoted for the filter level: force_style='FontSize=24'
            s.push_str(&format!(":force_style=FontSize={}", fmt_num(fs.round())));
        }
        f.push(s);
        if shift {
            f.push("setpts=PTS-STARTPTS".into());
        }
    }
    f
}

/// Filters after the join: scale, speed, fps, reverse or boomerang, fade,
/// then the color tags. May contain `;` (boomerang), so it is used as the
/// tail of a chain.
pub struct PostFilters {
    pub scale: Option<(u32, u32)>,
    /// a zoompan filter (zoom.rs) that takes the place of the scale
    pub zoom: Option<String>,
    /// a complete drawtext filter, drawn after the scale
    pub caption: Option<String>,
    /// make in-between frames up to this rate after the speed change (replaces `fps`)
    pub interpolate: Option<f64>,
    pub speed: f64,
    /// a speed ramp's `setpts=...` (replaces `speed`)
    pub ramp: Option<String>,
    pub fps: Option<f64>,
    pub playback: Playback,
    pub fade_in: f64,
    pub fade_out: f64,
    pub duration_out: f64,
    /// (colorspace, primaries, transfer)
    pub color: (String, String, String),
}

pub fn video_post(p: &PostFilters) -> String {
    let mut lin: Vec<String> = Vec::new();
    if let Some(z) = &p.zoom {
        lin.push(format!("{z},setsar=1"));
    } else if let Some((w, h)) = p.scale {
        // Square pixels at exactly w x h. When the picture's shape differs a
        // little from w:h (an odd crop such as 2160x1215 is cut to 2160x1214
        // for 4:2:0), scale keeps the shape with a sample aspect ratio
        // (1025:1024 there), and players and sites then see 2050 pixels
        // where 2048 were planned.
        lin.push(format!("scale={w}:{h}:flags=lanczos,setsar=1"));
    }
    if let Some(c) = &p.caption {
        lin.push(c.clone());
    }
    if let Some(r) = &p.ramp {
        lin.push(r.clone());
    } else if (p.speed - 1.0).abs() > 1e-9 {
        lin.push(format!("setpts=PTS/{}", fmt_num(p.speed)));
    }
    if let Some(f) = p.interpolate {
        lin.push(format!("minterpolate=fps={}:mi_mode=mci:mc_mode=aobmc:me_mode=bidir:vsbmc=1", fmt_num(f)));
    } else if let Some(fps) = p.fps {
        lin.push(format!("fps={}", fmt_num(fps)));
    }
    match p.playback {
        Playback::Forward => {}
        Playback::Reverse => lin.push("reverse".into()),
        Playback::Boomerang => lin.push("split[bf][bb];[bb]reverse[br];[bf][br]concat=n=2:v=1:a=0".into()),
    }
    if p.fade_in > 0.0 {
        lin.push(format!("fade=t=in:st=0:d={}", fmt_num(p.fade_in.min(p.duration_out))));
    }
    if p.fade_out > 0.0 {
        let d = p.fade_out.min(p.duration_out);
        lin.push(format!("fade=t=out:st={}:d={}", fmt_num(p.duration_out - d), fmt_num(d)));
    }
    let (space, prim, trc) = &p.color;
    // retag without converting, so the encoder's tags below match the frames
    lin.push(format!("setparams=colorspace={space}:color_primaries={prim}:color_trc={trc}"));
    lin.join(",")
}

/// Audio after the join: speed, reverse, loudness, volume, fade.
pub fn audio_post(speed: f64, reverse: bool, normalize: bool, volume_db: f64, fade_in: f64, fade_out: f64, duration_out: f64) -> String {
    let mut f = atempo_chain(speed);
    if reverse {
        f.push("areverse".into());
        // areverse keeps each frame's time, so they run backwards; with a speed ramp's
        // pieces the muxer then dropped all but the first 0.27 s. Count them afresh.
        f.push("asetpts=N/SR/TB".into());
    }
    if normalize {
        f.push("loudnorm=I=-16:TP=-1.5:LRA=11".into());
        // loudnorm outputs 192 kHz
        f.push("aresample=48000".into());
    }
    if volume_db.is_finite() && volume_db.abs() > 1e-9 {
        f.push(format!("volume={}dB", fmt_num(volume_db)));
    }
    if fade_in > 0.0 {
        f.push(format!("afade=t=in:st=0:d={}", fmt_num(fade_in.min(duration_out))));
    }
    if fade_out > 0.0 {
        let d = fade_out.min(duration_out);
        f.push(format!("afade=t=out:st={}:d={}", fmt_num(duration_out - d), fmt_num(d)));
    }
    f.join(",")
}

/// Output color tags. Tone-mapped output is bt709. Otherwise the source's
/// tags where known, else bt709 for HD and smpte170m (bt601) for SD.
pub fn color_tags(v: &VideoInfo, tonemapped: bool) -> (String, String, String) {
    let hd = v.width >= 1280 || v.height >= 720;
    let default = if hd || tonemapped { "bt709" } else { "smpte170m" };
    if tonemapped {
        return (default.into(), default.into(), default.into());
    }
    // "gbr" is an RGB source; the output is YUV
    let space = v.color_space.clone().filter(|s| s != "gbr").unwrap_or_else(|| default.into());
    let prim = v.color_primaries.clone().unwrap_or_else(|| default.into());
    let trc = v.color_trc.clone().unwrap_or_else(|| default.into());
    (space, prim, trc)
}

fn codec_label(c: Codec) -> &'static str {
    match c {
        Codec::Vp9 => "VP9",
        Codec::Vp8 => "VP8",
        Codec::Av1 => "AV1",
        Codec::H264 => "H.264",
        Codec::Hevc => "HEVC",
        Codec::Gif => "GIF",
        Codec::Webp => "WebP",
        Codec::Opus => "Opus",
        Codec::Mp3 => "MP3",
        Codec::Aac => "AAC",
    }
}

fn effort_index(e: Effort) -> usize {
    match e {
        Effort::Fast => 0,
        Effort::Balanced => 1,
        Effort::Best => 2,
    }
}

fn push(args: &mut Vec<String>, items: &[&str]) {
    args.extend(items.iter().map(|s| s.to_string()));
}

/// The picture of a video made from a sound file: an extra input, repeated
/// for the output's length.
struct Still {
    /// the chosen image, or the sound file itself for its embedded cover
    path: String,
    /// stream of that input (0 for an image file, the attached picture's index otherwise)
    stream: u32,
}

/// Frame rate of a still picture, as a number and as an ffmpeg rate. The
/// picture never changes, so about 1 frame per second is enough; with a fade
/// about 10, so the fade has steps. The rate is set so whole frames end with
/// the sound: at exactly 1 fps a 4.5 s clip would get a 5 s picture.
fn still_rate(duration_out: f64, fade: bool) -> (f64, String) {
    let base = if fade { 10.0 } else { 1.0 };
    let n = (duration_out * base - 1e-6).ceil().max(1.0);
    if duration_out <= 0.0 || (n / duration_out - base).abs() < 1e-6 {
        (base, fmt_num(base))
    } else {
        (n / duration_out, format!("{}/{}", fmt_num(n), fmt_num(duration_out)))
    }
}

/// Seconds between keyframes of a still picture. At 1 fps the encoders' own
/// defaults are 12 frames (libvpx) to 250 frames (x264), so a seek could
/// start minutes back. Each keyframe repeats the whole picture, so they are
/// not made more often.
const STILL_GOP_SECS: f64 = 10.0;

pub fn build(spec: &EncodeSpec, info: &MediaInfo, opts: &Opts) -> Result<Planned, String> {
    let layout = spec.layout.as_ref();
    if layout.is_some() && spec.container == Container::Audio {
        return Err("Splits make a picture: choose a video format (WebM, MP4, GIF or WebP).".into());
    }
    if spec.container == Container::Audio {
        return build_audio(spec, info, opts);
    }
    if spec.stream_copy && layout.is_none() {
        return build_copy(spec, info, opts);
    }
    let mut warnings = Vec::new();
    if let Some(l) = layout {
        if spec.segments.len() < 2 {
            return Err("Splits need at least two clips.".into());
        }
        if l.width < 16 || l.height < 16 || l.width > 8192 || l.height > 8192 {
            return Err(format!("The picture size {}x{} is not usable.", l.width, l.height));
        }
        if !l.fps.is_finite() || l.fps <= 0.0 || l.fps > 240.0 {
            return Err("The frame rate of the splits must be between 0 and 240.".into());
        }
        if spec.stream_copy {
            warnings.push("Cut only (no re-encode) does not apply to splits; they are encoded.".into());
        }
        if spec.crop.is_some() || spec.rotate != 0 || spec.mirror || spec.vflip {
            warnings.push("Crop, rotate and flip apply to one file and are not used with splits.".into());
        }
        if spec.subs.is_some() {
            warnings.push("Subtitles are not burned in on splits.".into());
        }
    }
    // a sound file becomes a video with a still picture: the chosen one, else the embedded cover
    if layout.is_some() && info.audio_only {
        return Err(format!("{} has no video stream, so it cannot be a clip of the splits.", info.file_name));
    }
    let still = if info.audio_only {
        let chosen = spec.cover.as_deref().map(str::trim).filter(|p| !p.is_empty());
        let (path, stream, size) = match (chosen, spec.cover_size) {
            (Some(p), Some((w, h))) if w > 0 && h > 0 => (p.to_string(), 0, (w, h)),
            (Some(_), _) => return Err("The picture's size is not known. Choose the picture again (Basic > Picture).".into()),
            (None, _) if info.video.width > 0 && info.video.height > 0 => {
                (spec.input.clone(), info.video.stream_index, (info.video.width, info.video.height))
            }
            (None, _) => return Err("Choose a picture for the video (Basic > Picture).".into()),
        };
        Some((Still { path, stream }, VideoInfo { codec: "png".into(), width: size.0, height: size.1, fps: 1.0, ..VideoInfo::default() }))
    } else {
        None
    };
    // splits: the canvas stands in for the source picture from here on
    let canvas = layout.map(|l| VideoInfo { width: l.width / 2 * 2, height: l.height / 2 * 2, fps: l.fps, ..VideoInfo::default() });
    let v = canvas.as_ref().or(still.as_ref().map(|(_, cv)| cv)).unwrap_or(&info.video);
    let still = still.as_ref().map(|(s, _)| s);
    let codec = spec.codec;

    match (spec.container, codec) {
        (Container::Webm, Codec::Vp9 | Codec::Vp8 | Codec::Av1)
        | (Container::Mp4, Codec::H264 | Codec::Hevc | Codec::Av1)
        | (Container::Gif, Codec::Gif)
        | (Container::Webp, Codec::Webp) => {}
        (Container::Gif, _) | (_, Codec::Gif) => return Err("GIF output needs the GIF format and codec together.".into()),
        (Container::Webp, _) | (_, Codec::Webp) => return Err("WebP output needs the WebP format and codec together.".into()),
        (_, c) if c.is_audio() => return Err(format!("{} is for audio-only output. Choose the Audio format.", codec_label(c))),
        (Container::Audio, _) => unreachable!("audio is planned by build_audio"),
        (Container::Webm, c) => return Err(format!("{} cannot be stored in WebM. Choose MP4.", codec_label(c))),
        (Container::Mp4, c) => return Err(format!("{} cannot be stored in MP4. Choose WebM.", codec_label(c))),
    }
    if !matches!(spec.rotate, 0 | 90 | 180 | 270) {
        return Err("Rotate must be 0, 90, 180 or 270.".into());
    }
    if !spec.speed.is_finite() || spec.speed <= 0.0 {
        return Err("Speed must be greater than 0.".into());
    }

    // --- cut ---
    // the file of each range: None = `input`, Some = another file (joined)
    let seg_src: Vec<Option<&MediaInfo>> = if spec.segments.is_empty() {
        vec![None]
    } else {
        (0..spec.segments.len())
            .map(|i| match spec.segment_sources.get(i).and_then(|s| s.as_deref()).filter(|p| *p != spec.input) {
                None => Ok(None),
                Some(p) => opts.sources.and_then(|m| m.get(p)).map(Some).ok_or(format!("{p} was not read. Open it again.")),
            })
            .collect::<Result<_, String>>()?
    };
    // splits: every clip is treated as a file of its own, the first one too
    let mixed = layout.is_some() || seg_src.iter().any(|s| s.is_some());
    // split screen (the clips at the same time), not one after another
    let split = layout.filter(|l| l.mode != crate::spec::LayoutMode::Sequence);
    let clip_info = |i: usize| -> &MediaInfo { seg_src.get(i).copied().flatten().unwrap_or(info) };
    if let (None, Some(m)) = (still, seg_src.iter().flatten().find(|m| m.audio_only)) {
        return Err(format!("{} has no video stream, so it cannot be joined to a video.", m.file_name));
    }
    let segs = if mixed {
        let mut out = Vec::new();
        for (i, g) in spec.segments.iter().enumerate() {
            // a picture has no length of its own: it lasts as long as its range says
            let m = clip_info(i);
            let d = if m.still { 0.0 } else { m.duration };
            let mut r = resolve_segments(std::slice::from_ref(g), d).map_err(|e| e.replace("Range 1", &format!("Range {}", i + 1)))?;
            if !m.still {
                snap_to_frames(&mut r, m.video.fps, m.video.vfr);
            }
            out.extend(r);
        }
        out
    } else {
        let mut r = resolve_segments(&spec.segments, info.duration)?;
        if still.is_none() {
            snap_to_frames(&mut r, info.video.fps, info.video.vfr);
        }
        r
    };
    let lens: Vec<f64> = segs.iter().map(|(s, e)| e - s).collect();
    let clip: f64 = match split.map(|l| l.length) {
        Some(crate::spec::SplitLength::Shortest) => lens.iter().cloned().fold(f64::INFINITY, f64::min),
        Some(crate::spec::SplitLength::Longest | crate::spec::SplitLength::Loop) => lens.iter().cloned().fold(0.0, f64::max),
        None => lens.iter().sum(),
    };
    // a speed ramp: points in the file time of `input`, so only for ranges of it
    let ramp = if spec.speed_ramp.is_empty() || still.is_some() {
        None
    } else if mixed {
        warnings.push("The speed ramp applies to one file, so it is not used when ranges come from other files.".into());
        None
    } else {
        crate::ramp::Ramp::new(&spec.speed_ramp, &segs)
    };
    // with a ramp the constant speed is not used
    let speed = if ramp.is_some() { 1.0 } else { spec.speed };
    // a still picture has nothing to play backwards; the sound plays forward
    let boomerang = spec.playback == Playback::Boomerang && still.is_none();
    if still.is_some() && spec.playback == Playback::Boomerang {
        warnings.push("Boomerang applies to the picture only; the sound plays forward.".into());
    }
    // one way through, before boomerang doubles it
    let forward_out = ramp.as_ref().map_or(clip / speed, |r| r.duration());
    let duration_out = forward_out * if boomerang { 2.0 } else { 1.0 };

    // splits: every clip is brought to the layout's rate first
    let base_fps = layout.map_or(v.fps, |l| l.fps);
    let src_fps_out = base_fps * speed;
    let fps_cap = spec
        .fps
        .filter(|f| f.is_finite() && *f > 0.0)
        .filter(|f| src_fps_out <= 0.0 || *f < src_fps_out - 0.01)
        .filter(|_| still.is_none());
    // GIF frame delays are whole centiseconds and players treat under 2 as
    // slow, so a GIF is capped at 50 fps
    let fps_cap = if codec.is_image() && fps_cap.unwrap_or(src_fps_out) > 50.0 { Some(50.0) } else { fps_cap };
    // interpolation (slow motion): frames are made up to the source rate (or the cap)
    let slowest = ramp.as_ref().map_or(speed, |r| r.min_speed());
    let interpolate = (spec.smooth_slowmo && slowest < 1.0 - 1e-9 && base_fps > 0.0 && !codec.is_image() && still.is_none())
        .then(|| fps_cap.map_or(base_fps, |c| c.min(base_fps)));
    let still_rate = still.map(|_| still_rate(duration_out, spec.fade_in > 0.0 || spec.fade_out > 0.0));
    let out_fps = match &still_rate {
        Some((fps, _)) => *fps,
        None => interpolate.unwrap_or(fps_cap.unwrap_or(if src_fps_out > 0.0 { src_fps_out } else { 30.0 })),
    };
    if interpolate.is_some() && duration_out > 20.0 {
        warnings.push("Interpolation computes every new frame and is slow: expect minutes per second of output.".into());
    }
    if codec.is_image() && out_fps > 25.0 {
        warnings.push(format!(
            "An animated {} at {} fps is large. 10 to 20 fps is usual; set a frame rate cap.",
            codec_label(codec),
            out_fps.round()
        ));
    }

    // --- audio ---
    let gif = codec == Codec::Gif;
    let image = codec.is_image();
    let audio = match spec.audio {
        Some(_) if image => {
            warnings.push(format!("{} has no sound, so audio is left out.", codec_label(codec)));
            None
        }
        Some(_) if boomerang => {
            warnings.push("Boomerang has no sound, so audio is left out.".into());
            None
        }
        Some(a) => {
            // with music replacing it, the source's sound is not read
            let replaced = spec.music.as_ref().filter(|m| !m.path.trim().is_empty()).is_some_and(|m| !m.mix || info.audio.is_empty());
            // splits: each clip's own track (or silence) is used, see the graph
            if !replaced && layout.is_none() && info.audio.get(a.track as usize).is_none() {
                return Err(format!("Audio track {} does not exist.", a.track));
            }
            if !a.bitrate_k.is_finite() || a.bitrate_k <= 0.0 {
                return Err("The audio bitrate must be greater than 0.".into());
            }
            Some(a)
        }
        None => None,
    };
    // a long sound at the chosen bitrate can be larger than the limit on its own: the sound
    // bitrate is lowered to leave room for the picture's one keyframe, sized from the picture
    // itself (the output can only be that size or smaller)
    let mut audio = audio;
    if let (Some(_), Rate::Size { max_bytes }, Some(a)) = (still, &spec.rate, audio.as_mut()) {
        let max_bytes = *max_bytes;
        let picture = v.width as f64 * v.height as f64 * STILL_BYTES_PER_PIXEL + STILL_MARGIN_BYTES;
        let room = (max_bytes * 0.985 - picture) * 8.0 / 1000.0 / duration_out;
        if max_bytes > 0.0 && duration_out > 0.0 && a.bitrate_k > room {
            // libvorbis (VP8) will not open below about 45 kbps for stereo
            let floor = if codec == Codec::Vp8 { MIN_VORBIS_KBPS } else { MIN_STILL_AUDIO_KBPS };
            let k = room.floor().max(floor);
            // too long only when the sound alone at the floor is over the limit
            let total = (max_bytes * 8.0 / 1000.0 / duration_out) * 0.985;
            warnings.push(if total < floor {
                format!("The sound is too long for the size limit even at {k} kbps. Shorten it or raise the limit.")
            } else {
                format!("The sound bitrate is lowered from {} to {k} kbps to fit the size limit.", a.bitrate_k)
            });
            a.bitrate_k = k;
        }
    }
    // split screen: the sound of one clip, all mixed, or none
    if let (Some(l), Some(_)) = (split, audio) {
        let n = segs.len();
        let none = |why: String, warnings: &mut Vec<String>| {
            warnings.push(why);
            None
        };
        audio = match l.sound {
            crate::spec::SplitSound::None => None,
            crate::spec::SplitSound::Clip if l.sound_clip >= n => return Err(format!("The sound is set to come from clip {}, which does not exist.", l.sound_clip + 1)),
            crate::spec::SplitSound::Clip if clip_info(l.sound_clip).audio.is_empty() => {
                none(format!("Clip {} has no sound, so the output is silent.", l.sound_clip + 1), &mut warnings)
            }
            crate::spec::SplitSound::Mix if (0..n).all(|i| clip_info(i).audio.is_empty()) => none("None of the clips has sound, so the output is silent.".into(), &mut warnings),
            _ => audio,
        };
    }
    let audio_kbps = audio.map(|a| a.bitrate_k).unwrap_or(0.0);

    // --- rate ---
    let size_mode = matches!(spec.rate, Rate::Size { .. });
    let video_kbps = match spec.rate {
        Rate::Size { max_bytes } => {
            if !max_bytes.is_finite() || max_bytes <= 0.0 {
                return Err("The size limit must be greater than 0.".into());
            }
            let computed = size_video_kbps(max_bytes, duration_out, audio_kbps);
            // a still picture is one keyframe whatever the bitrate, so these do not apply
            if computed < MIN_VIDEO_KBPS && still.is_none() {
                warnings.push(
                    "The size limit leaves almost no room for video. Shorten the clip, turn audio off, or raise the limit."
                        .into(),
                );
            }
            Some(opts.video_kbps.unwrap_or(computed).max(MIN_VIDEO_KBPS))
        }
        Rate::Crf { max_bitrate_k, .. } => max_bitrate_k.filter(|k| k.is_finite() && *k > 0.0),
    };

    // --- geometry ---
    // crop is drawn on a video frame; a still picture is used whole
    // splits: the canvas is used as it is (crop and rotation belong to one file)
    let crop = if still.is_some() || layout.is_some() { None } else { spec.crop };
    let rotate = if layout.is_some() { 0 } else { spec.rotate };
    let mut g = geometry(v.width, v.height, crop, rotate, spec.max_width, spec.max_height);
    // a follow path (zoom points): the output is the size of the window it moves (the picture over
    // the zoom), not the whole picture enlarged (a 2.89x path on 2160x3840 planned 2160x3840)
    let zoom_k = if spec.zoom.is_empty() || still.is_some() || layout.is_some() || mixed {
        1.0
    } else {
        crate::zoom::clean(&spec.zoom).iter().map(|p| p.zoom).reduce(f64::min).unwrap_or(1.0).max(1.0)
    };
    if zoom_k > 1.0 + 1e-9 {
        g.rot_w = ((g.rot_w as f64 / zoom_k).round() as u32).max(2);
        g.rot_h = ((g.rot_h as f64 / zoom_k).round() as u32).max(2);
        let f = fit_factor(g.rot_w, g.rot_h, spec.max_width, spec.max_height);
        g.out_w = even_down(g.rot_w as f64 * f);
        g.out_h = even_down(g.rot_h as f64 * f);
    }
    let mut auto_scaled = false;
    if let (true, true, Some(k)) = (size_mode, spec.auto_scale, video_kbps) {
        let (ng, ok) = auto_scale(g, spec.max_width, spec.max_height, k, out_fps, codec);
        if !ok {
            warnings.push(format!(
                "Even at {}x{} the bitrate is below what {} needs for a clean picture. Shorten the clip or raise the limit.",
                ng.out_w,
                ng.out_h,
                codec_label(codec)
            ));
        }
        auto_scaled = (ng.out_w, ng.out_h) != (g.out_w, g.out_h);
        g = ng;
    }
    if image && size_mode && !spec.auto_scale {
        warnings.push(format!(
            "{} has no bitrate setting: the size limit can only be met by the picture size. Choose Auto resolution, or the file may end over the limit.",
            codec_label(codec)
        ));
    }
    // ffmpeg's reverse keeps every frame it has been given, after the scale and the frame
    // rate change: output size x frames x 1.5 bytes (8-bit 4:2:0; more for 10-bit sources)
    if spec.playback != Playback::Forward && still.is_none() {
        let frames = (forward_out * out_fps).ceil();
        let bytes = g.out_w as f64 * g.out_h as f64 * 1.5 * frames;
        if bytes > REVERSE_WARN_BYTES {
            warnings.push(format!(
                "Reverse and boomerang keep every frame in memory: about {:.1} GB here ({frames} frames at {}x{}). Shorten the clip or lower the resolution or frame rate.",
                bytes / 1e9,
                g.out_w,
                g.out_h
            ));
        }
    }
    let bpp = video_kbps.map(|k| bits_per_pixel(k, g.out_w, g.out_h, out_fps));
    let qual = bpp.map(|b| quality(b, codec));
    if let (true, Some(k), Some(b), Some(Quality::Poor), None) = (size_mode, video_kbps, bpp, qual, still) {
        warnings.push(format!("Video bitrate is {} kbps ({b:.3} bits per pixel). Expect a blurry picture.", k.round()));
    }

    // --- tonemap ---
    let tonemap = spec.tonemap && v.hdr && {
        if !opts.zscale {
            warnings.push("Tone mapping needs the zscale filter, which this ffmpeg lacks. The picture stays HDR.".into());
        }
        opts.zscale
    };
    if v.hdr && !spec.tonemap {
        warnings.push("The source is HDR. Without tone mapping the colors look washed out on most screens.".into());
    }

    // --- subtitles ---
    let subtitles = match spec.subs.as_ref().filter(|_| layout.is_none()) {
        None => None,
        Some(SubsSel::Embedded { track }) => {
            let t = info.subs.get(*track as usize).ok_or(format!("Subtitle track {track} does not exist."))?;
            if t.text_based {
                Some((escape_filter_value(&spec.input), Some(*track)))
            } else {
                warnings.push(format!(
                    "Subtitle track {track} is a bitmap format ({}). Bitmap subtitles are not supported, so it is not burned in.",
                    t.codec
                ));
                None
            }
        }
        Some(SubsSel::File { path }) => Some((escape_filter_value(path), None)),
    };
    let subtitles = match subtitles {
        Some(_) if still.is_some() => {
            warnings.push("Subtitles are not burned in on a still picture.".into());
            None
        }
        s => s,
    };

    // --- stabilization ---
    // vid.stab looks at each range once (vidstabdetect, a pre-pass writing
    // <passlog>-stab<i>.trf) and the encode applies what it found. The
    // analysis sees the same frames as the encode up to the stabilizer: the
    // same -ss/-t and the filters before it (rebasing, deinterlacing).
    let deint = |i: usize| match (layout.is_some(), seg_src[i]) {
        (true, _) => clip_info(i).video.interlaced,
        (false, Some(m)) => spec.deinterlace && m.video.interlaced,
        (false, None) => spec.deinterlace,
    };
    let stab_base = if opts.passlog.is_empty() { "comfywebm" } else { opts.passlog };
    let stab_file = |i: usize| format!("{stab_base}-stab{i}.trf");
    let stabilized = |i: usize| spec.stabilize && still.is_none() && !clip_info(i).still;
    let stab: Vec<Option<String>> = (0..segs.len())
        .map(|i| {
            // Gyroflow works before the plan (gyro.rs): nothing in the graph
            (stabilized(i) && spec.stab_method != StabMethod::Gyroflow).then(|| match spec.stab_method {
                StabMethod::Deshake | StabMethod::Gyroflow => "deshake".to_string(),
                StabMethod::Vidstab => {
                    let fps = clip_info(i).video.fps;
                    // a second of frames on each side of a frame: vid.stab before 1.4 left
                    // 3.7 of a test clip's 9.4 shake at half a second, 0.8 at a second
                    let smoothing = (if fps > 0.0 { fps } else { 30.0 }).round().max(1.0);
                    // copy: ffmpeg 8.1 hands vidstabtransform frames that are sometimes
                    // read-only, and vid.stab before 1.4 then writes into an earlier frame's
                    // memory (FFmpeg ticket 22595, fixed after 8.1); a copy is always writable
                    format!(
                        "copy,vidstabtransform=input={}:smoothing={}:optzoom=1:crop=black:interpol=bicubic",
                        escape_filter_value(&stab_file(i)),
                        fmt_num(smoothing)
                    )
                }
            })
        })
        .collect();
    let mut prepasses: Vec<(Vec<String>, f64)> = Vec::new();
    if spec.stab_method == StabMethod::Vidstab {
        for (i, (s, e)) in segs.iter().enumerate() {
            if !stabilized(i) {
                continue;
            }
            let m = clip_info(i);
            let path = seg_src[i].map_or(spec.input.as_str(), |m| m.path.as_str());
            let detect = format!("vidstabdetect=shakiness=6:accuracy=15:result={}", escape_filter_value(&stab_file(i)));
            let chain = segment_chain(
                &SegmentFilters {
                    deinterlace: deint(i),
                    denoise: false,
                    stabilize: Some(&detect),
                    tonemap: false,
                    anamorphic: false,
                    crop: None,
                    rotate: 0,
                    mirror: false,
                    vflip: false,
                    adjust: None,
                    subtitles: None,
                    subs_delay: 0.0,
                    subs_font_size: None,
                },
                *s,
            );
            let mut a = Vec::new();
            push(&mut a, &["-hide_banner"]);
            if !opts.display {
                push(&mut a, &["-nostdin", "-loglevel", "error", "-progress", "pipe:1", "-nostats"]);
            }
            push(&mut a, &["-y"]);
            if *s > 0.0 {
                push(&mut a, &["-ss", &fmt_num(seek_arg(*s, m.video.fps, m.video.vfr))]);
            }
            push(&mut a, &["-t", &fmt_num(length_arg(*s, *e, m.video.fps, m.video.vfr)), "-i", path]);
            push(&mut a, &["-map", &format!("0:{}", m.video.stream_index), "-an", "-sn", "-dn", "-vf", &chain.join(","), "-f", "null", "-"]);
            prepasses.push((a, e - s));
        }
    }

    // --- filter graph ---
    let sf = SegmentFilters {
        deinterlace: spec.deinterlace,
        denoise: spec.denoise,
        stabilize: None,
        tonemap,
        anamorphic: v.anamorphic,
        crop: g.crop,
        rotate: spec.rotate,
        mirror: spec.mirror,
        vflip: spec.vflip,
        adjust: spec.adjust.as_ref(),
        subtitles: subtitles.as_ref(),
        subs_delay: if spec.subs_delay.is_finite() { spec.subs_delay } else { 0.0 },
        subs_font_size: spec.subs_font_size,
    };
    // Joined files: each keeps its own source filters (no crop or subtitles,
    // which belong to `input`), then every range is fitted into the main
    // file's frame (after crop and rotation) at its frame rate.
    let fit = format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={},format=yuv420p",
        fmt_num(if v.fps > 0.0 { v.fps } else { 30.0 }),
        w = g.rot_w,
        h = g.rot_h
    );
    let seg_chains: Vec<String> = segs
        .iter()
        .enumerate()
        .map(|(i, (s, _))| {
            let mut c = match seg_src[i] {
                None => segment_chain(&SegmentFilters { stabilize: stab[i].as_deref(), ..sf }, *s),
                Some(m) => segment_chain(
                    &SegmentFilters {
                        stabilize: stab[i].as_deref(),
                        deinterlace: spec.deinterlace && m.video.interlaced,
                        tonemap: spec.tonemap && m.video.hdr && opts.zscale,
                        anamorphic: m.video.anamorphic,
                        crop: None,
                        subtitles: None,
                        // rotation and flips were set for the first file's picture
                        rotate: 0,
                        mirror: false,
                        vflip: false,
                        ..sf
                    },
                    *s,
                ),
            };
            if mixed {
                c.push(fit.clone());
            }
            c.join(",")
        })
        .collect();
    if mixed && spec.subs.is_some() && layout.is_none() {
        warnings.push("Subtitles are burned in only on the ranges from the first file.".into());
    }
    // splits: one fragment per clip, from its input to [v{i}]: its own source
    // filters (deinterlace and tone mapping as that file needs), then fitted to
    // the canvas (one after another) or to its part of it (split screen)
    let cells = layout.map(|l| layout_cells(l, segs.len())).unwrap_or_default();
    let gap_color = layout.map(|l| filter_color(&l.gap_color)).unwrap_or_default();
    // split screen, Loop: the clips shorter than the output start again
    let loops: Vec<Option<ClipLoop>> = (0..segs.len())
        .map(|i| {
            let l = split.filter(|l| l.length == crate::spec::SplitLength::Loop)?;
            if clip_info(i).still || lens[i] <= 0.0 || lens[i] >= clip - 1e-3 {
                return None;
            }
            let frames = ((lens[i] * l.fps).round() as u32).max(1);
            let copies = (clip / lens[i]).ceil() as usize;
            let (w, h) = cells.get(i).and_then(|c| c.1).map_or((l.width, l.height), |r| (r.w, r.h));
            // yuv420p: 1.5 bytes a pixel
            let bytes = w as f64 * h as f64 * 1.5 * frames as f64;
            Some(if bytes <= LOOP_MEMORY || copies > LOOP_MAX_COPIES { ClipLoop::Frames { frames } } else { ClipLoop::Inputs { frames, copies } })
        })
        .collect();
    // the extra inputs of ClipLoop::Inputs come after all the others (the
    // watermark, music and still picture are worked out below; checked there)
    let extra_base = segs.len()
        + usize::from(spec.overlay.as_ref().is_some_and(|o| !o.path.trim().is_empty()))
        + usize::from(spec.music.as_ref().is_some_and(|m| !m.path.trim().is_empty()) && audio.is_some())
        + usize::from(still.is_some());
    let extra_inputs: Vec<Vec<usize>> = {
        let mut next = extra_base;
        loops
            .iter()
            .map(|lp| match lp {
                Some(ClipLoop::Inputs { copies, .. }) => {
                    let v: Vec<usize> = (next..next + copies - 1).collect();
                    next += copies - 1;
                    v
                }
                _ => Vec::new(),
            })
            .collect()
    };
    let layout_frags: Option<Vec<String>> = layout.map(|l| {
        (0..segs.len())
            .map(|i| {
                let m = clip_info(i);
                // the clip's own crop, kept inside its picture
                let crop = l.crops.get(i).copied().flatten().and_then(|r| {
                    let (fw, fh) = (m.video.width, m.video.height);
                    let x = r.x.min(fw.saturating_sub(2));
                    let y = r.y.min(fh.saturating_sub(2));
                    let w = r.w.min(fw - x).max(2);
                    let h = r.h.min(fh - y).max(2);
                    (fw > 0 && fh > 0 && (x, y, w, h) != (0, 0, fw, fh)).then_some((x, y, w, h))
                });
                let pre = segment_chain(
                    &SegmentFilters {
                        stabilize: stab[i].as_deref(),
                        deinterlace: m.video.interlaced,
                        tonemap: m.video.hdr && opts.zscale,
                        anamorphic: m.video.anamorphic,
                        crop,
                        subtitles: None,
                        rotate: 0,
                        mirror: false,
                        vflip: false,
                        ..sf
                    },
                    segs[i].0,
                )
                .join(",");
                let (bx, by, pad, hold, len) = match (split, cells.get(i)) {
                    (Some(_), Some((tile, Some(inner)))) => {
                        let hold = loops[i].is_none().then_some(clip - lens[i]);
                        (inner.w, inner.h, Some((*tile, *inner, gap_color.as_str())), hold, Some(clip))
                    }
                    _ => (v.width, v.height, None, None, None),
                };
                let si = m.video.stream_index;
                match loops[i] {
                    // every copy exactly `frames` frames long, then joined
                    Some(ClipLoop::Inputs { frames, .. }) => {
                        let mut g = String::new();
                        let ins: Vec<usize> = std::iter::once(i).chain(extra_inputs[i].iter().copied()).collect();
                        for (k, idx) in ins.iter().enumerate() {
                            g.push_str(&format!(
                                "[{idx}:{si}]setpts=PTS-STARTPTS,fps={},tpad=stop_mode=clone:stop=1,trim=end_frame={frames},setpts=PTS-STARTPTS[r{i}_{k}];",
                                fmt_num(l.fps)
                            ));
                        }
                        let all: String = (0..ins.len()).map(|k| format!("[r{i}_{k}]")).collect();
                        g.push_str(&format!("{all}concat=n={}:v=1:a=0[r{i}];", ins.len()));
                        g + &fit_fragment(&format!("[r{i}]"), &pre, &format!("[v{i}]"), &format!("f{i}"), l.fill, bx, by, l.fps, None, pad, hold, len)
                    }
                    lp => fit_fragment(
                        &format!("[{i}:{si}]"),
                        &pre,
                        &format!("[v{i}]"),
                        &format!("f{i}"),
                        l.fill,
                        bx,
                        by,
                        l.fps,
                        lp.map(ClipLoop::frames),
                        pad,
                        hold,
                        len,
                    ),
                }
            })
            .collect()
    });
    if layout.is_some() && (0..segs.len()).any(|i| clip_info(i).video.hdr) && !opts.zscale {
        warnings.push("A clip is HDR, and this ffmpeg lacks the zscale filter to tone map it; its colors look washed out.".into());
    }
    if let Some(c) = spec.caption.as_ref().filter(|c| !c.text.trim().is_empty()) {
        if c.font.as_deref().is_some_and(|f| !f.trim().is_empty()) && caption_own_font(c).is_none() {
            warnings.push("The caption font file was not found; a bold sans is used instead.".into());
        }
    }
    // zoom points: ranges of `input` only, not a still picture or splits
    let zoom = if spec.zoom.is_empty() || still.is_some() || layout.is_some() {
        None
    } else if mixed {
        warnings.push("The zoom applies to one file, so it is not used when ranges come from other files.".into());
        None
    } else {
        crate::zoom::filter(&spec.zoom, &segs, g.out_w, g.out_h, if v.fps > 0.0 { v.fps } else { 30.0 })
    };
    let vpost = video_post(&PostFilters {
        zoom,
        scale: ((g.out_w, g.out_h) != (g.rot_w, g.rot_h)).then_some((g.out_w, g.out_h)),
        caption: spec.caption.as_ref().and_then(|c| caption_filter(c, g.out_h)),
        speed: if still.is_some() { 1.0 } else { speed },
        ramp: ramp.as_ref().map(|r| r.setpts()),
        // After setpts the filter graph still reports the source rate, and
        // ffmpeg then writes the file at that rate: 1.5x of 30 fps kept 2 of
        // every 3 frames at 30 fps instead of all of them at 45 fps. Naming
        // the new rate keeps every frame (they already sit on its grid).
        // a ramp gives every frame its own time: fps makes a steady rate again
        fps: fps_cap.or(((speed - 1.0).abs() > 1e-9 && src_fps_out > 0.0 && still.is_none()).then_some(src_fps_out)).or(
            ramp.as_ref().map(|_| if v.fps > 0.0 { v.fps } else { 30.0 }),
        ),
        interpolate,
        playback: if still.is_some() { Playback::Forward } else { spec.playback },
        fade_in: spec.fade_in.max(0.0),
        fade_out: spec.fade_out.max(0.0),
        duration_out,
        color: color_tags(v, tonemap),
    });
    let apost = audio.map(|a| {
        let rest = audio_post(
            speed,
            spec.playback == Playback::Reverse,
            a.normalize,
            a.volume_db,
            spec.fade_in.max(0.0),
            spec.fade_out.max(0.0),
            duration_out,
        );
        // the ramp's sound first (it ends at the picture's length), then the rest
        match (&ramp, rest.is_empty()) {
            (Some(r), true) => r.audio("ra"),
            (Some(r), false) => format!("{},{rest}", r.audio("ra")),
            (None, _) => rest,
        }
    });
    let color = color_tags(v, tonemap);

    // libvpx and x264/x265 use 2 passes in size mode; libvpx also in crf mode
    // when asked. libsvtav1 is 1 pass; size mode relies on the retry loop.
    let two_pass = !opts.single_pass
        && match codec {
            Codec::Av1 => false,
            Codec::H264 | Codec::Hevc => size_mode,
            Codec::Vp9 | Codec::Vp8 => size_mode || spec.two_pass,
            Codec::Gif | Codec::Webp | Codec::Opus | Codec::Mp3 | Codec::Aac => false,
        };
    let reuse_pass1 = matches!(codec, Codec::Vp9 | Codec::Vp8 | Codec::H264);
    // GIF: one palette per clip from the frames' differences, then dithered
    let palette = "split[gs0][gs1];[gs0]palettegen=stats_mode=diff[gpal];[gs1][gpal]paletteuse=dither=sierra2_4a:diff_mode=rectangle";
    // watermark: an extra input after the ranges, scaled to a share of the output
    // width; drawn after everything else (before the GIF palette)
    let overlay = spec.overlay.as_ref().filter(|o| !o.path.trim().is_empty()).map(|o| {
        let (lg, place) = overlay_chain(o, g.out_w, g.out_h);
        (o.path.clone(), lg, place)
    });
    let vpost = if gif && overlay.is_none() {
        let base = if vpost.is_empty() { String::new() } else { format!("{vpost},") };
        format!("{base}{palette}")
    } else {
        vpost
    };

    // --- arguments ---
    let threads = opts.threads.max(1).to_string();
    let x265_stats = quote_param(&format!("{}-x265.log", opts.passlog));
    let kbps_arg = |k: f64| format!("{}k", k.round());
    // pass: 0 = single pass, 1 or 2
    let video_args = |pass: u32| -> Vec<String> {
        let mut a = Vec::new();
        let e = effort_index(spec.effort);
        match codec {
            Codec::Vp9 | Codec::Vp8 => {
                let cpu = [4u32, 2, 1][e];
                // pass 1 only gathers statistics; a faster speed costs little
                let cpu = if pass == 1 { cpu.max(4) } else { cpu };
                if codec == Codec::Vp9 {
                    push(&mut a, &["-c:v", "libvpx-vp9", "-row-mt", "1", "-tile-columns", "2", "-frame-parallel", "0"]);
                } else {
                    push(&mut a, &["-c:v", "libvpx"]);
                }
                push(&mut a, &["-deadline", "good", "-cpu-used", &cpu.to_string()]);
                push(&mut a, &["-auto-alt-ref", "1", "-lag-in-frames", "25"]);
                match spec.rate {
                    Rate::Size { .. } => push(&mut a, &["-b:v", &kbps_arg(video_kbps.unwrap_or(MIN_VIDEO_KBPS))]),
                    Rate::Crf { crf, .. } => {
                        let lo = if codec == Codec::Vp8 { 4.0 } else { 0.0 };
                        push(&mut a, &["-crf", &fmt_num(crf.clamp(lo, 63.0).round())]);
                        match video_kbps {
                            // constrained quality: b:v is the ceiling
                            Some(k) => push(&mut a, &["-b:v", &kbps_arg(k), "-maxrate", &kbps_arg(k)]),
                            None => push(&mut a, &["-b:v", "0"]),
                        }
                    }
                }
            }
            Codec::Av1 => {
                push(&mut a, &["-c:v", "libsvtav1", "-preset", ["10", "7", "4"][e]]);
                match spec.rate {
                    Rate::Size { .. } => push(&mut a, &["-b:v", &kbps_arg(video_kbps.unwrap_or(MIN_VIDEO_KBPS))]),
                    Rate::Crf { crf, .. } => {
                        push(&mut a, &["-crf", &fmt_num(crf.clamp(1.0, 63.0).round())]);
                        if let Some(k) = video_kbps {
                            push(&mut a, &["-maxrate", &kbps_arg(k)]);
                        }
                    }
                }
            }
            Codec::Gif => push(&mut a, &["-c:v", "gif"]),
            Codec::Opus | Codec::Mp3 | Codec::Aac => unreachable!("audio is planned by build_audio"),
            Codec::Webp => {
                // CRF mode: crf 0-100, lower = better, as quality 100 - crf.
                // Size mode: quality 75; the picture size is what changes.
                let q = match spec.rate {
                    Rate::Crf { crf, .. } => 100.0 - crf.clamp(0.0, 100.0).round(),
                    Rate::Size { .. } => 75.0,
                };
                push(&mut a, &["-c:v", "libwebp_anim", "-lossless", "0", "-q:v", &fmt_num(q), "-compression_level", ["3", "5", "6"][e]]);
            }
            Codec::H264 | Codec::Hevc => {
                if codec == Codec::H264 {
                    push(&mut a, &["-c:v", "libx264", "-profile:v", "high", "-preset", ["fast", "slow", "veryslow"][e]]);
                } else {
                    // hvc1 so Apple players accept the file
                    push(&mut a, &["-c:v", "libx265", "-tag:v", "hvc1", "-preset", ["fast", "medium", "slow"][e]]);
                }
                match spec.rate {
                    Rate::Size { .. } => push(&mut a, &["-b:v", &kbps_arg(video_kbps.unwrap_or(MIN_VIDEO_KBPS))]),
                    Rate::Crf { crf, .. } => {
                        push(&mut a, &["-crf", &fmt_num(crf.clamp(0.0, 51.0).round())]);
                        if let Some(k) = video_kbps {
                            push(&mut a, &["-maxrate", &kbps_arg(k), "-bufsize", &kbps_arg(k * 2.0)]);
                        }
                    }
                }
                if codec == Codec::Hevc {
                    let mut p = String::from("log-level=error");
                    if pass > 0 {
                        p.push_str(&format!(":pass={pass}:stats={x265_stats}"));
                    }
                    push(&mut a, &["-x265-params", &p]);
                }
            }
        }
        // ffmpeg passes -threads to x265 as --frame-threads, which must be
        // below 16 ("frameNumThreads (--frame-threads) must be [0 ..
        // X265_MAX_FRAME_THREADS)" on a 20-thread CPU). x265 sizes its own
        // thread pool, so it gets no -threads.
        if codec != Codec::Hevc {
            push(&mut a, &["-threads", &threads]);
        }
        if pass > 0 && codec != Codec::Hevc {
            // for the video stream only: ffmpeg's master branch (November 2026) applies a bare
            // -pass to the sound too, and pass 2 then fails reading a log libopus never wrote
            push(&mut a, &["-pass:v", &pass.to_string(), "-passlogfile:v", opts.passlog]);
        }
        a
    };

    // music: replaces the source's sound, or is mixed under it
    let music = spec.music.as_ref().filter(|m| !m.path.trim().is_empty() && audio.is_some());
    let music_replaces = music.is_some_and(|m| !m.mix || info.audio.is_empty());
    if spec.music.is_some() && music.is_none() && !image && !boomerang {
        warnings.push("Music needs sound in the output: turn on the audio.".into());
    }
    let music_idx = segs.len() + usize::from(overlay.is_some());
    let still_idx = music_idx + usize::from(music.is_some());
    debug_assert_eq!(extra_base, still_idx + usize::from(still.is_some()));
    // a still picture: its one frame repeated at its own rate, cut to the
    // output's length; then rotation, flips and color adjustments as on video
    let still_chain = still.zip(still_rate.as_ref()).map(|(s, (_, rate))| {
        let mut c = vec![
            "loop=loop=-1:size=1".to_string(),
            format!("setpts=N/({rate})/TB"),
            format!("fps={rate}"),
            format!("trim=duration={}", fmt_num(duration_out)),
        ];
        let pic = SegmentFilters {
            deinterlace: false,
            denoise: false,
            stabilize: None,
            tonemap: false,
            anamorphic: false,
            crop: None,
            subtitles: None,
            ..sf
        };
        c.extend(segment_chain(&pic, 0.0));
        // the size came from probing the picture; scaling to it keeps a wrong size from failing
        c.push(format!("scale={}:{},setsar=1", g.rot_w.max(2) / 2 * 2, g.rot_h.max(2) / 2 * 2));
        format!("[{still_idx}:{}]{}[vj];", s.stream, c.join(","))
    });
    // a logo, music or picture is one more input, so it needs the filter_complex form
    let multi = segs.len() > 1 || overlay.is_some() || music.is_some() || still.is_some();
    let track = audio.map(|a| a.track);
    let build_pass = |pass: u32| -> Vec<String> {
        let with_audio = pass != 1 && audio.is_some();
        let mut a = Vec::new();
        push(&mut a, &["-hide_banner"]);
        if !opts.display {
            push(&mut a, &["-nostdin", "-loglevel", "error", "-progress", "pipe:1", "-nostats"]);
        }
        push(&mut a, &["-y"]);
        for (i, (s, e)) in segs.iter().enumerate() {
            if layout.is_some() && clip_info(i).still {
                // one picture, repeated for the clip's length
                let path = seg_src[i].map_or(spec.input.as_str(), |m| m.path.as_str());
                push(&mut a, &["-loop", "1", "-t", &fmt_num(e - s), "-i", path]);
                continue;
            }
            let m = clip_info(i);
            if *s > 0.0 {
                push(&mut a, &["-ss", &fmt_num(seek_arg(*s, m.video.fps, m.video.vfr))]);
            }
            let path = seg_src[i].map_or(spec.input.as_str(), |m| m.path.as_str());
            push(&mut a, &["-t", &fmt_num(length_arg(*s, *e, m.video.fps, m.video.vfr)), "-i", path]);
        }
        if let Some((path, _, _)) = &overlay {
            push(&mut a, &["-i", path]);
        }
        if let Some(m) = music {
            if m.offset > 0.0 {
                push(&mut a, &["-ss", &fmt_num(m.offset)]);
            }
            push(&mut a, &["-i", &m.path]);
        }
        if let Some(s) = still {
            push(&mut a, &["-i", &s.path]);
        }
        // the copies of looping clips, read again from their files
        for (i, idx) in extra_inputs.iter().enumerate() {
            let (s, e) = segs[i];
            let path = seg_src[i].map_or(spec.input.as_str(), |m| m.path.as_str());
            let m = clip_info(i);
            for _ in idx {
                if s > 0.0 {
                    push(&mut a, &["-ss", &fmt_num(seek_arg(s, m.video.fps, m.video.vfr))]);
                }
                push(&mut a, &["-t", &fmt_num(length_arg(s, e, m.video.fps, m.video.vfr)), "-i", path]);
            }
        }
        if !multi {
            push(&mut a, &["-map", &format!("0:{}", v.stream_index)]);
            if let (true, Some(t)) = (with_audio, track) {
                push(&mut a, &["-map", &format!("0:a:{t}")]);
            }
            let vf = [seg_chains[0].as_str(), vpost.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(",");
            if !vf.is_empty() {
                push(&mut a, &["-vf", &vf]);
            }
            if with_audio {
                // rebased like the video (see segment_chain)
                let af = match apost.as_deref().filter(|s| !s.is_empty()) {
                    Some(af) => format!("asetpts=PTS-STARTPTS,{af}"),
                    None => "asetpts=PTS-STARTPTS".into(),
                };
                push(&mut a, &["-af", &af]);
            }
        } else {
            let mut graph = String::new();
            for (i, chain) in seg_chains.iter().enumerate() {
                if let Some(frags) = &layout_frags {
                    graph.push_str(&frags[i]);
                } else if still.is_none() {
                    let chain = if chain.is_empty() { "null" } else { chain.as_str() };
                    let vi = seg_src[i].map_or(v.stream_index, |m| m.video.stream_index);
                    graph.push_str(&format!("[{i}:{vi}]{chain}[v{i}];"));
                }
                // split screen: only the clip the sound comes from, or all of them (mix)
                let wanted = match split {
                    Some(l) if l.sound == crate::spec::SplitSound::Clip => i == l.sound_clip,
                    _ => true,
                };
                if let (true, Some(t), true) = (with_audio && !music_replaces, track, wanted) {
                    // joined files: one sample format and layout for concat; a
                    // file without sound gives silence of the range's length.
                    // Split screen: every sound lasts the output's length.
                    let norm = if mixed { ",aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo" } else { "" };
                    let (fit_len, silence_len) = match (split, loops[i]) {
                        // one pass as long as the picture's (norm made it 48 kHz), repeated
                        (Some(l), Some(lp)) => {
                            let n = (lp.frames() as f64 / l.fps * 48000.0).round().max(1.0);
                            (format!(",apad,atrim=end_sample={n},aloop=loop=-1:size={n},atrim=duration={}", fmt_num(clip)), clip)
                        }
                        (Some(_), None) => (format!(",apad,atrim=duration={}", fmt_num(clip)), clip),
                        (None, _) => (String::new(), segs[i].1 - segs[i].0),
                    };
                    // splits: every clip is a file of its own, the first one too
                    let src_m = if layout.is_some() { Some(clip_info(i)) } else { seg_src[i] };
                    match src_m {
                        Some(m) if m.audio.is_empty() => graph.push_str(&format!(
                            "anullsrc=channel_layout=stereo:sample_rate=48000,atrim=duration={},aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}];",
                            fmt_num(silence_len)
                        )),
                        Some(m) => {
                            let ft = if (t as usize) < m.audio.len() { t } else { 0 };
                            graph.push_str(&format!("[{i}:a:{ft}]asetpts=PTS-STARTPTS{norm}{fit_len}[a{i}];"));
                        }
                        None => graph.push_str(&format!("[{i}:a:{t}]asetpts=PTS-STARTPTS{norm}[a{i}];")),
                    }
                }
            }
            let src_audio = with_audio && !music_replaces;
            let n = segs.len();
            if let Some(l) = split {
                // every part already fills its tile; an empty grid tile gets the gap color
                let mut ins = String::new();
                let mut pos = Vec::new();
                for (k, (tile, inner)) in cells.iter().enumerate() {
                    if inner.is_some() && k < n {
                        ins.push_str(&format!("[v{k}]"));
                    } else {
                        graph.push_str(&format!(
                            "color=c={gap_color}:s={}x{}:r={}:d={},format=yuv420p[e{k}];",
                            tile.w,
                            tile.h,
                            fmt_num(l.fps),
                            fmt_num(clip)
                        ));
                        ins.push_str(&format!("[e{k}]"));
                    }
                    pos.push(format!("{}_{}", tile.x, tile.y));
                }
                graph.push_str(&format!("{ins}xstack=inputs={}:layout={}[vj];", cells.len(), pos.join("|")));
                if src_audio {
                    if l.sound == crate::spec::SplitSound::Mix {
                        let all: String = (0..n).map(|i| format!("[a{i}]")).collect();
                        graph.push_str(&format!("{all}amix=inputs={n}:duration=longest:normalize=0,atrim=duration={}[aj];", fmt_num(clip)));
                    } else {
                        graph.push_str(&format!("[a{}]anull[aj];", l.sound_clip));
                    }
                }
            } else {
                for i in 0..n {
                    if still.is_none() {
                        graph.push_str(&format!("[v{i}]"));
                    }
                    if src_audio {
                        graph.push_str(&format!("[a{i}]"));
                    }
                }
                match (&still_chain, src_audio) {
                    (None, true) => graph.push_str(&format!("concat=n={n}:v=1:a=1[vj][aj];")),
                    (None, false) => graph.push_str(&format!("concat=n={n}:v=1:a=0[vj];")),
                    (Some(c), true) => graph.push_str(&format!("concat=n={n}:v=0:a=1[aj];{c}")),
                    (Some(c), false) => graph.push_str(c),
                }
            }
            match &overlay {
                Some((_, logo, place)) => {
                    let n = segs.len();
                    let tail = if gif { format!(",{palette}") } else { String::new() };
                    graph.push_str(&format!(
                        "[vj]{}[vpre];[{n}:v]{logo}[lg];[vpre][lg]{place}{tail}[vout]",
                        if vpost.is_empty() { "null" } else { &vpost }
                    ));
                }
                None => graph.push_str(&format!("[vj]{}[vout]", if vpost.is_empty() { "null" } else { &vpost })),
            }
            match (with_audio, music) {
                (true, Some(m)) => {
                    // the music: from its offset, the output's length, faded like the clip
                    let mut mc = format!(
                        "[{music_idx}:a]atrim=duration={},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo",
                        fmt_num(duration_out)
                    );
                    if m.volume_db.is_finite() && m.volume_db != 0.0 {
                        mc.push_str(&format!(",volume={}dB", fmt_num(m.volume_db)));
                    }
                    let (fi, fo) = (spec.fade_in.max(0.0).min(duration_out), spec.fade_out.max(0.0).min(duration_out));
                    if fi > 0.0 {
                        mc.push_str(&format!(",afade=t=in:st=0:d={}", fmt_num(fi)));
                    }
                    if fo > 0.0 {
                        mc.push_str(&format!(",afade=t=out:st={}:d={}", fmt_num(duration_out - fo), fmt_num(fo)));
                    }
                    if music_replaces {
                        graph.push_str(&format!(";{mc}[aout]"));
                    } else {
                        let af = apost.as_deref().filter(|s| !s.is_empty()).unwrap_or("anull");
                        graph.push_str(&format!(
                            ";[aj]{af},aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[asrc];{mc}[amus];[asrc][amus]amix=inputs=2:duration=first:normalize=0[aout]"
                        ));
                    }
                }
                (true, None) => {
                    let af = apost.as_deref().filter(|s| !s.is_empty()).unwrap_or("anull");
                    graph.push_str(&format!(";[aj]{af}[aout]"));
                }
                _ => {}
            }
            push(&mut a, &["-filter_complex", &graph, "-map", "[vout]"]);
            if with_audio {
                push(&mut a, &["-map", "[aout]"]);
            }
        }
        if opts.frame {
            push(&mut a, &["-an", "-frames:v", "1", "-update", "1", "-c:v", "png", "-f", "image2", opts.output]);
            return a;
        }
        if still.is_some() {
            // the picture's last frame would otherwise run up to a second past the sound
            push(&mut a, &["-t", &fmt_num(duration_out)]);
        }
        if !gif {
            push(&mut a, &["-pix_fmt", "yuv420p"]);
        }
        a.extend(video_args(pass));
        if let (Some((fps, _)), false) = (&still_rate, image) {
            // in size mode every keyframe of the picture is paid in full and a lower bitrate
            // does not shrink it, so there is one at the start only; the frames after it are
            // empty, so seeking stays cheap
            let gop = if size_mode { fps * duration_out + 1.0 } else { fps * STILL_GOP_SECS };
            push(&mut a, &["-g", &fmt_num(gop.ceil().max(1.0))]);
        }
        if !image {
            push(&mut a, &["-colorspace", &color.0, "-color_primaries", &color.1, "-color_trc", &color.2]);
        }
        if pass == 1 {
            push(&mut a, &["-an"]);
            a.extend(spec.extra_args.iter().cloned());
            push(&mut a, &["-f", "null", "-"]);
            return a;
        }
        match audio {
            Some(au) => {
                let enc = match (spec.container, codec) {
                    (Container::Mp4, _) => "aac",
                    (_, Codec::Vp8) => "libvorbis",
                    _ => "libopus",
                };
                push(&mut a, &["-c:a", enc, "-b:a", &format!("{}k", fmt_num(au.bitrate_k.round())), "-ac", "2"]);
            }
            None => push(&mut a, &["-an"]),
        }
        push(&mut a, &["-map_metadata", "-1", "-map_chapters", "-1", "-sn"]);
        if let Some(t) = spec.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            push(&mut a, &["-metadata", &format!("title={t}")]);
        }
        a.extend(spec.extra_args.iter().cloned());
        match spec.container {
            Container::Webm => push(&mut a, &["-f", "webm", opts.output]),
            Container::Mp4 => push(&mut a, &["-movflags", "+faststart", "-f", "mp4", opts.output]),
            Container::Gif => push(&mut a, &["-loop", "0", "-f", "gif", opts.output]),
            Container::Webp => push(&mut a, &["-loop", "0", "-f", "webp", opts.output]),
            Container::Audio => unreachable!("audio is planned by build_audio"),
        }
        a
    };

    let passes = if two_pass { vec![build_pass(1), build_pass(2)] } else { vec![build_pass(0)] };

    Ok(Planned {
        out_width: g.out_w,
        out_height: g.out_h,
        duration_out,
        out_fps,
        video_kbps,
        audio_kbps,
        bits_per_pixel: bpp,
        quality: qual,
        auto_scaled,
        passes,
        prepasses,
        reuse_pass1,
        warnings,
        cells: if split.is_some() { cells.iter().take(segs.len()).filter_map(|(_, inner)| *inner).collect() } else { Vec::new() },
    })
}

/// Video and audio codecs (ffprobe names) a container can take by stream copy.
fn copy_fits(container: Container, video: &str, audio: Option<&str>) -> bool {
    let (v_ok, a_ok): (&[&str], &[&str]) = match container {
        Container::Webm => (&["vp8", "vp9", "av1"], &["opus", "vorbis"]),
        Container::Mp4 => (&["h264", "hevc", "av1", "mpeg4"], &["aac", "mp3", "opus", "ac3", "eac3", "flac", "alac"]),
        Container::Gif | Container::Webp | Container::Audio => (&[], &[]),
    };
    v_ok.contains(&video) && audio.is_none_or(|a| a_ok.contains(&a))
}

/// The container a stream copy of these codecs fits in, WebM first.
pub fn copy_container(video: &str, audio: Option<&str>) -> Option<Container> {
    [Container::Webm, Container::Mp4].into_iter().find(|c| copy_fits(*c, video, audio))
}

/// Cut only: one range, streams copied. ffmpeg starts at the key frame at
/// or before the in point, so the clip can start early; every filter and
/// the rate settings are ignored (and listed in the warnings when set).
fn build_copy(spec: &EncodeSpec, info: &MediaInfo, opts: &Opts) -> Result<Planned, String> {
    if info.audio_only {
        return Err("Cut only copies a video stream. For a sound file choose the Audio format, or turn off Cut only.".into());
    }
    let v = &info.video;
    let segs = resolve_segments(&spec.segments, info.duration)?;
    if segs.len() > 1 {
        return Err("Cut only (stream copy) makes one range per file. Turn on Separate files, or turn off Cut only.".into());
    }
    let audio = match spec.audio {
        Some(a) => Some(info.audio.get(a.track as usize).ok_or(format!("Audio track {} does not exist.", a.track))?),
        None => None,
    };
    let acodec = audio.map(|a| a.codec.as_str());
    if !copy_fits(spec.container, &v.codec, acodec) {
        let what = match acodec {
            Some(a) => format!("{} video with {a} audio", v.codec),
            None => format!("{} video", v.codec),
        };
        return Err(match copy_container(&v.codec, acodec) {
            Some(c) => format!("{what} cannot be copied into {}. Choose {}.", spec.container.ext().to_uppercase(), c.ext().to_uppercase()),
            None => format!("{what} cannot be copied into WebM or MP4. Turn off Cut only to encode it."),
        });
    }
    let mut warnings = vec!["Cut only: the clip starts at the key frame at or before the in point, so it can begin up to a few seconds early.".to_string()];
    let mut ignored = Vec::new();
    if spec.crop.is_some() {
        ignored.push("crop");
    }
    if spec.rotate != 0 || spec.mirror || spec.vflip {
        ignored.push("rotate and flip");
    }
    if spec.max_width.is_some_and(|w| w < v.width) || spec.max_height.is_some_and(|h| h < v.height) {
        ignored.push("resolution");
    }
    if spec.fps.is_some() || (spec.speed - 1.0).abs() > 1e-9 || !spec.speed_ramp.is_empty() || spec.playback != Playback::Forward {
        ignored.push("frame rate, speed and direction");
    }
    if spec.deinterlace || spec.denoise || spec.tonemap || spec.adjust.as_ref().is_some_and(|a| !adjust_filters(a).is_empty()) {
        ignored.push("filters and adjustments");
    }
    if spec.fade_in > 0.0 || spec.fade_out > 0.0 {
        ignored.push("fades");
    }
    if spec.subs.is_some() {
        ignored.push("subtitles");
    }
    if spec.audio.is_some_and(|a| a.normalize || a.volume_db != 0.0) {
        ignored.push("audio level");
    }
    if matches!(spec.rate, Rate::Size { .. }) {
        ignored.push("size limit");
    }
    if !ignored.is_empty() {
        warnings.push(format!("Cut only copies the streams, so these settings do nothing: {}.", ignored.join(", ")));
    }
    let (start, end) = segs[0];
    let mut a = Vec::new();
    push(&mut a, &["-hide_banner"]);
    if !opts.display {
        push(&mut a, &["-nostdin", "-loglevel", "error", "-progress", "pipe:1", "-nostats"]);
    }
    push(&mut a, &["-y"]);
    if start > 0.0 {
        push(&mut a, &["-ss", &fmt_num(start)]);
    }
    push(&mut a, &["-t", &fmt_num(end - start), "-i", &spec.input, "-map", &format!("0:{}", v.stream_index)]);
    match spec.audio {
        Some(au) => push(&mut a, &["-map", &format!("0:a:{}", au.track)]),
        None => push(&mut a, &["-an"]),
    }
    push(&mut a, &["-c", "copy", "-avoid_negative_ts", "make_zero", "-map_metadata", "-1", "-map_chapters", "-1", "-sn", "-dn"]);
    if let Some(t) = spec.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        push(&mut a, &["-metadata", &format!("title={t}")]);
    }
    a.extend(spec.extra_args.iter().cloned());
    match spec.container {
        Container::Mp4 => push(&mut a, &["-movflags", "+faststart", "-f", "mp4", opts.output]),
        _ => push(&mut a, &["-f", "webm", opts.output]),
    }
    Ok(Planned {
        out_width: v.width,
        out_height: v.height,
        duration_out: end - start,
        out_fps: v.fps,
        video_kbps: None,
        audio_kbps: 0.0,
        bits_per_pixel: None,
        quality: None,
        auto_scaled: false,
        passes: vec![a],
        prepasses: Vec::new(),
        reuse_pass1: false,
        warnings,
        cells: Vec::new(),
    })
}

/// Audio only: the chosen track of every range, joined, with speed,
/// direction, loudness, volume and fades; no picture. Size mode sets the
/// bitrate from the limit (8-320 kbps); quality mode uses the audio bitrate.
fn build_audio(spec: &EncodeSpec, info: &MediaInfo, opts: &Opts) -> Result<Planned, String> {
    if !spec.codec.is_audio() {
        return Err("Audio-only output needs Opus, MP3 or AAC.".into());
    }
    let au = spec.audio.ok_or("Audio only needs an audio track. Choose one in the Audio group.")?;
    if info.audio.get(au.track as usize).is_none() {
        return Err(if info.audio.is_empty() { "The source has no audio.".into() } else { format!("Audio track {} does not exist.", au.track) });
    }
    if !spec.speed.is_finite() || spec.speed <= 0.0 {
        return Err("Speed must be greater than 0.".into());
    }
    let segs = resolve_segments(&spec.segments, info.duration)?;
    let clip: f64 = segs.iter().map(|(s, e)| e - s).sum();
    let mut warnings = Vec::new();
    if spec.playback == Playback::Boomerang {
        warnings.push("Boomerang applies to the picture only; the sound plays forward.".into());
    }
    let ramp = crate::ramp::Ramp::new(&spec.speed_ramp, &segs);
    let speed = if ramp.is_some() { 1.0 } else { spec.speed };
    let duration_out = ramp.as_ref().map_or(clip / speed, |r| r.duration());
    let kbps = match spec.rate {
        Rate::Size { max_bytes } => {
            if !max_bytes.is_finite() || max_bytes <= 0.0 {
                return Err("The size limit must be greater than 0.".into());
            }
            let k = (max_bytes * 8.0 / 1000.0 / duration_out.max(0.01) * 0.97).floor();
            if k < 32.0 {
                warnings.push(format!("The size limit allows only {} kbps of sound. Shorten the clip or raise the limit.", k.max(8.0)));
            }
            // a size-fit retry passes the next bitrate here
            opts.video_kbps.map(|o| o.floor()).unwrap_or(k).clamp(8.0, 320.0)
        }
        Rate::Crf { .. } => au.bitrate_k.clamp(8.0, 320.0),
    };
    // the size-fit loop only helps below the 320 kbps cap
    let size_mode = matches!(spec.rate, Rate::Size { .. }) && kbps < 320.0;
    let rest = audio_post(
        speed,
        spec.playback == Playback::Reverse,
        au.normalize,
        au.volume_db,
        spec.fade_in.max(0.0),
        spec.fade_out.max(0.0),
        duration_out,
    );
    let apost = match (&ramp, rest.is_empty()) {
        (Some(r), true) => r.audio("ra"),
        (Some(r), false) => format!("{},{rest}", r.audio("ra")),
        (None, _) => rest,
    };
    let mut a = Vec::new();
    push(&mut a, &["-hide_banner"]);
    if !opts.display {
        push(&mut a, &["-nostdin", "-loglevel", "error", "-progress", "pipe:1", "-nostats"]);
    }
    push(&mut a, &["-y"]);
    for (s, e) in &segs {
        if *s > 0.0 {
            push(&mut a, &["-ss", &fmt_num(*s)]);
        }
        push(&mut a, &["-t", &fmt_num(e - s), "-i", &spec.input]);
    }
    let t = au.track;
    if segs.len() == 1 {
        push(&mut a, &["-map", &format!("0:a:{t}")]);
        let af = if apost.is_empty() { "asetpts=PTS-STARTPTS".to_string() } else { format!("asetpts=PTS-STARTPTS,{apost}") };
        push(&mut a, &["-af", &af]);
    } else {
        let mut g = String::new();
        for i in 0..segs.len() {
            g.push_str(&format!("[{i}:a:{t}]asetpts=PTS-STARTPTS[a{i}];"));
        }
        for i in 0..segs.len() {
            g.push_str(&format!("[a{i}]"));
        }
        g.push_str(&format!("concat=n={}:v=0:a=1[aj];[aj]{}[aout]", segs.len(), if apost.is_empty() { "anull" } else { &apost }));
        push(&mut a, &["-filter_complex", &g, "-map", "[aout]"]);
    }
    let k = format!("{}k", fmt_num(kbps));
    match spec.codec {
        Codec::Opus => push(&mut a, &["-c:a", "libopus", "-b:a", &k]),
        Codec::Mp3 => push(&mut a, &["-c:a", "libmp3lame", "-b:a", &k]),
        _ => push(&mut a, &["-c:a", "aac", "-b:a", &k]),
    }
    push(&mut a, &["-ac", "2", "-vn", "-sn", "-dn", "-map_metadata", "-1", "-map_chapters", "-1"]);
    if let Some(title) = spec.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        push(&mut a, &["-metadata", &format!("title={title}")]);
    }
    a.extend(spec.extra_args.iter().cloned());
    match spec.codec {
        Codec::Opus => push(&mut a, &["-f", "opus", opts.output]),
        Codec::Mp3 => push(&mut a, &["-f", "mp3", opts.output]),
        _ => push(&mut a, &["-movflags", "+faststart", "-f", "ipod", opts.output]),
    }
    Ok(Planned {
        out_width: 0,
        out_height: 0,
        duration_out,
        out_fps: 0.0,
        // size mode: the bitrate the size-fit loop steers (here the sound's)
        video_kbps: size_mode.then_some(kbps),
        audio_kbps: if size_mode { 0.0 } else { kbps },
        // (with the cap reached, the bitrate is reported as audio and no retry runs)
        bits_per_pixel: None,
        quality: None,
        auto_scaled: false,
        passes: vec![a],
        prepasses: Vec::new(),
        reuse_pass1: false,
        warnings,
        cells: Vec::new(),
    })
}

/// Joins arguments into one line for display. Arguments with characters a
/// shell treats specially are wrapped in double quotes.
pub fn shell_join(program: &str, args: &[String]) -> String {
    let mut out = String::from(program);
    for a in args {
        out.push(' ');
        let safe = !a.is_empty() && a.chars().all(|c| c.is_ascii_alphanumeric() || "-_+=.,:/\\@%".contains(c));
        if safe {
            out.push_str(a);
        } else {
            out.push('"');
            out.push_str(&a.replace('"', "\\\""));
            out.push('"');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{AudioSel, AudioTrack};

    fn media() -> MediaInfo {
        MediaInfo {
            path: "in.mp4".into(),
            file_name: "in.mp4".into(),
            size_bytes: 1,
            duration: 60.0,
            video: VideoInfo {
                codec: "h264".into(),
                width: 1280,
                height: 720,
                fps: 30.0,
                ..Default::default()
            },
            audio: vec![AudioTrack { track: 0, codec: "aac".into(), channels: 2, lang: None, title: None, sample_rate: None, bitrate_k: None }],
            subs: vec![],
            ..Default::default()
        }
    }

    fn spec(rate: Rate) -> EncodeSpec {
        EncodeSpec {
            input: "in.mp4".into(),
            segments: vec![Segment { start: 10.0, end: 14.0 }],
            crop: None,
            rotate: 0,
            mirror: false,
            vflip: false,
            adjust: None,
            max_width: None,
            max_height: None,
            auto_scale: false,
            fps: None,
            speed: 1.0,
            speed_ramp: vec![],
            zoom: vec![],
            playback: Playback::Forward,
            deinterlace: false,
            denoise: false,
            stabilize: false,
            stab_method: StabMethod::Deshake,
            smooth_slowmo: false,
            tonemap: false,
            fade_in: 0.0,
            fade_out: 0.0,
            container: Container::Webm,
            codec: Codec::Vp9,
            rate,
            two_pass: false,
            effort: Effort::Balanced,
            audio: Some(AudioSel { track: 0, bitrate_k: 64.0, volume_db: 0.0, normalize: false }),
            subs: None,
            title: None,
            extra_args: vec![],
            output_name: None,
            stream_copy: false,
            caption: None,
            music: None,
            segment_sources: vec![],
            layout: None,
            overlay: None,
            subs_delay: 0.0,
            subs_font_size: None,
            output_dir: None,
            cover: None,
            cover_size: None,
        }
    }

    /// a sound file: no video stream, optionally an embedded cover (stream 1, 600x600)
    fn sound(embedded: bool) -> MediaInfo {
        let mut m = media();
        m.path = "in.mp3".into();
        m.audio_only = true;
        m.video = if embedded {
            VideoInfo { codec: "mjpeg".into(), width: 600, height: 600, stream_index: 1, ..Default::default() }
        } else {
            VideoInfo::default()
        };
        m
    }

    fn opts() -> Opts<'static> {
        Opts { threads: 8, display: true, ..Opts::new("out.webm", "log") }
    }

    fn strs(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }

    const TAGS: &str = "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709";

    #[test]
    fn bitrate_math() {
        // 4194304 bytes over 60 s: 559.24 kbps total, minus 64 audio, times 0.985
        let k = size_video_kbps(4_194_304.0, 60.0, 64.0);
        assert!((k - 487.81).abs() < 0.01);
        // one attempt: scale toward 99% of the limit
        let r = next_video_kbps(&[(500.0, 1100)], 1000.0);
        assert!((r - 500.0 * 990.0 / 1100.0).abs() < 1e-9);
        let r = next_video_kbps(&[(500.0, 970)], 1000.0);
        assert!((r - 500.0 * 990.0 / 970.0).abs() < 1e-9);
        // at most 2x up
        assert_eq!(next_video_kbps(&[(500.0, 300)], 1000.0), 1000.0);
        // a bracketing pair: interpolate between them
        let r = next_video_kbps(&[(500.0, 960), (520.0, 1010)], 1000.0);
        assert!((r - (500.0 + 20.0 * (30.0 / 50.0))).abs() < 1e-9);
        // the closest pair is used
        let r = next_video_kbps(&[(400.0, 800), (600.0, 1200), (500.0, 960), (520.0, 1010)], 1000.0);
        assert!((r - 512.0).abs() < 1e-9);
    }

    #[test]
    fn output_dimensions() {
        let g = geometry(1920, 1080, None, 0, Some(1280), Some(1280));
        assert_eq!((g.out_w, g.out_h, g.crop), (1280, 720, None));
        // never upscale; odd crop rounds down to even
        let g = geometry(1280, 720, Some(Crop { x: 100.4, y: 50.0, w: 301.0, h: 201.0 }), 0, Some(4000), None);
        assert_eq!(g.crop, Some((100, 50, 301, 201)));
        assert_eq!((g.out_w, g.out_h), (300, 200));
        // crop past the edge is clamped
        let g = geometry(1280, 720, Some(Crop { x: 1200.0, y: 0.0, w: 400.0, h: 720.0 }), 0, None, None);
        assert_eq!(g.crop, Some((1200, 0, 80, 720)));
        // rotate swaps before the max size applies
        let g = geometry(1920, 1080, None, 90, None, Some(960));
        assert_eq!((g.rot_w, g.rot_h, g.out_w, g.out_h), (1080, 1920, 540, 960));
    }

    #[test]
    fn filter_graph_string() {
        let subs = (escape_filter_value("subs:a.srt"), None);
        let sf = SegmentFilters {
            deinterlace: true,
            denoise: false,
            stabilize: None,
            tonemap: false,
            anamorphic: false,
            crop: Some((0, 0, 640, 720)),
            rotate: 180,
            mirror: true,
            vflip: false,
            adjust: None,
            subtitles: Some(&subs),
            subs_delay: 0.0,
            subs_font_size: None,
        };
        assert_eq!(
            segment_chain(&sf, 5.5).join(","),
            "setpts=PTS-STARTPTS,bwdif=mode=send_frame,crop=640:720:0:0,vflip,setpts=PTS+5.5/TB,subtitles=filename=subs\\\\:a.srt,setpts=PTS-STARTPTS"
        );
        let post = video_post(&PostFilters {
            zoom: None,
            caption: None,
            interpolate: None,
            scale: Some((320, 360)),
            speed: 2.0,
            ramp: None,
            fps: Some(24.0),
            playback: Playback::Boomerang,
            fade_in: 0.5,
            fade_out: 1.0,
            duration_out: 4.0,
            color: ("bt709".into(), "bt709".into(), "bt709".into()),
        });
        assert_eq!(
            post,
            format!(
                "scale=320:360:flags=lanczos,setsar=1,setpts=PTS/2,fps=24,split[bf][bb];[bb]reverse[br];[bf][br]concat=n=2:v=1:a=0,\
                 fade=t=in:st=0:d=0.5,fade=t=out:st=3:d=1,{TAGS}"
            )
        );
    }

    #[test]
    fn range_edges_snap_to_the_frames_the_player_shows() {
        // 30 fps: a start inside frame 100 (3.333..3.367 s) is frame 100, an end inside frame 200 keeps it
        let mut r = [(3.35, 6.69), (10.0, 12.0)];
        snap_to_frames(&mut r, 30.0, false);
        assert!((r[0].0 - 100.0 / 30.0).abs() < 1e-12 && (r[0].1 - 201.0 / 30.0).abs() < 1e-12, "{r:?}");
        assert!((r[1].0 - 10.0).abs() < 1e-12 && (r[1].1 - 12.0).abs() < 1e-12, "{r:?}");
        // the argument comes before frame 100 and after frame 99, even rounded to milliseconds
        let ss: f64 = fmt_num(seek_arg(r[0].0, 30.0, false)).parse().unwrap();
        assert!(ss > 99.0 / 30.0 && ss < 100.0 / 30.0, "{ss}");
        // ffmpeg counts -t from the first frame it keeps; a WebM stores times in whole
        // milliseconds: frame 814 of 60 fps at 13.567 and frame 1091 at 18.183. The end
        // must stay before frame 1091 for a range of frames 814..=1090.
        let (s0, e0): (f64, f64) = (814.0 / 60.0, 1091.0 / 60.0);
        let first_kept = (s0 * 1000.0).round() / 1000.0;
        let t: f64 = fmt_num(length_arg(s0, e0, 60.0, false)).parse().unwrap();
        assert!(first_kept + t < 18.183 && first_kept + t > 18.167, "{}", first_kept + t);
        // variable frame rate: unchanged
        let mut v = [(3.35, 6.69)];
        snap_to_frames(&mut v, 30.0, true);
        assert_eq!(v, [(3.35, 6.69)]);
    }

    #[test]
    fn size_mode_args() {
        let p = build(&spec(Rate::Size { max_bytes: 300_000.0 }), &media(), &opts()).unwrap();
        assert_eq!(p.passes.len(), 2);
        // 300000*8/1000/4 = 600; (600-64)*0.985 = 527.96
        assert_eq!(p.video_kbps.unwrap().round(), 528.0);
        let vp9 = "-c:v libvpx-vp9 -row-mt 1 -tile-columns 2 -frame-parallel 0 -deadline good";
        // the range starts at 10 s; -ss is a quarter frame early (seek_arg)
        let head = "-hide_banner -y -ss 9.992 -t 3.992 -i in.mp4";
        let tags = "-colorspace bt709 -color_primaries bt709 -color_trc bt709";
        assert_eq!(
            strs(&p.passes[0]).join(" "),
            format!(
                "{head} -map 0:0 -vf setpts=PTS-STARTPTS,{TAGS} -pix_fmt yuv420p {vp9} -cpu-used 4 -auto-alt-ref 1 -lag-in-frames 25 \
                 -b:v 528k -threads 8 -pass:v 1 -passlogfile:v log {tags} -an -f null -"
            )
        );
        assert_eq!(
            strs(&p.passes[1]).join(" "),
            format!(
                "{head} -map 0:0 -map 0:a:0 -vf setpts=PTS-STARTPTS,{TAGS} -af asetpts=PTS-STARTPTS -pix_fmt yuv420p {vp9} -cpu-used 2 -auto-alt-ref 1 \
                 -lag-in-frames 25 -b:v 528k -threads 8 -pass:v 2 -passlogfile:v log {tags} -c:a libopus -b:a 64k -ac 2 \
                 -map_metadata -1 -map_chapters -1 -sn -f webm out.webm"
            )
        );
    }

    #[test]
    fn crf_mode_args() {
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.audio = None;
        sp.segments = vec![];
        sp.effort = Effort::Best;
        sp.title = Some("hello world".into());
        let p = build(&sp, &media(), &opts()).unwrap();
        assert_eq!(p.passes.len(), 1);
        assert_eq!((p.video_kbps, p.bits_per_pixel, p.quality), (None, None, None));
        let s = strs(&p.passes[0]);
        assert_eq!(&s[..7], ["-hide_banner", "-y", "-t", "59.992", "-i", "in.mp4", "-map"]);
        let tail = s[s.iter().position(|a| *a == "-cpu-used").unwrap()..].join(" ");
        assert_eq!(
            tail,
            "-cpu-used 1 -auto-alt-ref 1 -lag-in-frames 25 -crf 30 -b:v 0 -threads 8 -colorspace bt709 \
             -color_primaries bt709 -color_trc bt709 -an -map_metadata -1 -map_chapters -1 -sn -metadata \
             title=hello world -f webm out.webm"
        );
    }

    #[test]
    fn vidstab_analysis_per_range() {
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.segments = vec![Segment { start: 1.0, end: 3.0 }, Segment { start: 10.0, end: 12.5 }];
        sp.stabilize = true;
        // deshake: in the chain, no analysis
        let p = build(&sp, &media(), &opts()).unwrap();
        assert!(p.prepasses.is_empty());
        let g = strs(&p.passes[0]).join(" ");
        assert_eq!(g.matches(",deshake").count(), 2, "{g}");
        // vid.stab: one analysis per range, with the range's input arguments and the filters before it
        sp.stab_method = StabMethod::Vidstab;
        let p = build(&sp, &media(), &opts()).unwrap();
        assert_eq!(p.prepasses.len(), 2);
        let a = strs(&p.prepasses[1].0);
        assert_eq!(&a[1..7], ["-y", "-ss", "9.992", "-t", "2.492", "-i"]);
        let vf = a[a.iter().position(|x| *x == "-vf").unwrap() + 1];
        assert_eq!(vf, "setpts=PTS-STARTPTS,vidstabdetect=shakiness=6:accuracy=15:result=log-stab1.trf");
        assert!((p.prepasses[1].1 - 2.5).abs() < 1e-9);
        // the encode reads each range's file, after a copy (FFmpeg ticket 22595)
        let g = strs(&p.passes[0]).join(" ");
        assert!(g.contains("setpts=PTS-STARTPTS,copy,vidstabtransform=input=log-stab0.trf:smoothing=30:optzoom=1:crop=black:interpol=bicubic"), "{g}");
        assert!(g.contains("input=log-stab1.trf"), "{g}");
        // the displayed command shows the analysis first
        assert!(p.info().command.starts_with("ffmpeg -hide_banner -y -ss 0.992 -t 1.992 -i in.mp4 -map 0:0 -an"), "{}", p.info().command);
    }

    #[test]
    fn two_segment_graph() {
        let mut sp = spec(Rate::Size { max_bytes: 1_000_000.0 });
        sp.segments = vec![Segment { start: 1.0, end: 3.0 }, Segment { start: 10.0, end: 12.5 }];
        sp.crop = Some(Crop { x: 0.0, y: 0.0, w: 640.0, h: 360.0 });
        sp.audio = Some(AudioSel { track: 0, bitrate_k: 96.0, volume_db: -3.0, normalize: false });
        let p = build(&sp, &media(), &opts()).unwrap();
        assert!((p.duration_out - 4.5).abs() < 1e-9);
        let s = strs(&p.passes[1]);
        assert_eq!(&s[2..12], ["-ss", "0.992", "-t", "1.992", "-i", "in.mp4", "-ss", "9.992", "-t", "2.492"]);
        let g = s[s.iter().position(|a| *a == "-filter_complex").unwrap() + 1];
        assert_eq!(
            g,
            format!(
                "[0:0]setpts=PTS-STARTPTS,crop=640:360:0:0[v0];[0:a:0]asetpts=PTS-STARTPTS[a0];\
                 [1:0]setpts=PTS-STARTPTS,crop=640:360:0:0[v1];[1:a:0]asetpts=PTS-STARTPTS[a1];[v0][a0][v1][a1]concat=n=2:v=1:a=1[vj][aj];\
                 [vj]{TAGS}[vout];[aj]volume=-3dB[aout]"
            )
        );
        // pass 1 joins video only
        let s1 = strs(&p.passes[0]);
        let g1 = s1[s1.iter().position(|a| *a == "-filter_complex").unwrap() + 1];
        assert!(g1.contains("[v0][v1]concat=n=2:v=1:a=0[vj];[vj]"));
        assert!(!g1.contains("[aout]"));
    }

    #[test]
    fn auto_scale_choice() {
        // 1080p30, 60 s in 4 MiB, no audio: about 543 kbps
        let mut m = media();
        (m.video.width, m.video.height) = (1920, 1080);
        let mut sp = spec(Rate::Size { max_bytes: 4_194_304.0 });
        sp.segments = vec![Segment { start: 0.0, end: 60.0 }];
        sp.audio = None;
        sp.auto_scale = true;
        let p = build(&sp, &m, &opts()).unwrap();
        // 543 kbps / (w*h*30) >= 0.030 first holds at 540p: 960x540 gives 0.0349; 600p gives 0.0283
        assert_eq!((p.out_width, p.out_height, p.auto_scaled), (960, 540, true));
        assert_eq!(p.quality, Some(Quality::Fair));
        // a 240p floor that still fails warns
        sp.rate = Rate::Size { max_bytes: 100_000.0 };
        let p = build(&sp, &m, &opts()).unwrap();
        assert_eq!((p.out_width, p.out_height), (426, 240));
        assert!(p.warnings.iter().any(|w| w.starts_with("Even at 426x240")));
        // a sideways 4K fancam turned upright, 5 s at half speed (10 s), 4 MiB, at most 2048x2048:
        // 3305 kbps reaches 0.10 bits per pixel at 720x1280 (it was 1152x2048 at 0.047)
        (m.video.width, m.video.height, m.video.fps) = (3840, 2160, 59.94);
        sp.rate = Rate::Size { max_bytes: 4_194_304.0 };
        sp.segments = vec![Segment { start: 10.0, end: 15.0 }];
        sp.speed = 0.5;
        sp.rotate = 90;
        (sp.max_width, sp.max_height) = (Some(2048), Some(2048));
        let p = build(&sp, &m, &opts()).unwrap();
        assert_eq!((p.out_width, p.out_height), (720, 1280));
        // a 60 s 1080p clip in 4 MiB stays at 540p (smaller only looked worse in the tests)
        (m.video.width, m.video.height, m.video.fps) = (1920, 1080, 30.0);
        (sp.speed, sp.rotate, sp.max_width, sp.max_height) = (1.0, 0, None, None);
        sp.segments = vec![Segment { start: 0.0, end: 60.0 }];
        let p = build(&sp, &m, &opts()).unwrap();
        assert_eq!((p.out_width, p.out_height), (960, 540));
    }

    #[test]
    fn stream_copy_args() {
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.stream_copy = true;
        sp.container = Container::Mp4;
        sp.crop = Some(Crop { x: 0.0, y: 0.0, w: 100.0, h: 100.0 });
        let m = media();
        let p = build(&sp, &m, &Opts { output: "out.mp4", ..opts() }).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        assert!(s.contains("-ss 10 -t 4 -i in.mp4"), "{s}");
        assert!(s.contains("-c copy -avoid_negative_ts make_zero"), "{s}");
        assert!(s.ends_with("-movflags +faststart -f mp4 out.mp4"), "{s}");
        assert!(p.warnings.iter().any(|w| w.contains("crop")), "{:?}", p.warnings);
        // the test source is H.264: WebM cannot hold it
        sp.container = Container::Webm;
        let e = build(&sp, &m, &opts()).unwrap_err();
        assert!(e.contains("Choose MP4"), "{e}");
        // two ranges are refused
        sp.container = Container::Mp4;
        sp.segments.push(Segment { start: 20.0, end: 21.0 });
        assert!(build(&sp, &m, &opts()).is_err());
    }

    #[test]
    fn webp_args() {
        let mut sp = spec(Rate::Crf { crf: 20.0, max_bitrate_k: None });
        sp.container = Container::Webp;
        sp.codec = Codec::Webp;
        let p = build(&sp, &media(), &Opts { output: "out.webp", ..opts() }).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        assert!(s.contains("-c:v libwebp_anim -lossless 0 -q:v 80 -compression_level 5"), "{s}");
        assert!(!s.contains("-c:a") && !s.contains("-colorspace"), "{s}");
        assert!(s.ends_with("-loop 0 -f webp out.webp"), "{s}");
    }

    #[test]
    fn frame_mode_args() {
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.rotate = 90;
        sp.max_height = Some(720);
        let p = build(&sp, &media(), &Opts { output: "f.png", frame: true, ..opts() }).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        assert!(s.contains("transpose=clock,scale=404:720"), "{s}");
        assert!(s.ends_with("-an -frames:v 1 -update 1 -c:v png -f image2 f.png"), "{s}");
        assert!(!s.contains("libvpx"), "{s}");
    }

    #[test]
    fn audio_only_args() {
        let mut sp = spec(Rate::Size { max_bytes: 1_000_000.0 });
        sp.container = Container::Audio;
        sp.codec = Codec::Mp3;
        sp.segments.push(Segment { start: 20.0, end: 24.0 });
        let p = build(&sp, &media(), &Opts { output: "out.mp3", ..opts() }).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        // 8 s for 1,000,000 bytes: 970 kbps, capped at 320
        assert!(s.contains("concat=n=2:v=0:a=1[aj]"), "{s}");
        assert!(s.contains("-c:a libmp3lame -b:a 320k -ac 2 -vn"), "{s}");
        assert!(s.ends_with("-f mp3 out.mp3"), "{s}");
        assert_eq!(sp.ext(), "mp3");
        // a video codec in the Audio container, or an audio codec in WebM, is refused
        sp.codec = Codec::Vp9;
        assert!(build(&sp, &media(), &opts()).is_err());
        sp.container = Container::Webm;
        sp.codec = Codec::Opus;
        assert!(build(&sp, &media(), &opts()).is_err());
    }

    #[test]
    fn join_other_file() {
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        let mut other = media();
        other.path = "b.mp4".into();
        other.duration = 3.0;
        other.audio.clear();
        other.video.width = 320;
        other.video.height = 240;
        sp.segments.push(Segment { start: 0.0, end: 9.0 });
        sp.segment_sources = vec![None, Some("b.mp4".into())];
        // not probed: refused
        assert!(build(&sp, &media(), &opts()).is_err());
        let mut m = std::collections::HashMap::new();
        m.insert("b.mp4".to_string(), other);
        let p = build(&sp, &media(), &Opts { sources: Some(&m), ..opts() }).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        // the second range is clamped to b.mp4's 3 s and read from it
        assert!(s.contains("-t 2.992 -i b.mp4"), "{s}");
        assert!(s.contains("force_original_aspect_ratio=decrease,pad="), "{s}");
        assert!(s.contains("anullsrc=channel_layout=stereo:sample_rate=48000,atrim=duration=3"), "{s}");
        assert!((p.duration_out - 7.0).abs() < 1e-9);
    }

    #[test]
    fn cover_picture_from_file() {
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.input = "in.mp3".into();
        sp.cover = Some("pic.png".into());
        sp.cover_size = Some((641, 361));
        sp.crop = Some(Crop { x: 10.0, y: 10.0, w: 100.0, h: 100.0 });
        let p = build(&sp, &sound(false), &opts()).unwrap();
        assert_eq!((p.out_width, p.out_height), (640, 360));
        assert_eq!(p.duration_out, 4.0);
        let s = strs(&p.passes[0]).join(" ");
        assert!(s.contains("-ss 10 -t 4 -i in.mp3 -i pic.png"), "{s}");
        assert!(
            s.contains("[0:a:0]asetpts=PTS-STARTPTS[a0];[a0]concat=n=1:v=0:a=1[aj];[1:0]loop=loop=-1:size=1,setpts=N/(1)/TB,fps=1,trim=duration=4,"),
            "{s}"
        );
        assert!(s.contains("scale=640:360,setsar=1[vj]"), "{s}");
        assert!(!s.contains("crop="), "a still picture is not cropped: {s}");
        assert!(s.contains("-map [vout] -map [aout] -t 4 -pix_fmt yuv420p"), "{s}");
        assert!(s.contains("-g 10"), "{s}");
        // frames end with the sound
        assert_eq!(still_rate(4.5, false), (5.0 / 4.5, "5/4.5".to_string()));
        assert_eq!(still_rate(4.5, true).1, "10");
        // the embedded cover: the sound file again, its picture stream
        sp.cover = None;
        let s = strs(&build(&sp, &sound(true), &opts()).unwrap().passes[0]).join(" ");
        assert!(s.contains("-i in.mp3 -i in.mp3 -filter_complex"), "{s}");
        assert!(s.contains("[1:1]loop=loop=-1:size=1"), "{s}");
        // with music the picture is the input after it
        sp.music = Some(crate::spec::Music { path: "m.mp3".into(), mix: false, volume_db: 0.0, offset: 0.0 });
        let s = strs(&build(&sp, &sound(true), &opts()).unwrap().passes[0]).join(" ");
        assert!(s.contains("-i m.mp3 -i in.mp3"), "{s}");
        assert!(s.contains("[2:1]loop="), "{s}");
        assert!(!s.contains("concat"), "music replaces the sound: {s}");
    }

    fn test_layout(mode: crate::spec::LayoutMode) -> crate::spec::Layout {
        crate::spec::Layout {
            mode,
            width: 1280,
            height: 720,
            fill: crate::spec::Fill::Bars,
            fps: 30.0,
            sound: crate::spec::SplitSound::Clip,
            sound_clip: 0,
            length: crate::spec::SplitLength::Shortest,
            gap: 8,
            gap_color: "#000000".into(),
            weights: vec![],
            columns: 0,
            crops: vec![],
        }
    }

    /// two clips: in.mp4 (1280x720, 60 s, sound) 10-14 s and b.mp4 (1080x1920 portrait, 9 s, silent) 0-9 s
    fn split_spec(l: crate::spec::Layout) -> (EncodeSpec, std::collections::HashMap<String, MediaInfo>) {
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.segments.push(Segment { start: 0.0, end: 9.0 });
        sp.segment_sources = vec![None, Some("b.mp4".into())];
        sp.layout = Some(l);
        let mut other = media();
        other.path = "b.mp4".into();
        other.file_name = "b.mp4".into();
        other.duration = 9.0;
        other.audio.clear();
        other.video.width = 1080;
        other.video.height = 1920;
        other.video.fps = 60.0;
        let mut m = std::collections::HashMap::new();
        m.insert("b.mp4".to_string(), other);
        (sp, m)
    }

    #[test]
    fn layout_cells_tile_the_canvas() {
        use crate::spec::{LayoutMode, Rect};
        let l = test_layout(LayoutMode::Columns);
        let c = layout_cells(&l, 2);
        assert_eq!(c[0], (Rect { x: 0, y: 0, w: 640, h: 720 }, Some(Rect { x: 0, y: 0, w: 636, h: 720 })));
        assert_eq!(c[1], (Rect { x: 640, y: 0, w: 640, h: 720 }, Some(Rect { x: 644, y: 0, w: 636, h: 720 })));
        // weights 2:1
        let c = layout_cells(&crate::spec::Layout { weights: vec![2.0, 1.0], gap: 0, ..test_layout(LayoutMode::Rows) }, 2);
        assert_eq!((c[0].0.h, c[1].0.y, c[1].0.h), (480, 480, 240));
        // a 2x2 grid with 3 clips: the fourth tile is empty
        let c = layout_cells(&crate::spec::Layout { gap: 0, ..test_layout(LayoutMode::Grid) }, 3);
        assert_eq!(c.len(), 4);
        assert_eq!(c[3], (Rect { x: 640, y: 360, w: 640, h: 360 }, None));
        assert!(c.iter().all(|(t, _)| t.w % 2 == 0 && t.h % 2 == 0));
    }

    #[test]
    fn split_screen_plan() {
        use crate::spec::{Fill, LayoutMode, SplitLength, SplitSound};
        let (sp, m) = split_spec(crate::spec::Layout { length: SplitLength::Longest, sound: SplitSound::Mix, fill: Fill::Blur, ..test_layout(LayoutMode::Columns) });
        let p = build(&sp, &media(), &Opts { sources: Some(&m), ..opts() }).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        assert!(s.contains("xstack=inputs=2:layout=0_0|640_0[vj]"), "{s}");
        // the 4 s clip holds its last frame up to the 9 s one
        assert!(s.contains("tpad=stop_mode=clone:stop_duration=5"), "{s}");
        assert!(s.contains("gblur=sigma=30"), "{s}");
        // b.mp4 has no sound: silence of the output's length, mixed
        assert!(s.contains("anullsrc=channel_layout=stereo:sample_rate=48000,atrim=duration=9"), "{s}");
        assert!(s.contains("amix=inputs=2:duration=longest:normalize=0,atrim=duration=9[aj]"), "{s}");
        assert!((p.duration_out - 9.0).abs() < 1e-9);
        assert_eq!(p.out_fps, 30.0);
        assert_eq!(p.cells.len(), 2);
        // the layout's rate (24), not the first clip's (30), with the speed on top
        let (mut sp, m) = split_spec(crate::spec::Layout { fps: 24.0, ..test_layout(LayoutMode::Columns) });
        sp.speed = 2.0;
        assert_eq!(build(&sp, &media(), &Opts { sources: Some(&m), ..opts() }).unwrap().out_fps, 48.0);
        assert_eq!((p.out_width, p.out_height), (1280, 720));
        // shortest, sound from clip 2 (silent): no audio, with a warning
        let (sp, m) = split_spec(crate::spec::Layout { sound_clip: 1, ..test_layout(LayoutMode::Columns) });
        let p = build(&sp, &media(), &Opts { sources: Some(&m), ..opts() }).unwrap();
        assert!((p.duration_out - 4.0).abs() < 1e-9);
        assert_eq!(p.audio_kbps, 0.0);
        assert!(p.warnings.iter().any(|w| w.contains("Clip 2 has no sound")), "{:?}", p.warnings);
        // a grid with an empty tile gets a color source for it
        let (sp, m) = split_spec(test_layout(LayoutMode::Grid));
        let s = strs(&build(&sp, &media(), &Opts { sources: Some(&m), ..opts() }).unwrap().passes[0]).join(" ");
        assert!(s.contains("xstack=inputs=2:layout=0_0|640_0"), "{s}");
    }

    #[test]
    fn odd_crop_scaled_keeps_square_pixels() {
        // a 16:9 crop of a 2160x3840 portrait picture is 2160x1215; ffmpeg cuts it to 2160x1214, and
        // without setsar the scale to 2048x1152 left SAR 1025:1024 (shown 2050 wide, over 4chan's 2048)
        let mut m = media();
        m.video.width = 2160;
        m.video.height = 3840;
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.crop = Some(crate::spec::Crop { x: 0.0, y: 2625.0, w: 2160.0, h: 1215.0 });
        sp.max_width = Some(2048);
        sp.max_height = Some(2048);
        let p = build(&sp, &m, &opts()).unwrap();
        assert_eq!((p.out_width, p.out_height), (2048, 1152));
        let s = strs(&p.passes[0]).join(" ");
        assert!(s.contains("crop=2160:1215:0:2625,scale=2048:1152:flags=lanczos,setsar=1"), "{s}");
    }

    #[test]
    fn zoom_output_is_the_window_size() {
        // a 2x path on 1280x720: the output is 640x360, not 1280x720 enlarged
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.zoom = vec![crate::spec::ZoomPoint { t: 10.0, zoom: 2.0, x: 0.5, y: 0.5 }, crate::spec::ZoomPoint { t: 12.0, zoom: 2.0, x: 0.3, y: 0.5 }];
        let p = build(&sp, &media(), &opts()).unwrap();
        assert_eq!((p.out_width, p.out_height), (640, 360));
        assert!(strs(&p.passes[0]).join(" ").contains(":s=640x360:"));
    }

    #[test]
    fn split_screen_loop() {
        use crate::spec::{LayoutMode, SplitLength};
        // the 4 s clip (120 frames at 30 fps, small) loops in memory up to the 9 s one; its sound loops with it
        let (sp, m) = split_spec(crate::spec::Layout { length: SplitLength::Loop, ..test_layout(LayoutMode::Columns) });
        let p = build(&sp, &media(), &Opts { sources: Some(&m), ..opts() }).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        assert!((p.duration_out - 9.0).abs() < 1e-9);
        assert!(s.contains("tpad=stop_mode=clone:stop=1,trim=end_frame=120,loop=loop=-1:size=120,pad="), "{s}");
        assert!(!s.contains("stop_duration"), "a looping clip does not hold its last frame: {s}");
        assert!(s.contains("apad,atrim=end_sample=192000,aloop=loop=-1:size=192000,atrim=duration=9[a0]"), "{s}");
        // the 9 s clip is the output's length: it does not loop
        assert_eq!(s.matches(",loop=loop=-1").count(), 1, "{s}");
        assert_eq!(s.matches("-i in.mp4").count(), 1, "{s}");

        // a large canvas: 120 frames of 1916x2160 do not fit in memory, so the range is read 3 times and joined
        let (sp, m) = split_spec(crate::spec::Layout { length: SplitLength::Loop, width: 3840, height: 2160, ..test_layout(LayoutMode::Columns) });
        let p = build(&sp, &media(), &Opts { sources: Some(&m), ..opts() }).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        assert_eq!(s.matches("-ss 9.992 -t 3.992 -i in.mp4").count(), 3, "{s}");
        // inputs: 0 in.mp4, 1 b.mp4, then the two copies
        for k in [0, 2, 3] {
            assert!(s.contains(&format!("[{k}:0]setpts=PTS-STARTPTS,fps=30,tpad=stop_mode=clone:stop=1,trim=end_frame=120")), "{k}: {s}");
        }
        assert!(s.contains("[r0_0][r0_1][r0_2]concat=n=3:v=1:a=0[r0];[r0]setpts=PTS-STARTPTS"), "{s}");
        assert!(!s.contains(",loop=loop=-1"), "{s}");
        assert!(s.contains("aloop=loop=-1:size=192000"), "{s}");
    }

    #[test]
    fn split_with_a_still_picture_and_crops() {
        use crate::spec::{LayoutMode, Rect};
        // the picture is the first clip (the job's input), the video the second
        let (mut sp, mut m) = split_spec(crate::spec::Layout {
            crops: vec![Some(Rect { x: 100, y: 0, w: 400, h: 1000 }), Some(Rect { x: 0, y: 0, w: 1080, h: 1920 })],
            ..test_layout(LayoutMode::Columns)
        });
        sp.input = "pic.jpg".into();
        sp.segments = vec![Segment { start: 0.0, end: 9.0 }, Segment { start: 0.0, end: 9.0 }];
        sp.audio = Some(AudioSel { track: 0, bitrate_k: 96.0, volume_db: 0.0, normalize: false });
        let mut pic = media();
        pic.path = "pic.jpg".into();
        pic.duration = 0.04;
        pic.still = true;
        pic.audio.clear();
        pic.video.width = 800;
        pic.video.height = 1000;
        m.insert("pic.jpg".into(), pic.clone());
        let p = build(&sp, &pic, &Opts { sources: Some(&m), ..opts() }).unwrap();
        let s = strs(p.passes.last().unwrap()).join(" ");
        // looped for the whole 9 s, not cut to its 0.04 s
        assert!(s.contains("-loop 1 -t 9 -i pic.jpg -t 8.996 -i b.mp4"), "{s}");
        assert!((p.duration_out - 9.0).abs() < 1e-9, "{}", p.duration_out);
        // its crop is used; the second clip's covers all of it, so none
        assert!(s.contains("crop=400:1000:100:0"), "{s}");
        assert_eq!(s.matches("crop=1080:1920").count(), 0, "{s}");
        // sound from the picture (clip 1): it has none, so the output is silent
        assert!(s.contains(" -an "), "{s}");
        assert!(!p.warnings.is_empty());
    }

    #[test]
    fn speed_ramp_plan() {
        use crate::spec::SpeedPoint;
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.segments = vec![Segment { start: 0.0, end: 6.0 }];
        sp.speed = 3.0; // replaced by the ramp
        sp.speed_ramp = vec![SpeedPoint { t: 0.0, speed: 0.5 }, SpeedPoint { t: 4.0, speed: 2.0 }];
        sp.audio = Some(AudioSel { track: 0, bitrate_k: 96.0, volume_db: 0.0, normalize: false });
        sp.playback = Playback::Reverse;
        let p = build(&sp, &media(), &opts()).unwrap();
        let s = strs(p.passes.last().unwrap()).join(" ");
        assert!((p.duration_out - (4f64.ln() / 0.375 + 1.0)).abs() < 1e-9, "{}", p.duration_out);
        assert!(s.contains("setpts='(log((0.5+0.375*clip(T-0,0,4))/0.5)/0.375+clip(T-4,0,2)/2)/TB',fps=30"), "{s}");
        assert!(!s.contains("setpts=PTS/3"), "{s}");
        assert!(s.contains("apad,atrim=duration=4.696785,areverse,asetpts=N/SR/TB"), "{s}");
    }

    #[test]
    fn sequence_layout_plan() {
        use crate::spec::{Fill, LayoutMode};
        let (mut sp, m) = split_spec(crate::spec::Layout { fill: Fill::Crop, ..test_layout(LayoutMode::Sequence) });
        sp.crop = Some(Crop { x: 0.0, y: 0.0, w: 100.0, h: 100.0 });
        let p = build(&sp, &media(), &Opts { sources: Some(&m), ..opts() }).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        // both clips filled into 1280x720, then joined; the crop is not used
        assert_eq!(s.matches("force_original_aspect_ratio=increase:force_divisible_by=2,crop=1280:720").count(), 2, "{s}");
        assert!(s.contains("concat=n=2:v=1:a=1[vj][aj]"), "{s}");
        assert!(!s.contains("crop=100:100"), "{s}");
        assert!(p.warnings.iter().any(|w| w.contains("Crop, rotate and flip")), "{:?}", p.warnings);
        assert!((p.duration_out - 13.0).abs() < 1e-9);
        assert!(p.cells.is_empty());
        // one clip is not a layout
        sp.segments.truncate(1);
        assert!(build(&sp, &media(), &Opts { sources: Some(&m), ..opts() }).is_err());
    }

    #[test]
    fn watermark_placement() {
        let mut o = crate::spec::Overlay { path: "w.png".into(), corner: "tl".into(), size: 10.0, opacity: 1.0, x: 0.25, y: 0.75, spacing: 0.5, angle: 0.0 };
        let (lg, place) = overlay_chain(&o, 1000, 500);
        assert_eq!(lg, "scale=100:-2,format=rgba");
        assert_eq!(place, "overlay=x=15:y=15:format=auto");
        o.corner = "custom".into();
        assert_eq!(overlay_chain(&o, 1000, 500).1, "overlay=x=(W-w)*0.25:y=(H-h)*0.75:format=auto");
        // tile: cells of 150 px, 8 x 5 of them (1200 x 750) centered on 1000 x 500
        o.corner = "tile".into();
        o.opacity = 0.5;
        let (lg, place) = overlay_chain(&o, 1000, 500);
        assert!(lg.contains("colorchannelmixer=aa=0.5,pad=150:150:"), "{lg}");
        assert!(lg.contains("loop=loop=39:size=1,tile=layout=8x5,setpts=0"), "{lg}");
        assert_eq!(place, "overlay=x=-100:y=-125:format=auto");
        // turned 45 degrees: a square grid over the 1119 px diagonal (9 x 9 cells), then rotated
        o.angle = 45.0;
        let (lg, place) = overlay_chain(&o, 1000, 500);
        assert!(lg.contains("tile=layout=9x9,setpts=0,rotate=a=0.785398:ow=iw:oh=ih:c=none"), "{lg}");
        assert_eq!(place, "overlay=x=-175:y=-425:format=auto");
        // a single image: its turned bounding box is placed
        o.corner = "br".into();
        o.angle = -30.0;
        assert!(overlay_chain(&o, 1000, 500).0.ends_with(",rotate=a=-0.523599:ow=rotw(-0.523599):oh=roth(-0.523599):c=none"));
        o.angle = 360.0;
        assert!(!overlay_chain(&o, 1000, 500).0.contains("rotate"));
    }

    #[test]
    fn caption_font_color_outline() {
        let mut c = Caption {
            text: "hi".into(),
            top: false,
            size: 10.0,
            font: None,
            color: "#ff8800".into(),
            outline_color: "#112233".into(),
            outline: 10.0,
            custom: false,
            x: 0.5,
            y: 0.9,
        };
        let f = caption_filter(&c, 1000).unwrap();
        assert!(f.contains("fontcolor=0xFF8800"), "{f}");
        assert!(f.contains(":borderw=10:bordercolor=0x112233"), "{f}");
        assert!(f.ends_with(":x=(w-text_w)/2:y=h-text_h-40"), "{f}");
        c.custom = true;
        c.x = 0.125;
        assert!(caption_filter(&c, 1000).unwrap().ends_with(":x=(w-text_w)*0.125:y=(h-text_h)*0.9"));
        c.custom = false;
        // no outline
        c.outline = 0.0;
        assert!(!caption_filter(&c, 1000).unwrap().contains("borderw"));
        // a bad color cannot reach the filter string
        c.color = "red:x=0".into();
        assert!(caption_filter(&c, 1000).unwrap().contains("fontcolor=0xFFFFFF"));
        // a file that is not a font is not used, and the plan says so
        c.font = Some("C:/Windows/win.ini".into());
        assert!(!caption_filter(&c, 1000).unwrap().contains("win.ini"));
        let mut sp = spec(Rate::Size { max_bytes: 4194304.0 });
        sp.caption = Some(c.clone());
        let p = build(&sp, &media(), &opts()).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("caption font")), "{:?}", p.warnings);
        // an installed font is used
        if let Some(face) = crate::fonts::list_fonts().into_iter().next() {
            c.font = Some(face.path.clone());
            let f = caption_filter(&c, 1000).unwrap();
            assert!(f.contains(&format!("fontfile={}", escape_filter_value(&face.path))), "{f}");
        }
    }

    /// The JSON the frontend exchanges with the engine, checked in under `contract/`.
    /// `src/lib/contract.check.ts` compares its keys with `src/lib/types.ts`, so a field
    /// renamed, added or removed on either side fails a check. After an intended change run
    /// `CWM_WRITE_CONTRACT=1 cargo test -p comfywebm-engine contract` and update types.ts.
    #[test]
    fn contract_fixtures() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("contract");
        let check = |name: &str, v: serde_json::Value| {
            let p = dir.join(name);
            let text = serde_json::to_string_pretty(&v).unwrap() + "\n";
            if std::env::var_os("CWM_WRITE_CONTRACT").is_some() {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(&p, &text).unwrap();
                return;
            }
            let have = std::fs::read_to_string(&p).unwrap_or_default().replace("\r\n", "\n");
            assert!(have == text, "{name} no longer matches the Rust types. If intended, run with CWM_WRITE_CONTRACT=1 and update src/lib/types.ts.\n{text}");
        };
        // every optional part filled, so its keys are in the file
        let mut sp = spec(Rate::Size { max_bytes: 4194304.0 });
        sp.adjust = Some(crate::spec::Adjust::default());
        sp.layout = Some(test_layout(crate::spec::LayoutMode::Columns));
        check("encode_spec.json", serde_json::to_value(&sp).unwrap());
        check("media_info.json", serde_json::to_value(media()).unwrap());
        check("settings.json", serde_json::to_value(crate::spec::Settings::default()).unwrap());
    }

    #[test]
    fn reverse_memory_warning() {
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.playback = Playback::Reverse;
        let mut m = media();
        m.video.fps = 30.0;
        // 4 s of 1280x720 at 30 fps: 120 frames x 1.4 MB = 0.17 GB, no warning
        let p = build(&sp, &m, &opts()).unwrap();
        assert!(!p.warnings.iter().any(|w| w.contains("in memory")), "{:?}", p.warnings);
        // 40 s: 1200 frames, 1.7 GB
        sp.segments = vec![Segment { start: 0.0, end: 40.0 }];
        let p = build(&sp, &m, &opts()).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("about 1.7 GB here (1200 frames at 1280x720)")), "{:?}", p.warnings);
        // the same 40 s at a 640 px box (640x360): 0.4 GB, no warning
        sp.max_width = Some(640);
        sp.max_height = Some(640);
        let p = build(&sp, &m, &opts()).unwrap();
        assert!(!p.warnings.iter().any(|w| w.contains("in memory")), "{:?}", p.warnings);
    }

    #[test]
    fn cover_long_song_lowers_audio() {
        let mut sp = spec(Rate::Size { max_bytes: 4.0 * 1024.0 * 1024.0 });
        sp.input = "in.mp3".into();
        sp.segments = vec![Segment { start: 0.0, end: 400.0 }];
        sp.audio = Some(AudioSel { track: 0, bitrate_k: 96.0, volume_db: 0.0, normalize: false });
        let mut m = sound(true);
        m.duration = 400.0;
        let p = build(&sp, &m, &opts()).unwrap();
        // 4 MiB over 400 s is 83.9 kbps in all; 96k sound alone would be 4.6 MiB. The 600x600
        // cover keeps 0.9 x 360000 + 32768 bytes, leaving 75 kbps
        assert_eq!(p.audio_kbps, 75.0, "{:?}", p.warnings);
        assert!(p.warnings.iter().any(|w| w.contains("lowered from 96 to 75 kbps")), "{:?}", p.warnings);
        let s = strs(&p.passes[p.passes.len() - 1]).join(" ");
        assert!(s.contains("-b:a 75k"), "{s}");
        // 650 s leaves 46 kbps; VP8's Vorbis does not go below 48, which still fits
        sp.segments = vec![Segment { start: 0.0, end: 650.0 }];
        m.duration = 650.0;
        let p = build(&sp, &m, &opts()).unwrap();
        assert_eq!(p.audio_kbps, 46.0);
        assert_eq!(p.warnings.len(), 1, "{:?}", p.warnings);
        sp.container = Container::Webm;
        sp.codec = Codec::Vp8;
        let p = build(&sp, &m, &opts()).unwrap();
        assert_eq!(p.audio_kbps, 48.0);
        assert!(p.warnings.iter().any(|w| w.contains("lowered from 96 to 48 kbps")), "{:?}", p.warnings);
        sp.codec = Codec::Vp9;
        // a size-mode still picture has one keyframe, at the start
        let s = strs(&build(&sp, &m, &opts()).unwrap().passes[1]).join(" ");
        assert!(!s.contains("-g 10 "), "{s}");
        // a short one keeps its bitrate
        sp.segments = vec![Segment { start: 0.0, end: 60.0 }];
        m.duration = 60.0;
        let p = build(&sp, &m, &opts()).unwrap();
        assert_eq!(p.audio_kbps, 96.0);
    }

    #[test]
    fn cover_picture_needed_for_video() {
        let mut sp = spec(Rate::Size { max_bytes: 4_000_000.0 });
        sp.input = "in.mp3".into();
        let e = build(&sp, &sound(false), &opts()).unwrap_err();
        assert_eq!(e, "Choose a picture for the video (Basic > Picture).");
        sp.cover = Some("pic.png".into());
        assert!(build(&sp, &sound(false), &opts()).unwrap_err().contains("Choose the picture again"));
    }

    #[test]
    fn sound_file_to_audio_format() {
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.input = "in.mp3".into();
        sp.container = Container::Audio;
        sp.codec = Codec::Opus;
        let p = build(&sp, &sound(false), &opts()).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        assert!(s.contains("-ss 10 -t 4 -i in.mp3 -map 0:a:0"), "{s}");
        assert!(s.contains("-c:a libopus -b:a 64k"), "{s}");
        assert!(!s.contains("loop"), "{s}");
        // a sound file cannot be joined to a video
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.segments.push(Segment { start: 0.0, end: 2.0 });
        sp.segment_sources = vec![None, Some("in.mp3".into())];
        let mut sources = std::collections::HashMap::new();
        sources.insert("in.mp3".to_string(), sound(false));
        let o = Opts { sources: Some(&sources), ..opts() };
        assert!(build(&sp, &media(), &o).unwrap_err().contains("has no video stream"));
    }

    #[test]
    fn music_replace_and_mix() {
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.music = Some(crate::spec::Music { path: "m.mp3".into(), mix: false, volume_db: -6.0, offset: 5.0 });
        let p = build(&sp, &media(), &opts()).unwrap();
        let s = strs(&p.passes[0]).join(" ");
        assert!(s.contains("-ss 5 -i m.mp3"), "{s}");
        assert!(s.contains("[1:a]atrim=duration=4,asetpts=PTS-STARTPTS,aresample=48000"), "{s}");
        assert!(s.contains("volume=-6dB[aout]"), "{s}");
        assert!(!s.contains("[0:a:0]"), "replace must not read the source's sound: {s}");
        sp.music.as_mut().unwrap().mix = true;
        let s = strs(&build(&sp, &media(), &opts()).unwrap().passes[0]).join(" ");
        assert!(s.contains("[0:a:0]asetpts=PTS-STARTPTS[a0]"), "{s}");
        assert!(s.contains("[asrc][amus]amix=inputs=2:duration=first:normalize=0[aout]"), "{s}");
    }

    #[test]
    fn gif_args() {
        let mut sp = spec(Rate::Size { max_bytes: 4_194_304.0 });
        sp.container = Container::Gif;
        sp.codec = Codec::Gif;
        sp.fps = Some(15.0);
        let p = build(&sp, &media(), &Opts { output: "out.gif", ..opts() }).unwrap();
        assert_eq!(p.passes.len(), 1);
        let s = strs(&p.passes[0]).join(" ");
        assert!(s.contains("fps=15,"), "{s}");
        assert!(s.contains("palettegen=stats_mode=diff[gpal];[gs1][gpal]paletteuse"), "{s}");
        assert!(!s.contains("yuv420p") && !s.contains("-c:a"), "{s}");
        assert!(s.ends_with("-c:v gif -threads 8 -an -map_metadata -1 -map_chapters -1 -sn -loop 0 -f gif out.gif"), "{s}");
        // GIF only with the GIF codec
        sp.codec = Codec::Vp9;
        assert!(build(&sp, &media(), &opts()).is_err());
    }

    #[test]
    fn mp4_h264_args() {
        let mut sp = spec(Rate::Size { max_bytes: 2_000_000.0 });
        sp.container = Container::Mp4;
        sp.codec = Codec::H264;
        sp.effort = Effort::Fast;
        let p = build(&sp, &media(), &Opts { output: "out.mp4", ..opts() }).unwrap();
        assert_eq!(p.passes.len(), 2);
        assert!(p.reuse_pass1);
        let s = strs(&p.passes[1]).join(" ");
        let at = s.find("-pix_fmt").unwrap();
        assert_eq!(
            &s[at..],
            "-pix_fmt yuv420p -c:v libx264 -profile:v high -preset fast -b:v 3877k -threads 8 -pass:v 2 \
             -passlogfile:v log -colorspace bt709 -color_primaries bt709 -color_trc bt709 -c:a aac -b:a 64k -ac 2 \
             -map_metadata -1 -map_chapters -1 -sn -movflags +faststart -f mp4 out.mp4"
        );
        // VP9 in MP4 is refused
        sp.codec = Codec::Vp9;
        assert!(build(&sp, &media(), &opts()).is_err());
    }

    #[test]
    fn adjust_and_vflip() {
        assert!(adjust_filters(&Adjust::default()).is_empty());
        // an identity curve is not a filter
        assert!(adjust_filters(&Adjust { curve: vec![(0.0, 0.0), (1.0, 1.0)], ..Default::default() }).is_empty());
        let a = Adjust { brightness: 0.05, saturation: 1.2, hue: 10.0, curve: vec![(1.0, 1.0), (0.0, 0.0), (0.5, 0.6)], ..Default::default() };
        assert_eq!(
            adjust_filters(&a),
            vec!["eq=brightness=0.05:contrast=1:saturation=1.2:gamma=1", "hue=h=10", "curves=master='0/0 0.5/0.6 1/1'"]
        );
        let mut sp = spec(Rate::Crf { crf: 30.0, max_bitrate_k: None });
        sp.vflip = true;
        sp.adjust = Some(Adjust { gamma: 1.5, ..Default::default() });
        let p = build(&sp, &media(), &opts()).unwrap();
        let args = p.passes.last().unwrap();
        let vf = &args[args.iter().position(|x| x == "-vf").unwrap() + 1];
        // adjustments before the crop and flips, the vertical flip after rotate/mirror
        assert!(vf.contains("eq=brightness=0:contrast=1:saturation=1:gamma=1.5"), "{vf}");
        assert!(vf.contains("vflip"), "{vf}");
    }

    #[test]
    fn speed_sets_output_rate() {
        let crf = Rate::Crf { crf: 30.0, max_bitrate_k: None };
        let vf = |sp: &EncodeSpec| {
            let p = build(sp, &media(), &opts()).unwrap();
            let a = p.passes.last().unwrap().clone();
            let i = a.iter().position(|x| x == "-vf" || x == "-filter_complex").unwrap();
            a[i + 1].clone()
        };
        // 30 fps at 1.5x: every frame kept at 45 fps
        let mut sp = spec(crf);
        sp.speed = 1.5;
        assert!(vf(&sp).contains("setpts=PTS/1.5,fps=45"), "{}", vf(&sp));
        // 0.5x: 15 fps
        sp.speed = 0.5;
        assert!(vf(&sp).contains("setpts=PTS/0.5,fps=15"), "{}", vf(&sp));
        // a cap below the new rate wins
        sp.speed = 2.0;
        sp.fps = Some(30.0);
        assert!(vf(&sp).contains("setpts=PTS/2,fps=30"), "{}", vf(&sp));
        // normal speed: no fps filter
        sp.speed = 1.0;
        sp.fps = None;
        assert!(!vf(&sp).contains("fps="), "{}", vf(&sp));
    }

    #[test]
    fn bpp_quality() {
        assert!((bits_per_pixel(1000.0, 1000, 1000, 10.0) - 0.1).abs() < 1e-12);
        assert_eq!(quality(0.045, Codec::Vp9), Quality::Good);
        assert_eq!(quality(0.031, Codec::Vp9), Quality::Fair);
        assert_eq!(quality(0.029, Codec::Vp9), Quality::Poor);
        assert_eq!(quality(0.050, Codec::H264), Quality::Fair);
        assert_eq!(quality(0.034, Codec::Av1), Quality::Good);
    }
}
