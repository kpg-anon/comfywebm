// The IPC contract with the Rust side (crates/engine/src/spec.rs). Field names
// are camelCase on both sides; src/lib/contract.check.ts checks them.

// seconds on the source timeline; `id` is frontend-only (output order) and is not sent
export type WatermarkPlace = "tl" | "tr" | "bl" | "br" | "custom" | "tile";

export type LayoutMode = "sequence" | "columns" | "rows" | "grid";
export type Fill = "bars" | "crop" | "blur";
export type Rect = { x: number; y: number; w: number; h: number };
/** Splits: the segments (one per clip, from segmentSources) combined into one picture. */
export type Layout = {
  mode: LayoutMode;
  /** the canvas, before the output size */
  width: number;
  height: number;
  fill: Fill;
  fps: number;
  /** split screen: one clip's sound, all mixed, or none */
  sound: "clip" | "mix" | "none";
  soundClip: number;
  /** split screen: the shortest clip's length, or the longest (the others hold their last frame, or loop) */
  length: "shortest" | "longest" | "loop";
  /** split screen: space between the parts, canvas pixels */
  gap: number;
  gapColor: string;
  /** side by side / stacked: relative size of each part; [] = equal */
  weights: number[];
  /** grid columns; 0 = as square as possible */
  columns: number;
  /** the part of each clip used, in that clip's display pixels; null = all of it */
  crops: (Rect | null)[];
};

export type Segment = { start: number; end: number; id?: number };

/** A point of a speed ramp: at `t` seconds of the source file the speed is `speed`. */
export type SpeedPoint = { t: number; speed: number };

/** A zoom point: at `t` seconds of the source the picture is zoomed `zoom` times (1 = all of it)
 *  around (`x`, `y`), 0..1 of the picture after crop, rotation and flips (engine zoom.rs). */
/** With a follow window, x and y are 0..1 of the whole picture as shown and `rot` turns the
 *  view (degrees the picture turns clockwise); without one, 0..1 of the cropped picture. */
export type ZoomPoint = { t: number; zoom: number; x: number; y: number; rot?: number };
/** Follow's view at 1x, in pixels of the whole picture after rotation and flips (engine FollowWindow). */
export type FollowWindow = { w: number; h: number };

