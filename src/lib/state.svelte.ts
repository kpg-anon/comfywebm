// App state: source, playback, ranges, crop, spec fields, plan, jobs, settings.
import { convertFileSrc } from "@tauri-apps/api/core";
import * as ipc from "./ipc";
import { load, save } from "./persist";
import { AUDIO_LABEL, BUILTIN_PRESETS, CODECS_BY_CONTAINER, CRF_RANGE, DEFAULT_PRESET, audioCodecFor, isBuiltin } from "./presets";
import { fitRatio, isFull, move, ratioOf, round, type AspectLock } from "./crop";
import * as seg from "./segments";
import { fmtTime } from "./time";
import { motionWarnings } from "./motion";
import { exactBox, percentBox, stepBox, withLimit, type Box, type ResMode } from "./resolution";
import type {
  Codec,
  Container,
  Crop,
  Effort,
  EncodeSpec,
  Job,
  MediaInfo,
  PlanInfo,
  Playback,
  Preset,
  Rotate,
  Segment,
  Settings,
  ToolsStatus,
} from "./types";

export type SubsKind = "off" | "embedded" | "file";

export const DEFAULT_SETTINGS: Settings = {
  ffmpegPath: null,
  ytdlpPath: null,
  ytdlpArgs: [],
  downloadDir: null,
  keepDir: null,
  outputDir: null,
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
  "maxWidth", "maxHeight", "fps", "speed", "playback", "denoise", "fadeIn", "fadeOut", "effort",
  "twoPass", "audioOn", "audioBitrateK", "volumeDb", "normalize", "separate", "aspect",
  "resMode", "resStep", "resPct", "exactEdge", "exactVal",
] as const;

/** Display refresh rates offered for the motion check. */
export const REFRESH_RATES = [50, 60, 75, 90, 100, 120, 144, 165, 240];

