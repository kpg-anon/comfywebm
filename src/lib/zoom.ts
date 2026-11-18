// Zoom points for the preview: the same easing and limits as the engine's
// zoom.rs, so the window drawn on the picture is what the encode shows.
import type { Crop, ZoomPoint } from "./types";

export const MIN_ZOOM = 1;
export const MAX_ZOOM = 4;

export function clean(points: ZoomPoint[]): ZoomPoint[] {
  const cl = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
  const p = points
    .filter((q) => [q.t, q.zoom, q.x, q.y].every(Number.isFinite))
    .map((q) => ({ t: Math.max(0, q.t), zoom: cl(q.zoom, MIN_ZOOM, MAX_ZOOM), x: cl(q.x, 0, 1), y: cl(q.y, 0, 1) }))
    .sort((a, b) => a.t - b.t);
  return p.filter((q, i) => i === 0 || Math.abs(q.t - p[i - 1].t) >= 1e-6);
}

const ease = (u: number) => (1 - Math.cos(Math.PI * Math.min(1, Math.max(0, u)))) / 2;

/** Zoom and centre at source time t. */
export function at(points: ZoomPoint[], t: number): { zoom: number; x: number; y: number } {
  const p = clean(points);
  if (!p.length) return { zoom: 1, x: 0.5, y: 0.5 };
  if (t <= p[0].t) return { zoom: p[0].zoom, x: p[0].x, y: p[0].y };
  for (let i = 0; i + 1 < p.length; i++) {
    const a = p[i];
    const b = p[i + 1];
    if (t <= b.t) {
      const u = ease((t - a.t) / Math.max(1e-9, b.t - a.t));
      const l = (x: number, y: number) => x + (y - x) * u;
      return { zoom: l(a.zoom, b.zoom), x: l(a.x, b.x), y: l(a.y, b.y) };
    }
  }
  const q = p[p.length - 1];
  return { zoom: q.zoom, x: q.x, y: q.y };
}

/** The part of a W x H picture that shows at zoom z around (x, y), kept inside it (as zoompan does). */
export function windowOf(z: { zoom: number; x: number; y: number }, W: number, H: number): Crop {
  const w = W / z.zoom;
  const h = H / z.zoom;
  return {
    x: Math.min(Math.max(0, z.x * W - w / 2), W - w),
    y: Math.min(Math.max(0, z.y * H - h / 2), H - h),
    w,
    h,
  };
}
