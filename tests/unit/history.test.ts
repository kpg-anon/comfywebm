// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { describeEdit } from "../../src/lib/history.ts";

const base = () => ({
  segments: [
    { start: 2, end: 4, id: 1 },
    { start: 6, end: 8, id: 2 },
  ],
  order: null,
  crop: null,
  rotate: 0,
  mirror: false,
  vflip: false,
  adjust: { brightness: 0, contrast: 1, saturation: 1, gamma: 1, hue: 0, curve: [] },
  speedRamp: [],
});

test("ranges: added, removed, moved, in and out", () => {
  const a = base();
  assert.equal(describeEdit(a, { ...a, segments: [...a.segments, { start: 10, end: 15, id: 3 }] }), "Added range 3 (10.0 s to 15.0 s)");
  assert.equal(describeEdit(a, { ...a, segments: [a.segments[1]] }), "Removed range 1");
  assert.equal(describeEdit(a, { ...a, segments: [] }), "Removed all ranges");
  assert.equal(describeEdit(a, { ...a, segments: [a.segments[0], { start: 7, end: 9, id: 2 }] }), "Moved range 2 to 7.0 s");
  assert.equal(describeEdit(a, { ...a, segments: [{ start: 1.5, end: 4, id: 1 }, a.segments[1]] }), "Set In of range 1 to 1.5 s");
  assert.equal(describeEdit(a, { ...a, segments: [a.segments[0], { start: 6, end: 65.25, id: 2 }] }), "Set Out of range 2 to 1:05.3");
});

test("picture changes", () => {
  const a = base();
  assert.equal(describeEdit(a, { ...a, crop: { x: 0, y: 0, w: 960, h: 540 } }), "Crop 960x540");
  assert.equal(describeEdit({ ...a, crop: { x: 0, y: 0, w: 960, h: 540 } }, a), "Removed the crop");
  assert.equal(describeEdit(a, { ...a, rotate: 90 }), "Rotated to 90°");
  assert.equal(describeEdit(a, { ...a, mirror: true }), "Flipped horizontally");
  assert.equal(describeEdit(a, { ...a, adjust: { ...a.adjust, saturation: 1.25 } }), "Saturation 1.25");
  assert.equal(describeEdit(a, { ...a, speedRamp: [{ t: 1, speed: 2 }] }), "Added a speed point");
  // two at once, and more than two
  assert.equal(describeEdit(a, { ...a, rotate: 90, mirror: true }), "Rotated to 90°, Flipped horizontally");
  assert.equal(describeEdit(a, { ...a, rotate: 90, mirror: true, vflip: true }), "Rotated to 90°, Flipped horizontally and more");
  assert.equal(describeEdit(a, a), "Edit");
  // the zoom and the follow path
  assert.equal(describeEdit({ ...a, zoomLevel: 1 }, { ...a, zoomLevel: 2 }), "Zoom 2x");
  assert.equal(describeEdit(a, { ...a, zoom: [{ t: 1, zoom: 1, x: 0.5, y: 0.5 }] }), "Followed a person");
  assert.equal(describeEdit({ ...a, zoom: [{ t: 1, zoom: 1, x: 0.5, y: 0.5 }] }, { ...a, zoom: [] }), "Removed the follow path");
});
