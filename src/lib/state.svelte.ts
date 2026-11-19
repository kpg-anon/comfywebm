// App state: source, playback, ranges, crop, spec fields, plan, jobs, settings.
import { convertFileSrc } from "@tauri-apps/api/core";
import * as ipc from "./ipc";
import { load, save } from "./persist";
import { describeEdit } from "./history";
import * as zoomMath from "./zoom";
import { siteOf, refusal } from "./sites";
import * as ramp from "./ramp";
import { AUDIO_LABEL, BUILTIN_PRESETS, CODECS_BY_CONTAINER, CRF_RANGE, audioCodecFor } from "./presets";
import { fitRatio, fromView, isFull, move, ratioOf, round, toView, viewSize, type AspectLock, type View as Turn } from "./crop";
import * as seg from "./segments";
import { fmtTime } from "./time";
import { motionWarnings } from "./motion";
import { exactBox, percentBox, stepBox, withLimit, type Box, type ResMode } from "./resolution";
import type {
  Adjust,
  Codec,
  ConfigInfo,
  Container,
  Crop,
  Effort,
  EncodeSpec,
  Job,
  MediaInfo,
  PlanInfo,
  Playback,
  Preset,
  PresetEntry,
  Rate,
  Rotate,
  Segment,
  Settings,
  SpeedPoint,
  StabMethod,
  ToolsStatus,
  ZoomPoint,
  WatermarkPlace,
} from "./types";

export type SubsKind = "off" | "embedded" | "file";

export const DEFAULT_SETTINGS: Settings = {
  ffmpegPath: null,
  ytdlpPath: null,
  ytdlpArgs: [],
  downloadDir: null,
  keepDir: null,
  outputDir: null,
  outputMode: null,
  ytdlpCookies: null,
  nameTemplate: "{name}_{start}-{end}",
  lastPreset: null,
  presets: [],
};

/** Split an argument line into words; double quotes group words with spaces. */
export function splitArgs(text: string): string[] {
  const out: string[] = [];
  const re = /"([^"]*)"|(\S+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) out.push(m[1] ?? m[2]);
  return out;
}

/** The timeline track's height: standard and tallest, px. */
export const TRACK_MIN = 26;
/** the track's standard height: thumbnails large enough to see */
export const TRACK_STD = 72;
export const TRACK_MAX = 240;

/** "3.5 s", "1:05" */
const fmtSecs = (s: number) => (s < 60 ? `${Math.round(s * 10) / 10} s` : `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, "0")}`);

const numOrNull = (v: number | null | undefined): number | null =>
  v === null || v === undefined || !Number.isFinite(v) || v <= 0 ? null : v;

/** maxWidth/maxHeight are u32 on the Rust side, so a fraction would fail to deserialize. */
const intOrNull = (v: number | null | undefined): number | null => {
  const n = numOrNull(v);
  return n === null ? null : Math.max(1, Math.round(n));
};

/** Option fields remembered in localStorage between runs (not tied to one source). */
const REMEMBERED = [
  "presetName", "container", "codec", "rateMode", "maxBytes", "crf", "maxBitrateK", "autoScale",
  "maxWidth", "maxHeight", "fps", "speed", "playback", "denoise", "stabilize", "smoothSlowmo", "fadeIn", "fadeOut", "effort",
  "twoPass", "audioOn", "audioBitrateK", "volumeDb", "normalize", "separate", "aspect",
  "resMode", "resStep", "resPct", "exactEdge", "exactVal", "stepBy", "adjustPreview",
  "logoPath", "logoCorner", "logoSize", "logoOpacity", "logoX", "logoY", "logoSpacing", "logoAngle",
  "captionFont", "captionColor", "captionOutlineColor", "captionOutline",
] as const;

export const NEUTRAL_ADJUST: Adjust = { brightness: 0, contrast: 1, saturation: 1, gamma: 1, hue: 0, curve: [] };

export const isNeutral = (a: Adjust) =>
  a.brightness === 0 && a.contrast === 1 && a.saturation === 1 && a.gamma === 1 && a.hue === 0 && a.curve.every(([x, y]) => Math.abs(x - y) < 1e-4);

export type View = "editor" | "batch" | "splits" | "queue";

/** Per-file changes a batch item can make to the global settings. */
export type ItemOverrides = {
  segments: Segment[];
  crop: Crop | null;
  /** undefined = the global frame rate cap */
  fps?: number | null;
  audioOff: boolean;
};

/** A short message in the top-right stack (Toasts.svelte). */
export type Toast = {
  id: number;
  kind: "done" | "over" | "failed" | "info";
  title: string;
  text: string;
  /** a file the toast offers to play or reveal */
  path: string | null;
};

/** Display refresh rates offered for the motion check. */
export const REFRESH_RATES = [50, 60, 75, 90, 100, 120, 144, 165, 240];

/** One undo step: ranges, output order, selected range, crop, rotation, flips, adjustments. */
type EditSnap = {
  segments: Segment[];
  order: number[] | null;
  sel: number;
  crop: Crop | null;
  rotate: Rotate;
  mirror: boolean;
  vflip: boolean;
  adjust: Adjust;
  speedRamp: SpeedPoint[];
  zoom: ZoomPoint[];
  zoomLevel: number;
};
type EditMemory = { segments: Segment[]; order: number[] | null; crop: Crop | null; speedRamp?: SpeedPoint[]; zoom?: ZoomPoint[]; zoomLevel?: number; t: number };

function loadRecent(): string[] {
  const r = load<unknown>("recent");
  return Array.isArray(r) ? r.filter((p): p is string => typeof p === "string").slice(0, 10) : [];
}

export type SampleState = {
  busy: boolean;
  at: number;
  path: string | null;
  sizeBytes: number | null;
  kbps: number | null;
  error: string | null;
};

class AppState {
  // tools and settings
  tools = $state<ToolsStatus | null>(null);
  settings = $state<Settings>({ ...DEFAULT_SETTINGS });
  jobs = $state<Job[]>([]);
  settingsOpen = $state(false);
  keysOpen = $state(false);
  /** Recently opened files, newest first (localStorage, per machine). */
  recent = $state<string[]>(loadRecent());
  /** Settings > Look: the sidebar's width can be changed by dragging its edge (per machine). */
  sideResize = $state(load<boolean>("sideResize") ?? false);
  setSideResize(on: boolean) {
    this.sideResize = on;
    save("sideResize", on);
  }
  /** Settings > Look: the timeline can be made taller by dragging its top edge (per machine). */
  timelineResize = $state(load<boolean>("timelineResize") ?? false);
  setTimelineResize(on: boolean) {
    this.timelineResize = on;
    save("timelineResize", on);
  }
  /** The timeline track's height when it is resizable, px (26 is the standard). */
  trackHeight = $state(Math.min(TRACK_MAX, Math.max(TRACK_MIN, load<number>("trackHeight") ?? TRACK_STD)));
  setTrackHeight(h: number, remember = true) {
    this.trackHeight = Math.round(Math.min(TRACK_MAX, Math.max(TRACK_MIN, h)));
    if (remember) save("trackHeight", this.trackHeight);
  }
  get trackH(): number {
    return this.timelineResize ? this.trackHeight : TRACK_STD;
  }
  /** Settings > Files: keep and show the recent files list (per machine). */
  showRecent = $state(load<boolean>("showRecent") ?? true);
  setShowRecent(on: boolean) {
    this.showRecent = on;
    save("showRecent", on);
    // turned off: the list is not kept either
    if (!on) this.clearRecent();
  }
  clearRecent() {
    this.recent = [];
    save("recent", []);
  }
  ready = $state(false); // init() has loaded settings and restored the remembered options
  /** Jobs running or waiting. */
  activeJobs = $derived(this.jobs.filter((j) => j.status === "running" || j.status === "pending").length);
  runningJob = $derived(this.jobs.find((j) => j.status === "running") ?? null);
  /** Left side: the editor (player and timeline) or the queue. */
  view = $state<View>("editor");
  /** The batch item being edited in the editor (trim and crop), or null. */
  batchEdit = $state<{ id: number; name: string } | null>(null);
  /** Jobs that finished while the queue was not shown; cleared when it is. */
  unseen = $state({ done: 0, failed: 0 });
  urlOpen = $state(false);
  urlInitial = $state("");
  sample = $state<SampleState | null>(null);
  private sampleSeq = 0;
  /** Messages in the top-right stack, newest first. */
  toasts = $state<Toast[]>([]);
  private toastSeq = 0;
  /** config.toml lists and the preset files; null until loaded (the built-in list is used meanwhile). */
  config = $state<ConfigInfo | null>(null);

  // source
  media = $state<MediaInfo | null>(null);
  videoSrc = $state<string | null>(null);
  usingProxy = $state(false);
  /** The preview copy holds the source's own streams (full resolution). */
  proxyRemuxed = $state(false);
  proxyFraction = $state<number | null>(null); // non-null while a proxy is being made
  busy = $state<string | null>(null);
  error = $state<string | null>(null);
  dragging = $state(false);

  // playback; the <video> element registers itself here
  video: HTMLVideoElement | null = null;
  current = $state(0);
  paused = $state(true);
  loop = $state(false);
  private loopTouched = false;
  /** the preview's sound (not the encode's): muted, and the volume 0..1; remembered */
  muted = $state(load<boolean>("muted") === true);
  volume = $state(Math.min(1, Math.max(0, load<number>("volume") ?? 1)));

  toggleMute() {
    // unmuting at volume 0 would still be silent
    if (this.muted && this.volume === 0) this.setVolume(0.5);
    this.muted = !this.muted;
    save("muted", this.muted);
  }

  setVolume(v: number) {
    this.volume = Math.min(1, Math.max(0, Math.round(v * 100) / 100));
    this.muted = this.volume === 0;
    save("volume", this.volume);
    save("muted", this.muted);
  }

  // ranges on the source timeline, sorted by start; empty = whole file
  segments = $state<Segment[]>([]);
  sel = $state(-1); // index of the selected range
  separate = $state(false); // each range as its own job
  /** Cut only: copy the streams without encoding. Not remembered between sessions. */
  streamCopy = $state(false);
  /** Timeline pictures (Settings), remembered. */
  showFilm = $state(load<boolean>("timelineFilm") ?? true);
  showWave = $state(load<boolean>("timelineWave") ?? false);
  /** Thumbnails from every frame (made after the key frame ones, cached), not only from key frames. Remembered. */
  exactThumbs = $state(load<boolean>("timelineExact") ?? true);
  setExactThumbs(on: boolean) {
    this.exactThumbs = on;
    save("timelineExact", on);
  }
  setTimelinePics(film: boolean, wave: boolean) {
    this.showFilm = film;
    this.showWave = wave;
    save("timelineFilm", film);
    save("timelineWave", wave);
  }
  /** Output order as range ids, once the edit strip has reordered them. */
  order = $state<number[] | null>(null);
  private nextSegId = 1;
  /** Play the ranges one after another in output order. */
  playEdit = $state(false);
  private editIdx = 0;

  // crop, in source display pixels before rotate
  cropMode = $state(false);
  crop = $state<Crop | null>(null);
  aspect = $state<AspectLock>("free");