export type MediaInfo = {
  path: string;
  fileName: string;
  sizeBytes: number;
  duration: number; // seconds
  format: string; // container, e.g. "QuickTime / MOV"
  bitrateK: number | null; // overall
  created: string | null;
  encoder: string | null;
  /** sound but no video stream; `video` then holds the embedded cover picture, or zeros */
  audioOnly?: boolean;
  /** a single picture (PNG, JPEG, WebP...): no length of its own */
  still?: boolean;
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

export type Container = "webm" | "mp4" | "gif" | "webp" | "audio";
export type Codec = "vp9" | "vp8" | "av1" | "h264" | "hevc" | "gif" | "webp" | "opus" | "mp3" | "aac";
export type Effort = "fast" | "balanced" | "best";
/** Stabilize's method (engine spec.rs StabMethod) */
export type StabMethod = "deshake" | "vidstab" | "gyroflow";
export type Rotate = 0 | 90 | 180 | 270;
export type Playback = "forward" | "reverse" | "boomerang";

export type Crop = { x: number; y: number; w: number; h: number };

/** Color adjustments; neutral: 0, 1, 1, 1, 0, [], 0, 0. curve = master curve points [in, out], 0..1.
 *  temperature and tint (-1..1) are the white balance (autocolor.rs wb_gains). */
export type Adjust = {
  brightness: number;
  contrast: number;
  saturation: number;
  gamma: number;
  hue: number;
  curve: [number, number][];
  temperature?: number;
  tint?: number;
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
  /** a speed that changes over the clip (file time); with points it replaces `speed` */
  speedRamp: SpeedPoint[];
  /** zoom points (engine zoom.rs); empty = none */
  zoom?: ZoomPoint[];
  /** Follow's view (the crop's shape and size); it moves over the whole picture and the crop is not used */
  followWindow?: FollowWindow | null;
  playback: Playback;
  deinterlace: boolean;
  denoise: boolean;
  stabilize?: boolean;
  stabMethod?: StabMethod;
  smoothSlowmo?: boolean;
  tonemap: boolean;
  fadeIn: number;
  fadeOut: number;
  container: Container;
  codec: Codec;
  rate: Rate;
  twoPass: boolean;
  /** size mode: encode again when the file misses the limit (up to 4 attempts); off: one attempt */
  sizeRetries?: boolean;
  /** size mode: the share of the limit the encode aims at, sound and video together (0.5 to 1) */
  sizeTarget?: number;
  effort: Effort;
  audio: AudioSpec | null;
  subs: SubsSpec;
  title: string | null;
  extraArgs: string[];
  outputName: string | null;
  /** the preset the job was made with, for the {preset} file name token */
  presetName?: string | null;
  /** cut only: copy the streams, no encode (one range, filters ignored) */
  streamCopy?: boolean;
  /** text on the picture (white, black outline); size = % of the output height */
  caption?: {
    text: string;
    top: boolean;
    /** placed at x, y instead of the top or bottom edge */
    custom: boolean;
    /** custom: 0..1 of the room beside and below the text */
    x: number;
    y: number;
    size: number;
    /** font file; null = a bold sans the system has */
    font: string | null;
    /** "#rrggbb" */
    color: string;
    outlineColor: string;
    /** outline width in percent of the text height; 0 = none */
    outline: number;
  } | null;
  /** the file of each range by index (null = input); other files are fitted to input's size */
  segmentSources?: (string | null)[];
  /** splits */
  layout?: Layout | null;
  /** a sound file that replaces (mix false) or is mixed under the source's sound */
  music?: { path: string; mix: boolean; volumeDb: number; offset: number } | null;
  /** an image in a corner; size = % of the output width */
  overlay?: {
    path: string;
    /** a corner, "custom" (at x, y) or "tile" (repeated over the picture) */
    corner: WatermarkPlace;
    size: number;
    opacity: number;
    /** "custom": 0..1 of the room beside and below the image */
    x: number;
    y: number;
    /** "tile": space between copies, as a multiple of the image width */
    spacing: number;
    /** degrees, clockwise; for "tile" the whole grid is turned */
    angle: number;
  } | null;
  /** seconds the subtitles are shown later (negative: earlier) */
  subsDelay?: number;
  /** subtitle font size (18 = SRT default); null = the file's own */
  subsFontSize?: number | null;
  /** set by the backend when queued: the output folder ("" = beside the source) */
  outputDir?: string | null;
  /** the still picture of a video made from a sound file; null = its embedded cover */
  cover?: string | null;
  /** that picture's size [width, height] */
  coverSize?: [number, number] | null;
};

export type PlanInfo = {
  /** the file the job would write if it started now */
  output?: string | null;
  outWidth: number;
  outHeight: number;
  durationOut: number;
  /** frames per second of the output (0 for sound only) */
  outFps: number;
  videoKbps: number | null;
  audioKbps: number;
  bitsPerPixel: number | null;
  quality: "good" | "fair" | "poor" | null;
  autoScaled: boolean;
  command: string;
  warnings: string[];
  /** splits, split screen: each clip's box on the canvas */
  cells?: Rect[];
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
  result: { sizeBytes: number; elapsedSecs: number; attempts: number; overLimit: boolean; warnings?: string[] } | null;
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
  /** Stabilize's method; null = vid.stab when ffmpeg has it, else deshake */
  stabMethod?: StabMethod | null;
  /** the Gyroflow program; null = not set */
  gyroflowPath?: string | null;
  /** Follow > Track's tracker: "vittrack" or "sam2" when installed; null = the built-in one */
  trackMethod?: string | null;
  presets: Preset[];
};

export type ToolsStatus = {
  ffmpeg: string | null;
  ffprobe: string | null;
  version: string | null;
  ytdlp: string | null;
  ytdlpVersion: string | null;
  encoders: { vp8: boolean; vp9: boolean; av1: boolean; h264: boolean; hevc: boolean; gif: boolean; webp?: boolean; mp3?: boolean; opus: boolean; vorbis: boolean; aac: boolean };
  filters: { zscale: boolean; subtitles: boolean; libvmaf?: boolean; vidstab?: boolean };
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
