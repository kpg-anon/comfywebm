// The Splits tab: two or more clips (videos or pictures) combined into one
// output, one after another or at the same time (side by side, stacked, a
// grid). Every choice is a plain word; the canvas size is worked out here from
// them, and the engine (EncodeSpec.layout) does the rest. The sidebar's
// settings apply to the result, as in Batch.
import { app } from "./state.svelte";
import * as ipc from "./ipc";
import { fitRatio, isFull, move as moveRect, ratioOf, round, type AspectLock } from "./crop";
import type { EncodeSpec, Fill, Layout, LayoutMode, PlanInfo, Rect } from "./types";

export interface SplitClip {
  id: number;
  path: string;
  name: string;
  info: import("./types").MediaInfo;
  /** seconds in the file; a picture: 0 and its length */
  start: number;
  end: number;
  /** a picture, not a video */
  still: boolean;
  /** the part of the picture used, in its display pixels; null = all of it */
  crop: Rect | null;
  /** the crop's aspect lock: "part" = the shape of the space the clip fills */
  aspect: AspectLock;
}

/** how long a picture shows, one after another, until changed */
export const STILL_SECS = 5;

/** "auto" or an aspect as width:height */
export type Shape = "auto" | "16:9" | "9:16" | "1:1" | "4:5";
export type SizeChoice = "auto" | "small" | "medium" | "large";
export const SHAPES: { value: Shape; label: string; hint: string }[] = [
  { value: "auto", label: "Auto", hint: "the shape that wastes the least space" },
  { value: "16:9", label: "16:9 Wide", hint: "YouTube, a landscape screen" },
  { value: "9:16", label: "9:16 Tall", hint: "a phone held upright, Shorts, TikTok" },
  { value: "1:1", label: "1:1 Square", hint: "square" },
  { value: "4:5", label: "4:5", hint: "a tall post" },
];
/** the short side of the picture for each size */
export const SIZES: Record<Exclude<SizeChoice, "auto">, { label: string; short: number }> = {
  small: { label: "Small", short: 480 },
  medium: { label: "Medium", short: 720 },
  large: { label: "Large", short: 1080 },
};
export const GAPS = { none: 0, thin: 8, thick: 24 } as const;

const ratio = (s: Exclude<Shape, "auto">) => {
  const [w, h] = s.split(":").map(Number);
  return w / h;
};
const even = (v: number) => Math.max(2, Math.round(v / 2) * 2);
/** the clip's picture size in display pixels */
export const frameOf = (c: SplitClip): [number, number] => [c.info.video.width || 16, c.info.video.height || 9];
/** the clip's shape as shown (square pixels), after its crop */
const clipRatio = (c: SplitClip) => {
  if (c.crop) return c.crop.w / c.crop.h;
  const [w, h] = frameOf(c);
  return w / h;
};
/** the clip's own length (a picture: the length it was given) */
const own = (c: SplitClip) => Math.max(0, c.end - c.start);

class Splits {
  clips = $state<SplitClip[]>([]);
  busy = $state<string | null>(null);
  // the choices
  mode = $state<LayoutMode>("columns");
  shape = $state<Shape>("auto");
  /** split screen: the shape of each part; picking one sets the final shape */
  partShape = $state<Shape>("auto");
  size = $state<SizeChoice>("auto");
  fill = $state<Fill>("bars");
  /** split screen */
  sound = $state<"mix" | "none" | number>(0);
  /** split screen: as long as the shortest clip, or the longest (the shorter ones hold their last frame, or loop) */
  length = $state<"shortest" | "longest" | "loop">("shortest");
  gap = $state<keyof typeof GAPS>("none");
  gapColor = $state("#000000");
  /** side by side / stacked: each part as wide (tall) as its clip's shape needs, equal parts, the first clip twice
   * as big, or sizes set by dragging the line between two parts (`custom`) */
  parts = $state<"fit" | "equal" | "first" | "custom">("fit");
  custom = $state<number[]>([]);
  plan = $state<PlanInfo | null>(null);
  /** the layout the plan was made for: its cells are used only while the layout is the same */
  private planLayout = $state("");
  planError = $state<string | null>(null);
  // the preview's place in the output, in seconds; seekSeq changes on every seek
  time = $state(0);
  playing = $state(false);
  /** the preview starts again from the beginning when it reaches the end */
  loopPreview = $state(true);
  /** play / pause of the preview, set while the Splits tab shows it (the Space key) */
  togglePlay: (() => void) | null = null;
  seekSeq = $state(0);
  /** while an end of a clip's range is dragged: that clip shows that moment of its file */
  peek = $state<{ id: number; t: number } | null>(null);
  private nextId = 1;
  private planSeq = 0;

