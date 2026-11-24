// Rotation keyframes, as the engine's rotation.rs works them out: before the
// first keyframe the angle is the Rotate setting; at each keyframe it becomes
// that keyframe's angle, at once, or (smooth) turning from the previous one
// over the time between them, eased, the shorter way round. The frame has the
// picture's shape at the first output frame (the nearest quarter turn); a
// picture at another angle fits in it whole (black around it) or covers it.
import type { RotatePoint } from "./types";

/** An angle in (-180, 180]. */
export function norm(a: number): number {
  const r = ((a % 360) + 360) % 360;
  return r > 180 ? r - 360 : r;
}

/** Keyframes sorted by time, angles in (-180, 180], no two at the same time. */
export function clean(points: RotatePoint[]): RotatePoint[] {
  const p = points
    .filter((q) => Number.isFinite(q.t) && Number.isFinite(q.angle))
    .map((q) => ({ t: Math.max(0, q.t), angle: norm(q.angle), smooth: !!q.smooth }))
    .sort((a, b) => a.t - b.t);
  return p.filter((q, i) => i === 0 || Math.abs(q.t - p[i - 1].t) >= 1e-6);
}

/** Whether the keyframes turn the picture away from the Rotate setting at all. */
export function active(base: number, points: RotatePoint[]): boolean {
  const b = norm(base);
  return clean(points).some((p) => Math.abs(p.angle - b) > 1e-6);
}

/** The keyframes' angles, each the previous one plus the shorter way to its own (`b` is the
 *  setting's angle, `k` the keyframes in time order). */
export function keys(base: number, points: RotatePoint[]): { b: number; k: { t: number; a: number; smooth: boolean }[] } {
  const b = norm(base);
  let prev = b;
  const k = clean(points).map((p) => {
    prev += norm(p.angle - prev);
    return { t: p.t, a: prev, smooth: p.smooth };
  });
  return { b, k };
}

const ease = (u: number) => {
  const x = Math.min(1, Math.max(0, u));
  return x * x * (3 - 2 * x);
};

/** The angle in degrees at source time t (not wrapped to +-180). */
export function angleAt(base: number, points: RotatePoint[], t: number): number {
  const { b, k } = keys(base, points);
  if (!k.length || t < k[0].t) return b;
  for (let i = 0; i < k.length; i++) {
    const n = k[i + 1];
    if (!n) return k[i].a;
    if (t < n.t) return n.smooth ? k[i].a + (n.a - k[i].a) * ease((t - k[i].t) / Math.max(1e-6, n.t - k[i].t)) : k[i].a;
  }
  return b;
}

/** The nearest quarter turn: 0, 90, 180 or 270. */
export function quarter(a: number): 0 | 90 | 180 | 270 {
  return ((((Math.round(a / 90) % 4) + 4) % 4) * 90) as 0 | 90 | 180 | 270;
}

/** The time a frame's angle is taken at, for the frame holding source time t: its end
 *  (keyframes sit in the middle of a frame, so a keyframe on a frame turns that frame). */
export function frameTime(t: number, fps: number): number {
  return fps > 0 ? (Math.floor(t * fps + 1e-4) + 1) / fps : t;
}

/** The angle of the frame holding source time t. */
export function frameAngle(base: number, points: RotatePoint[], t: number, fps: number): number {
  return angleAt(base, points, frameTime(t, fps));
}

/** The width of the picture area the frame shows at `deg`, in picture pixels, for a pw x ph
 *  picture in a frame r wide for 1 high: all of the turned picture (fit), or the largest
 *  area inside it (fill). */
export function windowW(pw: number, ph: number, r: number, deg: number, fill: boolean): number {
  const a = (deg * Math.PI) / 180;
  const [c, s] = [Math.abs(Math.cos(a)), Math.abs(Math.sin(a))];
  if (fill) return r * Math.min(pw / (r * c + s), ph / (r * s + c));
  return Math.max(pw * c + ph * s, r * (pw * s + ph * c));
}

/** A keyframe time in the middle of its frame, where the player shows that frame. */
export function onFrame(t: number, fps: number): number {
  if (!(fps > 0)) return t;
  return Math.round(((Math.floor(t * fps + 1e-4) + 0.5) / fps) * 1e6) / 1e6;
}