  // spec fields
  presetName = $state("");
  container = $state<Container>("webm");
  codec = $state<Codec>("vp9");
  rateMode = $state<"size" | "crf">("size");
  maxBytes = $state(4194304);
  crf = $state(30);
  maxBitrateK = $state<number | null>(null);
  autoScale = $state(true);
  // the preset's picture limit
  maxWidth = $state<number | null>(2048);
  maxHeight = $state<number | null>(2048);
  // output size: auto (size mode picks), source, a long-edge step, a percentage, or one exact edge
  resMode = $state<ResMode>("auto");
  resStep = $state(720);
  /** Steps are heights (default) or widths. */
  stepBy = $state<"height" | "width">("height");
  resPct = $state(50);
  exactEdge = $state<"w" | "h">("w");
  exactVal = $state(1280);
  /** Refresh rate of the display the output is meant for; a per-machine preference. */
  refreshHz = $state(load<number>("refreshHz") ?? 60);
  /** Sidebar tab. */
  tab = $state<"basic" | "advanced" | "info">(((t) => (t === "advanced" || t === "info" ? t : "basic"))(load<string>("tab")));
  /** The folder new outputs go to, as the backend resolves it (null = beside the source). */
  outputPath = $state<string | null>(null);
  fps = $state<number | null>(null);
  speed = $state(1);
  /** speed ramp points (file time) of the open file; with points they replace `speed` */
  speedRamp = $state<SpeedPoint[]>([]);
  /** the speed ramp lane is shown under the timeline */
  rampLane = $state(false);
  /** Advanced > Zoom: 1 = the crop (or the whole picture); more is closer, less shows more of the picture around the crop */
  zoomLevel = $state(1);
  /** the follow path (Advanced > Zoom > Follow): centres over file time (engine zoom.rs); empty = none.
   *  Their `zoom` is not used: the job gets zoomLevel at every point. */
  zoomPoints = $state<ZoomPoint[]>([]);
  playback = $state<Playback>("forward");
  deinterlace = $state(false);
  denoise = $state(false);
  stabilize = $state(false);
  smoothSlowmo = $state(false);
  tonemap = $state(false);
  rotate = $state<Rotate>(0);
  mirror = $state(false);
  vflip = $state(false);
  adjust = $state<Adjust>({ ...NEUTRAL_ADJUST, curve: [] });
  /** show the color adjustments on the picture (an approximation, see Player.svelte) */
  adjustPreview = $state(true);
  fadeIn = $state(0);
  fadeOut = $state(0);
  effort = $state<Effort>("best");
  twoPass = $state(true);
  audioOn = $state(false);
  audioTrack = $state(0);
  audioBitrateK = $state(96);
  volumeDb = $state(0);
  normalize = $state(false);
  subsKind = $state<SubsKind>("off");
  subsTrack = $state(0);
  subsFile = $state<string | null>(null);
  subsDelay = $state(0);
  subsFontSize = $state<number | null>(null);
  title = $state("");
  /** Caption on the picture; cleared when another file opens. */
  captionText = $state("");
  captionTop = $state(false);
  /** Caption placed where it was dragged on the player (or typed in), 0..1 of the room left */
  captionCustom = $state(false);
  captionX = $state(0.5);
  captionY = $state(0.9);
  captionSize = $state(7);
  /** Caption look; remembered. Font is a font file path, null = default bold sans. */
  captionFont = $state<string | null>(null);
  captionColor = $state("#ffffff");
  captionOutlineColor = $state("#000000");
  captionOutline = $state(7);
  /** Installed fonts for the caption font list; read on first use. */
  fonts = $state<ipc.FontFace[]>([]);
  private fontsAsked = false;
  loadFonts() {
    if (this.fontsAsked) return;
    this.fontsAsked = true;
    ipc.listFonts().then((f) => (this.fonts = f)).catch(() => (this.fontsAsked = false));
  }
  /** Music: a sound file that replaces or is mixed under the source's sound. */
  musicPath = $state<string | null>(null);
  musicMix = $state(false);
  musicVolume = $state(0);
  musicOffset = $state(0);

  /** Watermark image; remembered, and used for batch items too. */
  logoPath = $state<string | null>(null);
  logoCorner = $state<WatermarkPlace>("br");
  /** "custom" placement: 0..1 of the room beside and below the image (dragged on the player) */
  logoX = $state(1);
  logoY = $state(1);
  /** "tile": space between copies, as a multiple of the image width */
  logoSpacing = $state(0.5);
  /** degrees, clockwise; for "tile" the whole grid turns */
  logoAngle = $state(0);
  logoSize = $state(15);
  logoOpacity = $state(1);
  /** A sound file is encoded as a video with a still picture: this image, else
   *  the file's embedded cover. Cleared when another file opens. */
  coverPath = $state<string | null>(null);
  coverSize = $state<[number, number] | null>(null);
  /** the embedded cover extracted to the cache (asset URL), for the player */
  embeddedCoverUrl = $state<string | null>(null);
  extraArgs = $state("");
  outputName = $state("");

  // undo/redo of ranges, output order, crop, rotation, flips and adjustments; see noteEdit()
  /** Every edit state of the open file, oldest first, with what changed and when; `at` is the
   *  current one. Undo and redo move `at`; the History panel jumps to any step. */
  steps = $state.raw<{ snap: EditSnap; label: string; time: number }[]>([]);
  at = $state(0);
  historyOpen = $state(false);
  /** the name of the next step, when it is not a plain edit (restoring the last edit of a file) */
  private nextLabel: string | null = null;
  private restoring = false;
  canUndo = $state(false);
  canRedo = $state(false);

  // plan result
  plan = $state<PlanInfo | null>(null);
  planError = $state<string | null>(null);
  private planSeq = 0;

  // ---------- derived ----------

  duration = $derived(this.media?.duration ?? 0);
  /** the open file has sound and no video */
  audioOnly = $derived(!!this.media?.audioOnly);
  /** the picture shown for a sound file, and its size */
  stillUrl = $derived(this.coverPath ? convertFileSrc(this.coverPath) : this.embeddedCoverUrl);
  stillSize = $derived.by((): [number, number] | null => {
    const m = this.media;
    if (!m?.audioOnly) return null;
    if (this.coverPath) return this.coverSize;
    return m.video.width > 0 ? [m.video.width, m.video.height] : null;
  });
  fpsSrc = $derived(this.media && this.media.video.fps > 0 ? this.media.video.fps : 30);
  minLen = $derived(1 / this.fpsSrc);
  hasRange = $derived(this.segments.length > 0);
  /** Ranges in output order (the order they are joined in). */
  outSegments = $derived.by((): Segment[] => {
    const o = this.order;
    if (!o) return this.segments;
    const byId = new Map(this.segments.map((s) => [s.id, s]));
    const list = o.map((id) => byId.get(id)).filter((s): s is Segment => !!s);
    // ranges added after the reorder go at the end
    for (const s of this.segments) if (!list.includes(s)) list.push(s);
    return list;
  });
  reordered = $derived(this.outSegments.some((s, i) => s !== this.segments[i]));
  selSeg = $derived<Segment | null>(this.segments[this.sel] ?? null);
  /** The selected range, or the whole file. */
  start = $derived(this.selSeg?.start ?? 0);
  end = $derived(this.selSeg?.end ?? this.duration);
  /** Source time that goes into the output (all ranges, or the whole file). */
  clipLen = $derived(this.hasRange ? seg.totalLength(this.segments) : this.duration);
  /** Length of the longest output file, after speed and boomerang. */
  outLen = $derived.by(() => {
    const back = this.playback === "boomerang" ? 2 : 1;
    const segs = this.hasRange ? this.outSegments : [{ start: 0, end: this.duration }];
    if (this.speedRamp.length) {
      const one = this.separate && segs.length > 1 ? Math.max(...segs.map((g) => ramp.outLength(this.speedRamp, [g]))) : ramp.outLength(this.speedRamp, segs);
      return one * back;
    }
    const src = this.separate && this.segments.length > 1 ? Math.max(...this.segments.map((s) => s.end - s.start)) : this.clipLen;
    return (src / (this.speed || 1)) * back;
  });

  presetEntries = $derived<PresetEntry[]>(
    this.config ? this.config.presets : BUILTIN_PRESETS.map((p) => ({ preset: p, group: "platform" as const, file: "" })),
  );
  presets = $derived<Preset[]>(this.presetEntries.map((e) => e.preset));
  defaultPreset = $derived(this.presets[0]?.name ?? "");
  /** Lists the UI offers, from config.toml (defaults until it is read). */
  stepList = $derived(this.config ? (this.stepBy === "width" ? this.config.widths : this.config.heights) : this.stepBy === "width" ? [426, 640, 854, 960, 1280, 1600, 1920, 2560, 3840] : [240, 360, 480, 540, 720, 900, 1080, 1440, 2160]);
  percentList = $derived(this.config?.percents ?? [25, 33, 50, 67, 75, 100]);
  fpsCaps = $derived(this.config?.fpsCaps ?? [10, 12, 15, 20, 24, 25, 30, 48, 50, 60, 120]);
  audioRates = $derived(this.config?.audioBitrates ?? [32, 48, 64, 80, 96, 112, 128, 160, 192, 256]);
  preset = $derived(this.presets.find((p) => p.name === this.presetName) ?? null);
  /** Settings that differ from what the selected preset sets (labels for the sidebar). */
  presetChanges = $derived.by((): string[] => {
    const p = this.preset;
    if (!p) return [];
    const out: string[] = [];
    if (p.container !== this.container || p.codec !== this.codec) out.push("format");
    if (p.rate.mode !== this.rateMode) out.push("rate mode");
    else if (p.rate.mode === "size" && p.rate.maxBytes !== this.maxBytes) out.push("size limit");
    else if (p.rate.mode === "crf" && (p.rate.crf !== this.crf || (p.rate.maxBitrateK ?? null) !== (this.maxBitrateK ?? null))) out.push("quality");
    if (p.autoScale !== this.autoScale || (p.autoScale ? this.resMode !== "auto" : this.resMode !== "source")) out.push("resolution");
    if ((p.maxWidth ?? null) !== (this.maxWidth ?? null) || (p.maxHeight ?? null) !== (this.maxHeight ?? null)) out.push("picture limit");
    if ((p.fps ?? null) !== (this.fps ?? null)) out.push("frame rate");
    // a file without sound cannot follow the preset's audio, so that is not a change
    const silent = !!this.media && !this.media.audio.length;
    if (!silent && (p.audioBitrateK === null ? this.audioOn : !this.audioOn || p.audioBitrateK !== this.audioBitrateK)) out.push("audio");
    if (p.effort !== this.effort || p.twoPass !== this.twoPass) out.push("encoder");
    return out;
  });

  ffmpegMissing = $derived(!!this.tools && (!this.tools.ffmpeg || !this.tools.ffprobe));
  ytdlpMissing = $derived(!!this.tools && !this.tools.ytdlp);
  audioCodec = $derived(audioCodecFor(this.container, this.codec));
  audioCodecName = $derived(AUDIO_LABEL[this.audioCodec]);
  crfRange = $derived(CRF_RANGE[this.codec]);

  /** The picture size before scaling: crop, then rotate. */
  baseW = $derived.by(() => this.baseSize()[0]);
  baseH = $derived.by(() => this.baseSize()[1]);
  /** Frame rate of the output (source fps x speed, or the cap). */
  outFps = $derived.by(() => {
    // a ramp: a steady output at the source rate (or the cap)
    if (this.speedRamp.length) return this.fps && this.fps > 0 && this.fps < this.fpsSrc - 0.01 ? this.fps : this.fpsSrc;
    const sp = this.speed || 1;
    // interpolation makes frames up to the source rate (or the cap)
    if (this.smoothSlowmo && sp < 1 && this.container !== "gif" && this.container !== "webp")
      return this.fps && this.fps > 0 ? Math.min(this.fps, this.fpsSrc) : this.fpsSrc;
    const r = this.fpsSrc * sp;
    return this.fps && this.fps > 0 && this.fps < r - 0.01 ? this.fps : r;
  });
  // with interpolation the output runs at the source rate, as at 1x
  motionIssues = $derived(
    this.media
      ? motionWarnings(this.fpsSrc, this.speedRamp.length || (this.smoothSlowmo && (this.speed || 1) < 1) ? 1 : this.speed || 1, numOrNull(this.fps), this.refreshHz)
      : [],
  );