  get split() {
    return this.mode !== "sequence";
  }

  /** grid columns and rows for the clips */
  get grid(): [number, number] {
    const n = Math.max(1, this.clips.length);
    const cols = this.mode === "columns" ? n : this.mode === "rows" ? 1 : Math.ceil(Math.sqrt(n));
    return [cols, Math.ceil(n / cols)];
  }

  /** relative part sizes for side by side / stacked */
  get weights(): number[] {
    const n = this.clips.length;
    if (this.mode === "grid" || this.mode === "sequence" || n < 2) return Array(n).fill(1);
    if (this.parts === "first") return [2, ...Array(n - 1).fill(1)];
    if (this.parts === "custom" && this.custom.length === n) return this.custom;
    if (this.parts === "fit" && this.partShape === "auto") {
      // each clip whole in its part: widths by shape side by side, heights by inverse shape stacked
      const r = this.clips.map((c) => clipRatio(c));
      return this.mode === "columns" ? r : r.map((x) => 1 / x);
    }
    return Array(n).fill(1);
  }

  /** the final picture's width / height */
  get aspect(): number {
    const n = this.clips.length;
    if (!n) return 16 / 9;
    const [cols, rows] = this.grid;
    const w = this.weights;
    const sum = w.reduce((a, b) => a + b, 0);
    if (this.split && this.partShape !== "auto") {
      const p = ratio(this.partShape);
      // parts of that shape side by side: as wide as their weights add up to (the
      // smaller parts have the shape; a part of weight 2 is twice as wide). Sizes
      // dragged by hand share the canvas of equal parts, so it does not jump.
      const k = this.parts === "custom" ? n : sum;
      if (this.mode === "columns") return p * k;
      if (this.mode === "rows") return p / k;
      return (p * cols) / rows;
    }
    if (this.shape !== "auto") return ratio(this.shape);
    const r = this.clips.map((c) => clipRatio(c));
    switch (this.mode) {
      case "sequence":
        return r[0];
      case "columns":
        // every clip at the same height, whole
        return r.reduce((a, b) => a + b, 0);
      case "rows":
        return 1 / r.reduce((a, b) => a + 1 / b, 0);
      case "grid":
        return ((r.reduce((a, b) => a + b, 0) / n) * cols) / rows;
    }
  }

  /** canvas width and height for the size choice (Auto: Large, made smaller by the size limit) */
  get canvas(): [number, number] {
    const a = Math.min(8, Math.max(1 / 8, this.aspect));
    const short = this.size === "auto" ? 1080 : SIZES[this.size].short;
    const [w, h] = a >= 1 ? [short * a, short] : [short, short / a];
    // keep the long side at most 3840
    const k = Math.min(1, 3840 / Math.max(w, h));
    return [even(w * k), even(h * k)];
  }

  /** frame rate: the highest of the videos (at most 60); a picture's reported rate does not count */
  get fps(): number {
    const vids = this.clips.filter((c) => !c.still);
    if (!vids.length) return 30;
    const f = Math.max(...vids.map((c) => c.info.video.fps || 30), 1);
    return Math.min(60, Math.round(f * 1000) / 1000);
  }

