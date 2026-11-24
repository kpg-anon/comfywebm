//! The IPC contract types, shared with src/lib/types.ts. JSON field names are camelCase.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub path: String,
    pub file_name: String,
    pub size_bytes: u64,
    /// seconds
    pub duration: f64,
    /// container, from ffprobe format_long_name (or format_name)
    #[serde(default)]
    pub format: String,
    /// overall bitrate
    #[serde(default)]
    pub bitrate_k: Option<u32>,
    /// creation_time and encoder tags, when present
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub encoder: Option<String>,
    /// For a file with sound and no video stream (`audio_only`): the embedded
    /// cover picture when there is one (codec, size, stream index), else all zero.
    pub video: VideoInfo,
    /// the file has sound but no video stream; a cover picture is not video
    #[serde(default)]
    pub audio_only: bool,
    /// a single picture (PNG, JPEG, WebP, BMP...): it has no length of its own
    #[serde(default)]
    pub still: bool,
    pub audio: Vec<AudioTrack>,
    pub subs: Vec<SubTrack>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub codec: String,
    /// display pixels (rotation and sample aspect ratio applied)
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub bitrate_k: Option<u32>,
    pub interlaced: bool,
    /// pixel format, profile, "space/primaries/transfer", frame count, variable frame rate
    #[serde(default)]
    pub pix_fmt: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frames: Option<u64>,
    #[serde(default)]
    pub vfr: bool,
    /// color_transfer is smpte2084 (PQ) or arib-std-b67 (HLG)
    #[serde(default)]
    pub hdr: bool,
    /// source color tags from ffprobe (None when missing or "unknown"); not sent over IPC
    #[serde(skip)]
    pub color_space: Option<String>,
    #[serde(skip)]
    pub color_primaries: Option<String>,
    #[serde(skip)]
    pub color_trc: Option<String>,
    /// absolute stream index of the video stream used (ffmpeg `0:<index>`); not sent over IPC
    #[serde(skip)]
    pub stream_index: u32,
    /// sample aspect ratio is not 1:1, so the filter graph first resamples to square pixels
    #[serde(skip)]
    pub anamorphic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrack {
    /// n-th audio stream, 0-based (ffmpeg `0:a:n`)
    pub track: u32,
    pub codec: String,
    pub channels: u32,
    pub lang: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub bitrate_k: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubTrack {
    /// n-th subtitle stream, 0-based (ffmpeg `0:s:n`)
    pub track: u32,
    pub codec: String,
    pub lang: Option<String>,
    pub title: Option<String>,
    pub text_based: bool,
}

/// Color adjustments (ffmpeg `eq`, `hue`, `curves`). Neutral values: 0, 1, 1, 1, 0, empty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Adjust {
    /// -1..1, added to luma
    pub brightness: f64,
    /// 0..3, 1 = unchanged
    pub contrast: f64,
    /// 0..3, 1 = unchanged
    pub saturation: f64,
    /// 0.1..10, 1 = unchanged
    pub gamma: f64,
    /// degrees, -180..180
    pub hue: f64,
    /// master curve points (input, output), both 0..1; fewer than 2 = none
    pub curve: Vec<(f64, f64)>,
    /// white balance, -1..1, 0 = unchanged: above 0 warmer (more red, less blue)
    #[serde(default)]
    pub temperature: f64,
    /// -1..1, 0 = unchanged: above 0 more magenta, below more green
    #[serde(default)]
    pub tint: f64,
}

