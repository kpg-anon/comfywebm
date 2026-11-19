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

/** A tracked position for `camera`, with its own band when the box's size is known. */
export type CamPoint = ZoomPoint & { mx?: number; my?: number };

/** A camera path for a tracked subject, as a camera operator would move: the
 *  subject may sit anywhere within `mx` / `my` (0..1 of the frame) of the
 *  view's centre, and inside that band the camera takes the shortest path, so
 *  it holds still or pans at an even speed instead of copying every bob and
 *  sway. `dense` is the subject's centre over time (0..1 of the frame).
 *  Worked out on 10 samples a second (projected over-relaxation on the path
 *  that pulls straight within the band), then the corners rounded over about
 *  a quarter of a second. Returns those samples. */
export function camera(dense: CamPoint[], mx: number, my: number): ZoomPoint[] {
  // sorted, finite, one per time; a point's own band (its box's size then) kept
  const p = dense
    .filter((q) => [q.t, q.x, q.y].every(Number.isFinite))
    .map((q) => ({ t: Math.max(0, q.t), zoom: 1, x: Math.min(1, Math.max(0, q.x)), y: Math.min(1, Math.max(0, q.y)), mx: q.mx ?? mx, my: q.my ?? my }))
    .sort((a, b) => a.t - b.t)
    .filter((q, i, a) => i === 0 || q.t - a[i - 1].t >= 1e-6);
  if (p.length < 3) return p.map(({ t, x, y }) => ({ t, zoom: 1, x, y }));
  const t0 = p[0].t;
  const t1 = p[p.length - 1].t;
  const n = Math.max(2, Math.round((t1 - t0) * 10) + 1);
  const ts = Array.from({ length: n }, (_, i) => t0 + ((t1 - t0) * i) / (n - 1));
  // the subject at each sample, linear between tracked frames
  let j = 0;
  const s = ts.map((t) => {
    while (j + 2 < p.length && p[j + 1].t < t) j++;
    const a = p[j];
    const b = p[j + 1];
    const u = Math.min(1, Math.max(0, (t - a.t) / Math.max(1e-9, b.t - a.t)));
    return { x: a.x + (b.x - a.x) * u, y: a.y + (b.y - a.y) * u };
  });
  // every tracked frame within half a sample of each sample: a quick move between
  // two samples has to stay in the band too
  const near = ts.map((t) => p.filter((q) => Math.abs(q.t - t) <= (t1 - t0) / (n - 1) / 2 + 1e-9));
  // the band at each sample: the narrowest of the tracked frames near it (a box that
  // grows as the person comes closer leaves less room)
  const bandAt = (i: number, k: "mx" | "my") => {
    const qs = near[i].length ? near[i] : [p.reduce((a, b) => (Math.abs(b.t - ts[i]) < Math.abs(a.t - ts[i]) ? b : a))];
    return Math.max(0, Math.min(...qs.map((q) => q[k])));
  };
  const axis = (v: number[], k: "x" | "y") => {
    const mk = k === "x" ? "mx" : "my";
    // a little inside the band, so rounding the corners keeps the subject in it
    const lo = v.map((x, i) => Math.max(x, ...near[i].map((q) => q[k])) - bandAt(i, mk) * 0.85);
    const hi = v.map((x, i) => Math.min(x, ...near[i].map((q) => q[k])) + bandAt(i, mk) * 0.85);
    // a move wider than the band within one sample: the middle of it
    for (let i = 0; i < n; i++) {
      if (lo[i] > hi[i]) lo[i] = hi[i] = (lo[i] + hi[i]) / 2;
    }
    const c = v.slice();
    for (let it = 0; it < 4000; it++) {
      let moved = 0;
      for (let i = 0; i < n; i++) {
        // the ends have no pull from outside: the camera starts and stops level
        const want = i === 0 ? c[1] : i === n - 1 ? c[n - 2] : (c[i - 1] + c[i + 1]) / 2;
        const next = Math.min(hi[i], Math.max(lo[i], c[i] + 1.9 * (want - c[i])));
        moved = Math.max(moved, Math.abs(next - c[i]));
        c[i] = next;
      }
      if (moved < 1e-7) break;
    }
    // round the corners: a Gaussian over about a quarter of a second (sigma 2.5 samples)
    return c.map((_, i) => {
      let sum = 0;
      let wsum = 0;
      for (let k = -8; k <= 8; k++) {
        const w = Math.exp(-(k * k) / 12.5);
        sum += w * c[Math.min(n - 1, Math.max(0, i + k))];
        wsum += w;
      }
      return sum / wsum;
    });
  };
  const cx = axis(s.map((q) => q.x), "x");
  const cy = axis(s.map((q) => q.y), "y");
  return ts.map((t, i) => ({ t: Math.round(t * 1000) / 1000, zoom: 1, x: cx[i], y: cy[i] }));
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
