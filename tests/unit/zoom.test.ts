// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { at, camera, clean, simplify, windowOf } from "../../src/lib/zoom.ts";

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

test("passes through a middle point without stopping, holds a turning point, no overshoot (as zoom.rs)", () => {
  let p = [pt(0, 2, 0, 0.5), pt(1, 2, 0.5, 0.5), pt(2, 2, 1, 0.5)];
  assert.ok(Math.abs(at(p, 1).x - 0.5) < 1e-9);
  assert.ok(at(p, 1.1).x > 0.55);
  p = [pt(0, 2, 0, 0.5), pt(1, 2, 0.8, 0.5), pt(2, 2, 0.2, 0.5)];
  for (let k = 0; k <= 20; k++) assert.ok(at(p, k / 10).x <= 0.8 + 1e-9);
});

test("simplify keeps few keyframes within the tolerance of a dense path", () => {
  const dense = Array.from({ length: 301 }, (_, i) => {
    const t = i / 30;
    return pt(t, 1, 0.5 + 0.3 * Math.sin(t), 0.5 + 0.1 * Math.cos(t / 2));
  });
  const keys = simplify(dense, 0.01);
  assert.ok(keys.length >= 3 && keys.length < 30, `${keys.length} keys`);
  assert.equal(keys[0].t, 0);
  assert.equal(keys[keys.length - 1].t, 10);
  for (const q of dense) {
    const c = at(keys, q.t);
    assert.ok(Math.hypot(c.x - q.x, c.y - q.y) <= 0.01 + 1e-9);
  }
});

test("camera holds still while the subject bobs inside the band, and keeps a travelling one in it", () => {
  // bobbing 0.03 around 0.5, band 0.06: the camera does not move
  const bob = Array.from({ length: 241 }, (_, i) => pt(i / 60, 1, 0.5 + 0.03 * Math.sin(i / 4), 0.5));
  const c = camera(bob, 0.06, 0.06);
  const xs = c.map((q) => q.x);
  assert.ok(Math.max(...xs) - Math.min(...xs) < 0.002, `moved ${Math.max(...xs) - Math.min(...xs)}`);
  // travelling from 0.2 to 0.8 with a bob: the camera follows and the subject stays within the band
  const go = Array.from({ length: 481 }, (_, i) => pt(i / 60, 1, 0.2 + (0.6 * i) / 480 + 0.02 * Math.sin(i / 3), 0.5));
  const keys = camera(go, 0.05, 0.05);
  for (const q of go) assert.ok(Math.abs(at(keys, q.t).x - q.x) <= 0.05 + 1e-3, `t ${q.t}`);
  assert.ok(Math.abs(at(keys, 8).x - 0.8) < 0.05);
});

test("the window stays inside the picture", () => {
  assert.deepEqual(windowOf({ zoom: 2, x: 0.5, y: 0.5 }, 640, 360), { x: 160, y: 90, w: 320, h: 180 });
  assert.deepEqual(windowOf({ zoom: 2, x: 0, y: 1 }, 640, 360), { x: 0, y: 180, w: 320, h: 180 });
  assert.deepEqual(windowOf({ zoom: 1, x: 0.2, y: 0.9 }, 640, 360), { x: 0, y: 0, w: 640, h: 360 });
});
