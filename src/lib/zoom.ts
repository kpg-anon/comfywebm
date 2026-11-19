// Zoom points for the preview: the same curve and limits as the engine's
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

type Key = "zoom" | "x" | "y";

/** The slope of one value at point i: 0 at the first and last point and where
 *  the value turns back (so a held point holds and nothing overshoots), else
 *  the mean of the slopes on each side, limited as in Fritsch-Carlson. Matches
 *  zoom.rs `slopes`. */
export function slopes(p: ZoomPoint[], k: Key): number[] {
  const n = p.length;
  const d = p.slice(1).map((b, i) => (b[k] - p[i][k]) / Math.max(1e-6, b.t - p[i].t));
  return p.map((_, i) => {
    if (i === 0 || i === n - 1) return 0;
    const a = d[i - 1];
    const b = d[i];
    if (a * b <= 0) return 0;
    const m = (a + b) / 2;
    const lim = 3 * Math.min(Math.abs(a), Math.abs(b));
    return Math.sign(m) * Math.min(Math.abs(m), lim);
  });
}

/** Zoom and centre at source time t: a smooth curve through the points that
 *  starts and stops gently at the first and last; outside them it holds. */
export function at(points: ZoomPoint[], t: number): { zoom: number; x: number; y: number } {
  const p = clean(points);
  if (!p.length) return { zoom: 1, x: 0.5, y: 0.5 };
  if (t <= p[0].t) return { zoom: p[0].zoom, x: p[0].x, y: p[0].y };
  const q = p[p.length - 1];
  if (t >= q.t) return { zoom: q.zoom, x: q.x, y: q.y };
  let i = 0;
  while (i + 2 < p.length && t > p[i + 1].t) i++;
  const a = p[i];
  const b = p[i + 1];
  const h = Math.max(1e-6, b.t - a.t);
  const u = (t - a.t) / h;
  const v = (k: Key) => {
    const m = slopes(p, k);
    return hermite(a[k], b[k], m[i] * h, m[i + 1] * h, u);
  };
  return { zoom: v("zoom"), x: v("x"), y: v("y") };
}

function hermite(a: number, b: number, ma: number, mb: number, u: number): number {
  const c2 = 3 * (b - a) - 2 * ma - mb;
  const c3 = 2 * (a - b) + ma + mb;
  return a + u * (ma + u * (c2 + u * c3));
}

/** Few keyframes for a dense path (a point per frame from the tracker): the
 *  first and last, then the point farthest from the curve through the kept
 *  ones, until no point is more than `tol` (0..1 of the frame) away. */
export function simplify(dense: ZoomPoint[], tol: number, maxKeys = 200): ZoomPoint[] {
  const p = clean(dense);
  if (p.length <= 2) return p;
  const keep = new Set([0, p.length - 1]);
  for (;;) {
    const keys = [...keep].sort((a, b) => a - b).map((i) => p[i]);
    let worst = -1;
    let wd = tol;
    p.forEach((q, i) => {
      if (keep.has(i)) return;
      const c = at(keys, q.t);
      const dd = Math.hypot(c.x - q.x, c.y - q.y);
      if (dd > wd) {
        wd = dd;
        worst = i;
      }
    });
    if (worst < 0 || keep.size >= maxKeys) return keys;
    keep.add(worst);
  }
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
