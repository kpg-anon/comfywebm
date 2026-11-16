// The Splits tab: two or more clips combined into one output, one after
// another or at the same time (side by side, stacked, a grid). Every choice is
// a plain word; the canvas size is worked out here from them, and the engine
// (EncodeSpec.layout) does the rest. The sidebar's settings apply to the
// result, as in Batch.
import { app } from "./state.svelte";
import * as ipc from "./ipc";
import type { EncodeSpec, Fill, Layout, LayoutMode, MediaInfo, PlanInfo } from "./types";

export interface SplitClip {
  id: number;
  path: string;
  name: string;
  info: MediaInfo;
  /** seconds in the file */
  start: number;
  end: number;
}

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
/** the clip's shape as shown (square pixels) */
const clipRatio = (m: MediaInfo) => (m.video.width && m.video.height ? m.video.width / m.video.height : 16 / 9);

class Splits {
  clips = $state<SplitClip[]>([]);
  busy = $state<string | null>(null);
  // the choices
  mode = $state<LayoutMode>("sequence");
  shape = $state<Shape>("auto");
  /** split screen: the shape of each part; picking one sets the final shape */
  partShape = $state<Shape>("auto");
  size = $state<SizeChoice>("auto");
  fill = $state<Fill>("bars");
  /** split screen */
  sound = $state<"mix" | "none" | number>(0);
  length = $state<"shortest" | "longest">("shortest");
  gap = $state<keyof typeof GAPS>("none");
  gapColor = $state("#000000");
  /** side by side / stacked: each part as wide (tall) as its clip's shape needs, equal parts, or the first clip twice as big */
  parts = $state<"fit" | "equal" | "first">("fit");
  plan = $state<PlanInfo | null>(null);
  planError = $state<string | null>(null);
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

  /** the final picture's width / height */
  /** relative part sizes for side by side / stacked */
  get weights(): number[] {
    const n = this.clips.length;
    if (this.mode === "grid" || this.mode === "sequence" || n < 2) return Array(n).fill(1);
    if (this.parts === "first") return [2, ...Array(n - 1).fill(1)];
    if (this.parts === "fit" && !(this.partShape !== "auto")) {
      // each clip whole in its part: widths by shape side by side, heights by inverse shape stacked
      const r = this.clips.map((c) => clipRatio(c.info));
      return this.mode === "columns" ? r : r.map((x) => 1 / x);
    }
    return Array(n).fill(1);
  }

  get aspect(): number {
    const n = this.clips.length;
    if (!n) return 16 / 9;
    const [cols, rows] = this.grid;
    const w = this.weights;
    const sum = w.reduce((a, b) => a + b, 0);
    if (this.split && this.partShape !== "auto") {
      const p = ratio(this.partShape);
      // parts of that shape side by side: as wide as their weights add up to (the
      // smaller parts have the shape; a part of weight 2 is twice as wide)
      if (this.mode === "columns") return p * sum;
      if (this.mode === "rows") return p / sum;
      return (p * cols) / rows;
    }
    if (this.shape !== "auto") return ratio(this.shape);
    const r = this.clips.map((c) => clipRatio(c.info));
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
    let [w, h] = a >= 1 ? [short * a, short] : [short, short / a];
    // keep the long side at most 3840
    const k = Math.min(1, 3840 / Math.max(w, h));
    return [even(w * k), even(h * k)];
  }

  /** frame rate: the highest of the clips (at most 60) */
  get fps(): number {
    const f = Math.max(...this.clips.map((c) => c.info.video.fps || 30), 1);
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

  get length_(): number {
    const lens = this.clips.map((c) => Math.max(0, c.end - c.start));
    if (!lens.length) return 0;
    if (!this.split) return lens.reduce((a, b) => a + b, 0);
    return this.length === "shortest" ? Math.min(...lens) : Math.max(...lens);
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
    };
  }

  /** the job: the sidebar's settings for the first clip, the clips as segments, the layout */
  spec(): EncodeSpec | null {
    if (this.clips.length < 2) return null;
    const first = this.clips[0];
    const s = $state.snapshot(app.specForItem(first.info, { segments: [], crop: null, audioOff: false })) as EncodeSpec;
    s.segments = this.clips.map((c) => ({ start: c.start, end: c.end }));
    s.segmentSources = this.clips.map((c, i) => (i === 0 ? null : c.path));
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
        this.clips.push({ id: this.nextId++, path: p, name: info.fileName, info, start: 0, end: info.duration });
      } catch (e) {
        app.toast({ kind: "failed", title: "Not added", text: ipc.errText(e), path: null });
      }
    }
    this.busy = null;
  }

  remove(id: number) {
    this.clips = this.clips.filter((c) => c.id !== id);
    if (typeof this.sound === "number" && this.sound >= this.clips.length) this.sound = 0;
  }

  move(id: number, by: number) {
    const i = this.clips.findIndex((c) => c.id === id);
    const j = i + by;
    if (i < 0 || j < 0 || j >= this.clips.length) return;
    const list = [...this.clips];
    [list[i], list[j]] = [list[j], list[i]];
    this.clips = list;
  }

  setTrim(id: number, start: number, end: number) {
    const c = this.clips.find((x) => x.id === id);
    if (!c) return;
    const d = c.info.duration;
    const s = Math.min(Math.max(0, start), d);
    c.start = s;
    c.end = Math.min(Math.max(s + 0.1, end), d);
  }

  clear() {
    this.clips = [];
    this.plan = null;
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