  /** the size of each choice for the Size buttons, as "1280x720" */
  sizeLabel(s: Exclude<SizeChoice, "auto">): string {
    const a = Math.min(8, Math.max(1 / 8, this.aspect));
    const short = SIZES[s].short;
    const [w, h] = a >= 1 ? [short * a, short] : [short, short / a];
    // at most 3840, and within the largest size the preset allows
    const k = Math.min(1, 3840 / Math.max(w, h), app.maxWidth ? app.maxWidth / w : 1, app.maxHeight ? app.maxHeight / h : 1);
    return `${even(w * k)}x${even(h * k)}`;
  }

  /** the output's length. Split screen: a picture lasts as long as the videos, so only they count. */
  get length_(): number {
    const all = this.clips.map(own);
    if (!all.length) return 0;
    if (!this.split) return all.reduce((a, b) => a + b, 0);
    const vids = this.clips.filter((c) => !c.still).map(own);
    const lens = vids.length ? vids : all;
    return this.length === "shortest" ? Math.min(...lens) : Math.max(...lens);
  }

  /** split screen: clip i starts again from its start when it ends, until the output ends */
  loops(i: number): boolean {
    const c = this.clips[i];
    return !!c && this.split && this.length === "loop" && !c.still && own(c) > 0 && own(c) < this.length_ - 1e-3;
  }

  /** how long clip i shows in the output (split screen: before it holds its last frame) */
  lenOf(i: number): number {
    const c = this.clips[i];
    if (!c) return 0;
    return this.split && c.still ? this.length_ : own(c);
  }

  /** one after another: when each clip starts in the output */
  get offsets(): number[] {
    let at = 0;
    return this.clips.map((c) => {
      const o = at;
      at += own(c);
      return o;
    });
  }

  /** clip i at output time t: its time in the file, and whether it is the one moving */
  at(i: number, t: number): { local: number; active: boolean } {
    const c = this.clips[i];
    if (!c) return { local: 0, active: false };
    let d = t - (this.split ? 0 : this.offsets[i]);
    const len = own(c);
    if (d >= len && d < this.length_ && this.loops(i)) d %= len;
    return { local: c.start + Math.min(Math.max(0, d), len), active: d >= 0 && d < len };
  }

  /** one after another: the clip shown at output time t */
  indexAt(t: number): number {
    const o = this.offsets;
    for (let i = o.length - 1; i >= 0; i--) if (t >= o[i]) return i;
    return 0;
  }

  seek(t: number) {
    this.time = Math.min(Math.max(0, t), this.length_);
    this.seekSeq++;
  }

  /** the space of each clip on the canvas: the engine's (plan.cells) once it has planned, worked out here until then */
  get boxes(): Rect[] {
    const [cw, ch] = this.canvas;
    if (!this.split) return this.clips.map(() => ({ x: 0, y: 0, w: cw, h: ch }));
    const n = this.clips.length;
    // the engine's cells while they are for this layout; while a choice or a drag
    // changes it, the boxes worked out here (the plan follows 300 ms later)
    const fromPlan = this.plan?.cells ?? [];
    if (fromPlan.length === n && this.planLayout === JSON.stringify(this.layout())) return fromPlan;
    if (this.mode === "grid") {
      const [cols, rows] = this.grid;
      return this.clips.map((_, i) => ({ x: ((i % cols) * cw) / cols, y: (Math.floor(i / cols) * ch) / rows, w: cw / cols, h: ch / rows }));
    }
    const w = this.weights;
    const sum = w.reduce((a, b) => a + b, 0) || 1;
    let at = 0;
    return w.map((x) => {
      const r = this.mode === "columns" ? { x: at, y: 0, w: (cw * x) / sum, h: ch } : { x: 0, y: at, w: cw, h: (ch * x) / sum };
      at += this.mode === "columns" ? r.w : r.h;
      return r;
    });
  }

