// Range (segment) editing on the source timeline. Segments are kept sorted by
// start and never overlap; each function returns a new list and the index of
// the range it touched.
import type { Segment } from "./types";

export type Edit = { list: Segment[]; index: number };

const sorted = (list: Segment[], touched: Segment): Edit => {
  const out = [...list].sort((a, b) => a.start - b.start);
  return { list: out, index: out.indexOf(touched) };
};

/** Index of the range that contains t, or -1. */
export function rangeAt(list: Segment[], t: number): number {
  return list.findIndex((s) => t >= s.start && t <= s.end);
}

/**
 * `I` at time t: the range containing t gets start = t. Otherwise the nearest
 * range to the right is extended back to t. With no range to the right, a new
 * range from t to the end of the file is added.
 */
export function setIn(list: Segment[], t: number, duration: number, minLen: number): Edit {
  let i = rangeAt(list, t);
  if (i < 0) i = list.findIndex((s) => s.start > t);
  if (i < 0) {
    if (duration - t < minLen) return { list, index: -1 };
    const s = { start: t, end: duration };
    return sorted([...list, s], s);
  }
  const s = { ...list[i], start: Math.min(t, list[i].end - minLen) };
  const out = [...list];
  out[i] = s;
  return sorted(out, s);
}

/**
 * `O` at time t: the range containing t gets end = t. Otherwise the nearest
 * range to the left is extended to t. With no range to the left, a new range
 * from 0 to t is added.
 */
export function setOut(list: Segment[], t: number, minLen: number): Edit {
  let i = rangeAt(list, t);
  if (i < 0) {
    for (let k = list.length - 1; k >= 0; k--)
      if (list[k].end < t) {
        i = k;
        break;
      }
  }
  if (i < 0) {
    if (t < minLen) return { list, index: -1 };
    const s = { start: 0, end: t };
    return sorted([...list, s], s);
  }
  const s = { ...list[i], end: Math.max(t, list[i].start + minLen) };
  const out = [...list];
  out[i] = s;
  return sorted(out, s);
}

/** `N`: a new range from t, up to `len` long, stopping at the next range or the end. */
export function addAt(list: Segment[], t: number, duration: number, len: number, minLen: number): Edit | null {
  if (rangeAt(list, t) >= 0) return null;
  const next = list.filter((s) => s.start > t).reduce((m, s) => Math.min(m, s.start), duration);
  const end = Math.min(next, t + len);
  if (end - t < minLen) return null;
  const s = { start: t, end };
  return sorted([...list, s], s);
}

/** Limits for the start and end of range i, from its neighbors. */
export function bounds(list: Segment[], i: number, duration: number) {
  return {
    min: i > 0 ? list[i - 1].end : 0,
    max: i < list.length - 1 ? list[i + 1].start : duration,
  };
}

/** Set start or end of range i, clamped between its neighbors and at least minLen long. */
export function setEdge(list: Segment[], i: number, edge: "start" | "end", t: number, duration: number, minLen: number): Segment[] {
  const b = bounds(list, i, duration);
  const s = { ...list[i] };
  if (edge === "start") s.start = Math.min(Math.max(t, b.min), s.end - minLen);
  else s.end = Math.max(Math.min(t, b.max), s.start + minLen);
  const out = [...list];
  out[i] = s;
  return out;
}

export const totalLength = (list: Segment[]) => list.reduce((n, s) => n + (s.end - s.start), 0);
