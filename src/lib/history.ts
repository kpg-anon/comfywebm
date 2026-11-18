// Names for the steps of the History panel: what changed from one edit state
// to the next (ranges, order, crop, rotation, flips, adjustments, speed ramp).
import type { Adjust, Crop, Segment, SpeedPoint, ZoomPoint } from "./types";

/** The parts of an edit state a step can change. */
export type EditState = {
  segments: Segment[];
  order: number[] | null;
  crop: Crop | null;
  rotate: number;
  mirror: boolean;
  vflip: boolean;
  adjust: Adjust;
  speedRamp: SpeedPoint[];
  zoom?: ZoomPoint[];
};

/** "1:05.2", "12.5 s" */
function t(s: number): string {
  if (s < 60) return `${(Math.round(s * 10) / 10).toFixed(1)} s`;
  const m = Math.floor(s / 60);
  const r = s - m * 60;
  return `${m}:${(Math.round(r * 10) / 10).toFixed(1).padStart(4, "0")}`;
}

const near = (a: number, b: number) => Math.abs(a - b) < 0.0005;

function rangeChange(a: Segment[], b: Segment[]): string | null {
  if (b.length > a.length) {
    const i = b.findIndex((s) => !a.some((x) => x.id === s.id));
    const k = i >= 0 ? i : b.length - 1;
    return `Added range ${k + 1} (${t(b[k].start)} to ${t(b[k].end)})`;
  }
  if (b.length < a.length) {
    const i = a.findIndex((s) => !b.some((x) => x.id === s.id));
    return b.length === 0 ? "Removed all ranges" : `Removed range ${(i >= 0 ? i : a.length - 1) + 1}`;
  }
  for (let i = 0; i < a.length; i++) {
    const x = a[i];
    const y = b[i];
    const ds = !near(x.start, y.start);
    const de = !near(x.end, y.end);
    if (!ds && !de) continue;
    if (ds && de && near(y.start - x.start, y.end - x.end)) return `Moved range ${i + 1} to ${t(y.start)}`;
    if (ds && de) return `Changed range ${i + 1} to ${t(y.start)} to ${t(y.end)}`;
    return ds ? `Set In of range ${i + 1} to ${t(y.start)}` : `Set Out of range ${i + 1} to ${t(y.end)}`;
  }
  return null;
}

const ADJUST_NAMES: [keyof Adjust, string][] = [
  ["brightness", "Brightness"],
  ["contrast", "Contrast"],
  ["saturation", "Saturation"],
  ["gamma", "Gamma"],
  ["hue", "Hue"],
];

function adjustChange(a: Adjust, b: Adjust): string | null {
  for (const [k, name] of ADJUST_NAMES) {
    if (a[k] !== b[k]) return `${name} ${typeof b[k] === "number" ? Math.round((b[k] as number) * 100) / 100 : ""}`.trim();
  }
  if (JSON.stringify(a.curve) !== JSON.stringify(b.curve)) return b.curve.length ? "Changed the curve" : "Reset the curve";
  return null;
}

/** What changed from `a` to `b`, as a short phrase ("Moved range 2 to 12.5 s", "Crop 960x540"). */
export function describeEdit(a: EditState, b: EditState): string {
  const parts: string[] = [];
  const r = rangeChange(a.segments, b.segments);
  if (r) parts.push(r);
  else if (JSON.stringify(a.order) !== JSON.stringify(b.order)) parts.push(b.order ? "Changed the order of the ranges" : "Reset the order of the ranges");
  if (JSON.stringify(a.crop) !== JSON.stringify(b.crop)) parts.push(b.crop ? `Crop ${Math.round(b.crop.w)}x${Math.round(b.crop.h)}` : "Removed the crop");
  if (a.rotate !== b.rotate) parts.push(b.rotate ? `Rotated to ${b.rotate}°` : "Rotation off");
  if (a.mirror !== b.mirror) parts.push(b.mirror ? "Flipped horizontally" : "Horizontal flip off");
  if (a.vflip !== b.vflip) parts.push(b.vflip ? "Flipped vertically" : "Vertical flip off");
  if (JSON.stringify(a.adjust) !== JSON.stringify(b.adjust)) parts.push(adjustChange(a.adjust, b.adjust) ?? "Changed the adjustments");
  if (JSON.stringify(a.speedRamp) !== JSON.stringify(b.speedRamp)) {
    const n = b.speedRamp.length - a.speedRamp.length;
    parts.push(
      n > 0 ? "Added a speed point" : n < 0 ? (b.speedRamp.length ? "Removed a speed point" : "Removed the speed ramp") : "Moved a speed point",
    );
  }
  const za = a.zoom ?? [];
  const zb = b.zoom ?? [];
  if (JSON.stringify(za) !== JSON.stringify(zb)) {
    const n = zb.length - za.length;
    if (n > 0) parts.push("Added a zoom point");
    else if (n < 0) parts.push(zb.length ? "Removed a zoom point" : "Removed the zoom");
    else {
      const i = zb.findIndex((q, k) => JSON.stringify(q) !== JSON.stringify(za[k]));
      const q = zb[i];
      const o = za[i];
      parts.push(q && o && q.zoom !== o.zoom ? `Zoom ${Math.round(q.zoom * 100) / 100}x` : q && o && q.t !== o.t ? "Moved a zoom point" : "Moved the zoom's centre");
    }
  }
  if (!parts.length) return "Edit";
  return parts.length > 2 ? `${parts.slice(0, 2).join(", ")} and more` : parts.join(", ");
}
