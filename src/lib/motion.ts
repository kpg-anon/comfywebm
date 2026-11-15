// Frame rate after speed and the frame rate cap, and whether it plays evenly
// on a display. A video plays evenly when every frame is shown for the same
// number of refreshes: 30 fps on 60 Hz shows each frame twice, 45 fps on 60 Hz
// shows frames for 1, 1, 2, 1, 1, 2... refreshes, which looks like judder.
// The engine's own math is in crates/engine/src/plan.rs ("src_fps_out", "fps_cap").

/** x is within 1.5% of a whole number >= 1 (29.97 on 60 Hz counts as 2). */
function whole(x: number): boolean {
  if (x < 0.985) return false;
  return Math.abs(x - Math.round(x)) <= 0.015 * Math.round(x);
}

/** A frame rate plays evenly on a display with refresh rate hz. */
export function evenOn(fps: number, hz: number): boolean {
  return fps > 0 && (whole(hz / fps) || whole(fps / hz));
}

export type Motion = {
  /** source fps times speed */
  rate: number;
  /** final frame rate of the file */
  out: number;
  /** the cap is applied: the fps filter turns `rate` into `out` */
  capped: boolean;
  /** the fps filter keeps frames at uneven steps (45 -> 30 keeps 2 of every 3) */
  uneven: boolean;
  /** `out` plays evenly on the display */
  even: boolean;
};

export function motion(srcFps: number, speed: number, cap: number | null, hz: number): Motion {
  const rate = srcFps * speed;
  const capped = cap !== null && cap > 0 && cap < rate - 0.01;
  const out = capped ? (cap as number) : rate;
  const uneven = capped && !whole(rate / out);
  return { rate, out, capped, uneven, even: evenOn(out, hz) };
}

/** A speed's motion is fine: even on the display and no uneven frame dropping. */
export const smooth = (m: Motion) => m.even && !m.uneven;

const round3 = (x: number) => Math.round(x * 1000) / 1000;

/**
 * Speeds from lo to hi whose output plays evenly on the display, for this
 * source and cap. Speed 1 is always included (the source as it is).
 */
export function smoothSpeeds(srcFps: number, cap: number | null, hz: number, lo = 0.25, hi = 4): number[] {
  if (!(srcFps > 0)) return [1];
  const targets = new Set<number>();
  for (let n = 1; n <= 8; n++) if (hz / n >= 10) targets.add(hz / n);
  for (let m = 2; m <= 4; m++) targets.add(hz * m);
  if (cap) for (let k = 1; k <= 6; k++) targets.add(cap * k);
  const out = new Set<number>([1]);
  for (const t of targets) {
    const s = round3(t / srcFps);
    if (s >= lo - 1e-9 && s <= hi + 1e-9 && smooth(motion(srcFps, s, cap, hz))) out.add(s);
  }
  return [...out].sort((a, b) => a - b);
}

/** The nearest smooth speeds below and above `speed` (either may be null). */
export function nearestSmooth(speed: number, list: number[]): [number | null, number | null] {
  let below: number | null = null;
  let above: number | null = null;
  for (const s of list) {
    if (s < speed - 1e-6) below = s;
    else if (s > speed + 1e-6 && above === null) above = s;
  }
  return [below, above];
}

export const fmtFps = (f: number) => `${Math.round(f * 100) / 100} fps`;
export const fmtSpeed = (s: number) => `${Math.round(s * 1000) / 1000}x`;

/**
 * Plain-language problems with the motion of the output, empty when there are
 * none. Speed 1 without a cap is the source as it is and is never reported.
 */
export function motionWarnings(srcFps: number, speed: number, cap: number | null, hz: number): string[] {
  const m = motion(srcFps, speed, cap, hz);
  const w: string[] = [];
  if (m.uneven)
    w.push(
      `The frame rate cap turns ${fmtFps(m.rate)} into ${fmtFps(m.out)}, which keeps frames at uneven steps; motion will stutter.`,
    );
  if (!m.even && (speed !== 1 || m.capped))
    w.push(`The output is ${fmtFps(m.out)}, which does not divide evenly into a ${hz} Hz display; motion will judder.`);
  return w;
}
