// The Batch view: many files encoded with the global settings (the sidebar),
// each with optional trim, crop, frame rate cap and audio switch. Every file
// is planned against the current settings and checked for problems, with
// suggested changes that can be applied with one click.
import * as ipc from "./ipc";
import { app, type ItemOverrides } from "./state.svelte";
import { BPP_FLOOR } from "./resolution";
import { fmtTime } from "./time";
import type { Codec, EncodeSpec, MediaInfo, PlanInfo, Segment } from "./types";

export type BatchItem = ItemOverrides & {
  id: number;
  path: string;
  name: string;
  info: MediaInfo | null;
  error: string | null;
  include: boolean;
  plan: PlanInfo | null;
  planError: string | null;
};

/** A change an issue suggests; applying it edits the item or the global settings. */
export type Fix = { label: string; apply: () => void };

export type Issue = {
  level: "error" | "warn" | "info";
  text: string;
  fixes: Fix[];
};

const kbpsLine = (p: PlanInfo) => (p.videoKbps !== null ? `${Math.round(p.videoKbps)} kbps` : "quality mode");

class Batch {
  items = $state<BatchItem[]>([]);
  busy = $state<string | null>(null);
  recursive = $state(false);
  private nextId = 1;
  private planSeq = 0;

  included = $derived(this.items.filter((i) => i.include && i.info));

  /** Specs for every included, readable item, in list order. */
  specs = $derived.by(() => this.included.map((i) => ({ id: i.id, spec: app.specForItem(i.info as MediaInfo, i) })));

  /** Add files; a path that is a folder adds the videos in it. */
  async addPaths(paths: string[]) {
    const have = new Set(this.items.map((i) => i.path.toLowerCase()));
    const queue = [...paths];
    let n = 0;
    while (queue.length) {
      const p = queue.shift() as string;
      if (have.has(p.toLowerCase())) continue;
      n++;
      this.busy = `Reading ${n}: ${p.split(/[\\/]/).pop()}`;
      try {
        const info = await ipc.probe(p);
        have.add(p.toLowerCase());
        this.items.push(this.item(p, info, null));
      } catch (e) {
        // a folder: add what is in it
        try {
          const inside = await ipc.listMedia(p, this.recursive);
          queue.unshift(...inside);
          n--;
          continue;
        } catch {
          // not a folder either
        }
        have.add(p.toLowerCase());
        this.items.push(this.item(p, null, ipc.errText(e)));
      }
    }
    this.busy = null;
  }

  async addFolder(dir: string) {
    this.busy = "Listing the folder...";
    try {
      const files = await ipc.listMedia(dir, this.recursive);
      if (!files.length) app.toast({ kind: "info", title: "No videos found", text: dir, path: null });
      await this.addPaths(files);
    } catch (e) {
      app.error = ipc.errText(e);
    } finally {
      this.busy = null;
    }
  }

  private item(path: string, info: MediaInfo | null, error: string | null): BatchItem {
    return {
      id: this.nextId++,
      path,
      name: path.split(/[\\/]/).pop() ?? path,
      info,
      error,
      include: !!info,
      segments: [],
      crop: null,
      fps: undefined,
      audioOff: false,
      plan: null,
      planError: null,
    };
  }

  find(id: number) {
    return this.items.find((i) => i.id === id);
  }

  update(id: number, patch: Partial<BatchItem>) {
    const i = this.items.findIndex((x) => x.id === id);
    if (i >= 0) this.items[i] = { ...this.items[i], ...patch };
  }

  /** Include or leave out every readable item. */
  setAllIncluded(on: boolean) {
    this.items = this.items.map((i) => (i.info ? { ...i, include: on } : i));
  }

  remove(id: number) {
    this.items = this.items.filter((i) => i.id !== id);
  }

  clear() {
    this.items = [];
  }

  /** Plan every included item with the current settings (newest call wins). */
  async planAll() {
    const seq = ++this.planSeq;
    const list = this.specs;
    for (const { id, spec } of list) {
      let plan: PlanInfo | null = null;
      let planError: string | null = null;
      try {
        plan = await ipc.plan($state.snapshot(spec) as EncodeSpec);
      } catch (e) {
        planError = ipc.errText(e);
      }
      if (seq !== this.planSeq) return;
      this.update(id, { plan, planError });
    }
  }

  async enqueue(start: boolean) {
    const specs = this.specs.map((s) => $state.snapshot(s.spec) as EncodeSpec);
    if (!specs.length) return;
    try {
      const jobs = await ipc.queueAddMany(specs, start);
      for (const j of jobs) if (!app.jobs.some((x) => x.id === j.id)) app.jobs.push(j);
      app.toast({
        kind: "info",
        title: `${jobs.length} file${jobs.length === 1 ? "" : "s"} added to the queue`,
        text: start ? "Encoding started." : "Press Start in the queue to encode them.",
        path: null,
      });
    } catch (e) {
      app.error = ipc.errText(e);
    }
  }