impl Default for Adjust {
    fn default() -> Self {
        Adjust { brightness: 0.0, contrast: 1.0, saturation: 1.0, gamma: 1.0, hue: 0.0, curve: Vec::new(), temperature: 0.0, tint: 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum Rate {
    /// output file must be <= max_bytes
    Size { max_bytes: f64 },
    /// crf on the codec's own scale (vp9/av1/vp8 0-63, h264/hevc 0-51)
    Crf { crf: f64, max_bitrate_k: Option<f64> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    Vp9,
    Vp8,
    Av1,
    H264,
    Hevc,
    /// animated GIF (ffmpeg's own encoder, two-step palette)
    Gif,
    /// animated WebP (libwebp_anim)
    Webp,
    /// audio only (container `audio`): .opus, .mp3, .m4a
    Opus,
    Mp3,
    Aac,
}

impl Codec {
    /// Animated image formats: no audio, no bitrate control.
    pub fn is_image(self) -> bool {
        matches!(self, Codec::Gif | Codec::Webp)
    }

    /// Audio-only output codecs.
    pub fn is_audio(self) -> bool {
        matches!(self, Codec::Opus | Codec::Mp3 | Codec::Aac)
    }
}

impl Codec {
    pub fn name(self) -> &'static str {
        match self {
            Codec::Vp9 => "vp9",
            Codec::Vp8 => "vp8",
            Codec::Av1 => "av1",
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
            Codec::Gif => "gif",
            Codec::Webp => "webp",
            Codec::Opus => "opus",
            Codec::Mp3 => "mp3",
            Codec::Aac => "aac",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    #[default]
    Webm,
    Mp4,
    Gif,
    Webp,
    /// sound only; the file type follows the codec
    Audio,
}

impl Container {
    pub fn ext(self) -> &'static str {
        match self {
            Container::Webm => "webm",
            Container::Mp4 => "mp4",
            Container::Gif => "gif",
            Container::Webp => "webp",
            Container::Audio => "m4a",
        }
    }
}

/// A sound file (music) for the output: it replaces the source's sound or is
/// mixed under it, from `offset` seconds into the file, cut to the output's length.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Music {
    pub path: String,
    /// true = mix with the source's sound; false = replace it
    #[serde(default)]
    pub mix: bool,
    #[serde(default)]
    pub volume_db: f64,
    #[serde(default)]
    pub offset: f64,
}

/// Splits: two or more clips (the `segments`, each from its `segment_sources`
/// file) combined into one picture of `width` x `height`, one after another or
/// at the same time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    #[serde(default)]
    pub mode: LayoutMode,
    /// the canvas, before the output size is applied
    pub width: u32,
    pub height: u32,
    /// how each clip fills its space
    #[serde(default)]
    pub fill: Fill,
    /// frame rate of the combined picture
    pub fps: f64,
    /// split screen: where the sound comes from
    #[serde(default)]
    pub sound: SplitSound,
    /// split screen, `sound` = clip: index of that clip
    #[serde(default)]
    pub sound_clip: usize,
    /// split screen: the shortest clip's length, or the longest (the others hold their last frame, or loop)
    #[serde(default)]
    pub length: SplitLength,
    /// split screen: space between the parts, in canvas pixels
    #[serde(default)]
    pub gap: u32,
    #[serde(default = "black")]
    pub gap_color: String,
    /// side by side / stacked: relative size of each part (empty = equal)
    #[serde(default)]
    pub weights: Vec<f64>,
    /// grid: columns (0 = as square as possible)
    #[serde(default)]
    pub columns: u32,
    /// the part of each clip that is used, in that clip's display pixels (None = all of it)
    #[serde(default)]
    pub crops: Vec<Option<Rect>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    /// one after another
    #[default]
    Sequence,
    /// side by side
    Columns,
    /// one above the other
    Rows,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fill {
    /// the whole picture, with bars
    #[default]
    Bars,
    /// fill the space, cutting the edges
    Crop,
    /// the whole picture over a blurred copy of itself
    Blur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitSound {
    #[default]
    Clip,
    Mix,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitLength {
    #[default]
    Shortest,
    /// the longest clip's length; the shorter clips hold their last frame
    Longest,
    /// the longest clip's length; the shorter clips start again from their start
    Loop,
}

/// A point of a speed ramp: at `t` seconds of the source file the speed is `speed`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpeedPoint {
    pub t: f64,
    pub speed: f64,
}

/// A point of the zoom: at `t` seconds of the source file the picture is
/// zoomed `zoom` times (1 = all of it) around the centre (`x`, `y`), 0..1 of
/// the picture after crop, rotation and flips (zoom.rs). With a follow window
/// (`EncodeSpec::follow_window`) the centre is 0..1 of the whole picture
/// (rotation and flips, no crop) and `rot` turns the view.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ZoomPoint {
    pub t: f64,
    pub zoom: f64,
    pub x: f64,
    pub y: f64,
    /// degrees the picture is turned clockwise around the view's centre (follow window only)
    #[serde(default)]
    pub rot: f64,
}

/// Follow's view at 1x: its size in pixels of the whole picture after rotation and
/// flips (the crop's size, or the whole picture). At zoom z the view is this over z,
/// and it moves over the whole picture (zoom.rs `window_filter`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FollowWindow {
    pub w: f64,
    pub h: f64,
}

/// A rectangle in canvas pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// A watermark image over the output picture.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overlay {
    pub path: String,
    /// placement: a corner ("tl", "tr", "bl", "br"), "custom" (at `x`, `y`)
    /// or "tile" (repeated over the picture)
    #[serde(default = "corner_br")]
    pub corner: String,
    /// width in percent of the output picture width
    #[serde(default = "overlay_size")]
    pub size: f64,
    /// 0..1
    #[serde(default = "one_f")]
    pub opacity: f64,
    /// "custom": 0..1 of the room beside (x) and below (y) the image; 0,0 = top left
    #[serde(default = "one_f")]
    pub x: f64,
    #[serde(default = "one_f")]
    pub y: f64,
    /// "tile": space between copies, as a multiple of the image width
    #[serde(default = "half")]
    pub spacing: f64,
    /// degrees, clockwise; for "tile" the whole grid is turned
    #[serde(default)]
    pub angle: f64,
}

fn half() -> f64 {
    0.5
}

fn corner_br() -> String {
    "br".into()
}

fn overlay_size() -> f64 {
    15.0
}

fn one_f() -> f64 {
    1.0
}

/// A line (or lines) of text on the picture, centered at the top or bottom.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caption {
    pub text: String,
    /// true = top edge, false = bottom edge (when not `custom`)
    #[serde(default)]
    pub top: bool,
    /// placed at `x`, `y` instead of the top or bottom edge
    #[serde(default)]
    pub custom: bool,
    /// `custom`: 0..1 of the room beside (x) and below (y) the text; 0,0 = top left
    #[serde(default = "half")]
    pub x: f64,
    #[serde(default = "caption_y")]
    pub y: f64,
    /// text height in percent of the output picture height
    #[serde(default = "caption_size")]
    pub size: f64,
    /// font file; None = a bold sans the system has (Arial Bold, DejaVu Sans Bold)
    #[serde(default)]
    pub font: Option<String>,
    /// text color, "#rrggbb"
    #[serde(default = "white")]
    pub color: String,
    /// outline color, "#rrggbb"
    #[serde(default = "black")]
    pub outline_color: String,
    /// outline width in percent of the text height; 0 = no outline
    #[serde(default = "caption_outline")]
    pub outline: f64,
}