  spec = $derived.by((): EncodeSpec | null => (this.media ? this.specFor(this.media, this.outSegments, true) : null));

  /** Warnings the UI adds on top of the ones from `plan`. */
  /** Stabilize's method: the one chosen in Settings when ffmpeg has it; unset, vid.stab when ffmpeg has it */
  get stabMethod(): StabMethod {
    const has = !!this.tools?.filters.vidstab;
    const chosen = this.settings.stabMethod ?? (has ? "vidstab" : "deshake");
    return chosen === "vidstab" && !has ? "deshake" : chosen;
  }

  uiWarnings = $derived.by((): string[] => {
    const w: string[] = [];
    // the preset's site does not take this format or codec (sites.ts)
    const no = refusal(siteOf(this.presetName), this.container, this.codec);
    if (no) w.push(`${no} Pick another format or codec, or another preset.`);
    const t = this.tools;
    if (t) {
      if (!this.codecAvailable(this.codec)) w.push(`This ffmpeg has no ${this.codec.toUpperCase()} encoder.`);
      if (this.container !== "audio" && this.audioOn && this.media?.audio.length && !t.encoders[this.audioCodec as keyof ToolsStatus["encoders"]])
        w.push(`This ffmpeg has no ${this.audioCodecName} encoder.`);
      if (this.subsKind !== "off" && !t.filters.subtitles) w.push("This ffmpeg has no subtitles filter; subtitles cannot be burned in.");
      if (this.stabilize && this.settings.stabMethod === "vidstab" && !t.filters.vidstab)
        w.push("Stabilize is set to vid.stab (Settings > Tools), but this ffmpeg has no vid.stab, so deshake is used.");
    }
    const p = this.preset;
    if (this.media && p?.maxDuration && this.outLen > p.maxDuration + 1e-6)
      w.push(`Output is ${fmtTime(this.outLen)} long; "${p.name}" allows at most ${fmtTime(p.maxDuration)}.`);
    if (this.media && this.clipLen <= 0) w.push("The selected ranges are empty.");
    if (this.rateMode === "size" && !(this.maxBytes > 0)) w.push("Set a size limit above 0.");
    if (this.subsKind === "file" && !this.subsFile) w.push("Choose a subtitle file, or turn subtitles off.");
    w.push(...this.motionIssues);
    return w;
  });

  canEncode = $derived(
    !!this.spec &&
      !this.ffmpegMissing &&
      this.clipLen > 0 &&
      (this.streamCopy || ((!this.tools || this.codecAvailable(this.codec)) && (this.rateMode !== "size" || this.maxBytes > 0))),
  );

  /** The size limit or quality setting, as a job gets it. */
  get rate(): Rate {
    return this.rateMode === "size"
      ? { mode: "size", maxBytes: this.maxBytes }
      : { mode: "crf", crf: this.crf, maxBitrateK: numOrNull(this.maxBitrateK) };
  }

  /** Build a spec from the current fields for one source. `own` = the open file (crop, subs, title apply). */
  specFor(m: MediaInfo, segments: Segment[], own: boolean): EncodeSpec {
    let subs: EncodeSpec["subs"] = null;
    if (own) {
      if (this.subsKind === "embedded" && m.subs.some((s) => s.track === this.subsTrack)) subs = { kind: "embedded", track: this.subsTrack };
      else if (this.subsKind === "file" && this.subsFile) subs = { kind: "file", path: this.subsFile };
    }
    const track = own ? this.audioTrack : (m.audio[0]?.track ?? 0);
    const box = this.boxFor(m, own ? this.crop : null);
    // music gives a silent source sound, so audio is on with it
    const music = this.musicPath && this.container !== "audio" ? { path: this.musicPath, mix: this.musicMix, volumeDb: Number(this.musicVolume) || 0, offset: Math.max(0, Number(this.musicOffset) || 0) } : null;
    const audioOk = (this.audioOn && m.audio.some((a) => a.track === track)) || (!!music && (this.audioOn || !m.audio.length));
    return {
      input: m.path,
      segments: segments.map((s) => ({ start: s.start, end: s.end })),
      crop: own ? this.jobCrop : null,
      rotate: this.rotate,
      mirror: this.mirror,
      vflip: this.vflip,
      adjust: isNeutral(this.adjust) ? null : ($state.snapshot(this.adjust) as Adjust),
      maxWidth: intOrNull(box.w),
      maxHeight: intOrNull(box.h),
      autoScale: this.rateMode === "size" && this.resMode === "auto",
      fps: numOrNull(this.fps),
      speed: this.speed,
      // the ramp is the open file's own (file time): Batch and Splits items do not get it
      speedRamp: own ? (ramp.clean($state.snapshot(this.speedRamp) as SpeedPoint[])) : [],
      zoom: own && this.zoomPoints.length ? zoomMath.clean($state.snapshot(this.zoomPoints) as ZoomPoint[]).map((p) => ({ ...p, zoom: Math.max(1, this.zoomLevel) })) : [],
      playback: this.playback,
      deinterlace: own ? this.deinterlace : m.video.interlaced,
      denoise: this.denoise,
      stabilize: this.stabilize,
      stabMethod: this.stabMethod,
      smoothSlowmo: this.smoothSlowmo,
      tonemap: own ? this.tonemap : m.video.hdr && !!this.tools?.filters.zscale,
      fadeIn: Math.max(0, this.fadeIn || 0),
      fadeOut: Math.max(0, this.fadeOut || 0),
      container: this.container,
      codec: this.codec,
      rate: this.rate,
      twoPass: this.rateMode === "size" ? true : this.twoPass,
      effort: this.effort,
      audio: audioOk ? { track, bitrateK: this.audioBitrateK, volumeDb: this.volumeDb || 0, normalize: this.normalize } : null,
      subs,
      title: own ? this.title.trim() || null : null,
      music,
      overlay: this.logoPath && this.container !== "audio" ? {
              path: this.logoPath,
              corner: this.logoCorner,
              size: this.logoSize,
              opacity: this.logoOpacity,
              x: this.logoX,
              y: this.logoY,
              spacing: this.logoSpacing,
              angle: this.logoAngle,
            } : null,
      subsDelay: Number(this.subsDelay) || 0,
      subsFontSize: this.subsFontSize,
      caption: own && this.captionText.trim() && this.container !== "audio" ? {
              text: this.captionText,
              top: this.captionTop,
              custom: this.captionCustom,
              x: this.captionX,
              y: this.captionY,
              size: this.captionSize,
              font: this.captionFont,
              color: this.captionColor,
              outlineColor: this.captionOutlineColor,
              outline: this.captionOutline,
            } : null,
      extraArgs: splitArgs(this.extraArgs),
      outputName: own ? this.outputName.trim() || null : null,
      cover: own && m.audioOnly && this.coverPath ? this.coverPath : null,
      coverSize: own && m.audioOnly && this.coverPath ? this.coverSize : null,
      streamCopy: this.streamCopy,
    };
  }

  // ---------- undo ----------

  /** The edit state as text; a change in it is an edit. The selection alone is not. */
  editKey(): string {
    return JSON.stringify([this.segments, this.order, this.crop, this.rotate, this.mirror, this.vflip, this.adjust, this.speedRamp, this.zoomPoints, this.zoomLevel]);
  }

  private snap(): EditSnap {
    const { segments, order, sel, crop, rotate, mirror, vflip, adjust, speedRamp } = this;
    const zoom = this.zoomPoints;
    const zoomLevel = this.zoomLevel;
    return $state.snapshot({ segments, order, sel, crop, rotate, mirror, vflip, adjust, speedRamp, zoom, zoomLevel }) as EditSnap;
  }

  private static key(s: EditSnap): string {
    return JSON.stringify([s.segments, s.order, s.crop, s.rotate, s.mirror, s.vflip, s.adjust, s.speedRamp, s.zoom, s.zoomLevel]);
  }

  /**
   * Called a moment after the edit state stops changing (+page.svelte), so a
   * drag becomes one step. Records the previous state as an undo step.
   */
  noteEdit() {
    const now = this.snap();
    const cur = this.steps[this.at];
    const changed = !cur || AppState.key(cur.snap) !== AppState.key(now);
    if (this.restoring) {
      this.restoring = false;
    } else if (!cur) {
      this.steps = [{ snap: now, label: "Opened the file", time: Date.now() }];
      this.at = 0;
    } else if (changed) {
      // a new step drops the ones that were undone
      let list = [...this.steps.slice(0, this.at + 1), { snap: now, label: this.nextLabel ?? describeEdit(cur.snap, now), time: Date.now() }];
      if (list.length > 201) list = list.slice(list.length - 201);
      this.steps = list;
      this.at = list.length - 1;
    }
    this.nextLabel = null;
    if (changed) this.saveEditMemory(now);
    this.canUndo = this.at > 0;
    this.canRedo = this.at < this.steps.length - 1;
  }

  /** History panel, "Start fresh": every edit of the open file goes (ranges, order, crop,
   *  rotation, flips, adjustments, speed ramp, zoom, follow path), and with it the history and
   *  the edit this file would get back when opened again. The encode settings stay. */
  startFresh() {
    if (!this.media) return;
    this.cancelFollow();
    this.cropMode = false;
    this.playEdit = false;
    this.segments = [];
    this.order = null;
    this.sel = -1;
    this.crop = null;
    this.rotate = 0;
    this.mirror = false;
    this.vflip = false;
    this.adjust = { ...NEUTRAL_ADJUST, curve: [] };
    this.speedRamp = [];
    this.zoomPoints = [];
    this.zoomLevel = 1;
    this.zoomBeforeFollow = null;
    const now = this.snap();
    this.saveEditMemory(now);
    this.steps = [{ snap: now, label: "Started fresh", time: Date.now() }];
    this.at = 0;
    this.nextLabel = null;
    this.canUndo = false;
    this.canRedo = false;
  }

  /** History panel: go to step i (back or forward). */
  goToStep(i: number) {
    if (!this.media) return;
    // an edit still waiting is recorded first; it may drop undone steps, so find the step again
    const want = this.steps[i];
    this.noteEdit();
    const k = want ? this.steps.indexOf(want) : -1;
    const s = this.steps[k];
    if (!s || k === this.at) return;
    this.at = k;
    this.restore(s.snap);
    this.canUndo = this.at > 0;
    this.canRedo = this.at < this.steps.length - 1;
  }

  private restore(s: EditSnap) {
    this.restoring = true;
    this.segments = s.segments;
    this.order = s.order;
    this.sel = Math.min(s.sel, s.segments.length - 1);
    this.crop = s.crop;
    this.rotate = s.rotate;
    this.mirror = s.mirror;
    this.vflip = s.vflip;
    this.adjust = s.adjust;
    this.speedRamp = s.speedRamp ?? [];
    this.zoomPoints = s.zoom ?? [];
    this.zoomLevel = s.zoomLevel ?? 1;
  }

  undo() {
    if (!this.media) return;
    this.noteEdit(); // an edit still waiting for its pause counts first
    if (this.at > 0) this.goToStep(this.at - 1);
  }

  redo() {
    if (!this.media) return;
    this.noteEdit();
    if (this.at < this.steps.length - 1) this.goToStep(this.at + 1);
  }

