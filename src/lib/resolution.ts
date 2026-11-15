// Output picture size choices. The engine takes a box (maxWidth, maxHeight),
// fits the picture inside it keeping the aspect ratio and never upscales
// (plan.rs `geometry`), so every mode here becomes a box. The preset's picture
// limit (for example 4chan's 2048x2048) is a second box; the smaller wins.
import type { Codec } from "./types";

export type ResMode = "auto" | "source" | "step" | "percent" | "exact";


export type Box = { w: number | null; h: number | null };

const even = (x: number) => Math.max(2, Math.floor(x / 2) * 2);

/** Output size for a picture of w x h fitted in a box, as the engine computes it. */
export function fit(w: number, h: number, box: Box): { w: number; h: number } {
  let f = 1;
  if (box.w) f = Math.min(f, box.w / w);
  if (box.h) f = Math.min(f, box.h / h);
  return { w: even(w * f), h: even(h * f) };
}

const minOrNull = (a: number | null, b: number | null) => (a === null ? b : b === null ? a : Math.min(a, b));

/** The box for a step: an output height, or an output width. The step lists are in config.toml. */
export function stepBox(by: "height" | "width", px: number): Box {
  return by === "height" ? { w: null, h: px } : { w: px, h: null };
}

export function percentBox(w: number, h: number, pct: number): Box {
  return { w: Math.max(2, Math.round((w * pct) / 100)), h: Math.max(2, Math.round((h * pct) / 100)) };
}

/** The box for one exact edge; the other edge follows from the aspect ratio. */
export function exactBox(edge: "w" | "h", val: number): Box {
  return edge === "w" ? { w: val, h: null } : { w: null, h: val };
}

/** Combine a mode box with the preset's picture limit. */
export function withLimit(box: Box, limit: Box): Box {
  return { w: minOrNull(box.w, limit.w), h: minOrNull(box.h, limit.h) };
}

/** Bits per pixel below which a codec's picture visibly breaks down (plan.rs `bpp_floor`). */
export const BPP_FLOOR: Record<Codec, number> = { av1: 0.022, vp9: 0.03, hevc: 0.03, h264: 0.045, vp8: 0.055 };

export type Quality = "good" | "fair" | "poor";

/** Same rule as plan.rs `quality`: good at 1.5x the floor or more, fair at the floor or more. */
export function quality(kbps: number, w: number, h: number, fps: number, codec: Codec): Quality {
  const bpp = (kbps * 1000) / (w * h * fps);
  const floor = BPP_FLOOR[codec];
  return bpp >= floor * 1.5 ? "good" : bpp >= floor ? "fair" : "poor";
}