  /** the shape of the space clip i fills */
  partRatio(i: number): number {
    const b = this.boxes[i];
    return b && b.h > 0 ? b.w / b.h : 16 / 9;
  }

  /** The space of clip i is worked out from the clips' shapes (and crops), so a crop changes it and there is no
   * fixed shape to lock to. Fixed: a final shape or a shape of each part is chosen (or, one after another, the
   * clip is not the first, whose shape the canvas takes). */
  partFollowsClip(i: number): boolean {
    if (!this.split) return this.shape === "auto" && i === 0;
    return this.shape === "auto" && this.partShape === "auto";
  }

  /** the ratio the crop of clip i keeps, or null for free */
  cropRatio(i: number): number | null {
    const c = this.clips[i];
    if (!c) return null;
    if (c.aspect === "part") return this.partFollowsClip(i) ? null : this.partRatio(i);
    return ratioOf(c.aspect);
  }

  setCrop(id: number, r: Rect | null) {
    const c = this.clips.find((x) => x.id === id);
    if (!c) return;
    if (!r) {
      c.crop = null;
      return;
    }
    const [W, H] = frameOf(c);
    const q = round(r, W, H);
    c.crop = isFull(q, W, H) ? null : q;
  }

  setCropAspect(id: number, a: AspectLock) {
    const i = this.clips.findIndex((x) => x.id === id);
    const c = this.clips[i];
    if (!c) return;
    c.aspect = a;
    const r = this.cropRatio(i);
    if (r) {
      const [W, H] = frameOf(c);
      this.setCrop(id, fitRatio(c.crop ?? { x: 0, y: 0, w: W, h: H }, r, W, H));
    }
  }

  /** the part of clip i the output shows: its crop, else (fill and cut) the middle that fills its space, else all of it */
  shown(i: number): Rect {
    const c = this.clips[i];
    const [W, H] = frameOf(c);
    if (c.crop) return c.crop;
    if (this.fill === "crop") return fitRatio({ x: 0, y: 0, w: W, h: H }, this.partRatio(i), W, H);
    return { x: 0, y: 0, w: W, h: H };
  }

  /** zoom of clip i: 1 = the largest rectangle of the crop's shape, 2 = half as wide */
  zoomOf(i: number): number {
    const c = this.clips[i];
    if (!c) return 1;
    const [W, H] = frameOf(c);
    const cur = this.shown(i);
    const most = fitRatio({ x: 0, y: 0, w: W, h: H }, cur.w / cur.h, W, H);
    return most.w / cur.w;
  }

  /** Zoom clip i to z, keeping the source point (px, py) (default: the middle of what shows) in place. */
  zoomTo(i: number, z: number, px?: number, py?: number) {
    const c = this.clips[i];
    if (!c) return;
    const [W, H] = frameOf(c);
    const cur = this.shown(i);
    const r = cur.w / cur.h;
    const most = fitRatio({ x: 0, y: 0, w: W, h: H }, r, W, H);
    const zz = Math.min(20, Math.max(1, z));
    const w = Math.max(16, most.w / zz);
    const h = w / r;
    const ax = px ?? cur.x + cur.w / 2;
    const ay = py ?? cur.y + cur.h / 2;
    // the point keeps its place in the rectangle
    const fx = (ax - cur.x) / cur.w;
    const fy = (ay - cur.y) / cur.h;
    const x = Math.min(Math.max(0, ax - fx * w), W - w);
    const y = Math.min(Math.max(0, ay - fy * h), H - h);
    this.setCrop(c.id, { x, y, w, h });
  }

  /** Move what clip i shows by (dx, dy) source pixels from `start`. */
  pan(i: number, start: Rect, dx: number, dy: number) {
    const c = this.clips[i];
    if (!c) return;
    const [W, H] = frameOf(c);
    this.setCrop(c.id, moveRect(start, dx, dy, W, H));
  }