  private resetHistory() {
    this.steps = [];
    this.at = 0;
    this.nextLabel = null;
    this.restoring = false;
    this.canUndo = false;
    this.canRedo = false;
  }

  /** Picture size of a source after its crop and the rotation. */
  baseSize(m: MediaInfo | null = this.media, crop: Crop | null = this.crop): [number, number] {
    if (!m) return [0, 0];
    let w = m.video.width;
    let h = m.video.height;
    // a sound file: the size of its picture, which is never cropped
    if (m.audioOnly) {
      const s = m === this.media ? this.stillSize : null;
      return s ? [s[0], s[1]] : [w, h];
    }
    if (crop) [w, h] = [crop.w, crop.h];
    return this.rotate === 90 || this.rotate === 270 ? [h, w] : [w, h];
  }

  /** The box the output is fitted in: the size mode's box within the preset's limit. */
  boxFor(m: MediaInfo, crop: Crop | null): Box {
    const limit: Box = { w: intOrNull(this.maxWidth), h: intOrNull(this.maxHeight) };
    const [w, h] = this.baseSize(m, crop);
    if (!w || !h) return limit;
    switch (this.resMode) {
      case "step":
        return withLimit(stepBox(this.stepBy, this.resStep), limit);
      case "percent":
        return withLimit(percentBox(w, h, this.resPct), limit);
      case "exact":
        return this.exactVal > 0 ? withLimit(exactBox(this.exactEdge, this.exactVal), limit) : limit;
      default:
        return limit;
    }
  }

  private remember(path: string) {
    if (!this.showRecent) return;
    const same = (p: string) => p.replace(/\\/g, "/").toLowerCase() === path.replace(/\\/g, "/").toLowerCase();
    this.recent = [path, ...this.recent.filter((p) => !same(p))].slice(0, 10);
    save("recent", this.recent);
  }

  forgetRecent(path: string) {
    this.recent = this.recent.filter((p) => p !== path);
    save("recent", this.recent);
  }

  /** Interface size (CSS zoom on the page), since browser zoom is turned off. */
  uiScale = $state(load<number>("uiScale") ?? 1);
  setUiScale(v: number) {
    this.uiScale = v;
    save("uiScale", v);
    this.applyUiScale();
  }
  applyUiScale() {
    try {
      document.documentElement.style.zoom = this.uiScale === 1 ? "" : String(this.uiScale);
    } catch {
      // no document (tests)
    }
  }

  setRefreshHz(hz: number) {
    this.refreshHz = hz;
    save("refreshHz", hz);
  }

  async refreshOutputDir() {
    try {
      this.outputPath = await ipc.outputDir();
    } catch {
      this.outputPath = null;
    }
  }

  setTab(t: "basic" | "advanced" | "info") {
    this.tab = t;
    save("tab", t);
  }

  // ---------- startup and remembered options ----------

  constructor() {
    // The preset field and its values are filled before any IPC answers: the
    // built-in presets and the remembered options are here already. init()
    // applies them again once the preset files (maybe edited) are read.
    this.applyPreset(this.defaultPreset, false);
    this.restoreOptions();
  }

  async init() {
    // The presets and settings are small files and come back at once; the tool
    // search (finding ffmpeg, reading its encoders and filters) can take seconds
    // on a cold start. The preset is shown from the first two, without waiting
    // for the tools; the codec is checked again when they arrive.
    const toolsP = ipc.toolsStatus();
    const jobsP = ipc.queueList();
    const [settings, config] = await Promise.allSettled([ipc.settingsGet(), ipc.configGet()]);
    if (config.status === "fulfilled") {
      this.config = config.value;
      this.stepBy = config.value.stepBy;
      if (config.value.errors.length) this.toast({ kind: "failed", title: "Some config files were not read", text: config.value.errors.join("\n"), path: null });
    } else this.error = `Could not read the config: ${ipc.errText(config.reason)}`;
    if (settings.status === "fulfilled") this.settings = { ...DEFAULT_SETTINGS, ...settings.value };
    else this.error = ipc.errText(settings.reason);
    const last = this.settings.lastPreset;
    this.applyPreset(last && this.presets.some((p) => p.name === last) ? last : this.defaultPreset, false);
    this.restoreOptions();
    void this.refreshOutputDir();
    // options changed from here on are saved
    this.ready = true;
    const [tools, jobs] = await Promise.allSettled([toolsP, jobsP]);
    if (tools.status === "fulfilled") {
      this.tools = tools.value;
      // the preset's (or remembered) codec was chosen before the encoders were known
      if (!this.codecAvailable(this.codec)) this.setContainer(this.container);
    } else this.error = ipc.errText(tools.reason);
    if (jobs.status === "fulfilled") this.mergeJobs(jobs.value);
    try {
      const f = await ipc.initialFile();
      if (f) await this.open(f);
    } catch (e) {
      this.error = ipc.errText(e);
    }
  }

  optionsSnapshot(): Record<string, unknown> {
    const o: Record<string, unknown> = {};
    for (const k of REMEMBERED) o[k] = this[k];
    // sound turned on only because a sound file is open is not remembered
    if (this.audioBeforeSound !== null) o.audioOn = this.audioBeforeSound;
    return o;
  }

  /** the Audio switch before a sound file turned it on; null when it did not */
  private audioBeforeSound: boolean | null = null;

  private restoreOptions() {
    const o = load<Record<string, unknown>>("options");
    if (!o || typeof o !== "object") return;
    const self = this as unknown as Record<string, unknown>;
    for (const k of REMEMBERED) {
      const v = o[k];
      if (v === undefined) continue;
      // only take values of the same type as the default (null allowed for nullable fields)
      const cur = self[k];
      if (v === null || cur === null || typeof v === typeof cur) self[k] = v;
    }
    if (!this.presets.some((p) => p.name === this.presetName)) this.presetName = this.defaultPreset;
    if (!CODECS_BY_CONTAINER[this.container]?.includes(this.codec)) this.setContainer(this.container in CODECS_BY_CONTAINER ? this.container : "webm");
    const [lo, hi] = this.crfRange;
    this.crf = Math.min(hi, Math.max(lo, Number(this.crf) || this.crfRange[2]));
    // a remembered step from another list (steps were widths before 1.2) snaps to the nearest
    const steps = this.stepList;
    if (steps.length && !steps.includes(this.resStep))
      this.resStep = steps.reduce((b, x) => (Math.abs(x - this.resStep) < Math.abs(b - this.resStep) ? x : b), steps[0]);
  }

  saveOptions() {
    save("options", this.optionsSnapshot());
  }

  async refreshTools() {
    try {
      this.tools = await ipc.toolsStatus();
    } catch (e) {
      this.error = ipc.errText(e);
    }
  }

  // ---------- source ----------

  private openSeq = 0;

  /** Key of a file's remembered edit: path, size and duration, so a replaced file does not get it. */
  private static editMemoryKey(m: MediaInfo): string {
    return `${m.path.replace(/\\/g, "/").toLowerCase()}|${m.sizeBytes}|${m.duration.toFixed(3)}`;
  }

  /** Ranges, output order and crop of the last 40 files, so reopening a file brings its edit back. */
  private saveEditMemory(s: EditSnap) {
    const m = this.media;
    if (!m) return;
    const key = AppState.editMemoryKey(m);
    const all = (load<Record<string, EditMemory>>("edits") ?? {}) as Record<string, EditMemory>;
    if (!s.segments.length && !s.crop && !s.speedRamp.length && !s.zoom.length && s.zoomLevel === 1) delete all[key];
    else
      all[key] = {
        segments: s.segments,
        order: s.order,
        crop: s.crop,
        speedRamp: s.speedRamp.length ? s.speedRamp : undefined,
        zoom: s.zoom.length ? s.zoom : undefined,
        zoomLevel: s.zoomLevel !== 1 ? s.zoomLevel : undefined,
        t: Date.now(),
      };
    const keep = Object.entries(all).sort((a, b) => b[1].t - a[1].t).slice(0, 40);
    save("edits", Object.fromEntries(keep));
  }

  async open(path: string, restoreEdit = true): Promise<boolean> {
    // the newest open wins: a slow probe of an earlier file must not replace it
    const seq = ++this.openSeq;
    this.busy = "Reading file...";
    this.error = null;
    try {
      const info = await ipc.probe(path);
      if (seq !== this.openSeq) return false;
      this.video?.pause();
      this.media = info;
      this.usingProxy = false;
      this.proxyFraction = null;
      this.videoSrc = convertFileSrc(path);
      this.current = 0;
      this.paused = true;
      this.segments = [];
      this.sel = -1;
      this.order = null;
      this.playEdit = false;
      this.resetHistory();
      this.loop = false;
      this.loopTouched = false;
      this.crop = null;
      this.cropMode = false;
      this.plan = null;
      this.planError = null;
      this.rotate = 0;
      this.mirror = false;
      this.vflip = false;
      this.adjust = { ...NEUTRAL_ADJUST, curve: [] };
      this.speedRamp = [];
      this.zoomPoints = [];
      this.zoomLevel = 1;
      this.zoomBeforeFollow = null;
      this.deinterlace = info.video.interlaced;
      this.tonemap = info.video.hdr && !!this.tools?.filters.zscale;
      this.audioTrack = info.audio[0]?.track ?? 0;
      if (this.audioBeforeSound !== null) this.audioOn = this.audioBeforeSound;
      this.audioBeforeSound = null;
      if (!info.audio.length) this.audioOn = false;
      else if (this.preset) this.audioOn = this.preset.audioBitrateK !== null;
      // a sound file is opened for its sound; the switch goes back when another file opens
      if (info.audioOnly && !this.audioOn) {
        this.audioBeforeSound = false;
        this.audioOn = true;
      }
      this.subsTrack = info.subs.find((s) => s.textBased)?.track ?? 0;
      this.subsKind = "off";
      this.subsFile = null;
      this.subsDelay = 0;
      this.outputName = "";
      this.title = "";
      this.cuts = null;
      this.findingCuts = false;
      this.captionText = "";
      this.coverPath = null;
      this.coverSize = null;
      this.embeddedCoverUrl = null;
      if (info.audioOnly && info.video.width > 0) {
        ipc.coverPicture(path).then((p) => {
          if (this.media?.path === path) this.embeddedCoverUrl = convertFileSrc(p);
        }).catch(() => {});
      }
      this.remember(path);
      const mem = load<Record<string, EditMemory>>("edits")?.[AppState.editMemoryKey(info)];
      if (restoreEdit && mem && (mem.segments.length || mem.crop || mem.speedRamp?.length || mem.zoom?.length || (mem.zoomLevel ?? 1) !== 1)) {
        // the empty state is the first undo step, so Ctrl+Z drops the restored edit
        this.steps = [{ snap: this.snap(), label: "Opened the file", time: Date.now() }];
        this.at = 0;
        this.nextLabel = "Brought back the last edit of this file";
        this.segments = mem.segments.filter((g) => g.end > g.start && g.start < info.duration + 0.5);
        this.order = mem.order && mem.order.length === this.segments.length ? mem.order : null;
        this.crop = info.audioOnly ? null : mem.crop;
        this.speedRamp = (mem.speedRamp ?? []).filter((p) => p.t <= info.duration + 0.5);
        if (this.speedRamp.length) this.rampLane = true;
        this.zoomPoints = info.audioOnly ? [] : (mem.zoom ?? []).filter((p) => p.t <= info.duration + 0.5);
        this.zoomLevel = info.audioOnly ? 1 : (mem.zoomLevel ?? 1);
        this.sel = this.segments.length ? 0 : -1;
        if (this.segments.length) this.loop = true;
        const parts = [
          this.segments.length ? `${this.segments.length} range${this.segments.length > 1 ? "s" : ""}` : "",
          mem.crop ? "the crop" : "",
          this.speedRamp.length ? "the speed ramp" : "",
          this.zoomPoints.length ? "the follow path" : this.zoomLevel !== 1 ? "the zoom" : "",
        ].filter(Boolean);
        const what = parts.length > 1 ? `${parts.slice(0, -1).join(", ")} and ${parts.at(-1)}` : parts[0];
        this.toast({ kind: "info", title: "Restored your last edit", text: `${what} from the last time this file was open. Ctrl+Z removes them.`, path: null });
      }
      return true;
    } catch (e) {
      if (seq === this.openSeq) this.error = `Could not open ${path}: ${ipc.errText(e)}`;
      return false;
    } finally {
      if (seq === this.openSeq) this.busy = null;
    }
  }

