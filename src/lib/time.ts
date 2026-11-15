// Time and size formatting.

/** Seconds -> "h:mm:ss.mmm"; the hour part is left out when it is 0 ("m:ss.mmm"). */
export function fmtTime(secs: number, alwaysHours = false): string {
  if (!Number.isFinite(secs) || secs < 0) secs = 0;
  const ms = Math.round(secs * 1000);
  const h = Math.floor(ms / 3600000);
  const m = Math.floor((ms % 3600000) / 60000);
  const s = Math.floor((ms % 60000) / 1000);
  const frac = String(ms % 1000).padStart(3, "0");
  const ss = String(s).padStart(2, "0");
  if (h > 0 || alwaysHours) return `${h}:${String(m).padStart(2, "0")}:${ss}.${frac}`;
  return `${m}:${ss}.${frac}`;
}

/**
 * Parse "h:mm:ss.mmm", "m:ss.mmm", "ss.mmm" or plain seconds. Returns null for
 * text that is not a time.
 */
export function parseTime(text: string): number | null {
  const t = text.trim();
  if (t === "") return null;
  const parts = t.split(":");
  if (parts.length > 3) return null;
  let total = 0;
  for (let i = 0; i < parts.length; i++) {
    const p = parts[i].trim();
    // only the last part may have a fraction
    const re = i === parts.length - 1 ? /^\d+(\.\d*)?$|^\.\d+$/ : /^\d+$/;
    if (!re.test(p)) return null;
    total = total * 60 + Number(p);
  }
  return Number.isFinite(total) ? total : null;
}

/** Short duration for ETA readouts: "42s", "3m 05s", "1h 02m". */
export function fmtEta(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, "0")}s`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m`;
}

/** Bytes -> "3.98 MiB" (binary units, matching the size-limit field). */
export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1048576) return `${(n / 1024).toFixed(1)} KiB`;
  if (n < 1073741824) return `${(n / 1048576).toFixed(2)} MiB`;
  return `${(n / 1073741824).toFixed(2)} GiB`;
}

/** 1 MiB in bytes; the size-limit field uses this unit. */
export const MB = 1048576;