  /** Make clip `id` as long as clip `other` (a video keeps its start when it can). */
  matchLength(id: number, other: number) {
    const c = this.clips.find((x) => x.id === id);
    const j = this.clips.findIndex((x) => x.id === other);
    if (!c || j < 0) return;
    const len = this.lenOf(j);
    if (len <= 0) return;
    if (c.still) {
      c.start = 0;
      c.end = Math.max(0.1, len);
      return;
    }
    const d = c.info.duration;
    c.end = Math.min(d, c.start + len);
    // not enough left after the start: start earlier
    if (c.end - c.start < len) c.start = Math.max(0, c.end - len);
  }

  /** a picture's length, one after another */
  setStillLength(id: number, secs: number) {
    const c = this.clips.find((x) => x.id === id);
    if (!c || !c.still || !Number.isFinite(secs)) return;
    c.start = 0;
    c.end = Math.min(3600, Math.max(0.1, secs));
  }

  layout(): Layout {
    const [width, height] = this.canvas;
    const n = this.clips.length;
    return {
      mode: this.mode,
      width,
      height,
      fill: this.fill,
      fps: this.fps,
      sound: this.sound === "mix" ? "mix" : this.sound === "none" ? "none" : "clip",
      soundClip: typeof this.sound === "number" ? Math.min(this.sound, Math.max(0, n - 1)) : 0,
      length: this.length,
      gap: GAPS[this.gap],
      gapColor: this.gapColor,
      weights: this.split && this.mode !== "grid" && n > 1 ? this.weights.map((x) => Math.round(x * 10000) / 10000) : [],
      columns: this.mode === "grid" ? this.grid[0] : 0,
      crops: this.clips.map((c) => (c.crop ? { x: Math.round(c.crop.x), y: Math.round(c.crop.y), w: Math.round(c.crop.w), h: Math.round(c.crop.h) } : null)),
    };
  }

  /** the job: the sidebar's settings for the first clip, the clips as segments, the layout */
  spec(): EncodeSpec | null {
    if (this.clips.length < 2) return null;
    const first = this.clips[0];
    const s = $state.snapshot(app.specForItem(first.info, { segments: [], crop: null, audioOff: false })) as EncodeSpec;
    // split screen: a picture lasts as long as the videos
    const len = this.length_;
    s.segments = this.clips.map((c) => (c.still ? { start: 0, end: Math.max(0.1, this.split ? len : own(c)) } : { start: c.start, end: c.end }));
    s.segmentSources = this.clips.map((c, i) => (i === 0 ? null : c.path));
    // Gyroflow steadies one file (engine gyro.rs): the clips of several files use the fallback
    if (s.stabMethod === "gyroflow") s.stabMethod = app.stabFallback;
    s.layout = this.layout();
    // the size choice decides the picture size (Auto: as large as the size limit allows),
    // within the largest size the preset allows (4chan: 2048x2048)
    s.maxWidth = app.maxWidth || null;
    s.maxHeight = app.maxHeight || null;
    s.autoScale = this.size === "auto" && app.rateMode === "size";
    s.crop = null;
    s.rotate = 0;
    s.mirror = false;
    s.vflip = false;
    s.subs = null;
    s.streamCopy = false;
    // sound when the sidebar has it on and any clip has some (the first may be silent)
    if (app.audioOn && this.clips.some((c) => c.info.audio.length)) {
      s.audio = { track: 0, bitrateK: app.audioBitrateK, volumeDb: app.volumeDb || 0, normalize: app.normalize };
    } else {
      s.audio = null;
    }
    s.outputName = null;
    return s;
  }