  /** One path opens it; several add one whole-file job each with the current settings. */
  async openPaths(paths: string[]) {
    if (paths.length === 1) {
      await this.open(paths[0]);
      return;
    }
    if (!paths.length) return;
    this.error = null;
    const specs: EncodeSpec[] = [];
    const failed: string[] = [];
    for (let i = 0; i < paths.length; i++) {
      this.busy = `Reading file ${i + 1} of ${paths.length}...`;
      try {
        specs.push(this.specFor(await ipc.probe(paths[i]), [], false));
      } catch (e) {
        failed.push(`${paths[i]}: ${ipc.errText(e)}`);
      }
    }
    this.busy = null;
    if (specs.length) {
      try {
        const jobs = await ipc.queueAddMany(specs, false);
        for (const j of jobs) if (!this.jobs.some((x) => x.id === j.id)) this.jobs.push(j);
        this.setView("queue");
        this.toast({ kind: "info", title: `Added ${jobs.length} files to the queue`, text: "Press Start in the queue to encode them.", path: null });
      } catch (e) {
        failed.push(ipc.errText(e));
      }
    }
    if (failed.length) this.error = `Some files were not added:\n${failed.join("\n")}`;
  }

  /** The webview cannot play the source: make an H.264 preview copy and play that. */
  async useProxy() {
    const m = this.media;
    if (!m || this.proxyFraction !== null) return;
    if (this.usingProxy && !this.proxyRemuxed) {
      this.error = "The preview copy cannot be played either. Encoding may still work.";
      return;
    }
    // a stream copy that does not play either: make the transcoded copy
    const remux = !this.usingProxy;
    this.proxyFraction = 0;
    try {
      const p = await ipc.makeProxy(m.path, remux);
      if (this.media?.path !== m.path) return; // another file was opened meanwhile
      this.usingProxy = true;
      this.proxyRemuxed = p.remuxed;
      this.videoSrc = convertFileSrc(p.path);
    } catch (e) {
      const msg = ipc.errText(e);
      if (this.media?.path === m.path && msg !== "Canceled") this.error = `Could not make a preview copy: ${msg}`;
    } finally {
      if (this.media?.path === m.path) this.proxyFraction = null;
    }
  }

  // ---------- playback ----------

  /** the playhead at the last tick or seek: playback that crosses a range's end stops (or loops) there */
  private lastT = 0;

  /** True from a seek until the player shows the frame at the playhead (Player's seeked). */
  seekPending = $state(false);
  /** A seek asked for while the player was still seeking: done when that one ends, newest only. */
  queuedSeek: number | null = null;

  seek(t: number) {
    const d = this.duration || this.video?.duration || 0;
    const v = Math.min(Math.max(0, t), d > 0 ? d : t);
    this.current = v;
    this.lastT = v;
    const el = this.video;
    if (!el) return;
    const at = this.videoTimeOf(v);
    // A new seek would cancel one under way, and with key frames seconds apart
    // each takes a few hundred ms: held keys or a drag would then show no
    // frame until they stop. Wait for it and go to the newest time.
    if (el.seeking) this.queuedSeek = at;
    else el.currentTime = at;
    this.seekPending = true;
  }

  /**
   * Where the player is sent for playhead time t: the middle of the frame that
   * holds t, as the engine counts it (plan.rs snap_to_frames). At a frame's exact
   * start (a typed 10.000) the player picks either neighbour depending on rounding.
   */
  videoTimeOf(t: number): number {
    const f = this.fpsSrc;
    if (!(f > 0) || this.media?.video.vfr || this.audioOnly) return t;
    return (Math.floor(t * f + 1e-4) + 0.5) / f;
  }

  /** Player: the video finished a seek. Returns false when a queued one started instead. */
  seekDone(): boolean {
    const el = this.video;
    if (el && this.queuedSeek !== null) {
      el.currentTime = this.queuedSeek;
      this.queuedSeek = null;
      return false;
    }
    this.seekPending = false;
    return true;
  }

  togglePlay() {
    const v = this.video;
    if (!v || !this.media) return;
    if (v.paused && this.playEdit && this.outSegments.length) {
      // continue the edit from the range under the playhead, or from the first
      const i = this.outSegments.findIndex((s) => this.current >= s.start && this.current < s.end - 0.01);
      this.editIdx = i >= 0 ? i : 0;
      if (i < 0) this.seek(this.outSegments[0].start);
      void v.play().catch(() => {});
      return;
    }
    if (v.paused) {
      // a selected range is what is being made: play it, from its start when the playhead is outside it
      const s = this.selSeg;
      if (s && (this.current >= s.end - 0.001 || this.current < s.start)) this.seek(s.start);
      void v.play().catch(() => {});
    } else v.pause();
  }

  /** Sets the picture of a sound file (null = its embedded cover). Its size comes
   *  from ffprobe, which also lets the player load it. */
  async setCover(path: string | null) {
    if (!path) {
      this.coverPath = null;
      this.coverSize = null;
      return;
    }
    const src = this.media?.path;
    try {
      const info = await ipc.probe(path);
      if (this.media?.path !== src) return;
      if (!info.video.width || !info.video.height || info.audioOnly) throw new Error("ffprobe found no picture in it.");
      this.coverSize = [info.video.width, info.video.height];
      this.coverPath = path;
    } catch (e) {
      this.error = `Could not use ${path} as the picture: ${ipc.errText(e)}`;
    }
  }

  stepFrame(n: number) {
    if (!this.media || this.audioOnly) return;
    this.video?.pause();
    // to the middle of the frame: at a frame's exact start time rounding decides
    // which of two frames the player shows. The engine cuts at the frame that
    // holds a time (plan.rs snap_to_frames), so this is the frame that gets cut.
    const f = this.fpsSrc;
    if (!(f > 0)) return;
    const k = Math.floor(this.current * f + 1e-4) + n;
    this.seek((Math.max(0, k) + 0.5) / f);
  }

  /** Called every animation frame while playing. */
  tick() {
    const v = this.video;
    if (!v) return;
    const prev = this.lastT;
    this.current = v.currentTime;
    this.lastT = this.current;
    if (this.playEdit) {
      this.tickEdit(v);
      return;
    }
    // at the end of the selected range: again from its start (loop), or stop there.
    // Only when playback runs across the end: after a click past it, playing goes on.
    const s = this.selSeg;
    if (s && !v.paused && prev < s.end && this.current >= s.end) {
      if (this.loop) this.seek(s.start);
      else {
        v.pause();
        this.seek(s.end);
      }
    }
  }

  /** Play the edit: at the end of a range jump to the next one in output order. */
  private tickEdit(v: HTMLVideoElement) {
    const list = this.outSegments;
    const s = list[this.editIdx];
    if (!s) return;
    if (this.current < s.end - 0.001 && !v.ended) return;
    this.editIdx++;
    if (this.editIdx >= list.length) {
      this.editIdx = 0;
      if (!this.loop) {
        v.pause();
        this.seek(list[0].start);
        return;
      }
    }
    this.seek(list[this.editIdx].start);
    if (v.paused) void v.play().catch(() => {});
  }

  togglePlayEdit() {
    this.playEdit = !this.playEdit;
    if (this.playEdit && this.outSegments.length) {
      this.editIdx = 0;
      this.seek(this.outSegments[0].start);
      void this.video?.play().catch(() => {});
    } else this.video?.pause();
  }

  /** Edit strip: move the range at output position `from` to position `to`. */
  moveOut(from: number, to: number) {
    const ids = this.outSegments.map((s) => s.id as number);
    if (from < 0 || from >= ids.length || to < 0 || to >= ids.length || from === to) return;
    const [id] = ids.splice(from, 1);
    ids.splice(to, 0, id);
    this.order = ids;
  }

  /** Edit strip: join the ranges in source order again. */
  resetOrder() {
    this.order = null;
  }

  onEnded() {
    if (this.playEdit && this.video && this.outSegments.length) {
      this.tickEdit(this.video);
      return;
    }
    const s = this.selSeg;
    if (this.loop && s && this.video) {
      this.seek(s.start);
      void this.video.play().catch(() => {});
    }
  }

  // ---------- ranges ----------

  private applyEdit(e: seg.Edit | null) {
    if (!e || e.index < 0) return;
    const had = this.hasRange;
    for (const s of e.list) if (s.id === undefined) s.id = this.nextSegId++;
    this.segments = e.list;
    this.sel = e.index;
    if (!had && this.hasRange && !this.loopTouched) this.loop = true;
  }

  /** `I`: set the start of the range under or nearest the playhead. */
  markIn(t = this.current) {
    this.applyEdit(seg.setIn(this.segments, t, this.duration, this.minLen));
  }

  /** `O`: set the end of the range under or nearest the playhead. */
  markOut(t = this.current) {
    this.applyEdit(seg.setOut(this.segments, t, this.minLen));
  }

  /** `N`: start a new range at the playhead (5 s, or up to the next range). */
  addRange() {
    if (!this.media) return;
    const e = seg.addAt(this.segments, this.current, this.duration, 5, this.minLen);
    if (!e) {
      this.toast({ kind: "info", title: "No room for a range here", text: "The playhead is inside a range or too close to the next one. Move it to a free spot.", path: null });
      return;
    }
    this.applyEdit(e);
  }

  /** Set an edge of the selected range (time fields and handle drags). */
  setEdge(i: number, edge: "start" | "end", t: number) {
    if (i < 0 || i >= this.segments.length) return;
    this.segments = seg.setEdge(this.segments, i, edge, t, this.duration, this.minLen);
    this.sel = i;
  }

  /** Drag a range's body: move it so it starts at `start`, keeping its length. */
  moveRange(i: number, start: number) {
    if (i < 0 || i >= this.segments.length) return;
    this.segments = seg.moveTo(this.segments, i, start, this.duration);
    this.sel = i;
  }

  /** Time field "In": edit the selected range, or create one. */
  setSelStart(t: number) {
    if (this.selSeg) this.setEdge(this.sel, "start", t);
    else this.markIn(t);
  }

  setSelEnd(t: number) {
    if (this.selSeg) this.setEdge(this.sel, "end", t);
    else this.markOut(t);
  }

  setSelLength(len: number) {
    if (len <= 0) return;
    if (this.selSeg) this.setEdge(this.sel, "end", this.selSeg.start + len);
    else this.markOut(len);
  }

  removeRange(i = this.sel) {
    if (i < 0 || i >= this.segments.length) return;
    this.segments = this.segments.filter((_, k) => k !== i);
    this.sel = this.segments.length ? Math.min(i, this.segments.length - 1) : -1;
  }

  clearRanges() {
    this.segments = [];
    this.sel = -1;
    this.order = null;
    this.playEdit = false;
  }

  toggleLoop() {
    this.loopTouched = true;
    this.loop = !this.loop;
  }

  // ---------- crop ----------

