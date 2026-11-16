//! Pure planning: bitrate math, output size, filter graph, ffmpeg arguments
//! and warnings. Nothing here touches the file system or spawns processes.

use crate::spec::{
    Adjust, Caption,
    Codec, Container, Crop, Effort, EncodeSpec, MediaInfo, PlanInfo, Playback, Quality, Rate, Segment, SubsSel,
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
    /// video bitrate passed to the encoder (size mode), or the cap (crf mode)
    pub video_kbps: Option<f64>,
    pub audio_kbps: f64,
    pub bits_per_pixel: Option<f64>,
    pub quality: Option<Quality>,
    pub auto_scaled: bool,
    /// one argument list per ffmpeg run, program name not included
    pub passes: Vec<Vec<String>>,
    /// a retry may run only pass 2 and reuse the pass 1 log (libvpx, x264)
    pub reuse_pass1: bool,
    pub warnings: Vec<String>,
}

impl Planned {
    pub fn info(&self) -> PlanInfo {
        PlanInfo {
            out_width: self.out_width,
            out_height: self.out_height,
            duration_out: self.duration_out,
            video_kbps: self.video_kbps.map(|k| k.round()),
            audio_kbps: self.audio_kbps,
            bits_per_pixel: self.bits_per_pixel.map(|b| (b * 10000.0).round() / 10000.0),
            quality: self.quality,
            auto_scaled: self.auto_scaled,
            command: self.passes.iter().map(|a| shell_join("ffmpeg", a)).collect::<Vec<_>>().join("\n"),
            warnings: self.warnings.clone(),
            output: None,
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
/// Share of a size limit (as kbps) kept for a still picture when the sound would
/// otherwise fill it: a 640x360 keyframe every 10 s is about 20-50 KB.
const STILL_VIDEO_KBPS: f64 = 40.0;
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

/// autoScale: the largest size from {source, LADDER} within max_w/max_h whose
/// bits per pixel reach the codec's floor. The ladder applies to the shorter
/// side, so a portrait clip at "720" is 720 pixels wide. Returns the smallest
/// candidate and false when none qualifies.
pub fn auto_scale(g: Geometry, max_w: Option<u32>, max_h: Option<u32>, kbps: f64, fps: f64, codec: Codec) -> (Geometry, bool) {
    let (rw, rh) = (g.rot_w as f64, g.rot_h as f64);
    let short = rw.min(rh);
    let limit = fit_factor(g.rot_w, g.rot_h, max_w, max_h);
    let mut factors = vec![1.0f64];
    factors.extend(LADDER.iter().map(|h| *h as f64).filter(|h| *h < short).map(|h| h / short));
    let mut last = g;
    for f in factors {
        let f = f.min(limit);
        let cand = Geometry { out_w: even_down(rw * f), out_h: even_down(rh * f), ..g };
        last = cand;
        if bits_per_pixel(kbps, cand.out_w, cand.out_h, fps) >= bpp_floor(codec) {
            return (cand, true);
        }
    }
    (last, false)
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

/// The drawtext filter for a caption on a picture `out_h` pixels high.
pub fn caption_filter(c: &Caption, out_h: u32) -> Option<String> {
    let text = c.text.trim_end();
    if text.trim().is_empty() {
        return None;
    }
    let size = ((out_h as f64) * c.size.clamp(2.0, 30.0) / 100.0).round().max(8.0);
    let border = (size / 14.0).round().max(1.0);
    let margin = (out_h as f64 * 0.04).round();
    let y = if c.top { format!("{margin}") } else { format!("h-text_h-{margin}") };
    let font = caption_font().map(|f| format!("fontfile={f}:")).unwrap_or_default();
    // centered lines (text_align needs ffmpeg 6.1 or newer, so only when there are several)
    let align = if text.contains('\n') { "text_align=C:" } else { "" };
    Some(format!(
        "drawtext={font}{align}text={}:expansion=none:fontsize={size}:fontcolor=white:borderw={border}:bordercolor=black:line_spacing={}:x=(w-text_w)/2:y={y}",
        escape_filter_text(text),
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
    pub stabilize: bool,
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
    if sf.stabilize {
        // before the crop, so the borders it pulls in can be cropped away
        f.push("deshake".into());
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
    /// a complete drawtext filter, drawn after the scale
    pub caption: Option<String>,
    /// make in-between frames up to this rate after the speed change (replaces `fps`)
    pub interpolate: Option<f64>,
    pub speed: f64,
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
    if let Some((w, h)) = p.scale {
        lin.push(format!("scale={w}:{h}:flags=lanczos"));
    }
    if let Some(c) = &p.caption {
        lin.push(c.clone());
    }
    if (p.speed - 1.0).abs() > 1e-9 {
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
    if spec.container == Container::Audio {
        return build_audio(spec, info, opts);
    }
    if spec.stream_copy {
        return build_copy(spec, info, opts);
    }
    let mut warnings = Vec::new();
    // a sound file becomes a video with a still picture: the chosen one, else the embedded cover
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
    let v = still.as_ref().map_or(&info.video, |(_, cv)| cv);
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
    let mixed = seg_src.iter().any(|s| s.is_some());
    if let (None, Some(m)) = (still, seg_src.iter().flatten().find(|m| m.audio_only)) {
        return Err(format!("{} has no video stream, so it cannot be joined to a video.", m.file_name));
    }
    let segs = if mixed {
        let mut out = Vec::new();
        for (i, g) in spec.segments.iter().enumerate() {
            let d = seg_src[i].map_or(info.duration, |m| m.duration);
            out.extend(resolve_segments(std::slice::from_ref(g), d).map_err(|e| e.replace("Range 1", &format!("Range {}", i + 1)))?);
        }
        out
    } else {
        resolve_segments(&spec.segments, info.duration)?
    };
    let clip: f64 = segs.iter().map(|(s, e)| e - s).sum();
    let speed = spec.speed;
    // a still picture has nothing to play backwards; the sound plays forward
    let boomerang = spec.playback == Playback::Boomerang && still.is_none();
    if still.is_some() && spec.playback == Playback::Boomerang {
        warnings.push("Boomerang applies to the picture only; the sound plays forward.".into());
    }
    let duration_out = clip / speed * if boomerang { 2.0 } else { 1.0 };

    let src_fps_out = v.fps * speed;
    let fps_cap = spec
        .fps
        .filter(|f| f.is_finite() && *f > 0.0)
        .filter(|f| src_fps_out <= 0.0 || *f < src_fps_out - 0.01)
        .filter(|_| still.is_none());
    // GIF frame delays are whole centiseconds and players treat under 2 as
    // slow, so a GIF is capped at 50 fps
    let fps_cap = if codec.is_image() && fps_cap.unwrap_or(src_fps_out) > 50.0 { Some(50.0) } else { fps_cap };
    // smooth slow motion: frames are made up to the source rate (or the cap)
    let interpolate = (spec.smooth_slowmo && speed < 1.0 - 1e-9 && v.fps > 0.0 && !codec.is_image() && still.is_none())
        .then(|| fps_cap.map_or(v.fps, |c| c.min(v.fps)));
    let still_rate = still.map(|_| still_rate(duration_out, spec.fade_in > 0.0 || spec.fade_out > 0.0));
    let out_fps = match &still_rate {
        Some((fps, _)) => *fps,
        None => interpolate.unwrap_or(fps_cap.unwrap_or(if src_fps_out > 0.0 { src_fps_out } else { 30.0 })),
    };
    if interpolate.is_some() && duration_out > 20.0 {
        warnings.push("Smooth slow motion computes every new frame and is slow: expect minutes per second of output.".into());
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
            if !replaced && info.audio.get(a.track as usize).is_none() {
                return Err(format!("Audio track {} does not exist.", a.track));
            }
            if !a.bitrate_k.is_finite() || a.bitrate_k <= 0.0 {
                return Err("The audio bitrate must be greater than 0.".into());
            }
            Some(a)
        }
        None => None,
    };
    // a still picture needs little of the budget, but a long song at the chosen bitrate can
    // be larger than the limit on its own: the sound bitrate is lowered to leave the picture
    // STILL_VIDEO_KBPS
    let mut audio = audio;
    if let (Some(_), Rate::Size { max_bytes }, Some(a)) = (still, &spec.rate, audio.as_mut()) {
        let max_bytes = *max_bytes;
        let room = (max_bytes * 8.0 / 1000.0 / duration_out) * 0.985 - STILL_VIDEO_KBPS;
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
    let audio_kbps = audio.map(|a| a.bitrate_k).unwrap_or(0.0);

    // --- rate ---
    let size_mode = matches!(spec.rate, Rate::Size { .. });
    let video_kbps = match spec.rate {
        Rate::Size { max_bytes } => {
            if !max_bytes.is_finite() || max_bytes <= 0.0 {
                return Err("The size limit must be greater than 0.".into());
            }
            let computed = size_video_kbps(max_bytes, duration_out, audio_kbps);
            if computed < MIN_VIDEO_KBPS {
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
    let crop = if still.is_some() { None } else { spec.crop };
    let mut g = geometry(v.width, v.height, crop, spec.rotate, spec.max_width, spec.max_height);
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
        let frames = (clip / speed * out_fps).ceil();
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
    if let (true, Some(k), Some(b), Some(Quality::Poor)) = (size_mode, video_kbps, bpp, qual) {
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
    let subtitles = match &spec.subs {
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

    // --- filter graph ---
    let sf = SegmentFilters {
        deinterlace: spec.deinterlace,
        denoise: spec.denoise,
        stabilize: spec.stabilize,
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
                None => segment_chain(&sf, *s),
                Some(m) => segment_chain(
                    &SegmentFilters {
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
    if mixed && spec.subs.is_some() {
        warnings.push("Subtitles are burned in only on the ranges from the first file.".into());
    }
    let vpost = video_post(&PostFilters {
        scale: ((g.out_w, g.out_h) != (g.rot_w, g.rot_h)).then_some((g.out_w, g.out_h)),
        caption: spec.caption.as_ref().and_then(|c| caption_filter(c, g.out_h)),
        speed: if still.is_some() { 1.0 } else { speed },
        // After setpts the filter graph still reports the source rate, and
        // ffmpeg then writes the file at that rate: 1.5x of 30 fps kept 2 of
        // every 3 frames at 30 fps instead of all of them at 45 fps. Naming
        // the new rate keeps every frame (they already sit on its grid).
        fps: fps_cap.or(((speed - 1.0).abs() > 1e-9 && src_fps_out > 0.0 && still.is_none()).then_some(src_fps_out)),
        interpolate,
        playback: if still.is_some() { Playback::Forward } else { spec.playback },
        fade_in: spec.fade_in.max(0.0),
        fade_out: spec.fade_out.max(0.0),
        duration_out,
        color: color_tags(v, tonemap),
    });
    let apost = audio.map(|a| {
        audio_post(
            speed,
            spec.playback == Playback::Reverse,
            a.normalize,
            a.volume_db,
            spec.fade_in.max(0.0),
            spec.fade_out.max(0.0),
            duration_out,
        )
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
    // logo: an extra input after the ranges, scaled to a share of the output
    // width, placed in a corner; drawn after everything else (before the GIF palette)
    let overlay = spec.overlay.as_ref().filter(|o| !o.path.trim().is_empty()).map(|o| {
        let w = ((g.out_w as f64 * o.size.clamp(2.0, 100.0) / 100.0 / 2.0).round() as u32 * 2).max(2);
        let m = (g.out_w.min(g.out_h) as f64 * 0.03).round();
        let x = if o.corner.ends_with('l') { format!("{m}") } else { format!("W-w-{m}") };
        let y = if o.corner.starts_with('t') { format!("{m}") } else { format!("H-h-{m}") };
        let a = o.opacity.clamp(0.0, 1.0);
        let fade = if a < 1.0 { format!(",colorchannelmixer=aa={}", fmt_num((a * 100.0).round() / 100.0)) } else { String::new() };
        (o.path.clone(), format!("scale={w}:-2,format=rgba{fade}"), format!("overlay=x={x}:y={y}:format=auto"))
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
            push(&mut a, &["-pass", &pass.to_string(), "-passlogfile", opts.passlog]);
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
            stabilize: false,
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
            if *s > 0.0 {
                push(&mut a, &["-ss", &fmt_num(*s)]);
            }
            let path = seg_src[i].map_or(spec.input.as_str(), |m| m.path.as_str());
            push(&mut a, &["-t", &fmt_num(e - s), "-i", path]);
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
                let chain = if chain.is_empty() { "null" } else { chain.as_str() };
                if still.is_none() {
                    let vi = seg_src[i].map_or(v.stream_index, |m| m.video.stream_index);
                    graph.push_str(&format!("[{i}:{vi}]{chain}[v{i}];"));
                }
                if let (true, Some(t)) = (with_audio && !music_replaces, track) {
                    // joined files: one sample format and layout for concat; a
                    // file without sound gives silence of the range's length
                    let norm = if mixed { ",aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo" } else { "" };
                    match seg_src[i] {
                        Some(m) if m.audio.is_empty() => graph.push_str(&format!(
                            "anullsrc=channel_layout=stereo:sample_rate=48000,atrim=duration={},aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}];",
                            fmt_num(segs[i].1 - segs[i].0)
                        )),
                        Some(m) => {
                            let ft = if (t as usize) < m.audio.len() { t } else { 0 };
                            graph.push_str(&format!("[{i}:a:{ft}]asetpts=PTS-STARTPTS{norm}[a{i}];"));
                        }
                        None => graph.push_str(&format!("[{i}:a:{t}]asetpts=PTS-STARTPTS{norm}[a{i}];")),
                    }
                }
            }
            let src_audio = with_audio && !music_replaces;
            for i in 0..segs.len() {
                if still.is_none() {
                    graph.push_str(&format!("[v{i}]"));
                }
                if src_audio {
                    graph.push_str(&format!("[a{i}]"));
                }
            }
            let n = segs.len();
            match (&still_chain, src_audio) {
                (None, true) => graph.push_str(&format!("concat=n={n}:v=1:a=1[vj][aj];")),
                (None, false) => graph.push_str(&format!("concat=n={n}:v=1:a=0[vj];")),
                (Some(c), true) => graph.push_str(&format!("concat=n={n}:v=0:a=1[aj];{c}")),
                (Some(c), false) => graph.push_str(c),
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
        video_kbps,
        audio_kbps,
        bits_per_pixel: bpp,
        quality: qual,
        auto_scaled,
        passes,
        reuse_pass1,
        warnings,
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
    if spec.fps.is_some() || (spec.speed - 1.0).abs() > 1e-9 || spec.playback != Playback::Forward {
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
        video_kbps: None,
        audio_kbps: 0.0,
        bits_per_pixel: None,
        quality: None,
        auto_scaled: false,
        passes: vec![a],
        reuse_pass1: false,
        warnings,
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
    let duration_out = clip / spec.speed;
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
    let apost = audio_post(
        spec.speed,
        spec.playback == Playback::Reverse,
        au.normalize,
        au.volume_db,
        spec.fade_in.max(0.0),
        spec.fade_out.max(0.0),
        duration_out,
    );
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
        // size mode: the bitrate the size-fit loop steers (here the sound's)
        video_kbps: size_mode.then_some(kbps),
        audio_kbps: if size_mode { 0.0 } else { kbps },
        // (with the cap reached, the bitrate is reported as audio and no retry runs)
        bits_per_pixel: None,
        quality: None,
        auto_scaled: false,
        passes: vec![a],
        reuse_pass1: false,
        warnings,
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
            playback: Playback::Forward,
            deinterlace: false,
            denoise: false,
            stabilize: false,
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
            stabilize: false,
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
            caption: None,
            interpolate: None,
            scale: Some((320, 360)),
            speed: 2.0,
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
                "scale=320:360:flags=lanczos,setpts=PTS/2,fps=24,split[bf][bb];[bb]reverse[br];[bf][br]concat=n=2:v=1:a=0,\
                 fade=t=in:st=0:d=0.5,fade=t=out:st=3:d=1,{TAGS}"
            )
        );
    }

    #[test]
    fn size_mode_args() {
        let p = build(&spec(Rate::Size { max_bytes: 300_000.0 }), &media(), &opts()).unwrap();
        assert_eq!(p.passes.len(), 2);
        // 300000*8/1000/4 = 600; (600-64)*0.985 = 527.96
        assert_eq!(p.video_kbps.unwrap().round(), 528.0);
        let vp9 = "-c:v libvpx-vp9 -row-mt 1 -tile-columns 2 -frame-parallel 0 -deadline good";
        let head = "-hide_banner -y -ss 10 -t 4 -i in.mp4";
        let tags = "-colorspace bt709 -color_primaries bt709 -color_trc bt709";
        assert_eq!(
            strs(&p.passes[0]).join(" "),
            format!(
                "{head} -map 0:0 -vf setpts=PTS-STARTPTS,{TAGS} -pix_fmt yuv420p {vp9} -cpu-used 4 -auto-alt-ref 1 -lag-in-frames 25 \
                 -b:v 528k -threads 8 -pass 1 -passlogfile log {tags} -an -f null -"
            )
        );
        assert_eq!(
            strs(&p.passes[1]).join(" "),
            format!(
                "{head} -map 0:0 -map 0:a:0 -vf setpts=PTS-STARTPTS,{TAGS} -af asetpts=PTS-STARTPTS -pix_fmt yuv420p {vp9} -cpu-used 2 -auto-alt-ref 1 \
                 -lag-in-frames 25 -b:v 528k -threads 8 -pass 2 -passlogfile log {tags} -c:a libopus -b:a 64k -ac 2 \
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
        assert_eq!(&s[..7], ["-hide_banner", "-y", "-t", "60", "-i", "in.mp4", "-map"]);
        let tail = s[s.iter().position(|a| *a == "-cpu-used").unwrap()..].join(" ");
        assert_eq!(
            tail,
            "-cpu-used 1 -auto-alt-ref 1 -lag-in-frames 25 -crf 30 -b:v 0 -threads 8 -colorspace bt709 \
             -color_primaries bt709 -color_trc bt709 -an -map_metadata -1 -map_chapters -1 -sn -metadata \
             title=hello world -f webm out.webm"
        );
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
        assert_eq!(&s[2..12], ["-ss", "1", "-t", "2", "-i", "in.mp4", "-ss", "10", "-t", "2.5"]);
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
        assert!(s.contains("-t 3 -i b.mp4"), "{s}");
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

    /// The JSON the frontend exchanges with the engine, checked in under `contract/`.
    /// `src/lib/contract.check.ts` compares its keys with `src/lib/types.ts`, so a field
    /// renamed, added or removed on either side fails a check. After an intended change run
    /// `CWM_WRITE_CONTRACT=1 cargo test -p comfywebm-engine contract` and update types.ts.
    #[test]
    fn contract_fixtures() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("contract");
        let check = |name: &str, v: serde_json::Value| {
            let p = dir.join(name);
            let text = serde_json::to_string_pretty(&v).unwrap() + "
";
            if std::env::var_os("CWM_WRITE_CONTRACT").is_some() {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(&p, &text).unwrap();
                return;
            }
            let have = std::fs::read_to_string(&p).unwrap_or_default().replace("
", "
");
            assert!(have == text, "{name} no longer matches the Rust types. If intended, run with CWM_WRITE_CONTRACT=1 and update src/lib/types.ts.
{text}");
        };
        // every optional part filled, so its keys are in the file
        let mut sp = spec(Rate::Size { max_bytes: 4194304.0 });
        sp.adjust = Some(crate::spec::Adjust::default());
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
        // 4 MiB over 400 s is 83.9 kbps in all; 96k sound alone would be 4.6 MiB
        assert_eq!(p.audio_kbps, 42.0, "{:?}", p.warnings);
        assert!(p.warnings.iter().any(|w| w.contains("lowered from 96 to 42 kbps")), "{:?}", p.warnings);
        let s = strs(&p.passes[p.passes.len() - 1]).join(" ");
        assert!(s.contains("-b:a 42k"), "{s}");
        // VP8's Vorbis does not go below 48 kbps; that still fits here
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
            "-pix_fmt yuv420p -c:v libx264 -profile:v high -preset fast -b:v 3877k -threads 8 -pass 2 \
             -passlogfile log -colorspace bt709 -color_primaries bt709 -color_trc bt709 -c:a aac -b:a 64k -ac 2 \
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
