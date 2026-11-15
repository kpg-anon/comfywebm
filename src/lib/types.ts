// The IPC contract from PLAN.md (Contract v2 in Amendment 2). Field names are
// camelCase on both sides.

// seconds on the source timeline; `id` is frontend-only (output order) and is not sent
export type Segment = { start: number; end: number; id?: number };

export type MediaInfo = {
  path: string;
  fileName: string;
  sizeBytes: number;
  duration: number; // seconds
  format: string; // container, e.g. "QuickTime / MOV"
  bitrateK: number | null; // overall
  created: string | null;
  encoder: string | null;
  // width/height are display pixels (rotation and sample aspect ratio applied)
  video: {
    codec: string;
    width: number;
    height: number;
    fps: number;
    bitrateK: number | null;
    interlaced: boolean;
    hdr: boolean;
    pixFmt: string | null;
    profile: string | null;
    color: string | null; // "space / primaries / transfer"
    frames: number | null;
    vfr: boolean;
  };
  // track = n-th stream of that kind, 0-based (ffmpeg 0:a:n / 0:s:n)
  audio: {
    track: number;
    codec: string;
    channels: number;
    lang: string | null;
    title: string | null;
    sampleRate: number | null;
    bitrateK: number | null;
  }[];
  subs: { track: number; codec: string; lang: string | null; title: string | null; textBased: boolean }[];
};

export type Rate =
  | { mode: "size"; maxBytes: number }
  | { mode: "crf"; crf: number; maxBitrateK: number | null };

export type Container = "webm" | "mp4";
export type Codec = "vp9" | "vp8" | "av1" | "h264" | "hevc";
export type Effort = "fast" | "balanced" | "best";
export type Rotate = 0 | 90 | 180 | 270;
export type Playback = "forward" | "reverse" | "boomerang";

export type Crop = { x: number; y: number; w: number; h: number };

/** Color adjustments; neutral: 0, 1, 1, 1, 0, []. curve = master curve points [in, out], 0..1. */
export type Adjust = {
  brightness: number;
  contrast: number;
  saturation: number;
  gamma: number;
  hue: number;
  curve: [number, number][];
};

export type SubsSpec = { kind: "embedded"; track: number } | { kind: "file"; path: string } | null;

export type AudioSpec = { track: number; bitrateK: number; volumeDb: number; normalize: boolean };

export type EncodeSpec = {
  input: string;
  segments: Segment[]; // empty = whole file; several = joined in order
  crop: Crop | null; // source display pixels, before rotate
  rotate: Rotate; // clockwise
  mirror: boolean; // horizontal flip, after rotate
  vflip: boolean; // vertical flip, after rotate
  adjust: Adjust | null;
  maxWidth: number | null;
  maxHeight: number | null;
  autoScale: boolean;
  fps: number | null;
  speed: number;
  playback: Playback;
  deinterlace: boolean;
  denoise: boolean;
  tonemap: boolean;
  fadeIn: number;
  fadeOut: number;
  container: Container;
  codec: Codec;
  rate: Rate;
  twoPass: boolean;
  effort: Effort;
  audio: AudioSpec | null;
  subs: SubsSpec;
  title: string | null;
  extraArgs: string[];
  outputName: string | null;
};

export type PlanInfo = {
  outWidth: number;
  outHeight: number;
  durationOut: number;
  videoKbps: number | null;
  audioKbps: number;
  bitsPerPixel: number | null;
  quality: "good" | "fair" | "poor" | null;
  autoScaled: boolean;
  command: string;
  warnings: string[];
};

export type JobStatus = "pending" | "running" | "done" | "failed" | "canceled";

export type Job = {
  id: number;
  spec: EncodeSpec;
  status: JobStatus;
  output: string | null;
  progress: {
    fraction: number;
    pass: number;
    passes: number;
    attempt: number;
    speed: number | null;
    etaSecs: number | null;
  } | null;
  result: { sizeBytes: number; elapsedSecs: number; attempts: number; overLimit: boolean } | null;
  error: string | null;
};

export type Preset = {
  name: string;
  container: Container;
  codec: Codec;
  rate: Rate;
  autoScale: boolean;
  maxWidth: number | null;
  maxHeight: number | null;
  fps: number | null;
  audioBitrateK: number | null; // null = no audio
  effort: Effort;
  twoPass: boolean;
  maxDuration: number | null;
};

export type Settings = {
  ffmpegPath: string | null;
  ytdlpPath: string | null;
  ytdlpArgs: string[];
  downloadDir: string | null;
  keepDir: string | null; // "Download only" saves here; null = the user's Downloads folder
  outputDir: string | null; // the folder for outputMode "folder"
  outputMode: "desktop" | "beside" | "folder" | null; // null: "folder" if outputDir is set, else "desktop"
  ytdlpCookies: string | null; // yt-dlp --cookies-from-browser value
  nameTemplate: string;
  lastPreset: string | null;
  presets: Preset[];
};

export type ToolsStatus = {
  ffmpeg: string | null;
  ffprobe: string | null;
  version: string | null;
  ytdlp: string | null;
  ytdlpVersion: string | null;
  encoders: { vp8: boolean; vp9: boolean; av1: boolean; h264: boolean; hevc: boolean; opus: boolean; vorbis: boolean; aac: boolean };
  filters: { zscale: boolean; subtitles: boolean };
};

export type UrlInfo = {
  url: string;
  id: string;
  title: string;
  uploader: string | null;
  duration: number | null;
  isLive: boolean;
  thumbnail: string | null;
  heights: number[]; // distinct video heights offered, descending
};

export type PresetEntry = { preset: Preset; group: "platform" | "saved"; file: string };

/** config.toml and the preset files (Rust config.rs). */
export type ConfigInfo = {
  dir: string;
  heights: number[];
  widths: number[];
  stepBy: "height" | "width";
  percents: number[];
  fpsCaps: number[];
  audioBitrates: number[];
  presets: PresetEntry[];
  errors: string[];
};

export type ProxyProgress = { path: string; fraction: number };

export type DownloadProgress = {
  url: string;
  fraction: number;
  speed: string | null;
  eta: string | null;
  stage: "download" | "merge";
};

/** `queue-state` event and `queue_state`: whether the queue takes the next pending job. */
export type QueueState = { started: boolean };