  get srcW() {
    return this.media?.video.width ?? 0;
  }
  get srcH() {
    return this.media?.video.height ?? 0;
  }

  setCrop(c: Crop | null) {
    if (!c || !this.media) {
      this.crop = null;
      return;
    }
    const r = round(c, this.srcW, this.srcH);
    this.crop = isFull(r, this.srcW, this.srcH) ? null : r;
  }

  // The crop as shown: the picture turned and flipped as the encode will (crop
  // mode shows it that way). The crop itself stays in source pixels.
  get turn(): Turn {
    return { rotate: this.rotate, mirror: this.mirror, vflip: this.vflip };
  }
  get viewW() {
    return viewSize(this.srcW, this.srcH, this.turn)[0];
  }
  get viewH() {
    return viewSize(this.srcW, this.srcH, this.turn)[1];
  }
  get viewCrop(): Crop | null {
    return this.crop ? toView(this.crop, this.srcW, this.srcH, this.turn) : null;
  }
  /** Set the crop from one drawn on the shown picture; `commit` false while dragging (no rounding). */
  setViewCrop(c: Crop | null, commit = true) {
    const s = c ? fromView(c, this.srcW, this.srcH, this.turn) : null;
    if (commit) this.setCrop(s);
    else this.crop = s;
  }

  /** The aspect lock is the shape of the crop as shown (9:16 is upright on screen). */
  setAspect(a: AspectLock) {
    this.aspect = a;
    const r = ratioOf(a);
    if (r && this.media) {
      const base = this.viewCrop ?? { x: 0, y: 0, w: this.viewW, h: this.viewH };
      this.setViewCrop(fitRatio(base, r, this.viewW, this.viewH));
    }
  }

  // ---------- speed ramp (points in file time, kept sorted) ----------

  /** Adds a point at file time t (at the ramp's speed there, or 1x); returns its index. */
  addRampPoint(t: number, speed?: number): number {
    const p = ramp.clean(this.speedRamp);
    const at = Math.min(Math.max(0, t), this.duration || t);
    const s = ramp.niceSpeed(speed ?? (p.length ? ramp.speedAt(p, at) : this.speed || 1));
    const next = ramp.clean([...p.filter((q) => Math.abs(q.t - at) >= 0.01), { t: at, speed: s }]);
    this.speedRamp = next;
    this.rampLane = true;
    return next.findIndex((q) => q.t === at);
  }

  /** Moves point i; it stays between its neighbours. */
  setRampPoint(i: number, t: number, speed: number) {
    const p = [...this.speedRamp];
    if (!p[i]) return;
    const lo = i > 0 ? p[i - 1].t + 0.01 : 0;
    const hi = i < p.length - 1 ? p[i + 1].t - 0.01 : this.duration || t;
    p[i] = { t: Math.min(Math.max(lo, t), Math.max(lo, hi)), speed: ramp.niceSpeed(speed) };
    this.speedRamp = p;
  }

  removeRampPoint(i: number) {
    this.speedRamp = this.speedRamp.filter((_, k) => k !== i);
  }

  clearRamp() {
    this.speedRamp = [];
  }

  // ---------- zoom (Advanced > Zoom) and the follow path (engine zoom.rs) ----------

  /** The zoom's frame, as shown: the crop, or the whole picture. */
  get zoomFrame(): Crop {
    return this.viewCrop ?? { x: 0, y: 0, w: this.viewW, h: this.viewH };
  }
  /** The widest zoom: the crop grown around its middle until one side reaches the picture's (1 without a crop). */
  get zoomMin(): number {
    const c = this.viewCrop;
    return c && this.viewW && this.viewH ? Math.min(1, Math.max(c.w / this.viewW, c.h / this.viewH)) : 1;
  }
  setZoomLevel(z: number) {
    if (!Number.isFinite(z)) return;
    this.zoomLevel = Math.round(Math.min(zoomMath.MAX_ZOOM, Math.max(this.zoomMin, z)) * 100) / 100;
  }
  /** Without a follow path: what the output shows, as shown (the frame scaled around its middle by the zoom, kept inside the picture). */
  get zoomView(): Crop {
    const f = this.zoomFrame;
    const z = Math.max(this.zoomMin, this.zoomLevel);
    if (Math.abs(z - 1) < 1e-9) return f;
    const w = Math.min(this.viewW, f.w / z);
    const h = Math.min(this.viewH, f.h / z);
    return {
      x: Math.min(Math.max(0, f.x + f.w / 2 - w / 2), this.viewW - w),
      y: Math.min(Math.max(0, f.y + f.h / 2 - h / 2), this.viewH - h),
      w,
      h,
    };
  }
  /** The crop the job gets (source pixels): the crop with the zoom in it; with a follow path the crop
   *  alone (the zoom then moves along the path in the encode). */
  get jobCrop(): Crop | null {
    if (this.zoomPoints.length || Math.abs(this.zoomLevel - 1) < 1e-9 || !this.viewW) return this.crop ? { ...this.crop } : null;
    const r = round(fromView(this.zoomView, this.srcW, this.srcH, this.turn), this.srcW, this.srcH);
    return isFull(r, this.srcW, this.srcH) ? null : r;
  }
  /** What the output shows at source time t, as shown: the follow window there, or the zoom's view. */
  outViewAt(t: number): Crop {
    if (!this.zoomPoints.length) return this.zoomView;
    const p = zoomMath.at(this.zoomPoints, t);
    const f = this.zoomFrame;
    const w = zoomMath.windowOf({ zoom: Math.max(1, this.zoomLevel), x: p.x, y: p.y }, f.w, f.h);
    return { x: f.x + w.x, y: f.y + w.y, w: w.w, h: w.h };
  }
  /** The follow point nearest source time t (its index), or -1. */
  followPointAt(t: number): number {
    let best = -1;
    let d = Infinity;
    this.zoomPoints.forEach((p, i) => {
      if (Math.abs(p.t - t) < d) {
        d = Math.abs(p.t - t);
        best = i;
      }
    });
    return best;
  }

  /** Changes point i (time kept between its neighbours). */
  setZoomPoint(i: number, ch: Partial<ZoomPoint>) {
    const p = [...this.zoomPoints];
    const q = p[i];
    if (!q) return;
    const lo = i > 0 ? p[i - 1].t + 0.01 : 0;
    const hi = i < p.length - 1 ? p[i + 1].t - 0.01 : this.duration || q.t;
    const t = ch.t === undefined ? q.t : Math.min(Math.max(lo, ch.t), Math.max(lo, hi));
    const cl = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
    p[i] = {
      t,
      zoom: cl(ch.zoom ?? q.zoom, zoomMath.MIN_ZOOM, zoomMath.MAX_ZOOM),
      x: cl(ch.x ?? q.x, 0, 1),
      y: cl(ch.y ?? q.y, 0, 1),
    };
    this.zoomPoints = p;
  }

  // ---------- follow a person (engine track.rs) ----------

  /** Drawing the box to follow on the picture. */
  followMode = $state(false);
  /** the box being drawn, in picture-as-shown pixels */
  followBox = $state<Crop | null>(null);
  /** while following: how far, 0..1 */
  following = $state<number | null>(null);
  /** the zoom before Follow set one (Remove puts it back), or null */
  private zoomBeforeFollow: number | null = null;

  /** Follow the box drawn on the picture (as shown) from the playhead to the end of the
   *  selected range (or of the file), and make the follow path that keeps it in the picture. */
  async follow(viewBox: Crop) {
    const m = this.media;
    if (!m || m.audioOnly || this.following !== null) return;
    this.followMode = false;
    this.followBox = null;
    const start = this.current;
    const s = this.selSeg;
    const end = s && start >= s.start && start < s.end ? s.end : this.duration;
    if (end - start < 0.2) {
      this.toast({ kind: "info", title: "Nothing to follow", text: "Put the playhead before the end of the range first.", path: null });
      return;
    }
    const src = fromView(viewBox, this.srcW, this.srcH, this.turn);
    const rect = { x: Math.round(src.x), y: Math.round(src.y), w: Math.max(8, Math.round(src.w)), h: Math.max(8, Math.round(src.h)) };
    this.following = 0;
    try {
      const pts = await ipc.trackBox(m.path, start, end, rect);
      if (this.media?.path !== m.path || !pts.length) return;
      // the zoom's frame: the crop (or the whole picture) as shown
      const frame = this.zoomFrame;
      const first = toView({ x: pts[0].x - pts[0].w / 2, y: pts[0].y - pts[0].h / 2, w: pts[0].w, h: pts[0].h }, this.srcW, this.srcH, this.turn);
      // the box fills about 85% of the frame
      const zoom = Math.round(Math.min(zoomMath.MAX_ZOOM, Math.max(zoomMath.MIN_ZOOM, 0.85 * Math.min(frame.w / first.w, frame.h / first.h))) * 100) / 100;
      const made: ZoomPoint[] = [];
      for (let t = start; t <= end + 1e-6; t += 0.25) {
        const p = pts.reduce((a, b) => (Math.abs(b.t - t) < Math.abs(a.t - t) ? b : a));
        const c = toView({ x: p.x, y: p.y, w: 0, h: 0 }, this.srcW, this.srcH, this.turn);
        made.push({ t: Math.round(t * 1000) / 1000, zoom, x: (c.x - frame.x) / frame.w, y: (c.y - frame.y) / frame.h });
      }
      const keep = this.zoomPoints.filter((q) => q.t < start - 0.01 || q.t > end + 0.01);
      this.zoomPoints = zoomMath.clean([...keep, ...made]);
      // following needs a zoom past the frame; the box then fills about 85% of it
      if (this.zoomLevel <= 1.001) {
        this.zoomBeforeFollow = this.zoomLevel;
        this.zoomLevel = zoom;
      }
      const weak = pts.filter((p) => p.score < 0.35).length;
      this.toast({
        kind: "info",
        title: `Following for ${fmtSecs(end - start)}`,
        text: `At ${this.zoomLevel}x (Advanced > Zoom).${weak ? ` In ${weak} of ${pts.length} frames the box was hard to find (something in front of it); it stayed where it was there.` : ""} Drag the picture to correct the path where it is off; Remove takes it away.`,
        path: null,
      });
    } catch (e) {
      if (ipc.errText(e) !== "Canceled") this.error = `Could not follow the box: ${ipc.errText(e)}`;
    } finally {
      this.following = null;
    }
  }

  cancelFollow() {
    this.followMode = false;
    this.followBox = null;
    if (this.following !== null) void ipc.trackCancel().catch(() => {});
  }

  /** Remove the follow path, and the zoom Follow set (a zoom chosen by hand stays). */
  clearFollow() {
    this.zoomPoints = [];
    if (this.zoomBeforeFollow !== null) this.zoomLevel = this.zoomBeforeFollow;
    this.zoomBeforeFollow = null;
  }

  /** Arrow keys in crop mode, as shown on screen: move, or with `grow` change the size from the top-left corner. */
  nudgeCrop(dx: number, dy: number, grow: boolean) {
    const c = this.viewCrop;
    if (!c || !this.media) return;
    const [W, H] = [this.viewW, this.viewH];
    if (grow) {
      const w = Math.min(Math.max(1, c.w + dx), W - c.x);
      const h = Math.min(Math.max(1, c.h + dy), H - c.y);
      this.setViewCrop({ x: c.x, y: c.y, w, h }, false);
    } else this.setViewCrop(move(c, dx, dy, W, H), false);
  }

  // ---------- container and codec ----------

  codecAvailable(c: Codec): boolean {
    // ToolsStatus from a build without the gif field: ffmpeg always has the GIF encoder
    return !this.tools || (this.tools.encoders[c as keyof ToolsStatus["encoders"]] ?? c === "gif");
  }