/** One undo step: ranges, output order, selected range, crop. */
type EditSnap = { segments: Segment[]; order: number[] | null; sel: number; crop: Crop | null };

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
  ready = $state(false); // init() has loaded settings and restored the remembered options
  /** Jobs running or waiting. */
  activeJobs = $derived(this.jobs.filter((j) => j.status === "running" || j.status === "pending").length);
  runningJob = $derived(this.jobs.find((j) => j.status === "running") ?? null);
  /** Left side: the editor (player and timeline) or the queue. */
  view = $state<"editor" | "queue">("editor");
  /** Jobs that finished while the queue was not shown; cleared when it is. */
  unseen = $state({ done: 0, failed: 0 });
  urlOpen = $state(false);
  urlInitial = $state("");
  sample = $state<SampleState | null>(null);
  private sampleSeq = 0;
  /** A short message with an optional file to reveal, e.g. after a frame export. */
  notice = $state<{ text: string; path: string | null } | null>(null);

  // source
  media = $state<MediaInfo | null>(null);
  videoSrc = $state<string | null>(null);
  usingProxy = $state(false);
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

  // ranges on the source timeline, sorted by start; empty = whole file
  segments = $state<Segment[]>([]);
  sel = $state(-1); // index of the selected range
  separate = $state(false); // each range as its own job
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
  presetName = $state(DEFAULT_PRESET);
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
  resStep = $state(1280);
  resPct = $state(50);
  exactEdge = $state<"w" | "h">("w");
  exactVal = $state(1280);
  /** Refresh rate of the display the output is meant for; a per-machine preference. */
  refreshHz = $state(load<number>("refreshHz") ?? 60);
  /** Sidebar tab. */
  tab = $state<"basic" | "advanced">(load<string>("tab") === "advanced" ? "advanced" : "basic");
  fps = $state<number | null>(null);
  speed = $state(1);
  playback = $state<Playback>("forward");
  deinterlace = $state(false);
  denoise = $state(false);
  tonemap = $state(false);
  rotate = $state<Rotate>(0);
  mirror = $state(false);
  fadeIn = $state(0);
  fadeOut = $state(0);
  effort = $state<Effort>("balanced");
  twoPass = $state(true);
  audioOn = $state(false);
  audioTrack = $state(0);
  audioBitrateK = $state(96);
  volumeDb = $state(0);
  normalize = $state(false);
  subsKind = $state<SubsKind>("off");
  subsTrack = $state(0);
  subsFile = $state<string | null>(null);
  title = $state("");
  extraArgs = $state("");
  outputName = $state("");

  // undo/redo of ranges, output order and crop; see noteEdit()
  private past: EditSnap[] = [];
  private future: EditSnap[] = [];
  private committed: EditSnap | null = null;
  private restoring = false;
  canUndo = $state(false);
  canRedo = $state(false);

  // plan result
  plan = $state<PlanInfo | null>(null);
  planError = $state<string | null>(null);
  private planSeq = 0;

  // ---------- derived ----------

  duration = $derived(this.media?.duration ?? 0);
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
    const src = this.separate && this.segments.length > 1 ? Math.max(...this.segments.map((s) => s.end - s.start)) : this.clipLen;
    return (src / (this.speed || 1)) * (this.playback === "boomerang" ? 2 : 1);
  });

  presets = $derived<Preset[]>([...BUILTIN_PRESETS, ...this.settings.presets]);
  preset = $derived(this.presets.find((p) => p.name === this.presetName) ?? null);

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
    const r = this.fpsSrc * (this.speed || 1);
    return this.fps && this.fps > 0 && this.fps < r - 0.01 ? this.fps : r;
  });
  motionIssues = $derived(this.media ? motionWarnings(this.fpsSrc, this.speed || 1, numOrNull(this.fps), this.refreshHz) : []);

  spec = $derived.by((): EncodeSpec | null => (this.media ? this.specFor(this.media, this.outSegments, true) : null));

  /** Warnings the UI adds on top of the ones from `plan`. */
  uiWarnings = $derived.by((): string[] => {
    const w: string[] = [];
    const t = this.tools;
    if (t) {
      if (!t.encoders[this.codec]) w.push(`This ffmpeg has no ${this.codec.toUpperCase()} encoder.`);
      if (this.audioOn && this.media?.audio.length && !t.encoders[this.audioCodec])
        w.push(`This ffmpeg has no ${this.audioCodecName} encoder.`);
      if (this.subsKind !== "off" && !t.filters.subtitles) w.push("This ffmpeg has no subtitles filter; subtitles cannot be burned in.");
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
      (!this.tools || this.tools.encoders[this.codec]) &&
      (this.rateMode !== "size" || this.maxBytes > 0),
  );

  /** Build a spec from the current fields for one source. `own` = the open file (crop, subs, title apply). */
  specFor(m: MediaInfo, segments: Segment[], own: boolean): EncodeSpec {
    let subs: EncodeSpec["subs"] = null;
    if (own) {
      if (this.subsKind === "embedded" && m.subs.some((s) => s.track === this.subsTrack)) subs = { kind: "embedded", track: this.subsTrack };
      else if (this.subsKind === "file" && this.subsFile) subs = { kind: "file", path: this.subsFile };
    }
    const track = own ? this.audioTrack : (m.audio[0]?.track ?? 0);
    const box = this.boxFor(m, own);
    const audioOk = this.audioOn && m.audio.some((a) => a.track === track);
    return {
      input: m.path,
      segments: segments.map((s) => ({ start: s.start, end: s.end })),
      crop: own && this.crop ? { ...this.crop } : null,
      rotate: this.rotate,
      mirror: this.mirror,
      maxWidth: intOrNull(box.w),
      maxHeight: intOrNull(box.h),
      autoScale: this.rateMode === "size" && this.resMode === "auto",
      fps: numOrNull(this.fps),
      speed: this.speed,
      playback: this.playback,
      deinterlace: own ? this.deinterlace : m.video.interlaced,
      denoise: this.denoise,
      tonemap: own ? this.tonemap : m.video.hdr && !!this.tools?.filters.zscale,
      fadeIn: Math.max(0, this.fadeIn || 0),
      fadeOut: Math.max(0, this.fadeOut || 0),
      container: this.container,
      codec: this.codec,
      rate:
        this.rateMode === "size"
          ? { mode: "size", maxBytes: this.maxBytes }
          : { mode: "crf", crf: this.crf, maxBitrateK: numOrNull(this.maxBitrateK) },
      twoPass: this.rateMode === "size" ? true : this.twoPass,
      effort: this.effort,
      audio: audioOk ? { track, bitrateK: this.audioBitrateK, volumeDb: this.volumeDb || 0, normalize: this.normalize } : null,
      subs,
      title: own ? this.title.trim() || null : null,
      extraArgs: splitArgs(this.extraArgs),
      outputName: own ? this.outputName.trim() || null : null,
    };
  }

  // ---------- undo ----------

  /** The edit state as text; a change in it is an edit. The selection alone is not. */
  editKey(): string {
    return JSON.stringify([this.segments, this.order, this.crop]);
  }

  private snap(): EditSnap {
    return $state.snapshot({ segments: this.segments, order: this.order, sel: this.sel, crop: this.crop }) as EditSnap;
  }

  private static key(s: EditSnap): string {
    return JSON.stringify([s.segments, s.order, s.crop]);
  }

  /**
   * Called a moment after the edit state stops changing (+page.svelte), so a
   * drag becomes one step. Records the previous state as an undo step.
   */
  noteEdit() {
    const now = this.snap();
    if (this.restoring) {
      this.restoring = false;
    } else if (this.committed && AppState.key(this.committed) !== AppState.key(now)) {
      this.past.push(this.committed);
      if (this.past.length > 200) this.past.shift();
      this.future = [];
    }
    this.committed = now;
    this.canUndo = this.past.length > 0;
    this.canRedo = this.future.length > 0;
  }

  private restore(s: EditSnap) {
    this.restoring = true;
    this.segments = s.segments;
    this.order = s.order;
    this.sel = Math.min(s.sel, s.segments.length - 1);
    this.crop = s.crop;
    this.committed = s;
  }

  undo() {
    if (!this.media) return;
    this.noteEdit(); // an edit still waiting for its pause counts first
    const s = this.past.pop();
    if (!s) return;
    this.future.push(this.snap());
    this.restore(s);
    this.canUndo = this.past.length > 0;
    this.canRedo = true;
  }

  redo() {
    if (!this.media) return;
    this.noteEdit();
    const s = this.future.pop();
    if (!s) return;
    this.past.push(this.snap());
    this.restore(s);
    this.canUndo = true;
    this.canRedo = this.future.length > 0;
  }

  private resetHistory() {
    this.past = [];
    this.future = [];
    this.committed = null;
    this.restoring = false;
    this.canUndo = false;
    this.canRedo = false;
  }

  /** Picture size after crop and rotate for a source (`own`: the open file, with its crop). */
  private baseSize(m: MediaInfo | null = this.media, own = true): [number, number] {
    if (!m) return [0, 0];
    let w = m.video.width;
    let h = m.video.height;
    if (own && this.crop) [w, h] = [this.crop.w, this.crop.h];
    return this.rotate === 90 || this.rotate === 270 ? [h, w] : [w, h];
  }

  /** The box the output is fitted in: the size mode's box within the preset's limit. */
  boxFor(m: MediaInfo, own: boolean): Box {
    const limit: Box = { w: intOrNull(this.maxWidth), h: intOrNull(this.maxHeight) };
    const [w, h] = this.baseSize(m, own);
    if (!w || !h) return limit;
    switch (this.resMode) {
      case "step":
        return withLimit(stepBox(w, h, this.resStep), limit);
      case "percent":
        return withLimit(percentBox(w, h, this.resPct), limit);
      case "exact":
        return this.exactVal > 0 ? withLimit(exactBox(this.exactEdge, this.exactVal), limit) : limit;
      default:
        return limit;
    }
  }

  private remember(path: string) {
    const same = (p: string) => p.replace(/\\/g, "/").toLowerCase() === path.replace(/\\/g, "/").toLowerCase();
    this.recent = [path, ...this.recent.filter((p) => !same(p))].slice(0, 10);
    save("recent", this.recent);
  }

  forgetRecent(path: string) {
    this.recent = this.recent.filter((p) => p !== path);
    save("recent", this.recent);
  }

  setRefreshHz(hz: number) {
    this.refreshHz = hz;
    save("refreshHz", hz);
  }

  setTab(t: "basic" | "advanced") {
    this.tab = t;
    save("tab", t);
  }

  // ---------- startup and remembered options ----------

  async init() {
    const [tools, settings, jobs] = await Promise.allSettled([ipc.toolsStatus(), ipc.settingsGet(), ipc.queueList()]);
    if (tools.status === "fulfilled") this.tools = tools.value;
    else this.error = ipc.errText(tools.reason);
    if (settings.status === "fulfilled") this.settings = { ...DEFAULT_SETTINGS, ...settings.value };
    else this.error = ipc.errText(settings.reason);
    if (jobs.status === "fulfilled") this.mergeJobs(jobs.value);
    const last = this.settings.lastPreset;
    this.applyPreset(last && this.presets.some((p) => p.name === last) ? last : DEFAULT_PRESET, false);
    this.restoreOptions();
    this.ready = true;
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
    return o;
  }

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
    if (!this.presets.some((p) => p.name === this.presetName)) this.presetName = DEFAULT_PRESET;
    if (!CODECS_BY_CONTAINER[this.container]?.includes(this.codec)) this.setContainer(this.container === "mp4" ? "mp4" : "webm");
    const [lo, hi] = this.crfRange;
    this.crf = Math.min(hi, Math.max(lo, Number(this.crf) || this.crfRange[2]));
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

  async open(path: string): Promise<boolean> {
    this.busy = "Reading file...";
    this.error = null;
    try {
      const info = await ipc.probe(path);
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
      this.deinterlace = info.video.interlaced;
      this.tonemap = info.video.hdr && !!this.tools?.filters.zscale;
      this.audioTrack = info.audio[0]?.track ?? 0;
      if (!info.audio.length) this.audioOn = false;
      else if (this.preset) this.audioOn = this.preset.audioBitrateK !== null;
      this.subsTrack = info.subs.find((s) => s.textBased)?.track ?? 0;
      this.subsKind = "off";
      this.subsFile = null;
      this.outputName = "";
      this.title = "";
      this.remember(path);
      return true;
    } catch (e) {
      this.error = `Could not open ${path}: ${ipc.errText(e)}`;
      return false;
    } finally {
      this.busy = null;
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
        this.notice = { text: `Added ${jobs.length} files to the queue. Press Start to encode them.`, path: null };
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
    if (this.usingProxy) {
      this.error = "The preview copy cannot be played either. Encoding may still work.";
      return;
    }
    this.proxyFraction = 0;
    try {
      const p = await ipc.makeProxy(m.path);
      if (this.media?.path !== m.path) return; // another file was opened meanwhile
      this.usingProxy = true;
      this.videoSrc = convertFileSrc(p);
    } catch (e) {
      if (this.media?.path === m.path) this.error = `Could not make a preview copy: ${ipc.errText(e)}`;
    } finally {
      if (this.media?.path === m.path) this.proxyFraction = null;
    }
  }

  // ---------- playback ----------

  seek(t: number) {
    const d = this.duration || this.video?.duration || 0;
    const v = Math.min(Math.max(0, t), d > 0 ? d : t);
    this.current = v;
    if (this.video) this.video.currentTime = v;
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
      // with loop on, play from the range start when the playhead is outside the selected range
      const s = this.selSeg;
      if (this.loop && s && (this.current >= s.end - 0.001 || this.current < s.start)) this.seek(s.start);
      void v.play().catch(() => {});
    } else v.pause();
  }

  stepFrame(n: number) {
    if (!this.media) return;
    this.video?.pause();
    this.seek(this.current + n / this.fpsSrc);
  }

  /** Called every animation frame while playing. */
  tick() {
    const v = this.video;
    if (!v) return;
    this.current = v.currentTime;
    if (this.playEdit) {
      this.tickEdit(v);
      return;
    }
    const s = this.selSeg;
    if (this.loop && s && !v.paused && this.current >= s.end) this.seek(s.start);
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
      this.notice = { text: "The playhead is inside a range or too close to the next one. Move it to a free spot to add a range.", path: null };
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

  setAspect(a: AspectLock) {
    this.aspect = a;
    const r = ratioOf(a);
    if (r && this.media) {
      const base = this.crop ?? { x: 0, y: 0, w: this.srcW, h: this.srcH };
      this.setCrop(fitRatio(base, r, this.srcW, this.srcH));
    }
  }

  /** Arrow keys in crop mode: move, or with `grow` change the size from the top-left corner. */
  nudgeCrop(dx: number, dy: number, grow: boolean) {
    const c = this.crop;
    if (!c || !this.media) return;
    if (grow) {
      const w = Math.min(Math.max(1, c.w + dx), this.srcW - c.x);
      const h = Math.min(Math.max(1, c.h + dy), this.srcH - c.y);
      this.crop = { x: c.x, y: c.y, w, h };
    } else this.crop = move(c, dx, dy, this.srcW, this.srcH);
  }

  // ---------- container and codec ----------

  codecAvailable(c: Codec): boolean {
    return !this.tools || this.tools.encoders[c];
  }

  setContainer(c: Container) {
    this.container = c;
    const list = CODECS_BY_CONTAINER[c];
    if (!list.includes(this.codec) || !this.codecAvailable(this.codec)) this.setCodec(list.find((k) => this.codecAvailable(k)) ?? list[0]);
  }

  /** Changing the codec moves CRF to that codec's default, since the scales differ. */
  setCodec(c: Codec) {
    if (c === this.codec) return;
    this.codec = c;
    this.crf = CRF_RANGE[c][2];
  }

  // ---------- presets ----------

  applyPreset(name: string, remember = true) {
    const p = this.presets.find((q) => q.name === name);
    if (!p) return;
    this.presetName = p.name;
    this.container = p.container === "mp4" ? "mp4" : "webm";
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
    // a step is kept as a square box (it limits the long edge of any picture);
    // a percentage or an exact edge depends on the source and is not kept
    const step = this.resMode === "step";
    const lim = (v: number | null) => {
      const n = intOrNull(v);
      return step ? (n === null ? this.resStep : Math.min(n, this.resStep)) : n;
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
      maxWidth: lim(this.maxWidth),
      maxHeight: lim(this.maxHeight),
      fps: numOrNull(this.fps),
      audioBitrateK: this.audioOn ? this.audioBitrateK : null,
      effort: this.effort,
      twoPass: this.rateMode === "size" ? true : this.twoPass,
      maxDuration: this.preset?.maxDuration ?? null,
    };
  }

  private nameError(n: string, except: string | null = null): string | null {
    if (!n) return "Enter a name.";
    if (isBuiltin(n)) return "A built-in preset has that name.";
    if (n !== except && this.settings.presets.some((p) => p.name === n)) return "A saved preset has that name.";
    return null;
  }

  /** Save the current fields as a new user preset. Returns an error message, or null. */
  async savePresetAs(name: string): Promise<string | null> {
    const n = name.trim();
    const bad = this.nameError(n);
    if (bad) return bad;
    const err = await this.saveSettings({ ...this.settings, presets: [...this.settings.presets, this.currentAsPreset(n)], lastPreset: n });
    if (!err) this.presetName = n;
    return err;
  }

  /** Replace the selected user preset with the current fields. */
  async overwritePreset(name: string): Promise<string | null> {
    if (isBuiltin(name)) return "Built-in presets cannot be changed.";
    const keep = this.settings.presets.find((p) => p.name === name);
    const p = { ...this.currentAsPreset(name), maxDuration: keep?.maxDuration ?? null };
    return this.saveSettings({ ...this.settings, presets: this.settings.presets.map((q) => (q.name === name ? p : q)) });
  }

  async renamePreset(from: string, to: string): Promise<string | null> {
    const n = to.trim();
    if (isBuiltin(from)) return "Built-in presets cannot be renamed.";
    const bad = this.nameError(n, from);
    if (bad) return bad;
    const list = this.settings.presets.map((q) => (q.name === from ? { ...q, name: n } : q));
    const err = await this.saveSettings({ ...this.settings, presets: list, lastPreset: n });
    if (!err) this.presetName = n;
    return err;
  }

  async deletePreset(name: string) {
    if (isBuiltin(name)) return;
    const list = this.settings.presets.filter((q) => q.name !== name);
    const err = await this.saveSettings({ ...this.settings, presets: list, lastPreset: DEFAULT_PRESET });
    if (!err) this.applyPreset(DEFAULT_PRESET, false);
  }

  async exportPresets(path: string): Promise<string | null> {
    try {
      await ipc.presetsExport(path, $state.snapshot(this.settings.presets) as Preset[]);
      return null;
    } catch (e) {
      return ipc.errText(e);
    }
  }

  /** Import presets from a file. Same names replace saved ones; a built-in name gets " (imported)". */
  async importPresets(path: string): Promise<string | null> {
    let got: Preset[];
    try {
      got = await ipc.presetsImport(path);
    } catch (e) {
      return ipc.errText(e);
    }
    const list = [...this.settings.presets];
    for (const p of got) {
      const name = isBuiltin(p.name) ? `${p.name} (imported)` : p.name;
      const q = { ...p, name };
      const i = list.findIndex((x) => x.name === name);
      if (i >= 0) list[i] = q;
      else list.push(q);
    }
    const err = await this.saveSettings({ ...this.settings, presets: list });
    if (!err) this.notice = { text: `Imported ${got.length} preset${got.length === 1 ? "" : "s"}.`, path: null };
    return err;
  }

  /** Save settings to the backend. Returns an error message, or null. */
  async saveSettings(s: Settings): Promise<string | null> {
    try {
      await ipc.settingsSet($state.snapshot(s) as Settings);
      this.settings = s;
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

  /** `P`: encode 3 seconds at the playhead with the current settings. */
  async runSample() {
    const s = this.spec;
    if (!s || this.ffmpegMissing) return;
    const seq = ++this.sampleSeq;
    const at = this.current;
    this.video?.pause();
    this.sample = { busy: true, at, path: null, sizeBytes: null, kbps: null, error: null };
    try {
      const path = await ipc.previewSample($state.snapshot(s) as EncodeSpec, at);
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
    this.sampleSeq++;
    this.sample = null;
  }

  /** `S`: save the frame at the playhead as a PNG, with the current crop. */
  async exportFrame() {
    const m = this.media;
    if (!m || this.ffmpegMissing) return;
    this.video?.pause();
    try {
      const path = await ipc.exportFrame(m.path, this.current, this.crop ? { ...this.crop } : null);
      this.notice = { text: `Saved frame: ${path}`, path };
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
    // a job that just ended while the editor is shown: count it for the Queue tab's badge
    if (this.view !== "queue" && (was === "running" || was === "pending")) {
      if (j.status === "done") this.unseen = { ...this.unseen, done: this.unseen.done + 1 };
      else if (j.status === "failed") this.unseen = { ...this.unseen, failed: this.unseen.failed + 1 };
    }
  }

  setView(v: "editor" | "queue") {
    this.view = v;
    if (v === "queue") this.unseen = { done: 0, failed: 0 };
  }

  /** Remove every finished, failed or canceled job from the list. */
  async clearFinished() {
    for (const j of this.jobs.filter((x) => x.status !== "running" && x.status !== "pending")) await this.removeJob(j.id);
  }

  /** Encode / Add to queue. With "Separate files" and several ranges, one job per range. */
  async enqueue(start: boolean) {
    const s = this.spec;
    if (!s || !this.canEncode) return;
    try {
      const snap = $state.snapshot(s) as EncodeSpec;
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
