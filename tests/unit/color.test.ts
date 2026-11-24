// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { wbGains } from "../../src/lib/color.ts";

const near = (a, b, eps = 1e-4) => assert.ok(Math.abs(a - b) < eps, `${a} != ${b}`);

test("white balance gains match the engine's (plan.rs adjust_and_vflip: rr=0.8963 gg=1.0156 bb=1.1508)", () => {
  const [r, g, b] = wbGains(-0.5, 0);
  near(r, 0.8963);
  near(g, 1.0156);
  near(b, 1.1508);
  assert.deepEqual(wbGains(0, 0), [1, 1, 1]);
});

test("the light level stays the same", () => {
  for (const [t, n] of [[0.4, 0], [-1, 1], [0.2, -0.6]]) {
    const [r, g, b] = wbGains(t, n);
    near(0.2126 * r + 0.7152 * g + 0.0722 * b, 1, 1e-9);
  }
});