  /** WebM or MP4, whichever can hold the source's streams as they are (plan.rs `copy_container`). */
  copyContainer(m: MediaInfo | null = this.media): Container | null {
    if (!m) return null;
    const a = this.audioOn ? m.audio.find((t) => t.track === this.audioTrack)?.codec : undefined;
    const fits = (v: string[], au: string[]) => v.includes(m.video.codec) && (!a || au.includes(a));
    if (fits(["vp8", "vp9", "av1"], ["opus", "vorbis"])) return "webm";
    if (fits(["h264", "hevc", "av1", "mpeg4"], ["aac", "mp3", "opus", "ac3", "eac3", "flac", "alac"])) return "mp4";
    return null;
  }

  setStreamCopy(on: boolean) {
    this.streamCopy = on;
    const c = on ? this.copyContainer() : null;
    if (c && c !== this.container) this.setContainer(c);
  }

  setContainer(c: Container) {
    this.container = c;
    // audio-only output needs a track
    if (c === "audio" && (!this.media || this.media.audio.length)) this.audioOn = true;
    const list = CODECS_BY_CONTAINER[c];
    if (!list.includes(this.codec) || !this.codecAvailable(this.codec)) this.setCodec(list.find((k) => this.codecAvailable(k)) ?? list[0]);
  }

  /** Changing the codec moves CRF to that codec's default, since the scales differ. */
  setCodec(c: Codec) {
    if (c === this.codec) return;
    this.codec = c;
    this.crf = CRF_RANGE[c][2];
  }

  // ---------- presets (TOML files, Rust config.rs) ----------

  /** Read config.toml and the preset files again (after editing them by hand). */
  async reloadConfig(quiet = false) {
    try {
      this.config = await ipc.configGet();
      if (this.config.errors.length) this.toast({ kind: "failed", title: "Some config files were not read", text: this.config.errors.join("\n"), path: null });
      else if (!quiet) this.toast({ kind: "info", title: "Config reloaded", text: `${this.config.presets.length} presets`, path: null });
    } catch (e) {
      this.error = `Could not read the config: ${ipc.errText(e)}`;
    }
  }

  applyPreset(name: string, remember = true) {
    const p = this.presets.find((q) => q.name === name);
    if (!p) return;
    this.presetName = p.name;
    this.container = p.container in CODECS_BY_CONTAINER ? p.container : "webm";
    const list = CODECS_BY_CONTAINER[this.container];
    this.codec = list.includes(p.codec) && this.codecAvailable(p.codec) ? p.codec : (list.find((k) => this.codecAvailable(k)) ?? p.codec);
    if (p.rate.mode === "size") {
      this.rateMode = "size";
      this.maxBytes = p.rate.maxBytes;
    } else {
      this.rateMode = "crf";
      const [lo, hi] = CRF_RANGE[this.codec];
      this.crf = Math.min(hi, Math.max(lo, p.rate.crf));
      this.maxBitrateK = p.rate.maxBitrateK;
    }
    this.autoScale = p.autoScale;
    this.resMode = p.autoScale ? "auto" : "source";
    this.maxWidth = p.maxWidth;
    this.maxHeight = p.maxHeight;
    this.fps = p.fps;
    this.effort = p.effort;
    this.twoPass = p.twoPass;
    if (p.audioBitrateK === null) this.audioOn = false;
    else {
      this.audioBitrateK = p.audioBitrateK;
      this.audioOn = !this.media || this.media.audio.length > 0;
    }
    if (remember && this.settings.lastPreset !== p.name) void this.saveSettings({ ...this.settings, lastPreset: p.name });
  }

  /** The current fields as a preset. maxDuration is kept from the selected preset. */
  currentAsPreset(name: string): Preset {
    // a step becomes the preset's limit on that edge; a percentage or an exact
    // edge depends on the source and is not kept
    const step = this.resMode === "step";
    const lim = (v: number | null, edge: "height" | "width") => {
      const n = intOrNull(v);
      return step && this.stepBy === edge ? (n === null ? this.resStep : Math.min(n, this.resStep)) : n;
    };
    return {
      name,
      container: this.container,
      codec: this.codec,
      rate:
        this.rateMode === "size"
          ? { mode: "size", maxBytes: this.maxBytes }
          : { mode: "crf", crf: this.crf, maxBitrateK: numOrNull(this.maxBitrateK) },
      autoScale: this.resMode === "auto",
      maxWidth: lim(this.maxWidth, "width"),
      maxHeight: lim(this.maxHeight, "height"),
      fps: numOrNull(this.fps),
      audioBitrateK: this.audioOn ? this.audioBitrateK : null,
      effort: this.effort,
      twoPass: this.rateMode === "size" ? true : this.twoPass,
      maxDuration: this.preset?.maxDuration ?? null,
    };
  }

  private nameError(n: string, except: string | null = null): string | null {
    if (!n) return "Enter a name.";
    if (n !== except && this.presets.some((p) => p.name === n)) return "A preset has that name.";
    return null;
  }

  private async writePreset(p: Preset, file: string | null): Promise<string | null> {
    try {
      await ipc.presetSave($state.snapshot(p) as Preset, file);
      await this.reloadConfig(true);
      return null;
    } catch (e) {
      return ipc.errText(e);
    }
  }

  /** Save the current fields as a new preset file in presets/saved. Returns an error message, or null. */
  async savePresetAs(name: string): Promise<string | null> {
    const n = name.trim();
    const bad = this.nameError(n);
    if (bad) return bad;
    const err = await this.writePreset(this.currentAsPreset(n), null);
    if (!err) {
      this.presetName = n;
      void this.saveSettings({ ...this.settings, lastPreset: n });
    }
    return err;
  }

  /** Replace the selected preset's file with the current fields. */
  async overwritePreset(name: string): Promise<string | null> {
    const e = this.presetEntries.find((x) => x.preset.name === name);
    if (!e?.file) return "This preset has no file.";
    return this.writePreset({ ...this.currentAsPreset(name), maxDuration: e.preset.maxDuration }, e.file);
  }

  async renamePreset(from: string, to: string): Promise<string | null> {
    const n = to.trim();
    const e = this.presetEntries.find((x) => x.preset.name === from);
    if (!e?.file) return "This preset has no file.";
    const bad = this.nameError(n, from);
    if (bad) return bad;
    const err = await this.writePreset({ ...e.preset, name: n }, e.file);
    if (!err) {
      this.presetName = n;
      void this.saveSettings({ ...this.settings, lastPreset: n });
    }
    return err;
  }

  async deletePreset(name: string): Promise<string | null> {
    const e = this.presetEntries.find((x) => x.preset.name === name);
    if (!e?.file) return "This preset has no file.";
    try {
      await ipc.presetDelete(e.file);
    } catch (err) {
      return ipc.errText(err);
    }
    await this.reloadConfig(true);
    if (this.defaultPreset) this.applyPreset(this.defaultPreset);
    return null;
  }

  /** Write the platform presets again (missing or edited ones get their defaults back). */
  async restorePlatformPresets(): Promise<string | null> {
    try {
      await ipc.configRestorePlatform();
      await this.reloadConfig(true);
      return null;
    } catch (e) {
      return ipc.errText(e);
    }
  }

  /** Export the saved presets (presets/saved) as one JSON file. Returns an error message,
   *  null when done, or undefined when the dialog was canceled. */
  async exportPresets(): Promise<string | null | undefined> {
    try {
      const saved = this.presetEntries.filter((e) => e.group === "saved").map((e) => e.preset);
      return (await ipc.presetsExport($state.snapshot(saved) as Preset[])) === null ? undefined : null;
    } catch (e) {
      return ipc.errText(e);
    }
  }

  /** Import presets from a JSON file into presets/saved. A name in use gets " (imported)". */
  async importPresets(): Promise<string | null | undefined> {
    let got: Preset[];
    try {
      const r = await ipc.presetsImport();
      if (!r) return undefined;
      got = r;
    } catch (e) {
      return ipc.errText(e);
    }
    for (const p of got) {
      const name = this.presets.some((q) => q.name === p.name) ? `${p.name} (imported)` : p.name;
      try {
        await ipc.presetSave({ ...p, name }, null);
      } catch (e) {
        return ipc.errText(e);
      }
    }
    await this.reloadConfig(true);
    this.toast({ kind: "info", title: `Imported ${got.length} preset${got.length === 1 ? "" : "s"}`, text: "", path: null });
    return null;
  }

  /** Save settings to the backend. Returns an error message, or null. */
  async saveSettings(s: Settings): Promise<string | null> {
    try {
      await ipc.settingsSet($state.snapshot(s) as Settings);
      this.settings = s;
      void this.refreshOutputDir();
      return null;
    } catch (e) {
      const msg = ipc.errText(e);
      this.error = `Could not save settings: ${msg}`;
      return msg;
    }
  }

  // ---------- plan, sample, frame ----------

  async runPlan(spec: EncodeSpec | null) {
    const seq = ++this.planSeq;
    if (!spec || this.ffmpegMissing) {
      this.plan = null;
      this.planError = null;
      return;
    }
    try {
      const p = await ipc.plan(spec);
      if (seq !== this.planSeq) return;
      this.plan = p;
      this.planError = null;
    } catch (e) {
      if (seq !== this.planSeq) return;
      this.plan = null;
      this.planError = ipc.errText(e);
    }
  }

  // ---------- quality check (engine quality.rs) ----------

  /** The check of the current settings: VMAF of a 2 s probe, null while it runs or when it failed. */
  quality = $state<{ key: string; vmaf: number | null; busy: boolean; error: string | null } | null>(null);
  private qualityCache = new Map<string, number>();
  private qualitySeq = 0;

  /** Whether the current output can be checked (a video encode, ffmpeg with libvmaf). */
  get qualityCheckable(): boolean {
    return (
      !!this.media &&
      !this.audioOnly &&
      !this.ffmpegMissing &&
      !!this.tools?.filters.libvmaf &&
      !this.streamCopy &&
      !["gif", "webp", "audio"].includes(this.container)
    );
  }

  /** Runs automatically 1.5 s after the settings stop changing (+page.svelte); not while the queue encodes. */
  async runQualityCheck(spec: EncodeSpec | null) {
    const seq = ++this.qualitySeq;
    if (!spec || !this.qualityCheckable || !this.plan) {
      this.quality = null;
      return;
    }
    const snap = $state.snapshot(spec) as EncodeSpec;
    const key = JSON.stringify(snap);
    const cached = this.qualityCache.get(key);
    if (cached !== undefined) {
      this.quality = { key, vmaf: cached, busy: false, error: null };
      return;
    }
    if (this.runningJob) {
      // the check would slow the encode down; it runs when the queue is done
      if (this.quality?.key !== key) this.quality = null;
      return;
    }
    this.quality = { key, vmaf: null, busy: true, error: null };
    try {
      const v = await ipc.qualityCheck(snap);
      this.qualityCache.set(key, v);
      if (seq === this.qualitySeq) this.quality = { key, vmaf: v, busy: false, error: null };
    } catch (e) {
      if (seq !== this.qualitySeq) return;
      const msg = ipc.errText(e);
      this.quality = msg === "Canceled" ? null : { key, vmaf: null, busy: false, error: msg };
    }
  }

  /** `P`: encode 3 seconds at the playhead with the current settings. */
  /** Sample length in seconds (3, 6 or 10), remembered for the session. */
  sampleSecs = $state(3);

