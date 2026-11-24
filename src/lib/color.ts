// White balance as red, green and blue gains, the same numbers as the engine's
// autocolor.rs wb_gains (the encode uses them in colorchannelmixer, the preview in an
// SVG color matrix). Temperature 1: red e^0.25 up and blue e^0.25 down; tint 1: green
// e^-0.3 against red and blue e^0.15. Scaled so the light level (Rec. 709) stays the same.

const TEMP_LOG = 0.25;
const TINT_LOG = 0.15;

export function wbGains(temperature: number, tint: number): [number, number, number] {
  const cl = (v: number) => Math.min(1, Math.max(-1, Number.isFinite(v) ? v : 0));
  const [t, n] = [cl(temperature), cl(tint)];
  const r = Math.exp(TEMP_LOG * t + TINT_LOG * n);
  const g = Math.exp(-2 * TINT_LOG * n);
  const b = Math.exp(-TEMP_LOG * t + TINT_LOG * n);
  const k = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return [r / k, g / k, b / k];
}