  /**
   * One job that joins the included files in list order: each file's trim
   * (or the whole file), fitted to the first file's picture size and frame
   * rate. The first file's crop and the global settings apply.
   */
  async join(start: boolean) {
    const items = this.included;
    if (items.length < 2) return;
    const first = items[0];
    const spec = $state.snapshot(app.specForItem(first.info as MediaInfo, first)) as EncodeSpec;
    const segments: Segment[] = [];
    const sources: (string | null)[] = [];
    for (const it of items) {
      const own = it.segments.length ? it.segments : [{ start: 0, end: (it.info as MediaInfo).duration }];
      for (const g of own) {
        segments.push({ start: g.start, end: g.end });
        sources.push(it === first ? null : it.path);
      }
    }
    spec.segments = segments;
    spec.segmentSources = sources;
    spec.outputName = null;
    try {
      // check first, so a problem shows here and not when the job starts
      await ipc.plan(spec);
      const j = await ipc.queueAdd(spec, start);
      if (!app.jobs.some((x) => x.id === j.id)) app.jobs.push(j);
      app.toast({
        kind: "info",
        title: `${items.length} files joined into one job`,
        text: `In list order, at ${first.name}'s picture size and frame rate.${start ? " Encoding started." : ""}`,
        path: null,
      });
    } catch (e) {
      app.error = `Could not join: ${ipc.errText(e)}`;
    }
  }

  // ---------- trim and crop in the editor ----------

  async editInEditor(id: number) {
    const it = this.find(id);
    if (!it?.info) return;
    if (!(await app.open(it.path))) return;
    // ranges need ids for the editor's output order
    app.segments = it.segments.map((s, k) => ({ start: s.start, end: s.end, id: 100000 + k }));
    app.sel = app.segments.length ? 0 : -1;
    app.crop = it.crop ? { ...it.crop } : null;
    app.batchEdit = { id, name: it.name };
    app.setView("editor");
  }

  saveFromEditor() {
    const be = app.batchEdit;
    if (!be) return;
    const segments: Segment[] = app.outSegments.map((s) => ({ start: s.start, end: s.end }));
    this.update(be.id, { segments, crop: app.crop ? { ...app.crop } : null });
    app.batchEdit = null;
    app.setView("batch");
  }

  cancelEdit() {
    app.batchEdit = null;
    app.setView("batch");
  }

  // ---------- checks ----------

  /** The orientation and source size most files share. */
  majority = $derived.by(() => {
    const ok = this.included.map((i) => i.info as MediaInfo);
    const count = <T>(f: (m: MediaInfo) => T) => {
      const c = new Map<T, number>();
      for (const m of ok) c.set(f(m), (c.get(f(m)) ?? 0) + 1);
      let best: T | null = null;
      let n = 0;
      for (const [k, v] of c) if (v > n) [best, n] = [k, v];
      return { value: best, share: ok.length ? n / ok.length : 0 };
    };
    return {
      orient: count((m) => (m.video.width > m.video.height ? "landscape" : m.video.width < m.video.height ? "portrait" : "square")),
      size: count((m) => `${m.video.width}x${m.video.height}`),
    };
  });