  async runSample(at = this.current) {
    const s = this.spec;
    if (!s || this.ffmpegMissing) return;
    const seq = ++this.sampleSeq;
    this.video?.pause();
    this.sample = { busy: true, at, path: null, sizeBytes: null, kbps: null, error: null };
    try {
      const path = await ipc.previewSample($state.snapshot(s) as EncodeSpec, at, this.sampleSecs);
      if (seq !== this.sampleSeq) return;
      // the sample's size and bitrate come from probing the file
      let sizeBytes: number | null = null;
      let kbps: number | null = null;
      try {
        const info = await ipc.probe(path);
        sizeBytes = info.sizeBytes;
        kbps = info.duration > 0 ? (info.sizeBytes * 8) / info.duration / 1000 : null;
      } catch {
        // the sample still plays without the numbers
      }
      if (seq !== this.sampleSeq) return;
      this.sample = { busy: false, at, path, sizeBytes, kbps, error: null };
    } catch (e) {
      if (seq !== this.sampleSeq) return;
      this.sample = { busy: false, at, path: null, sizeBytes: null, kbps: null, error: ipc.errText(e) };
    }
  }

  closeSample() {
    if (this.sample?.busy) ipc.sampleCancel().catch(() => {});
    this.sampleSeq++;
    this.sample = null;
  }

  /** `S`: save the frame at the playhead as a PNG, with the current crop. */
  /** Save the frame at the playhead: the source with the crop, or with `processed` as the encode sees it. */
  async exportFrame(processed = false) {
    const m = this.media;
    if (!m || this.ffmpegMissing) return;
    // a sound file has only the picture as the encode makes it
    if (m.audioOnly) processed = true;
    this.video?.pause();
    try {
      const s = this.spec;
      const path =
        processed && s
          ? await ipc.exportFrameProcessed($state.snapshot(s) as EncodeSpec, this.current)
          : await ipc.exportFrame(m.path, this.current, this.crop ? { ...this.crop } : null);
      this.toast({ kind: "done", title: processed ? "Frame saved as encoded (filters and size)" : "Frame saved", text: path, path });
    } catch (e) {
      this.error = `Could not save the frame: ${ipc.errText(e)}`;
    }
  }

  // ---------- queue ----------

  mergeJobs(list: Job[]) {
    for (const j of list) this.upsertJob(j);
  }

  upsertJob(j: Job) {
    const i = this.jobs.findIndex((x) => x.id === j.id);
    const was = i >= 0 ? this.jobs[i].status : null;
    if (i >= 0) this.jobs[i] = j;
    else this.jobs.push(j);
    if (was === "running" && (j.status === "done" || j.status === "failed")) this.toastJob(j);
    // a job that just ended while the editor is shown: count it for the Queue tab's badge
    if (this.view !== "queue" && (was === "running" || was === "pending")) {
      if (j.status === "done") this.unseen = { ...this.unseen, done: this.unseen.done + 1 };
      else if (j.status === "failed") this.unseen = { ...this.unseen, failed: this.unseen.failed + 1 };
    }
  }

  toast(t: Omit<Toast, "id">) {
    this.toasts = [{ ...t, id: ++this.toastSeq }, ...this.toasts].slice(0, 6);
  }

  dismissToast(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  private toastJob(j: Job) {
    const name = (j.output ?? j.spec.input).split(/[\\/]/).pop() ?? "";
    if (j.status === "failed") {
      this.toast({ kind: "failed", title: "Encode failed", text: `${name}\n${(j.error ?? "").split("\n").slice(-2).join("\n")}`, path: null });
      return;
    }
    const r = j.result;
    const mib = r ? `${(r.sizeBytes / 1048576).toFixed(2)} MiB` : "";
    const extra = r && r.attempts > 1 ? ` · ${r.attempts} attempts` : "";
    const warn = r?.warnings?.length ? `\n${r.warnings.join("\n")}` : "";
    this.toast({
      kind: r?.overLimit || r?.warnings?.length ? "over" : "done",
      title: r?.overLimit ? "Encoded, but over the size limit" : r?.warnings?.length ? "Encoded, but the output differs from the plan" : "Encode finished",
      text: `${name}\n${mib}${extra}${warn}`,
      path: j.output,
    });
  }

  /**
   * The spec for one batch file: the global settings (as for any file other
   * than the open one: no crop, subtitles, title or name; deinterlace and tone
   * map follow the source) plus the item's own trim, crop, frame rate cap and
   * audio switch.
   */
  specForItem(m: MediaInfo, o: ItemOverrides): EncodeSpec {
    const s = this.specFor(m, o.segments, false);
    if (o.crop) {
      s.crop = { ...o.crop };
      const box = this.boxFor(m, o.crop);
      s.maxWidth = intOrNull(box.w);
      s.maxHeight = intOrNull(box.h);
    }
    if (o.fps !== undefined) s.fps = numOrNull(o.fps);
    if (o.audioOff) s.audio = null;
    return s;
  }

  setView(v: View) {
    this.view = v;
    if (v === "queue") this.unseen = { done: 0, failed: 0 };
  }

  /** Remove every finished, failed or canceled job from the list. */
  async clearFinished() {
    for (const j of this.jobs.filter((x) => x.status !== "running" && x.status !== "pending")) await this.removeJob(j.id);
  }

  /** Encode / Add to queue. With "Separate files" and several ranges, one job per range. */
  private lastEnqueue: { key: string; t: number } | null = null;

  async enqueue(start: boolean) {
    const s = this.spec;
    if (!s || !this.canEncode) return;
    const snap = $state.snapshot(s) as EncodeSpec;
    // a double click or a repeated Ctrl+Enter within a second adds the same job once
    const key = JSON.stringify([snap, this.separate, start]);
    const now = Date.now();
    if (this.lastEnqueue && this.lastEnqueue.key === key && now - this.lastEnqueue.t < 1000) return;
    this.lastEnqueue = { key, t: now };
    try {
      const got =
        this.separate && snap.segments.length > 1
          ? await ipc.queueAddMany(snap.segments.map((g) => ({ ...snap, segments: [g] })), start)
          : [await ipc.queueAdd(snap, start)];
      // a job-update for these jobs may have arrived first and is newer
      for (const j of got) if (!this.jobs.some((x) => x.id === j.id)) this.jobs.push(j);
    } catch (e) {
      this.error = ipc.errText(e);
    }
  }

  /** The queue takes the next pending job when the running one ends. */
  queueStarted = $state(false);

  async pauseQueue() {
    try {
      await ipc.queuePause();
    } catch (e) {
      this.error = ipc.errText(e);
    }
  }

  /** Put a finished, failed or canceled job back in the queue (at the end). */
  async retryJob(id: number) {
    try {
      const j = await ipc.queueRetry(id);
      this.jobs = [...this.jobs.filter((x) => x.id !== id), j];
    } catch (e) {
      this.error = ipc.errText(e);
    }
  }

  /** Scene cut times of the open file (seconds), from "Find cuts"; null = not looked for. */
  cuts = $state<number[] | null>(null);
  findingCuts = $state(false);
  /** how far "Find cuts" has read the video, 0..1 */
  cutsFraction = $state(0);
  /** fewer cuts (only clear ones), normal, or more (also between similar shots); remembered */
  cutSensitivity = $state<ipc.CutSensitivity>(load<ipc.CutSensitivity>("cutSensitivity") ?? "normal");

  async findCuts(quiet = false) {
    const m = this.media;
    if (!m || m.audioOnly || this.findingCuts || this.ffmpegMissing) return;
    this.findingCuts = true;
    this.cutsFraction = 0;
    try {
      const c = await ipc.sceneCuts(m.path, this.cutSensitivity);
      if (this.media?.path !== m.path) return;
      this.cuts = c;
      if (!quiet) this.toast({ kind: "info", title: `${c.length} scene cut${c.length === 1 ? "" : "s"} found`, text: "Range edges snap to them; [ and ] jump between them.", path: null });
    } catch (e) {
      if (this.media?.path === m.path && ipc.errText(e) !== "Canceled") this.error = `Could not find scene cuts: ${ipc.errText(e)}`;
    } finally {
      if (this.media?.path === m.path) this.findingCuts = false;
    }
  }

  /** The video was read once, so the cuts for another sensitivity come at once. */
  setCutSensitivity(s: ipc.CutSensitivity) {
    this.cutSensitivity = s;
    save("cutSensitivity", s);
    if (this.cuts !== null) void this.findCuts(true);
  }
  /** Seek to the previous (-1) or next (+1) scene cut. */
  jumpCut(dir: number) {
    const c = this.cuts;
    if (!c?.length) return;
    const t = this.current;
    const next = dir > 0 ? c.find((x) => x > t + 0.02) : [...c].reverse().find((x) => x < t - 0.02);
    if (next !== undefined) this.seek(next);
  }

  detectingCrop = $state(false);
  /** Set the crop to the source's black bars (ffmpeg cropdetect at three points). */
  async detectBars() {
    const m = this.media;
    if (!m || m.audioOnly || this.detectingCrop || this.ffmpegMissing) return;
    this.detectingCrop = true;
    try {
      const c = await ipc.detectCrop(m.path);
      if (this.media?.path !== m.path) return;
      if (c) {
        this.setCrop(c);
        this.toast({ kind: "info", title: "Black bars cropped", text: `${c.w}x${c.h} at ${c.x},${c.y}. Ctrl+Z undoes it.`, path: null });
      } else this.toast({ kind: "info", title: "No black bars found", text: "Checked 2 seconds at a quarter, half and three quarters of the file.", path: null });
    } catch (e) {
      this.error = `Could not check for black bars: ${ipc.errText(e)}`;
    } finally {
      this.detectingCrop = false;
    }
  }

  /** Open a job's source in the editor with the job's ranges, crop and flips. */
  async editJob(j: Job) {
    if (!(await this.open(j.spec.input, false))) return;
    const s = j.spec;
    // a joined job: only the ranges of its first file belong on this timeline
    this.segments = s.segments.filter((_, i) => !s.segmentSources?.[i] || s.segmentSources[i] === s.input).map((g) => ({ ...g }));
    this.order = null;
    this.sel = this.segments.length ? 0 : -1;
    this.crop = s.crop ? { ...s.crop } : null;
    this.rotate = s.rotate;
    this.mirror = s.mirror;
    this.vflip = !!s.vflip;
    this.loop = this.segments.length > 0;
    this.setView("editor");
  }

  /** Move a job up (-1) or down (+1) in the list. */
  /** Moves a waiting job past its neighbour among the jobs still to run (finished ones are listed apart). */
  async moveJob(id: number, delta: number) {
    const live = this.jobs.filter((x) => x.status === "pending" || x.status === "running");
    const nb = live[live.findIndex((x) => x.id === id) + delta];
    if (!nb) return;
    try {
      this.jobs = await ipc.queueMove(id, this.jobs.findIndex((x) => x.id === nb.id));
    } catch (e) {
      this.error = ipc.errText(e);
    }
  }

  async startQueue() {
    try {
      await ipc.queueStart();
    } catch (e) {
      this.error = ipc.errText(e);
    }
  }

  async cancelJob(id: number) {
    try {
      await ipc.queueCancel(id);
      // a pending job is removed by the backend; a running one sends job-update
      const j = this.jobs.find((x) => x.id === id);
      if (j && j.status === "pending") this.jobs = this.jobs.filter((x) => x.id !== id);
    } catch (e) {
      this.error = ipc.errText(e);
    }
  }

  async removeJob(id: number) {
    try {
      await ipc.queueRemove(id);
      this.jobs = this.jobs.filter((x) => x.id !== id);
    } catch (e) {
      this.error = ipc.errText(e);
    }
  }
}

export const app = new AppState();
app.applyUiScale();
