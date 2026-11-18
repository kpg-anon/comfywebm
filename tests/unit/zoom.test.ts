// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { at, clean, windowOf } from "../../src/lib/zoom.ts";

const pt = (t, zoom, x, y) => ({ t, zoom, x, y });

test("eases between points and holds outside, as zoom.rs", () => {
  const p = [pt(2, 1, 0.5, 0.5), pt(4, 3, 0.2, 0.8)];
  assert.deepEqual(at(p, 0), { zoom: 1, x: 0.5, y: 0.5 });
  assert.deepEqual(at(p, 9), { zoom: 3, x: 0.2, y: 0.8 });
  const m = at(p, 3);
  assert.ok(Math.abs(m.zoom - 2) < 1e-9 && Math.abs(m.x - 0.35) < 1e-9);
  assert.ok(at(p, 2.5).zoom < 1.5);
  assert.deepEqual(clean([pt(5, 9, 2, -1), pt(1, 0.5, 0.5, 0.5)]), [pt(1, 1, 0.5, 0.5), pt(5, 4, 1, 0)]);
  assert.deepEqual(at([], 3), { zoom: 1, x: 0.5, y: 0.5 });
});

test("the window stays inside the picture", () => {
  assert.deepEqual(windowOf({ zoom: 2, x: 0.5, y: 0.5 }, 640, 360), { x: 160, y: 90, w: 320, h: 180 });
  assert.deepEqual(windowOf({ zoom: 2, x: 0, y: 1 }, 640, 360), { x: 0, y: 180, w: 320, h: 180 });
  assert.deepEqual(windowOf({ zoom: 1, x: 0.2, y: 0.9 }, 640, 360), { x: 0, y: 0, w: 640, h: 360 });
});