fn caption_size() -> f64 {
    7.0
}

fn caption_y() -> f64 {
    0.9
}

fn caption_outline() -> f64 {
    7.0
}

fn white() -> String {
    "#ffffff".into()
}

fn black() -> String {
    "#000000".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Playback {
    #[default]
    Forward,
    Reverse,
    /// forward then reverse
    Boomerang,
}

/// How Stabilize steadies the picture (Settings > Tools).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StabMethod {
    /// ffmpeg's deshake: one pass, shifts only
    #[default]
    Deshake,
    /// vid.stab: an analysis pass per range, then vidstabtransform
    Vidstab,
    /// Gyroflow (gyro.rs): the span of the ranges steadied by Gyroflow before the encode
    Gyroflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Fast,
    Balanced,
    Best,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    /// seconds on the source timeline
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSel {
    pub track: u32,
    pub bitrate_k: f64,
    #[serde(default)]
    pub volume_db: f64,
    #[serde(default)]
    pub normalize: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SubsSel {
    Embedded { track: u32 },
    File { path: String },
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeSpec {
    pub input: String,
    /// empty = whole file; several = joined in order
    #[serde(default)]
    pub segments: Vec<Segment>,
    /// source display pixels, before rotate
    pub crop: Option<Crop>,
    /// clockwise: 0, 90, 180 or 270
    #[serde(default)]
    pub rotate: u32,
    /// horizontal flip, after rotate
    #[serde(default)]
    pub mirror: bool,
    /// vertical flip, after rotate
    #[serde(default)]
    pub vflip: bool,
    /// color adjustments; None or all neutral = none
    #[serde(default)]
    pub adjust: Option<Adjust>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    #[serde(default)]
    pub auto_scale: bool,
    pub fps: Option<f64>,
    #[serde(default = "one")]
    pub speed: f64,
    /// a speed that changes over the clip (points in source file time); when
    /// it has points it replaces `speed`. Applies to ranges of `input` only.
    #[serde(default)]
    pub speed_ramp: Vec<SpeedPoint>,
    /// zoom points in source file time (zoom.rs); empty = no zoom. Ranges of `input` only.
    #[serde(default)]
    pub zoom: Vec<ZoomPoint>,
    /// with zoom points: Follow's view (its shape and size at 1x), which moves over the
    /// whole picture and can turn; `crop` is then not used. None = the zoom points move
    /// a window inside the cropped picture (zoompan).
    #[serde(default)]
    pub follow_window: Option<FollowWindow>,
    #[serde(default)]
    pub playback: Playback,
    #[serde(default)]
    pub deinterlace: bool,
    #[serde(default)]
    pub denoise: bool,
    /// steadies a shaky picture, per range, with `stab_method`
    #[serde(default)]
    pub stabilize: bool,
    #[serde(default)]
    pub stab_method: StabMethod,
    /// slow motion (speed below 1) gets new in-between frames (minterpolate)
    /// instead of repeated ones
    #[serde(default)]
    pub smooth_slowmo: bool,
    #[serde(default)]
    pub tonemap: bool,
    #[serde(default)]
    pub fade_in: f64,
    #[serde(default)]
    pub fade_out: f64,
    #[serde(default)]
    pub container: Container,
    pub codec: Codec,
    pub rate: Rate,
    #[serde(default)]
    pub two_pass: bool,
    /// size mode: encode again (up to 4 attempts, run.rs) when the file is over the
    /// limit or well under it. Off (the default): one attempt, so the job does not seem
    /// to start over after finishing.
    #[serde(default)]
    pub size_retries: bool,
    /// size mode: the share of the limit the (first) attempt aims at, sound and video
    /// together; plan::DEFAULT_SIZE_TARGET (webm.py's 1000/1024) when not given
    #[serde(default = "default_size_target")]
    pub size_target: f64,
    pub effort: Effort,
    pub audio: Option<AudioSel>,
    pub subs: Option<SubsSel>,
    pub title: Option<String>,
    #[serde(default)]
    pub extra_args: Vec<String>,
    pub output_name: Option<String>,
    /// the preset the job was made with, for the `{preset}` file name token
    #[serde(default)]
    pub preset_name: Option<String>,
    /// cut only: copy the streams without encoding (one range, no filters)
    #[serde(default)]
    pub stream_copy: bool,
    /// text drawn on the picture
    #[serde(default)]
    pub caption: Option<Caption>,
    /// the file each range comes from, by index; missing or None = `input`.
    /// Ranges from other files are fitted to `input`'s frame size and rate.
    #[serde(default)]
    pub segment_sources: Vec<Option<String>>,
    /// another sound file in place of, or mixed with, the source's sound
    #[serde(default)]
    pub music: Option<Music>,
    /// an image (logo, watermark) over the picture
    #[serde(default)]
    pub overlay: Option<Overlay>,
    /// splits: the segments, one per clip, combined into one picture
    #[serde(default)]
    pub layout: Option<Layout>,
    /// seconds the burned-in subtitles are shown later (negative: earlier)
    #[serde(default)]
    pub subs_delay: f64,
    /// subtitle font size (libass units; 18 is the SRT default); None = the file's own
    #[serde(default)]
    pub subs_font_size: Option<f64>,
    /// output folder fixed when the job was queued ("" = beside the source);
    /// None = the folder from Settings when the job starts
    #[serde(default)]
    pub output_dir: Option<String>,
    /// the still picture of a video made from a sound file; None = the file's
    /// embedded cover, if any
    #[serde(default)]
    pub cover: Option<String>,
    /// the picture's display size (width, height), from probing it; plan
    /// cannot read files, and the queue probes only the source
    #[serde(default)]
    pub cover_size: Option<(u32, u32)>,
}

impl EncodeSpec {
    /// File extension of the output.
    pub fn ext(&self) -> &'static str {
        match (self.container, self.codec) {
            (Container::Audio, Codec::Opus) => "opus",
            (Container::Audio, Codec::Mp3) => "mp3",
            (c, _) => c.ext(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Good,
    Fair,
    Poor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanInfo {
    pub out_width: u32,
    pub out_height: u32,
    pub duration_out: f64,
    /// frames per second of the output (0 for sound only)
    #[serde(default)]
    pub out_fps: f64,
    pub video_kbps: Option<f64>,
    pub audio_kbps: f64,
    /// video_kbps*1000 / (out_width*out_height*out_fps)
    pub bits_per_pixel: Option<f64>,
    pub quality: Option<Quality>,
    /// autoScale reduced the picture size
    pub auto_scaled: bool,
    pub command: String,
    pub warnings: Vec<String>,
    /// the file the job would write now (the `plan` command fills it)
    #[serde(default)]
    pub output: Option<String>,
    /// splits: where each clip's picture is on the canvas (split screen: one per clip)
    #[serde(default)]
    pub cells: Vec<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// whole job, 0..1
    pub fraction: f64,
    pub pass: u32,
    pub passes: u32,
    pub attempt: u32,
    pub speed: Option<f64>,
    pub eta_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobResult {
    pub size_bytes: u64,
    pub elapsed_secs: f64,
    pub attempts: u32,
    pub over_limit: bool,
    /// differences between the probed output and the plan (empty = as planned)
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    pub spec: EncodeSpec,
    pub status: JobStatus,
    pub output: Option<String>,
    pub progress: Option<Progress>,
    pub result: Option<JobResult>,
    pub error: Option<String>,
}

fn effort_balanced() -> Effort {
    Effort::Balanced
}

/// Defaults on every field but name, codec and rate, so presets saved by
/// earlier builds (no container, no autoScale) still load.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub container: Container,
    pub codec: Codec,
    pub rate: Rate,
    #[serde(default)]
    pub auto_scale: bool,
    #[serde(default)]
    pub max_width: Option<u32>,
    #[serde(default)]
    pub max_height: Option<u32>,
    #[serde(default)]
    pub fps: Option<f64>,
    /// None = no audio
    #[serde(default)]
    pub audio_bitrate_k: Option<f64>,
    #[serde(default = "effort_balanced")]
    pub effort: Effort,
    #[serde(default)]
    pub two_pass: bool,
    /// advisory; the UI warns when the clip is longer
    #[serde(default)]
    pub max_duration: Option<f64>,
}

pub const DEFAULT_NAME_TEMPLATE: &str = "{name}_{start}-{end}";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// folder or file; None = auto
    pub ffmpeg_path: Option<String>,
    /// file or folder; None = auto (beside the exe, then PATH)
    pub ytdlp_path: Option<String>,
    /// extra yt-dlp arguments, e.g. --cookies-from-browser firefox
    pub ytdlp_args: Vec<String>,
    /// None = "downloads" in the app cache dir
    pub download_dir: Option<String>,
    /// where "Download only" keeps originals; None = the user's Downloads folder
    pub keep_dir: Option<String>,
    /// the folder used when output_mode is "folder"
    pub output_dir: Option<String>,
    /// "desktop", "beside" (the source file) or "folder" (output_dir).
    /// None: "folder" when output_dir is set (settings from 1.1.0), else "desktop".
    pub output_mode: Option<String>,
    /// yt-dlp --cookies-from-browser value (firefox, chrome, edge, ...); None = no cookies
    pub ytdlp_cookies: Option<String>,
    pub name_template: String,
    pub last_preset: Option<String>,
    /// Stabilize's method (Settings > Tools); None = vid.stab when ffmpeg has it, else deshake
    pub stab_method: Option<StabMethod>,
    /// the Gyroflow program (Settings > Tools); None = not set
    pub gyroflow_path: Option<String>,
    /// Follow > Track's tracker (Settings > Tools): "vittrack" or "sam2" when that is
    /// installed (src-tauri install.rs); None or anything else = the built-in one (track.rs).
    /// A plain string, so older builds read a settings file that has a new name in it.
    pub track_method: Option<String>,
    /// presets kept in settings.json by older builds; moved to presets/saved/*.toml on start
    pub presets: Vec<Preset>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ffmpeg_path: None,
            ytdlp_path: None,
            ytdlp_args: Vec::new(),
            download_dir: None,
            keep_dir: None,
            output_dir: None,
            output_mode: None,
            ytdlp_cookies: None,
            name_template: DEFAULT_NAME_TEMPLATE.to_string(),
            last_preset: None,
            stab_method: None,
            gyroflow_path: None,
            track_method: None,
            presets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Encoders {
    pub vp8: bool,
    pub vp9: bool,
    pub av1: bool,
    pub h264: bool,
    pub hevc: bool,
    /// ffmpeg's built-in GIF encoder
    #[serde(default)]
    pub gif: bool,
    /// libwebp_anim
    #[serde(default)]
    pub webp: bool,
    pub opus: bool,
    pub vorbis: bool,
    pub aac: bool,
    /// libmp3lame
    #[serde(default)]
    pub mp3: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Filters {
    /// needed for tonemap
    pub zscale: bool,
    pub subtitles: bool,
    /// needed for the quality check (quality.rs)
    #[serde(default)]
    pub libvmaf: bool,
    /// vidstabdetect and vidstabtransform (Stabilize with vid.stab)
    #[serde(default)]
    pub vidstab: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsStatus {
    pub ffmpeg: Option<String>,
    pub ffprobe: Option<String>,
    pub version: Option<String>,
    pub ytdlp: Option<String>,
    pub ytdlp_version: Option<String>,
    pub encoders: Encoders,
    pub filters: Filters,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlInfo {
    pub url: String,
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub is_live: bool,
    pub thumbnail: Option<String>,
    /// distinct video heights offered, descending
    pub heights: Vec<u32>,
}

/// Payload of the `download-progress` event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub url: String,
    pub fraction: f64,
    pub speed: Option<String>,
    pub eta: Option<String>,
    /// "download" or "merge"
    pub stage: String,
}

fn default_size_target() -> f64 {
    crate::plan::DEFAULT_SIZE_TARGET
}
