// Speed ramps on the timeline: points of (file time, speed); the speed changes
// in a straight line between two points and stays at the first / last point's
// speed before / after them. The same math as crates/engine/src/ramp.rs, for
// the output length, the preview's playback rate and the lane on the timeline.
import type { Segment, SpeedPoint } from "./types";

export const MIN_SPEED = 0.25;
export const MAX_SPEED = 4;

const clampSpeed = (s: number) => Math.min(MAX_SPEED, Math.max(MIN_SPEED, s));

/** The points sorted by time, with usable values; [] when none. */
export function clean(points: SpeedPoint[]): SpeedPoint[] {
  const p = points
    .filter((q) => Number.isFinite(q.t) && Number.isFinite(q.speed) && q.speed > 0)
    .map((q) => ({ t: Math.max(0, q.t), speed: clampSpeed(q.speed) }))
    .sort((a, b) => a.t - b.t);
  return p.filter((q, i) => i === 0 || Math.abs(q.t - p[i - 1].t) >= 1e-6);
}

/** The speed at file time t (points sorted, at least one). */
export function speedAt(points: SpeedPoint[], t: number): number {
  if (!points.length) return 1;
  const first = points[0];
  const last = points[points.length - 1];
  if (t <= first.t) return first.speed;
  if (t >= last.t) return last.speed;
  for (let i = 1; i < points.length; i++) {
    const p = points[i - 1];
    const q = points[i];
    if (t <= q.t) return q.t > p.t ? p.speed + ((q.speed - p.speed) * (t - p.t)) / (q.t - p.t) : q.speed;
  }
  return last.speed;
}

/** Output seconds for x seconds that start at speed a and change by b per second. */
function pieceOut(a: number, b: number, x: number): number {
  return Math.abs(b) < 1e-9 ? x / a : Math.log((a + b * x) / a) / b;
}

/** Output seconds for file time s..e (one range), with the ramp. */
export function rangeOut(points: SpeedPoint[], s: number, e: number): number {
  if (e <= s) return 0;
  const cuts = [s, ...points.map((p) => p.t).filter((t) => t > s + 1e-6 && t < e - 1e-6), e];
  let out = 0;
  for (let i = 1; i < cuts.length; i++) {
    const x0 = cuts[i - 1];
    const x1 = cuts[i];
    const a = speedAt(points, x0);
    out += pieceOut(a, (speedAt(points, x1) - a) / (x1 - x0), x1 - x0);
  }
  return out;
}

/** Output length of the ranges (file time) with the ramp. */
export function outLength(points: SpeedPoint[], segments: Segment[]): number {
  const p = clean(points);
  if (!p.length) return segments.reduce((a, g) => a + Math.max(0, g.end - g.start), 0);
  return segments.reduce((a, g) => a + rangeOut(p, g.start, g.end), 0);
}

/** The lowest and highest speed over the ranges. */
export function speedRange(points: SpeedPoint[], segments: Segment[]): [number, number] {
  const p = clean(points);
  if (!p.length) return [1, 1];
  const vals: number[] = [];
  for (const g of segments) {
    vals.push(speedAt(p, g.start), speedAt(p, g.end));
    for (const q of p) if (q.t > g.start && q.t < g.end) vals.push(q.speed);
  }
  return [Math.min(...vals), Math.max(...vals)];
}

/** Log scale for the lane: 0 at MIN_SPEED, 1 at MAX_SPEED. */
export const speedToY = (s: number) => Math.log(clampSpeed(s) / MIN_SPEED) / Math.log(MAX_SPEED / MIN_SPEED);
export const yToSpeed = (y: number) => clampSpeed(MIN_SPEED * Math.pow(MAX_SPEED / MIN_SPEED, Math.min(1, Math.max(0, y))));

/** A speed rounded for the lane: 2 decimals, and snapped to 1x near it. */
export function niceSpeed(s: number): number {
  const r = Math.round(clampSpeed(s) * 100) / 100;
  return Math.abs(r - 1) < 0.04 ? 1 : r;
}
