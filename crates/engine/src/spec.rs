//! The IPC contract types (PLAN.md, "Contract" and "Contract v2"). JSON field names are camelCase.

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
}

impl Default for Adjust {
    fn default() -> Self {
        Adjust { brightness: 0.0, contrast: 1.0, saturation: 1.0, gamma: 1.0, hue: 0.0, curve: Vec::new() }
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

/// An image placed in a corner of the output picture.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overlay {
    pub path: String,
    /// "tl", "tr", "bl" or "br"
    #[serde(default = "corner_br")]
    pub corner: String,
    /// width in percent of the output picture width
    #[serde(default = "overlay_size")]
    pub size: f64,
    /// 0..1
    #[serde(default = "one_f")]
    pub opacity: f64,
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

/// A line (or lines) of text on the picture: white, black outline, bold sans.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caption {
    pub text: String,
    /// true = top edge, false = bottom edge
    #[serde(default)]
    pub top: bool,
    /// text height in percent of the output picture height
    #[serde(default = "caption_size")]
    pub size: f64,
}

fn caption_size() -> f64 {
    7.0
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
    #[serde(default)]
    pub playback: Playback,
    #[serde(default)]
    pub deinterlace: bool,
    #[serde(default)]
    pub denoise: bool,
    /// steadies a shaky picture (deshake), per range
    #[serde(default)]
    pub stabilize: bool,
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
    pub effort: Effort,
    pub audio: Option<AudioSel>,
    pub subs: Option<SubsSel>,
    pub title: Option<String>,
    #[serde(default)]
    pub extra_args: Vec<String>,
    pub output_name: Option<String>,
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
    /// presets kept in settings.json before 1.2.0; moved to presets/saved/*.toml on start
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
