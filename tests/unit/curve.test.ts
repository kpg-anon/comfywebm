// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { normalize, sample, isIdentity, IDENTITY, CURVE_PRESETS } from "../../src/lib/curve.ts";

const near = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) < eps, `${a} != ${b}`);

test("normalize sorts, clamps, drops near-duplicate x and adds missing ends", () => {
  assert.deepEqual(normalize([[0.5, 0.4]]), [[0, 0], [0.5, 0.4], [1, 1]]);
  assert.deepEqual(normalize([[1.5, -1], [0.3, 0.2], [0.30001, 0.9], [NaN, 0]]), [[0, 0], [0.3, 0.2], [1, 0]]);
  assert.deepEqual(normalize([]), IDENTITY);
  assert.deepEqual(normalize([[0, 0.06], [1, 0.94]]), [[0, 0.06], [1, 0.94]]);
});

test("two points sample as a straight line", () => {
  assert.deepEqual(sample(IDENTITY, 5), [0, 0.25, 0.5, 0.75, 1]);
  near(sample([[0, 0.1], [1, 0.9]], 3)[1], 0.5);
});

test("the spline passes through its points and matches a natural cubic spline between them", () => {
  const s = sample([[0, 0], [0.5, 0.4], [1, 1]], 5);
  near(s[0], 0);
  near(s[2], 0.4);
  near(s[4], 1);
  // one interior knot, h = 0.5: M1 = 6 * (0.6/0.5 - 0.4/0.5) / (2 * (0.5 + 0.5)) = 1.2, M0 = M2 = 0
  // y(0.25) = M1 * 0.25^3 / (6 * 0.5) + (0.4/0.5 - M1 * 0.5/6) * 0.25 = 0.00625 + 0.175
  near(s[1], 0.18125);
  // y(0.75) = M1 * 0.25^3 / (6 * 0.5) + (0.4/0.5 - M1 * 0.5/6) * 0.25 + (1/0.5) * 0.25 = 0.00625 + 0.175 + 0.5
  near(s[3], 0.68125);
});

test("preset curves stay in 0..1 and do not decrease", () => {
  for (const { name, points } of CURVE_PRESETS) {
    const s = sample(points, 64);
    assert.equal(s.length, 64);
    assert.ok(s.every((v) => v >= 0 && v <= 1), name);
    for (let i = 1; i < s.length; i++) assert.ok(s[i] >= s[i - 1] - 1e-9, `${name} at ${i}`);
  }
});

test("isIdentity", () => {
  assert.equal(isIdentity(IDENTITY), true);
  assert.equal(isIdentity([[0, 0], [0.5, 0.50001], [1, 1]]), true);
  assert.equal(isIdentity([[0, 0], [0.5, 0.4], [1, 1]]), false);
});