  issues(it: BatchItem): Issue[] {
    const out: Issue[] = [];
    if (it.error) return [{ level: "error", text: `Cannot be read: ${it.error}`, fixes: [] }];
    const m = it.info;
    if (!m) return out;
    if (it.planError) out.push({ level: "error", text: it.planError, fixes: [] });
    const p = it.plan;
    const len = it.segments.length ? it.segments.reduce((n, s) => n + s.end - s.start, 0) : m.duration;
    const sizeMode = app.rateMode === "size";
    const srcFps = m.video.fps || 30;
    const cap = it.fps !== undefined ? it.fps : app.fps;
    const outFps = Math.min(cap ?? Infinity, srcFps * (app.speed || 1));
    const floor = BPP_FLOOR[app.codec as Codec];

    if (p && sizeMode && p.videoKbps !== null && (p.quality === "poor" || p.quality === "fair")) {
      const fixes: Fix[] = [];
      const poor = p.quality === "poor";
      // fewer frames: more bits for each
      if (outFps > 30.5) {
        const gain = outFps / 30;
        fixes.push({ label: `Cap at 30 fps (${gain.toFixed(1)}x the bits per frame)`, apply: () => this.update(it.id, { fps: 30 }) });
      }
      // a shorter clip: the longest length that reaches a clean picture at this output size
      const audioK = p.audioKbps;
      const needK = (floor * 1.5 * p.outWidth * p.outHeight * Math.min(outFps, srcFps * (app.speed || 1))) / 1000;
      const maxLen = ((app.maxBytes * 8) / 1000 / (needK / 0.985 + audioK)) * (app.speed || 1);
      if (maxLen >= 2 && maxLen < len - 1) {
        const start = it.segments[0]?.start ?? 0;
        const n = Math.floor(maxLen);
        fixes.push({ label: `Trim to ${fmtTime(n).replace(/\.000$/, "")}`, apply: () => this.update(it.id, { segments: [{ start, end: Math.min(m.duration, start + n) }] }) });
      }
      if (audioK > 0 && !it.audioOff) {
        const total = (app.maxBytes * 8) / 1000 / Math.max(0.1, p.durationOut);
        if (audioK / total > 0.12) fixes.push({ label: `Leave out the sound (+${Math.round(audioK)} kbps for video)`, apply: () => this.update(it.id, { audioOff: true }) });
      }
      out.push({
        level: poor ? "warn" : "info",
        text: poor
          ? `${fmtTime(p.durationOut)} is too long for this size limit: ${kbpsLine(p)} at ${p.outWidth}x${p.outHeight} gives a blurry, blocky picture.`
          : `Acceptable but not clean: ${kbpsLine(p)} at ${p.outWidth}x${p.outHeight}.`,
        fixes,
      });
    }
    if (p?.autoScaled) out.push({ level: "info", text: `Scaled down to ${p.outWidth}x${p.outHeight} to fit the bitrate.`, fixes: [] });

    const maxDur = app.preset?.maxDuration;
    if (maxDur && p && p.durationOut > maxDur + 0.01) {
      const start = it.segments[0]?.start ?? 0;
      out.push({
        level: "warn",
        text: `${fmtTime(p.durationOut)} long; "${app.preset?.name}" allows at most ${fmtTime(maxDur)}.`,
        fixes: [{ label: `Trim to ${fmtTime(maxDur).replace(/\.000$/, "")}`, apply: () => this.update(it.id, { segments: [{ start, end: Math.min(m.duration, start + maxDur * (app.speed || 1)) }] }) }],
      });
    }

    const maj = this.majority;
    const orient = m.video.width > m.video.height ? "landscape" : m.video.width < m.video.height ? "portrait" : "square";
    if (this.included.length > 1 && maj.orient.value && orient !== maj.orient.value && maj.orient.share > 0.5) {
      out.push({
        level: "info",
        text: `${orient[0].toUpperCase() + orient.slice(1)}, unlike most files here (${maj.orient.value}); the output is ${p ? `${p.outWidth}x${p.outHeight}` : "the same shape"}.`,
        fixes: [{ label: "Crop in the editor", apply: () => void this.editInEditor(it.id) }],
      });
    } else if (this.included.length > 1 && maj.size.value && maj.size.share > 0.5) {
      const [mw, mh] = String(maj.size.value).split("x").map(Number);
      const ratio = (m.video.width * m.video.height) / (mw * mh);
      if (ratio < 0.6 || ratio > 1.7) out.push({ level: "info", text: `${m.video.width}x${m.video.height}, while most files are ${maj.size.value}.`, fixes: [] });
    }

    if (m.video.vfr) out.push({ level: "info", text: "Variable frame rate (typical of phone recordings); frame steps are approximate.", fixes: [] });
    if (m.video.interlaced) out.push({ level: "info", text: "Interlaced; it is deinterlaced.", fixes: [] });
    if (m.video.hdr)
      out.push({
        level: app.tools?.filters.zscale === false ? "warn" : "info",
        text: app.tools?.filters.zscale === false ? "HDR, and this ffmpeg cannot tone map it (no zscale); colors will look washed out." : "HDR; it is tone mapped to normal colors.",
        fixes: [],
      });
    if (app.audioOn && !it.audioOff && !m.audio.length) out.push({ level: "info", text: "No audio track; the output is silent.", fixes: [] });
    return out;
  }

  /** Changes to the global settings that help several files at once. */
  suggestions = $derived.by((): (Fix & { text: string })[] => {
    const out: (Fix & { text: string })[] = [];
    const poor = this.included.filter((i) => i.plan?.quality === "poor");
    if (app.rateMode === "size" && poor.length >= 2) {
      if ((app.codec === "vp9" || app.codec === "h264" || app.codec === "vp8") && app.codecAvailable("av1")) {
        out.push({
          text: `${poor.length} files will look poor at this size. AV1 gives a better picture at the same size (it encodes slower).`,
          label: "Use AV1",
          apply: () => app.setCodec("av1"),
        });
      }
      const high = poor.filter((i) => (i.info?.video.fps ?? 0) > 30.5);
      if (high.length >= 2 && (!app.fps || app.fps > 30)) {
        out.push({ text: `${high.length} of them are above 30 fps.`, label: "Cap all at 30 fps", apply: () => (app.fps = 30) });
      }
    }
    return out;
  });
}

export const batch = new Batch();