  async refreshPlan() {
    const seq = ++this.planSeq;
    const s = this.spec();
    if (!s) {
      this.plan = null;
      this.planError = null;
      return;
    }
    try {
      const p = await ipc.plan(s);
      if (seq !== this.planSeq) return;
      this.plan = p;
      this.planLayout = JSON.stringify(s.layout);
      this.planError = null;
    } catch (e) {
      if (seq !== this.planSeq) return;
      this.plan = null;
      this.planError = ipc.errText(e);
    }
  }

  async addPaths(paths: string[]) {
    for (const p of paths) {
      this.busy = `Reading ${p.split(/[\\/]/).pop()}...`;
      try {
        // probe also lets the player read the file
        const info = await ipc.probe(p);
        if (info.audioOnly) {
          app.toast({ kind: "failed", title: "Not added", text: `${info.fileName} has no picture, so it cannot be a clip of the splits.`, path: null });
          continue;
        }
        const still = !!info.still;
        this.clips.push({ id: this.nextId++, path: p, name: info.fileName, info, start: 0, end: still ? STILL_SECS : info.duration, still, crop: null, aspect: "part" });
      } catch (e) {
        app.toast({ kind: "failed", title: "Not added", text: ipc.errText(e), path: null });
      }
    }
    this.busy = null;
    this.countChanged();
    // sound from a clip that has some (a picture, or a silent first clip, would make the output silent)
    if (typeof this.sound === "number" && !this.clips[this.sound]?.info.audio.length) {
      const k = this.clips.findIndex((c) => c.info.audio.length > 0);
      if (k >= 0) this.sound = k;
    }
  }

  /** parts sized by hand are for that many clips */
  private countChanged() {
    if (this.parts === "custom" && this.custom.length !== this.clips.length) this.parts = "fit";
    if (this.time > this.length_) this.seek(this.length_);
  }

  remove(id: number) {
    const i = this.clips.findIndex((c) => c.id === id);
    if (i < 0) return;
    this.clips = this.clips.filter((c) => c.id !== id);
    if (typeof this.sound === "number") {
      if (this.sound === i) {
        const k = this.clips.findIndex((c) => c.info.audio.length > 0);
        this.sound = k >= 0 ? k : 0;
      } else if (this.sound > i) this.sound--;
    }
    this.countChanged();
  }

  move(id: number, by: number) {
    const i = this.clips.findIndex((c) => c.id === id);
    const j = i + by;
    if (i < 0 || j < 0 || j >= this.clips.length) return;
    const list = [...this.clips];
    [list[i], list[j]] = [list[j], list[i]];
    this.clips = list;
    // the sound and the hand-set sizes stay with their clips
    if (this.sound === i) this.sound = j;
    else if (this.sound === j) this.sound = i;
    if (this.parts === "custom" && this.custom.length === list.length) {
      const w = [...this.custom];
      [w[i], w[j]] = [w[j], w[i]];
      this.custom = w;
    }
  }

  setTrim(id: number, start: number, end: number) {
    const c = this.clips.find((x) => x.id === id);
    if (!c) return;
    if (c.still) return this.setStillLength(id, end - start);
    const d = c.info.duration;
    const s = Math.min(Math.max(0, start), d);
    c.start = s;
    c.end = Math.min(Math.max(s + 0.1, end), d);
  }

  clear() {
    this.clips = [];
    this.plan = null;
    this.playing = false;
    if (this.parts === "custom") this.parts = "fit";
    this.seek(0);
  }

  async enqueue(start: boolean) {
    const s = this.spec();
    if (!s) return;
    try {
      await ipc.plan(s);
      const j = await ipc.queueAdd(s, start);
      if (!app.jobs.some((x) => x.id === j.id)) app.jobs.push(j);
      app.toast({
        kind: "info",
        title: `${this.clips.length} clips in one job`,
        text: start ? "Encoding started." : "Press Start in the queue to encode it.",
        path: null,
      });
    } catch (e) {
      app.error = `Could not make the splits: ${ipc.errText(e)}`;
    }
  }
}

export const splits = new Splits();
