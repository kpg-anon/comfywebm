// Crop rectangle math. All values are source display pixels.
import type { Crop } from "./types";

export type Handle = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

export type AspectLock = "free" | "1:1" | "16:9" | "9:16" | "4:3" | "21:9";
export const ASPECTS: AspectLock[] = ["free", "1:1", "16:9", "9:16", "4:3", "21:9"];

/** width / height for a lock, or null for free. */
export function ratioOf(a: AspectLock): number | null {
  if (a === "free") return null;
  const [w, h] = a.split(":").map(Number);
  return w / h;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/**
 * Resize `start` by dragging `handle` by (dx, dy). The opposite edge or corner
 * stays fixed; dragging past it flips the rectangle. With a ratio, the result
 * keeps it and is scaled toward the fixed point until it fits in W x H.
 */
export function resize(start: Crop, handle: Handle, dx: number, dy: number, ratio: number | null, W: number, H: number): Crop {
  let L = start.x;
  let T = start.y;
  let R = start.x + start.w;
  let B = start.y + start.h;
  const hasE = handle.includes("e");
  const hasW = handle.includes("w");
  const hasN = handle.includes("n");
  const hasS = handle.includes("s");
  if (hasE) R = clamp(R + dx, 0, W);
  if (hasW) L = clamp(L + dx, 0, W);
  if (hasS) B = clamp(B + dy, 0, H);
  if (hasN) T = clamp(T + dy, 0, H);
  // fixed point: the edge or corner opposite the handle
  const fixedX = hasE ? start.x : hasW ? start.x + start.w : null;
  const fixedY = hasS ? start.y : hasN ? start.y + start.h : null;
  let x0 = Math.min(L, R);
  let x1 = Math.max(L, R);
  let y0 = Math.min(T, B);
  let y1 = Math.max(T, B);

  if (ratio) {
    let w = x1 - x0;
    let h = y1 - y0;
    if (fixedX !== null && fixedY !== null) {
      // corner: grow the short side to match the ratio
      if (w / ratio >= h) h = w / ratio;
      else w = h * ratio;
    } else if (fixedX !== null) {
      h = w / ratio;
    } else {
      w = h * ratio;
    }
    // anchor point and direction the rectangle extends from it
    const ax = fixedX ?? start.x + start.w / 2;
    const ay = fixedY ?? start.y + start.h / 2;
    const signX = fixedX === null ? 0 : x0 < fixedX ? -1 : 1;
    const signY = fixedY === null ? 0 : y0 < fixedY ? -1 : 1;
    x0 = signX === 0 ? ax - w / 2 : signX < 0 ? ax - w : ax;
    y0 = signY === 0 ? ay - h / 2 : signY < 0 ? ay - h : ay;
    x1 = x0 + w;
    y1 = y0 + h;
    // scale toward the anchor until it fits
    let f = 1;
    if (x0 < 0 && ax - x0 > 0) f = Math.min(f, ax / (ax - x0));
    if (x1 > W && x1 - ax > 0) f = Math.min(f, (W - ax) / (x1 - ax));
    if (y0 < 0 && ay - y0 > 0) f = Math.min(f, ay / (ay - y0));
    if (y1 > H && y1 - ay > 0) f = Math.min(f, (H - ay) / (y1 - ay));
    f = Math.max(0, f);
    x0 = ax + (x0 - ax) * f;
    x1 = ax + (x1 - ax) * f;
    y0 = ay + (y0 - ay) * f;
    y1 = ay + (y1 - ay) * f;
  }
  return round({ x: x0, y: y0, w: x1 - x0, h: y1 - y0 }, W, H);
}

/** Start a new rectangle at (px, py) and drag it by (dx, dy). */
export function draw(px: number, py: number, dx: number, dy: number, ratio: number | null, W: number, H: number): Crop {
  const handle = ((dy < 0 ? "n" : "s") + (dx < 0 ? "w" : "e")) as Handle;
  return resize({ x: px, y: py, w: 0, h: 0 }, handle, dx, dy, ratio, W, H);
}

/** Move `start` by (dx, dy), kept inside W x H. */
export function move(start: Crop, dx: number, dy: number, W: number, H: number): Crop {
  return {
    x: Math.round(clamp(start.x + dx, 0, W - start.w)),
    y: Math.round(clamp(start.y + dy, 0, H - start.h)),
    w: start.w,
    h: start.h,
  };
}

/** The largest rectangle with `ratio` that fits inside `c`, centered in it. */
export function fitRatio(c: Crop, ratio: number, W: number, H: number): Crop {
  let w = c.w;
  let h = c.w / ratio;
  if (h > c.h) {
    h = c.h;
    w = c.h * ratio;
  }
  return round({ x: c.x + (c.w - w) / 2, y: c.y + (c.h - h) / 2, w, h }, W, H);
}

/** Round to whole pixels and keep the rectangle inside W x H, at least 1x1. */
export function round(c: Crop, W: number, H: number): Crop {
  const x = clamp(Math.round(c.x), 0, Math.max(0, W - 1));
  const y = clamp(Math.round(c.y), 0, Math.max(0, H - 1));
  const w = clamp(Math.round(c.w), 1, W - x);
  const h = clamp(Math.round(c.h), 1, H - y);
  return { x, y, w, h };
}

/** Same rectangle as the full picture: no crop. */
export function isFull(c: Crop, W: number, H: number): boolean {
  return c.x === 0 && c.y === 0 && c.w === W && c.h === H;
}
