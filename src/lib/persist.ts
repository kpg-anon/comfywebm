// localStorage access. Storage can be missing or throw (private mode, cleared
// site data), so every read and write is wrapped and failure is ignored.

const PREFIX = "comfywebm.";

export function load<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    return raw === null ? null : (JSON.parse(raw) as T);
  } catch {
    return null;
  }
}

export function save(key: string, value: unknown) {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    // storage unavailable: the value is simply not remembered
  }
}
