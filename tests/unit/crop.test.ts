// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { ratioOf, resize, draw, move, fitRatio, round, isFull, toView, fromView, viewSize } from "../../src/lib/crop.ts";

test("a crop as shown turned and flipped, and back", () => {
  // a 1920x1080 picture, the crop in its top-left corner
  const c = { x: 0, y: 0, w: 400, h: 200 };
  const v = (rotate, mirror = false, vflip = false) => ({ rotate, mirror, vflip });
  // turned clockwise: the top-left corner goes to the top right of a 1080x1920 picture
  assert.deepEqual(viewSize(1920, 1080, v(90)), [1080, 1920]);
  assert.deepEqual(toView(c, 1920, 1080, v(90)), { x: 880, y: 0, w: 200, h: 400 });
  assert.deepEqual(toView(c, 1920, 1080, v(180)), { x: 1520, y: 880, w: 400, h: 200 });
  assert.deepEqual(toView(c, 1920, 1080, v(270)), { x: 0, y: 1520, w: 200, h: 400 });
  assert.deepEqual(toView(c, 1920, 1080, v(0, true)), { x: 1520, y: 0, w: 400, h: 200 });
  assert.deepEqual(toView(c, 1920, 1080, v(90, true)), { x: 0, y: 0, w: 200, h: 400 });
  // every turn and flip goes back to the same crop
  const d = { x: 123, y: 45, w: 678, h: 321 };
  for (const r of [0, 90, 180, 270])
    for (const m of [false, true])
      for (const f of [false, true]) assert.deepEqual(fromView(toView(d, 1920, 1080, v(r, m, f)), 1920, 1080, v(r, m, f)), d, `${r} ${m} ${f}`);
});

test("ratioOf parses W:H, WxH and plain numbers", () => {
  assert.equal(ratioOf("16:9"), 16 / 9);
  assert.equal(ratioOf(" 2.39 : 1 "), 2.39);
  assert.equal(ratioOf("4x3"), 4 / 3);
  assert.equal(ratioOf("1.85"), 1.85);
});

test("ratioOf rejects free, junk, zero and extreme ratios", () => {
  for (const s of ["free", "", "abc", "16:", "0:1", "1:0", "1:2:3", "100:1"]) assert.equal(ratioOf(s), null, s);
});

const sq = { x: 100, y: 100, w: 100, h: 100 };

test("resize without a ratio moves only the dragged edges and flips past the fixed edge", () => {
  assert.deepEqual(resize(sq, "e", 50, 0, null, 1000, 1000), { x: 100, y: 100, w: 150, h: 100 });
  assert.deepEqual(resize(sq, "nw", -20, -30, null, 1000, 1000), { x: 80, y: 70, w: 120, h: 130 });
  assert.deepEqual(resize(sq, "e", -150, 0, null, 1000, 1000), { x: 50, y: 100, w: 50, h: 100 });
  // clamped to the picture
  assert.deepEqual(resize(sq, "se", 5000, 5000, null, 300, 250), { x: 100, y: 100, w: 200, h: 150 });
});

test("resize with a ratio keeps it and scales toward the fixed corner to fit", () => {
  assert.deepEqual(resize(sq, "se", 50, 0, 1, 1000, 1000), { x: 100, y: 100, w: 150, h: 150 });
  // grows to 400x400, then is scaled to fit H = 300 with (100,100) fixed
  assert.deepEqual(resize(sq, "se", 300, 0, 1, 1000, 300), { x: 100, y: 100, w: 200, h: 200 });
  // edge handle: the other axis is centred on the old centre (150, 150)
  assert.deepEqual(resize(sq, "e", 100, 0, 2, 1000, 1000), { x: 100, y: 100, w: 200, h: 100 });
  assert.deepEqual(resize(sq, "s", 0, 100, 0.5, 1000, 1000), { x: 100, y: 100, w: 100, h: 200 });
  assert.deepEqual(resize(sq, "e", 300, 0, 2, 1000, 1000), { x: 100, y: 50, w: 400, h: 200 });
  const r = resize(sq, "e", 233, 0, 16 / 9, 1920, 1080);
  assert.ok(Math.abs(r.w / r.h - 16 / 9) < 0.02, JSON.stringify(r));
});

test("draw picks the handle from the drag direction", () => {
  assert.deepEqual(draw(100, 100, -40, -30, null, 1000, 1000), { x: 60, y: 70, w: 40, h: 30 });
  assert.deepEqual(draw(100, 100, 0, 0, null, 1000, 1000), { x: 100, y: 100, w: 1, h: 1 });
  assert.deepEqual(draw(0, 0, 160, 10, 16 / 9, 1920, 1080), { x: 0, y: 0, w: 160, h: 90 });
});

test("move keeps the rectangle inside the picture", () => {
  assert.deepEqual(move(sq, 1000, -1000, 400, 300), { x: 300, y: 0, w: 100, h: 100 });
  assert.deepEqual(move(sq, 10.4, 10.6, 400, 300), { x: 110, y: 111, w: 100, h: 100 });
});

test("fitRatio centres the largest rectangle of that ratio", () => {
  assert.deepEqual(fitRatio({ x: 0, y: 0, w: 1920, h: 1080 }, 1, 1920, 1080), { x: 420, y: 0, w: 1080, h: 1080 });
  // 1080 / (16/9) = 607.5, centred: y = (1920 - 607.5) / 2 = 656.25
  assert.deepEqual(fitRatio({ x: 0, y: 0, w: 1080, h: 1920 }, 16 / 9, 1080, 1920), { x: 0, y: 656, w: 1080, h: 608 });
});

test("round clamps to the picture with a 1x1 minimum; isFull", () => {
  assert.deepEqual(round({ x: -5, y: 2.6, w: 0.2, h: 500 }, 100, 100), { x: 0, y: 3, w: 1, h: 97 });
  assert.deepEqual(round({ x: 150, y: 0, w: 10, h: 10 }, 100, 100), { x: 99, y: 0, w: 1, h: 10 });
  assert.equal(isFull({ x: 0, y: 0, w: 1920, h: 1080 }, 1920, 1080), true);
  assert.equal(isFull({ x: 0, y: 0, w: 1918, h: 1080 }, 1920, 1080), false);
});
