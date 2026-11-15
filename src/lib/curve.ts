// The master curve of the color adjustments. ffmpeg's `curves` filter joins
// the points with a natural cubic spline; this does the same, so the curve
// drawn in the editor and the preview table match the encode.

export type Pt = [number, number];

export const IDENTITY: Pt[] = [
  [0, 0],
  [1, 1],
];

/** Sorted points with distinct x, clamped to 0..1; the two ends are added when missing. */
export function normalize(points: Pt[]): Pt[] {
  const p = points
    .filter(([x, y]) => Number.isFinite(x) && Number.isFinite(y))
    .map(([x, y]) => [Math.min(1, Math.max(0, x)), Math.min(1, Math.max(0, y))] as Pt)
    .sort((a, b) => a[0] - b[0]);
  const out: Pt[] = [];
  for (const q of p) if (!out.length || q[0] - out[out.length - 1][0] > 1e-4) out.push(q);
  if (!out.length || out[0][0] > 0) out.unshift([0, 0]);
  if (out[out.length - 1][0] < 1) out.push([1, 1]);
  return out;
}

/** n values of the curve at x = 0, 1/(n-1), ..., 1, each clamped to 0..1. */
export function sample(points: Pt[], n = 64): number[] {
  const p = normalize(points);
  const k = p.length;
  if (k === 2) {
    const [[x0, y0], [x1, y1]] = p;
    return Array.from({ length: n }, (_, i) => {
      const x = i / (n - 1);
      return Math.min(1, Math.max(0, y0 + ((y1 - y0) * (x - x0)) / (x1 - x0 || 1)));
    });
  }
  // natural cubic spline: second derivatives m[i], with m[0] = m[k-1] = 0
  const h = p.slice(0, -1).map((q, i) => p[i + 1][0] - q[0]);
  const a = new Array(k).fill(0);
  const b = new Array(k).fill(1);
  const c = new Array(k).fill(0);
  const d = new Array(k).fill(0);
  for (let i = 1; i < k - 1; i++) {
    a[i] = h[i - 1];
    b[i] = 2 * (h[i - 1] + h[i]);
    c[i] = h[i];
    d[i] = 6 * ((p[i + 1][1] - p[i][1]) / h[i] - (p[i][1] - p[i - 1][1]) / h[i - 1]);
  }
  // tridiagonal solve (Thomas)
  for (let i = 1; i < k; i++) {
    const w = a[i] / b[i - 1];
    b[i] -= w * c[i - 1];
    d[i] -= w * d[i - 1];
  }
  const m = new Array(k).fill(0);
  m[k - 1] = d[k - 1] / b[k - 1];
  for (let i = k - 2; i >= 0; i--) m[i] = (d[i] - c[i] * m[i + 1]) / b[i];
  m[0] = 0;
  m[k - 1] = 0;
  const out: number[] = [];
  let seg = 0;
  for (let i = 0; i < n; i++) {
    const x = i / (n - 1);
    while (seg < k - 2 && x > p[seg + 1][0]) seg++;
    const [x0, y0] = p[seg];
    const [x1, y1] = p[seg + 1];
    const hh = x1 - x0;
    const t0 = x1 - x;
    const t1 = x - x0;
    const y =
      (m[seg] * t0 ** 3) / (6 * hh) +
      (m[seg + 1] * t1 ** 3) / (6 * hh) +
      (y0 / hh - (m[seg] * hh) / 6) * t0 +
      (y1 / hh - (m[seg + 1] * hh) / 6) * t1;
    out.push(Math.min(1, Math.max(0, y)));
  }
  return out;
}

export const isIdentity = (points: Pt[]) => points.every(([x, y]) => Math.abs(x - y) < 1e-4);

/** Starting shapes offered under the curve editor. */
export const CURVE_PRESETS: { name: string; points: Pt[] }[] = [
  { name: "Linear", points: IDENTITY },
  { name: "More contrast", points: [[0, 0], [0.25, 0.18], [0.75, 0.82], [1, 1]] },
  { name: "Less contrast", points: [[0, 0.06], [0.25, 0.3], [0.75, 0.7], [1, 0.94]] },
  { name: "Lift shadows", points: [[0, 0], [0.25, 0.35], [1, 1]] },
  { name: "Darken", points: [[0, 0], [0.5, 0.4], [1, 1]] },
  { name: "Faded", points: [[0, 0.12], [0.5, 0.52], [1, 0.92]] },
];
