<script lang="ts">
  import { untrack } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import * as ipc from "$lib/ipc";
  import { app, isNeutral } from "$lib/state.svelte";
  import { sample } from "$lib/curve";
  import CropOverlay from "./CropOverlay.svelte";
  import { ratioOf } from "$lib/crop";
  import * as ramp from "$lib/ramp";
  import * as zoomMath from "$lib/zoom";
  import * as rot from "$lib/rotation";
  import type { Crop, SpeedPoint, ZoomPoint } from "$lib/types";
  import { wbGains } from "$lib/color";
  import Icon from "./Icon.svelte";

  let video = $state<HTMLVideoElement>();
  let sw = $state(0);
  let sh = $state(0);
  // the plan command adds the logo file to the asset scope; a logo set while an older plan is
  // shown can load before that and fail, so a failed load is tried again (a few times)
  let logoTry = $state(0);
  $effect(() => {
    void app.logoPath;
    logoTry = 0;
  });
  // height / width of the watermark image, for placing and dragging it
  let logoAspect = $state(1);

  type Rect = { x: number; y: number; w: number; h: number };
  /** The watermark's top-left inside `r` (the output picture on screen). */
  function logoPos(r: Rect, lw: number, lh: number) {
    const m = Math.min(r.w, r.h) * 0.03;
    const c = app.logoCorner;
    if (c === "custom") return { x: (r.w - lw) * app.logoX, y: (r.h - lh) * app.logoY };
    return { x: c.endsWith("l") ? m : r.w - lw - m, y: c.startsWith("t") ? m : r.h - lh - m };
  }

  /**
   * Drags a box inside an area: `start` is its top-left in the area, `room` the
   * space left beside and below it; `apply` gets the position as 0..1 of that room.
   */
  function dragIn(e: PointerEvent, start: { x: number; y: number }, room: { w: number; h: number }, apply: (fx: number, fy: number) => void) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const frac = (v: number, r: number) => (r > 0 ? Math.min(1, Math.max(0, v / r)) : 0.5);
    const set = (x: number, y: number) => apply(frac(x, room.w), frac(y, room.h));
    set(start.x, start.y);
    const [px, py] = [e.clientX, e.clientY];
    const move = (ev: PointerEvent) => set(start.x + ev.clientX - px, start.y + ev.clientY - py);
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  // Dragging the watermark places it anywhere (switches to Custom). bw, bh: its
  // bounding box on screen (larger than the image when it is turned).
  function dragLogo(e: PointerEvent, r: Rect, bw: number, bh: number) {
    dragIn(e, logoPos(r, bw, bh), { w: r.w - bw, h: r.h - bh }, (fx, fy) => {
      app.logoX = fx;
      app.logoY = fy;
    });
    app.logoCorner = "custom";
  }

  // Dragging the caption places it anywhere (switches to Custom).
  function dragCaption(e: PointerEvent) {
    const s = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const d = (e.currentTarget as HTMLElement).parentElement!.getBoundingClientRect();
    dragIn(e, { x: s.left - d.left, y: s.top - d.top }, { w: d.width - s.width, h: d.height - s.height }, (fx, fy) => {
      app.captionX = fx;
      app.captionY = fy;
    });
    app.captionCustom = true;
  }
  let capW = $state(0);
  let capH = $state(0);

  /** Bounding box of a w x h box turned by `deg`. */
  function turnedBox(w: number, h: number, deg: number) {
    const a = (deg * Math.PI) / 180;
    const [c, s] = [Math.abs(Math.cos(a)), Math.abs(Math.sin(a))];
    return { w: w * c + h * s, h: w * s + h * c };
  }

  /** The tile grid in output pixels, as the engine lays it out (plan.rs overlay_chain). */
  function tileGrid(outW: number, outH: number) {
    const w = Math.max(2, Math.round((outW * Math.min(100, Math.max(2, app.logoSize))) / 100 / 2) * 2);
    const cell = Math.max(w, 2, Math.round(w * (1 + Math.min(3, Math.max(0, app.logoSpacing)))));
    // a turned grid covers the picture's diagonal
    const d = Math.ceil(Math.hypot(outW, outH));
    const [cw, ch] = app.logoAngle ? [d, d] : [outW, outH];
    const cols = Math.ceil(cw / cell) + 1;
    const rows = Math.ceil(ch / cell) + 1;
    return { w, cell, cols, rows, x: Math.trunc((outW - cols * cell) / 2), y: Math.trunc((outH - rows * cell) / 2) };
  }

  // Caption font for the preview: an installed font by its family name; a font
  // file that is not installed is loaded through the asset protocol once the
  // plan command has granted it (tried again a few times, as the logo is).
  const captionFace = $derived(app.captionFont ? (app.fonts.find((f) => f.path === app.captionFont) ?? null) : null);
  let fileFontReady = $state(false);
  $effect(() => {
    const path = app.captionFont;
    fileFontReady = false;
    if (!path || captionFace || !app.fonts.length) return;
    let stop = false;
    let face: FontFace | null = null;
    (async () => {
      for (let n = 0; n < 6 && !stop; n++) {
        try {
          face = new FontFace("cwm-caption-file", `url("${convertFileSrc(path)}")`);
          await face.load();
          if (stop) return;
          document.fonts.add(face);
          fileFontReady = true;
          return;
        } catch {
          await new Promise((r) => setTimeout(r, 400));
        }
      }
    })();
    return () => {
      stop = true;
      if (face) document.fonts.delete(face);
    };
  });
  const captionCss = $derived(
    captionFace
      ? { family: `"${captionFace.family.replace(/"/g, "")}", Arial, sans-serif`, weight: String(captionFace.weight), style: captionFace.italic ? "italic" : "normal" }
      : app.captionFont && fileFontReady
        ? { family: `"cwm-caption-file", Arial, sans-serif`, weight: "normal", style: "normal" }
        : null,
  );

  $effect(() => {
    app.video = video ?? null;
    return () => {
      if (app.video === video) app.video = null;
    };
  });

  // The picture box inside the stage. object-fit would letterbox; the box is
  // computed here instead so the crop overlay and the rotate/mirror preview
  // share it. The source display size gives the aspect, so the proxy (smaller
  // pixel size, same aspect) maps to the same box. The picture is shown turned
  // and flipped as the output will be. Outside crop mode the stage shows what
  // the output shows (the crop with the zoom, or the follow window at the
  // playhead), as large as it fits; the rest of the picture is off the stage.
  // with rotation keyframes: the frame's quarter turn (the picture's shape at the first output frame)
  const turned = $derived(app.viewRotate === 90 || app.viewRotate === 270);
  /** the picture's size as shown, in its own pixels */
  const vsize = $derived.by(() => {
    const m = app.media;
    // a sound file: the shape of its picture
    const still = app.stillSize;
    const aw = (m?.audioOnly ? still?.[0] : m?.video.width) || 16;
    const ah = (m?.audioOnly ? still?.[1] : m?.video.height) || 9;
    return turned ? { w: ah, h: aw } : { w: aw, h: ah };
  });
  // While playing with follow keyframes, the window is placed for the frame on
  // screen: requestVideoFrameCallback gives that frame's time. currentTime,
  // read on each animation frame, runs ahead of or behind the shown frame, and
  // the window placed for another frame made the picture shake.
  let frameT = $state<number | null>(null);
  $effect(() => {
    const v = video as (HTMLVideoElement & { requestVideoFrameCallback?: (cb: (now: number, m: { mediaTime: number }) => void) => number; cancelVideoFrameCallback?: (id: number) => void }) | undefined;
    if (!v || app.paused || !(app.zoomPoints.length || app.rotatePoints.length) || !v.requestVideoFrameCallback) {
      frameT = null;
      return;
    }
    let id = 0;
    const each = (_: number, m: { mediaTime: number }) => {
      frameT = m.mediaTime;
      id = v.requestVideoFrameCallback!(each);
    };
    id = v.requestVideoFrameCallback(each);
    return () => {
      v.cancelVideoFrameCallback?.(id);
      frameT = null;
    };
  });
  /** the part of the picture (as shown) the stage is fitted to */
  /** px around the picture in crop mode, for the crop's handles */
  const CROP_MARGIN = 16;

  // Crop mode with a follow path: the stage shows the largest view the path can have at the
  // playhead (the whole picture over Follow's zoom, no crop), held still while cropping, and
  // the crop box is the view as it is now inside it. The box can shrink, grow back to the
  // whole area and move; letting go applies it (applyFollowCrop). The area is taken again
  // only when the playhead moves.
  let fctx = $state<(Crop & { rot: number }) | null>(null);
  /** the box while it is being dragged */
  let fdraft = $state<Crop | null>(null);
  // a derived flag, so a change to the path (applying the crop moves it) does not take the area again
  const hasPath = $derived(app.zoomPoints.length > 0);
  $effect(() => {
    const on = app.cropMode && hasPath && !!app.viewW && !app.audioOnly;
    const t = app.current;
    if (!on) {
      fctx = null;
      fdraft = null;
      return;
    }
    fctx = untrack(() => zoomMath.viewAt(app.zoomPoints, t, app.viewW, app.viewH, app.viewW, app.viewH));
    fdraft = null;
  });
  /** the view as it is now, in the held area's own (upright) pixels */
  function viewInCtx(c: Crop & { rot: number }): Crop {
    const v = app.outViewAt(app.current);
    const [dx, dy] = [v.x + v.w / 2 - (c.x + c.w / 2), v.y + v.h / 2 - (c.y + c.h / 2)];
    // the area is shown turned by its rot, so the offset turns with it
    const a = ((c.rot || 0) * Math.PI) / 180;
    const [lx, ly] = [dx * Math.cos(a) - dy * Math.sin(a), dx * Math.sin(a) + dy * Math.cos(a)];
    return { x: c.w / 2 + lx - v.w / 2, y: c.h / 2 + ly - v.h / 2, w: v.w, h: v.h };
  }
  const fbox = $derived(fctx ? (fdraft ?? viewInCtx(fctx)) : null);
  // a view that no longer fits in the held area (moved toward its edge, or 1:1 around a moved
  // middle): the area is taken again, around the path where it is now
  $effect(() => {
    void app.zoomPoints;
    void app.crop;
    const c = untrack(() => fctx);
    if (!c || untrack(() => fdraft)) return;
    const b = untrack(() => viewInCtx(c));
    if (b.x < -0.5 || b.y < -0.5 || b.x + b.w > c.w + 0.5 || b.y + b.h > c.h + 0.5)
      fctx = untrack(() => zoomMath.viewAt(app.zoomPoints, app.current, app.viewW, app.viewH, app.viewW, app.viewH));
  });
  function commitFollowCrop(c: Crop | null) {
    const ctx = fctx;
    fdraft = null;
    if (!c || !ctx) return;
    const v = viewInCtx(ctx);
    // applyFollowCrop takes the box relative to the view as it is now
    app.applyFollowCrop({ ...v, rot: ctx.rot }, { x: c.x - v.x, y: c.y - v.y, w: c.w, h: c.h });
  }
  /** what the output shows now, or null while the whole picture is shown (crop mode without a path, drawing a box, clicking points) */
  const view = $derived(
    fctx ?? (app.cropMode || app.followMode || app.pointsMode || app.audioOnly || !app.viewW ? null : app.outViewAt(frameT ?? app.current)),
  );
  const focus = $derived.by(() => {
    const whole = { x: 0, y: 0, w: vsize.w, h: vsize.h };
    if (!view) return whole;
    // outViewAt is in viewW pixels, the same as vsize for a video
    const k = vsize.w / app.viewW;
    return { x: view.x * k, y: view.y * k, w: Math.max(1, view.w * k), h: Math.max(1, view.h * k) };
  });
  /** a canvas's shape (width over height) while the output is shown; null = the shown part's own */
  const frameR = $derived(view && !fctx ? app.frameRatio : null);
  /** the output frame's shape on screen */
  const rF = $derived(frameR ?? focus.w / Math.max(1e-6, focus.h));
  /** the output frame on screen, as large as fits the stage */
  const frameBox = $derived.by(() => {
    if (sw <= 0 || sh <= 0) return { x: 0, y: 0, w: 0, h: 0 };
    // crop mode: a margin, so a crop the size of the picture keeps its corner and edge handles on
    // the stage (on its edge they sat under the timeline's divider and the bar above, and missed)
    const m = app.cropMode ? Math.min(CROP_MARGIN, sw / 6, sh / 6) : 0;
    const w = Math.min(sw - 2 * m, (sh - 2 * m) * rF);
    const h = w / rF;
    return { x: (sw - w) / 2, y: (sh - h) / 2, w, h };
  });
  /** the picture's size on screen per pixel of it: the shown part fitted into the frame (or covering it, Fill) */
  const kPic = $derived.by(() => {
    const [a, b] = [frameBox.w / focus.w, frameBox.h / focus.h];
    return frameR && app.frameFill ? Math.max(a, b) : Math.min(a, b);
  });
  /** Rotation keyframes, outside crop mode: how far the frame on screen is turned from the
   *  frame's quarter turn, and how much larger or smaller it then shows (Fit or Fill), as the
   *  encode does (rotation.rs). The flips come after the turn in the encode; turned after
   *  them here, the turn goes the other way when one flip is on. */
  const keyTurn = $derived.by(() => {
    if (!view || !app.rotating) return null;
    const q = app.viewRotate;
    const a = app.rotateAt(frameT ?? app.current);
    // the picture before turning, in the units of the view (q turned back)
    const [pw, ph] = q % 180 ? [focus.h, focus.w] : [focus.w, focus.h];
    const s = rot.windowW(pw, ph, rF, q, app.frameFill) / Math.max(1e-6, rot.windowW(pw, ph, rF, a, app.frameFill));
    const d = a - q;
    return { deg: app.mirror !== app.vflip ? -d : d, scale: s };
  });
  /** the frame has room around the picture (a canvas, a turned part): black, or a blurred copy */
  const framed = $derived(!!view && !fctx && (!!keyTurn || !!frameR));
  const blurBg = $derived(framed && !app.frameFill && app.frameBg === "blur");
  let blurCanvas = $state<HTMLCanvasElement>();
  // Blur: the picture drawn small into a canvas behind it, covering the frame, turned as the
  // output is; CSS blurs it. Drawn on each animation frame while it shows.
  $effect(() => {
    const cv = blurCanvas;
    const v = video;
    if (!blurBg || !cv || !v) return;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    let raf = 0;
    const draw = () => {
      raf = requestAnimationFrame(draw);
      const cw = 96;
      const ch = Math.max(2, Math.round(cw / rF));
      if (cv.width !== cw || cv.height !== ch) [cv.width, cv.height] = [cw, ch];
      const c = app.jobCrop ?? { x: 0, y: 0, w: app.srcW, h: app.srcH };
      if (!v.videoWidth || !c.w || !c.h) return;
      const a = app.rotating ? app.rotateAt(frameT ?? app.current) : app.viewRotate;
      // the picture covering the frame at this angle, in canvas pixels per source pixel
      const s = cw / Math.max(1e-6, rot.windowW(c.w, c.h, rF, a, true));
      const vk = v.videoWidth / Math.max(1, app.srcW);
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, cw, ch);
      ctx.translate(cw / 2, ch / 2);
      // flips after the turn, as in the encode
      ctx.scale(app.mirror ? -1 : 1, app.vflip ? -1 : 1);
      ctx.rotate((a * Math.PI) / 180);
      ctx.scale(s, s);
      try {
        ctx.drawImage(v, c.x * vk, c.y * vk, c.w * vk, c.h * vk, -c.w / 2, -c.h / 2, c.w, c.h);
      } catch {
        // not decoded yet
      }
    };
    draw();
    return () => cancelAnimationFrame(raf);
  });
  /** Follow's turn: the picture turns clockwise around the view's middle, which is the stage's middle */
  const spin = $derived(view?.rot ?? 0);
  const spinCss = $derived(
    keyTurn ? `rotate(${keyTurn.deg}deg) scale(${keyTurn.scale})` : spin ? `rotate(${spin}deg)` : null,
  );
  /** the picture's box as shown on screen (may reach past the stage), the shown part in the frame's middle */
  const shown = $derived.by(() => {
    if (sw <= 0 || sh <= 0) return { x: 0, y: 0, w: 0, h: 0 };
    const k = kPic;
    return { x: sw / 2 - (focus.x + focus.w / 2) * k, y: sh / 2 - (focus.y + focus.h / 2) * k, w: vsize.w * k, h: vsize.h * k };
  });
  /** the element's box: it keeps the unrotated shape; the transform turns it */
  const box = $derived(turned ? { x: shown.x + (shown.w - shown.h) / 2, y: shown.y + (shown.h - shown.w) / 2, w: shown.h, h: shown.w } : shown);
  /** the output's area on screen: the frame (with a canvas larger or smaller than the shown part) */
  const outRect = $derived(frameBox);
  // the flips apply after the rotation, as in the encode (rotate, then hflip, then vflip)
  const transform = $derived(`${app.vflip ? "scaleY(-1) " : ""}${app.mirror ? "scaleX(-1) " : ""}rotate(${app.viewRotate}deg)`);

  // Color adjustments on the picture: an SVG filter close to ffmpeg's eq,
  // hue and curves (not exact: eq works on YUV, this on RGB). Sample shows
  // the real result.
  // the preview's mute and volume (the speaker in the corner, M)
  $effect(() => {
    if (!video) return;
    video.muted = app.muted;
    video.volume = app.volume;
  });
  const hasSound = $derived(!!app.media?.audio.length);

  // With follow keyframes, dragging the picture moves the view (the picture moves with the
  // pointer, the view the other way): the keyframes selected on the Follow lane (a click on
  // the lane adds one and selects it), or the whole path when none are selected, so a
  // tracked face can be framed higher or lower without bending the path. Shift + drag
  // always moves the whole path. A click without a drag plays and pauses as before.
  let pan: { x0: number; y0: number; i: number; cx: number; cy: number; moved: boolean; all: ZoomPoint[] | null; only: number[] | null } | null = null;
  let panned = false;
  function panDown(e: PointerEvent) {
    if (e.button !== 0 || !app.zoomPoints.length || app.cropMode || app.followMode || app.pointsMode) return;
    const i = app.followKeyAt(app.current);
    const p = i >= 0 ? app.zoomPoints[i] : zoomMath.at(app.zoomPoints, app.current);
    const sel = app.followSelected;
    pan = { x0: e.clientX, y0: e.clientY, i, cx: p.x, cy: p.y, moved: false, all: app.zoomPoints.map((q) => ({ ...q })), only: e.shiftKey || !sel.length ? null : sel };
  }
  function panMove(e: PointerEvent) {
    if (!pan) return;
    const dx = e.clientX - pan.x0;
    const dy = e.clientY - pan.y0;
    if (!pan.moved) {
      if (Math.abs(dx) + Math.abs(dy) < 4) return;
      pan.moved = true;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      app.video?.pause();
    }
    // the drag on the screen, turned back onto the picture (it is shown turned by spin)
    const a = (-spin * Math.PI) / 180;
    const [px, py] = [dx * Math.cos(a) - dy * Math.sin(a), dx * Math.sin(a) + dy * Math.cos(a)];
    // keyframes are 0..1 of the whole picture; shown.w is its width on the screen
    if (pan.all) {
      const [mx, my] = [px / shown.w, py / shown.h];
      const cl = (v: number) => Math.min(1, Math.max(0, v));
      const only = pan.only;
      const moves = (t: number) => !only || only.some((s) => Math.abs(s - t) < 1e-6);
      app.zoomPoints = pan.all.map((q) => (moves(q.t) ? { ...q, x: cl(q.x - mx), y: cl(q.y - my) } : q));
    } else app.setZoomPoint(pan.i, { x: pan.cx - px / shown.w, y: pan.cy - py / shown.h });
  }
  function panUp() {
    if (pan?.moved) panned = true;
    pan = null;
  }
  function soundWheel(e: WheelEvent) {
    // the wheel over the speaker sets the volume; elsewhere it steps frames
    e.preventDefault();
    e.stopPropagation();
    const d = e.deltaMode === 1 ? e.deltaY * 40 : e.deltaY;
    app.setVolume((app.muted ? 0 : app.volume) - Math.sign(d) * 0.05);
  }

  // the speed setting plays in the preview too (pitch kept, as atempo does)
  // (defaultPlaybackRate too: loading a new source resets the rate to it)
  $effect(() => {
    if (app.speedRamp.length) return;
    const r = Math.min(16, Math.max(0.0625, Number(app.speed) || 1));
    if (video) {
      video.defaultPlaybackRate = r;
      video.playbackRate = r;
    }
  });
  // A speed ramp: the rate follows the ramp at the playhead, each frame. The
  // loop reads the video's own time, not app.current, so it does not re-run
  // the effect on every frame.
  $effect(() => {
    const v = video;
    const pts = ramp.clean($state.snapshot(app.speedRamp) as SpeedPoint[]);
    if (!v || !pts.length) return;
    let raf = 0;
    const step = () => {
      const r = ramp.speedAt(pts, v.currentTime);
      if (Math.abs(v.playbackRate - r) > 0.005) v.playbackRate = r;
      v.defaultPlaybackRate = r;
      raf = requestAnimationFrame(step);
    };
    step();
    return () => cancelAnimationFrame(raf);
  });
  // Fade preview: how dark the picture is at the playhead, from the fades at
  // the start of the first output range and the end of the last (in source
  // seconds, so the speed counts).
  const fadeDark = $derived.by(() => {
    const fi = Number(app.fadeIn) || 0;
    const fo = Number(app.fadeOut) || 0;
    if ((!fi && !fo) || app.cropMode || app.playback !== "forward") return 0;
    const segs = app.outSegments;
    // with a ramp, the speed where each fade is (near enough for a preview)
    const pts = app.speedRamp.length ? ramp.clean(app.speedRamp) : null;
    const start = segs.length ? segs[0].start : 0;
    const end = segs.length ? segs[segs.length - 1].end : app.duration;
    const t = app.current;
    let d = 0;
    const si = pts ? ramp.speedAt(pts, start) : Number(app.speed) || 1;
    const so = pts ? ramp.speedAt(pts, end) : Number(app.speed) || 1;
    if (fi > 0 && t >= start && t < start + fi * si) d = Math.max(d, 1 - (t - start) / (fi * si));
    if (fo > 0 && t <= end && t > end - fo * so) d = Math.max(d, 1 - (end - t) / (fo * so));
    return Math.min(1, Math.max(0, d));
  });
  const adj = $derived(app.adjust);
  const showAdj = $derived(!app.adjustHold && !isNeutral(adj));
  const wb = $derived(adj.temperature || adj.tint ? wbGains(adj.temperature ?? 0, adj.tint ?? 0) : null);
  const table = $derived(adj.curve.length ? sample(adj.curve, 33).map((v) => v.toFixed(4)).join(" ") : "");

  // follow the playhead every frame while playing (timeupdate fires about 4x a second)
  $effect(() => {
    if (app.paused) return;
    let raf = 0;
    const loop = () => {
      app.tick();
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  // Wheel: one frame per notch. Trackpads send many small deltas, so they are summed.
  let wheelAcc = 0;
  function onWheel(e: WheelEvent) {
    if (!app.media) return;
    e.preventDefault();
    const d = e.deltaMode === 1 ? e.deltaY * 40 : e.deltaMode === 2 ? e.deltaY * 400 : e.deltaY;
    wheelAcc += d;
    while (Math.abs(wheelAcc) >= 50) {
      const dir = Math.sign(wheelAcc);
      app.stepFrame(dir);
      wheelAcc -= dir * 100;
      if (Math.sign(wheelAcc) !== dir) wheelAcc = 0;
    }
  }

  function onMeta() {
    const v = video;
    if (!v) return;
    // no picture: the webview cannot decode the video (a sound file has none)
    if (v.videoWidth === 0 && !app.audioOnly) {
      void app.useProxy();
      return;
    }
    // a new source: a seek on the old one ends without a seeked event
    app.queuedSeek = null;
    app.seekPending = false;
    // keep the playhead when switching to the proxy
    if (app.current > 0) app.seek(app.current);
  }
</script>

<div class="stage" class:panning={app.zoomPoints.length > 0 && !app.cropMode} bind:clientWidth={sw} bind:clientHeight={sh} onwheel={onWheel} onpointerdown={panDown} onpointermove={panMove} onpointerup={panUp} onpointercancel={panUp} role="presentation">
  {#if app.videoSrc}
    {#if framed}
      <!-- a canvas or rotation keyframes: black where the picture does not reach, or a blurred copy of it -->
      <div class="turnbg" style:left="{outRect.x}px" style:top="{outRect.y}px" style:width="{outRect.w}px" style:height="{outRect.h}px">
        {#if blurBg}<canvas class="blurbg" bind:this={blurCanvas} style:filter={showAdj ? "url(#cwm-adjust) blur(14px) brightness(0.85)" : "blur(14px) brightness(0.85)"}></canvas>{/if}
      </div>
    {/if}
    <div class="spin" style:transform={spinCss}>
    <div class="pic" style:left="{box.x}px" style:top="{box.y}px" style:width="{box.w}px" style:height="{box.h}px" style:transform={transform}>
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
      style:filter={showAdj ? "url(#cwm-adjust)" : null}
      bind:this={video}
      src={app.videoSrc}
      preload="auto"
      playsinline
      tabindex="-1"
      onloadedmetadata={onMeta}
      onerror={() => void app.useProxy()}
      onplay={() => (app.paused = false)}
      onpause={() => {
        app.paused = true;
        if (video) app.current = video.currentTime;
      }}
      onseeked={() => {
        if (!app.seekDone()) return;
        // the playhead keeps its time when the player went to the middle of its frame;
        // it takes the player's time when the player ended elsewhere (past the end)
        if (video && video.paused && Math.abs(video.currentTime - app.videoTimeOf(app.current)) > 1e-3) app.current = video.currentTime;
      }}
      onended={() => app.onEnded()}
      onclick={() => {
        if (panned) {
          panned = false;
          return;
        }
        if (!app.cropMode) app.togglePlay();
      }}
      ></video>
      {#if app.audioOnly && app.stillUrl}
        <img class="still" src={app.stillUrl} alt="" draggable="false" style:filter={showAdj ? "url(#cwm-adjust)" : null} />
      {/if}
    </div>
    </div>
    {#if !app.cropMode && !app.followMode && !app.audioOnly}
      <!-- outside the output's area: the stage's own colour, as the encode has nothing there -->
      <div class="outmask" class:edge={framed} style:left="{outRect.x}px" style:top="{outRect.y}px" style:width="{outRect.w}px" style:height="{outRect.h}px"></div>
    {/if}
    {#if app.thirds && app.followLane && view && !app.cropMode && !app.followMode && !app.pointsMode}
      <!-- rule of thirds over the output, for aiming Follow's keyframes -->
      <div class="thirds" style:left="{outRect.x}px" style:top="{outRect.y}px" style:width="{outRect.w}px" style:height="{outRect.h}px">
        <span class="v" style:left="33.333%"></span><span class="v" style:left="66.667%"></span>
        <span class="h" style:top="33.333%"></span><span class="h" style:top="66.667%"></span>
      </div>
    {/if}
    {#if fctx}
      <!-- crop mode with a follow path: outside the largest view the path can have here, darker
           (the crop box cannot reach it) -->
      <div class="outmask soft" style:left="{outRect.x}px" style:top="{outRect.y}px" style:width="{outRect.w}px" style:height="{outRect.h}px"></div>
    {/if}
    {#if app.followMode}
      <!-- draw the box to follow; letting go starts following it -->
      <CropOverlay
        rect={shown}
        W={app.viewW}
        H={app.viewH}
        crop={app.followBox}
        ratio={null}
        active
        onchange={(c) => (app.followBox = c)}
        oncommit={(c) => (c ? void app.follow(c) : (app.followBox = null))}
      />
      <div class="followhint">Draw a box around the person to follow (Esc cancels)</div>
    {/if}
    {#if app.pointsMode}
      <!-- click points to follow: one for position, two (for example the eyes) to keep the line between them level -->
      <div
        class="ptpick"
        style:left="{shown.x}px"
        style:top="{shown.y}px"
        style:width="{shown.w}px"
        style:height="{shown.h}px"
        role="presentation"
        onpointerdown={(e) => {
          if (e.button !== 0) return;
          e.stopPropagation();
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          app.addTrackPt(((e.clientX - r.left) / r.width) * app.viewW, ((e.clientY - r.top) / r.height) * app.viewH);
        }}
      >
        {#each app.trackPts as p, i (i)}
          <span class="ptmark" style:left="{(p.x / app.viewW) * 100}%" style:top="{(p.y / app.viewH) * 100}%">{i + 1}</span>
        {/each}
        {#if app.trackPts.length === 2}
          <svg class="ptline" viewBox="0 0 {app.viewW} {app.viewH}" preserveAspectRatio="none" aria-hidden="true">
            <line x1={app.trackPts[0].x} y1={app.trackPts[0].y} x2={app.trackPts[1].x} y2={app.trackPts[1].y} vector-effect="non-scaling-stroke" />
          </svg>
        {/if}
      </div>
      <div class="followhint pts">
        {app.pointsKind === "face"
          ? "Click on the face to follow; the view stays on it and keeps the eyes level"
          : app.trackPts.length === 0
          ? "Click a point to follow, or two (her eyes) to also keep the line between them level"
          : app.trackPts.length === 1
            ? "One point: follows its position. Click a second to also keep the face level."
            : "Two points: follows them and turns the view to keep the line level."}
        {#if app.pointsKind !== "face"}<button class="btn small" disabled={!app.trackPts.length} onclick={() => void app.trackPoints()}>Track <kbd>Enter</kbd></button>{/if}
        <button class="btn small" onclick={() => app.cancelPoints()}>Cancel <kbd>Esc</kbd></button>
      </div>
    {/if}
    <!-- over the picture as shown (turned and flipped), so the crop is drawn on what you see;
         outside crop mode the stage shows the crop itself -->
    {#if app.cropMode && !app.followMode && fctx && fbox}
    <!-- with a follow path: the view as it is now, inside the largest it can be here -->
    <CropOverlay
      rect={outRect}
      W={fctx.w}
      H={fctx.h}
      crop={fbox}
      ratio={ratioOf(app.aspect)}
      active
      onchange={(c) => (fdraft = c)}
      oncommit={commitFollowCrop}
    />
    {:else if app.cropMode && !app.followMode}
    <CropOverlay
      rect={shown}
      W={app.viewW}
      H={app.viewH}
      crop={app.cropBox}
      ratio={ratioOf(app.aspect)}
      active
      onchange={(c) => app.setCropBox(c, false)}
      oncommit={(c) => app.setCropBox(c)}
    />
    {/if}
    {#if fadeDark > 0}
      <div class="fade" style:left="{outRect.x}px" style:top="{outRect.y}px" style:width="{outRect.w}px" style:height="{outRect.h}px" style:opacity={fadeDark}></div>
    {/if}
    {#if app.logoPath && app.plan && !app.cropMode && app.container !== "audio"}
      {@const r = outRect}
      {@const src = convertFileSrc(app.logoPath)}
      {@const retry = () => logoTry < 5 && setTimeout(() => logoTry++, 400)}
      {#key logoTry}
        {#if app.logoCorner === "tile" && app.plan.outWidth > 0}
          {@const t = tileGrid(app.plan.outWidth, app.plan.outHeight)}
          {@const k = r.w / app.plan.outWidth}
          <div class="tiles" style:left="{r.x}px" style:top="{r.y}px" style:width="{r.w}px" style:height="{r.h}px" style:opacity={app.logoOpacity}>
            <div
              class="grid"
              style:left="{t.x * k}px"
              style:top="{t.y * k}px"
              style:width="{t.cols * t.cell * k}px"
              style:height="{t.rows * t.cell * k}px"
              style:transform={app.logoAngle ? `rotate(${app.logoAngle}deg)` : null}
            >
              {#each { length: t.rows } as _, j (j)}
                {#each { length: t.cols } as _, i (i)}
                  <span class="cell" style:left="{i * t.cell * k}px" style:top="{j * t.cell * k}px" style:width="{t.cell * k}px" style:height="{t.cell * k}px">
                    <img {src} alt="" draggable="false" onerror={i + j === 0 ? retry : null} style:max-width="{t.w * k}px" style:max-height="{t.w * k}px" />
                  </span>
                {/each}
              {/each}
            </div>
          </div>
        {:else}
          {@const lw = (r.w * app.logoSize) / 100}
          {@const lh = lw * logoAspect}
          {@const b = turnedBox(lw, lh, app.logoAngle)}
          {@const p = logoPos(r, b.w, b.h)}
          <!-- the bounding box of the (turned) image is what gets placed, as in the engine -->
          <div
            class="logo"
            class:custom={app.logoCorner === "custom"}
            title="Drag to move the watermark"
            role="presentation"
            onpointerdown={(e) => dragLogo(e, r, b.w, b.h)}
            style:width="{b.w}px"
            style:height="{b.h}px"
            style:left="{r.x + p.x}px"
            style:top="{r.y + p.y}px"
          >
            <img
              onerror={retry}
              onload={(e) => {
                const im = e.currentTarget as HTMLImageElement;
                if (im.naturalWidth) logoAspect = im.naturalHeight / im.naturalWidth;
              }}
              {src}
              alt=""
              draggable="false"
              style:width="{lw}px"
              style:left="{(b.w - lw) / 2}px"
              style:top="{(b.h - lh) / 2}px"
              style:opacity={app.logoOpacity}
              style:transform={app.logoAngle ? `rotate(${app.logoAngle}deg)` : null}
            />
          </div>
        {/if}
      {/key}
    {/if}
    {#if app.captionText.trim() && !app.cropMode && app.container !== "audio"}
      {@const r = outRect}
      {@const fs = (r.h * app.captionSize) / 100}
      <div class="caption" class:top={app.captionTop} class:custom={app.captionCustom} style:left="{r.x}px" style:top="{r.y}px" style:width="{r.w}px" style:height="{r.h}px">
        <span
          role="presentation"
          title="Drag to move the caption"
          bind:offsetWidth={capW}
          bind:offsetHeight={capH}
          onpointerdown={dragCaption}
          style:left={app.captionCustom ? `${(r.w - capW) * app.captionX}px` : null}
          style:top={app.captionCustom ? `${(r.h - capH) * app.captionY}px` : null}
          style:font-size="{fs}px"
          style:color={app.captionColor}
          style:-webkit-text-stroke={app.captionOutline > 0 ? `${Math.max(1, (fs * app.captionOutline) / 100) * 2}px ${app.captionOutlineColor}` : null}
          style:font-family={captionCss?.family}
          style:font-weight={captionCss?.weight}
          style:font-style={captionCss?.style}>{app.captionText}</span
        >
      </div>
    {/if}
  {/if}

  <svg class="fx" aria-hidden="true">
    <filter id="cwm-adjust" color-interpolation-filters="sRGB">
      {#if wb}
        <!-- white balance first, as in the encode (colorchannelmixer before eq) -->
        <feColorMatrix type="matrix" values="{wb[0]} 0 0 0 0  0 {wb[1]} 0 0 0  0 0 {wb[2]} 0 0  0 0 0 1 0" />
      {/if}
      <feComponentTransfer>
        <feFuncR type="linear" slope={adj.contrast} intercept={0.5 - 0.5 * adj.contrast + adj.brightness} />
        <feFuncG type="linear" slope={adj.contrast} intercept={0.5 - 0.5 * adj.contrast + adj.brightness} />
        <feFuncB type="linear" slope={adj.contrast} intercept={0.5 - 0.5 * adj.contrast + adj.brightness} />
      </feComponentTransfer>
      <feComponentTransfer>
        <feFuncR type="gamma" amplitude="1" exponent={1 / adj.gamma} offset="0" />
        <feFuncG type="gamma" amplitude="1" exponent={1 / adj.gamma} offset="0" />
        <feFuncB type="gamma" amplitude="1" exponent={1 / adj.gamma} offset="0" />
      </feComponentTransfer>
      <feColorMatrix type="saturate" values={String(adj.saturation)} />
      <feColorMatrix type="hueRotate" values={String(adj.hue)} />
      {#if table}
        <feComponentTransfer>
          <feFuncR type="table" tableValues={table} />
          <feFuncG type="table" tableValues={table} />
          <feFuncB type="table" tableValues={table} />
        </feComponentTransfer>
      {/if}
    </filter>
  </svg>

  {#if app.proxyFraction !== null}
    <div class="note">
      <div>This file cannot be played directly. Making a preview copy...</div>
      <div class="bar"><div style:width="{Math.round(app.proxyFraction * 100)}%"></div></div>
      <div class="mono muted">{Math.round(app.proxyFraction * 100)}%</div>
      <button class="btn small" onclick={() => ipc.proxyCancel().catch(() => {})}>Cancel</button>
    </div>
  {/if}
  {#if app.videoSrc && app.followLane && !app.audioOnly}
    <div class="vctl left">
      <button
        class="spk ovbtn"
        class:on={app.thirds}
        aria-pressed={app.thirds}
        aria-label="Rule of thirds lines"
        title="Rule of thirds: lines at a third and two thirds of the output, for placing the subject while you aim Follow's keyframes"
        onclick={() => app.toggleThirds()}
      ><Icon name="thirds" size={16} /></button>
    </div>
  {/if}
  {#if app.videoSrc}
    <div class="vctl">
    <button
      class="spk ovbtn"
      disabled={!app.media || app.ffmpegMissing}
      aria-label="Save the frame at the playhead (S)"
      title="Save the frame at the playhead as PNG with the crop (S). Shift+click (Shift+S): as the encode sees it, with filters, adjustments, rotation and the output size."
      onclick={(e) => app.exportFrame(e.shiftKey)}
    ><Icon name="image" size={16} /></button>
    {#if hasSound}
    <div class="sound" class:off={app.muted} onwheel={soundWheel}>
      <input
        type="range"
        min="0"
        max="1"
        step="0.01"
        aria-label="Preview volume"
        value={app.muted ? 0 : app.volume}
        oninput={(e) => app.setVolume(Number((e.currentTarget as HTMLInputElement).value))}
      />
      <button
        class="spk"
        aria-label={app.muted ? "Unmute the preview (M)" : "Mute the preview (M)"}
        title="{app.muted ? 'Unmute' : 'Mute'} the preview (M). The encode's sound is set in the sidebar."
        onclick={() => app.toggleMute()}
      >
        <Icon name={app.muted ? "volumeOff" : app.volume < 0.5 ? "volumeLow" : "volume"} size={18} />
      </button>
    </div>
    {/if}
    <button
      class="spk ovbtn"
      class:on={app.loop}
      aria-pressed={app.loop}
      aria-label="Loop the selected range (L)"
      title="Loop the selected range (L)"
      onclick={() => app.toggleLoop()}
    ><Icon name="loop" size={16} /></button>
    </div>
  {/if}
  {#if app.seekPending && app.paused && app.media && !app.audioOnly}
    <!-- shown after 150 ms, so quick seeks do not flash it -->
    <span class="loading" title="The player is still decoding the frame at the playhead; the picture is the previous one until this goes away">Loading frame...</span>
  {/if}
  {#if app.adjustHold && !isNeutral(adj) && app.media && !app.audioOnly}
    <span class="before">Original (without the adjustments)</span>
  {/if}
  {#if app.usingProxy}
    <span class="badge" title={app.proxyRemuxed ? "The source's own video and audio, copied into an MP4 the player can read" : "A 480p H.264 copy for the player; encoding uses the original"}>{app.proxyRemuxed ? "Preview copy (full quality)" : "Preview copy (480p)"}</span>
  {/if}
</div>

<style>
  .loading {
    position: absolute;
    top: 8px;
    left: 8px;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.7);
    color: #fff;
    font-size: 11px;
    pointer-events: none;
    opacity: 0;
    animation: loading-show 0s linear 150ms forwards;
  }
  @keyframes loading-show {
    to {
      opacity: 1;
    }
  }
  .logo {
    position: absolute;
    cursor: move;
    touch-action: none;
  }
  .logo img {
    position: absolute;
    pointer-events: none;
  }
  .logo.custom {
    outline: 1px dashed rgb(255 255 255 / 0.6);
    outline-offset: 2px;
  }
  .tiles {
    position: absolute;
    overflow: hidden;
    pointer-events: none;
  }
  .tiles .grid {
    position: absolute;
    transform-origin: center;
  }
  .tiles .cell {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .fade {
    position: absolute;
    background: #000;
    pointer-events: none;
  }
  .caption {
    position: absolute;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: center;
    padding: 4% 0;
    box-sizing: border-box;
    pointer-events: none;
    overflow: hidden;
  }
  .caption.top {
    justify-content: flex-start;
  }
  .caption.custom {
    display: block;
    padding: 0;
  }
  .caption.custom span {
    position: absolute;
  }
  .caption span {
    color: #fff;
    font-family: Arial, "DejaVu Sans", Helvetica, sans-serif;
    font-weight: 700;
    line-height: 1.15;
    text-align: center;
    white-space: pre;
    paint-order: stroke fill;
    pointer-events: auto;
    cursor: move;
    touch-action: none;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    background: var(--bg-stage);
    overflow: hidden;
  }
  .pic {
    position: absolute;
  }
  /* the picture of a sound file, over the (empty) video, which still plays the sound */
  .still {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
  }
  .fx {
    position: absolute;
    width: 0;
    height: 0;
  }
  video {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
  }
  .note {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: rgba(0, 0, 0, 0.75);
    color: var(--text-secondary);
  }
  .bar {
    width: 260px;
    height: 4px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }
  .bar > div {
    height: 100%;
    background: var(--accent);
  }
  .followhint {
    position: absolute;
    left: 50%;
    top: 10px;
    transform: translateX(-50%);
    padding: 3px 10px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.7);
    color: #fff;
    font-size: 12px;
    pointer-events: none;
  }
  .followhint.pts {
    display: flex;
    align-items: center;
    gap: 8px;
    pointer-events: auto;
    white-space: nowrap;
  }
  /* turns the picture for Follow around the stage's middle (where the view's middle is) */
  .spin {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .turnbg {
    position: absolute;
    overflow: hidden;
    background: #000;
    pointer-events: none;
  }
  /* a little larger than the frame, so the blur's faded edge is outside it */
  .blurbg {
    position: absolute;
    left: -6%;
    top: -6%;
    width: 112%;
    height: 112%;
  }
  .spin > :global(*) {
    pointer-events: auto;
  }
  .ptpick {
    position: absolute;
    cursor: crosshair;
  }
  .ptmark {
    position: absolute;
    width: 18px;
    height: 18px;
    margin: -9px 0 0 -9px;
    border: 2px solid #fff;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
    font-size: 10px;
    font-weight: 700;
    line-height: 14px;
    text-align: center;
    pointer-events: none;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.6);
  }
  .ptline {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  .ptline line {
    stroke: #fff;
    stroke-width: 1.5;
    stroke-dasharray: 5 4;
  }
  .outmask {
    position: absolute;
    box-shadow: 0 0 0 9999px var(--bg-stage);
    pointer-events: none;
  }
  /* a canvas or turned parts: the frame's edge, so black bars are told from the stage */
  .outmask.edge {
    outline: 1px solid rgb(255 255 255 / 0.28);
  }
  .outmask.soft {
    box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.7);
  }
  .stage.panning {
    cursor: grab;
  }
  /* the corner controls, bottom right: the speaker, and Loop in the corner */
  .vctl.left {
    right: auto;
    left: 10px;
  }
  .thirds {
    position: absolute;
    pointer-events: none;
  }
  /* a light line with a dark edge, seen on bright and dark pictures */
  .thirds .v,
  .thirds .h {
    position: absolute;
    background: rgb(255 255 255 / 0.55);
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.25);
  }
  .thirds .v {
    top: 0;
    bottom: 0;
    width: 1px;
  }
  .thirds .h {
    left: 0;
    right: 0;
    height: 1px;
  }
  .vctl {
    position: absolute;
    right: 10px;
    bottom: 10px;
    display: flex;
    /* the sound's slider grows upward; the buttons stay on the bottom line */
    align-items: flex-end;
    gap: 6px;
  }
  /* the round buttons over the picture: frame, loop */
  .ovbtn {
    background: rgb(0 0 0 / 0.55) !important;
    color: #fff;
    opacity: 0.7;
  }
  .ovbtn:hover {
    opacity: 1;
  }
  .ovbtn:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .ovbtn.on {
    opacity: 1;
    color: var(--accent);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  /* the preview's sound, bottom right: the speaker, and the volume slider above it while the pointer is over it */
  .sound {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 0;
    border-radius: 14px;
    background: rgb(0 0 0 / 0.55);
    color: #fff;
    opacity: 0.7;
    transition: opacity 0.15s;
  }
  .sound:hover {
    opacity: 1;
  }
  .sound.off {
    opacity: 0.95;
  }
  /* upright, loud at the top */
  .sound input {
    writing-mode: vertical-lr;
    direction: rtl;
    width: 16px;
    height: 0;
    opacity: 0;
    margin: 0;
    transition:
      height 0.15s,
      opacity 0.15s,
      margin 0.15s;
  }
  .sound:hover input {
    height: 90px;
    opacity: 1;
    margin: 10px 0 4px;
  }
  .spk {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }
  .spk:hover {
    background: rgb(255 255 255 / 0.15);
  }
  .before {
    position: absolute;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.7);
    color: #fff;
    font-size: 11px;
    pointer-events: none;
  }
  .badge {
    position: absolute;
    right: 8px;
    top: 8px;
    padding: 1px 6px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.6);
    color: var(--text-muted);
    font-size: 11px;
    pointer-events: none;
  }
</style>
