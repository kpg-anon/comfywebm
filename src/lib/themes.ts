// Theme presets, and the Custom palette. Each preset id other than
// comfy-dark maps to a :root[data-theme="id"] block in theme.css. Custom sets
// the tokens as inline styles on <html>: a few base colors are chosen, the
// rest are mixed from them. The choice is a per-machine UI preference, kept in
// localStorage and applied before the first paint (+layout.svelte).
import { load, save } from "./persist";

export interface ThemePreset {
  id: string;
  name: string;
  what: string;
  /** panel, accent, primary, range: drawn in the picker */
  swatches: string[];
}

export const THEMES: ThemePreset[] = [
  { id: "comfy-dark", name: "Comfy Dark", what: "The family look: dense, flat, quiet", swatches: ["#141920", "#c287ff", "#82d65a", "#e2b84f"] },
  { id: "nord", name: "Nord Frost", what: "Rounder, softer, title-case headings", swatches: ["#2e3440", "#88c0d0", "#a3be8c", "#ebcb8b"] },
  { id: "gruvbox", name: "Gruvbox Retro", what: "Square, thick borders, beveled buttons", swatches: ["#282828", "#fabd2f", "#fe8019", "#83a598"] },
  { id: "catppuccin", name: "Catppuccin Mocha", what: "Pill controls, pastel gradients, glow", swatches: ["#1e1e2e", "#cba6f7", "#f5c2e7", "#89b4fa"] },
  { id: "phosphor", name: "Phosphor", what: "A green terminal: monospace, bracketed buttons", swatches: ["#000000", "#b6ff3d", "#6dff6d", "#ffd23f"] },
];

export const DEFAULT_THEME = "comfy-dark";
export const CUSTOM = "custom";

/** The colors a custom palette sets, with their labels in the editor. */
export const CUSTOM_KEYS = [
  ["bg-app", "Window background"],
  ["bg-panel", "Panels"],
  ["bg-input", "Text fields"],
  ["border", "Borders"],
  ["text-primary", "Text"],
  ["text-muted", "Quiet text"],
  ["accent", "Accent (selected, focus)"],
  ["primary", "Encode button"],
  ["range", "Timeline ranges"],
  ["playhead", "Playhead"],
  ["green", "Good"],
  ["yellow", "Warning"],
  ["red", "Error"],
] as const;

export type CustomKey = (typeof CUSTOM_KEYS)[number][0];

export type CustomTheme = {
  name: string;
  light: boolean;
  /** corner radius in px, 0-16 */
  radius: number;
  /** monospace for all text */
  mono: boolean;
  colors: Record<CustomKey, string>;
};

export const DEFAULT_CUSTOM: CustomTheme = {
  name: "My theme",
  light: false,
  radius: 7,
  mono: false,
  colors: {
    "bg-app": "#111418",
    "bg-panel": "#141920",
    "bg-input": "#0e1115",
    border: "#2b313a",
    "text-primary": "#d8dde7",
    "text-muted": "#7f8794",
    accent: "#c287ff",
    primary: "#82d65a",
    range: "#e2b84f",
    playhead: "#57c7e3",
    green: "#82d65a",
    yellow: "#e2b84f",
    red: "#ef6b6b",
  },
};

const HEX = /^#[0-9a-f]{6}$/i;

/** "#abc" or "#aabbcc" -> "#aabbcc"; null for anything else. */
export function hex6(v: string): string | null {
  const t = v.trim();
  if (/^#[0-9a-f]{3}$/i.test(t)) return `#${[...t.slice(1)].map((c) => c + c).join("")}`.toLowerCase();
  return HEX.test(t) ? t.toLowerCase() : null;
}

/** Relative luminance (WCAG) of a #rrggbb color. */
function luminance(hex: string): number {
  const n = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * n[0] + 0.7152 * n[1] + 0.0722 * n[2];
}

/** Black or white text, whichever reads better on `bg`. */
const onColor = (bg: string) => (luminance(bg) > 0.35 ? "#101214" : "#ffffff");

/** Every token the custom palette sets, base colors and mixes. */
function customTokens(t: CustomTheme): Record<string, string> {
  const c = t.colors;
  const mix = (a: string, pa: number, b: string) => `color-mix(in srgb, ${a} ${pa}%, ${b})`;
  const r = Math.min(16, Math.max(0, Math.round(t.radius)));
  return {
    ...Object.fromEntries(CUSTOM_KEYS.map(([k]) => [`--${k}`, c[k]])),
    "--bg-panel-alt": mix(c["bg-panel"], 94, c["text-primary"]),
    "--bg-chip": mix(c["bg-panel"], 90, c["text-primary"]),
    "--bg-hover": mix(c["bg-panel"], 82, c["text-primary"]),
    "--bg-stage": t.light ? "#1b1b1b" : mix(c["bg-app"], 50, "#000"),
    "--border-muted": mix(c.border, 55, c["bg-panel"]),
    "--header-bg": c["bg-panel"],
    "--sidebar-bg": c["bg-panel"],
    "--text-secondary": mix(c["text-primary"], 70, c["text-muted"]),
    "--text-inverse": onColor(c.accent),
    "--accent-strong": mix(c.accent, 80, c["text-primary"]),
    "--accent-bg": mix(c.accent, 22, c["bg-panel"]),
    "--primary-strong": mix(c.primary, 85, c["text-primary"]),
    "--primary-text": onColor(c.primary),
    "--primary-fill": c.primary,
    "--orange": mix(c.yellow, 50, c.red),
    "--cyan": c.playhead,
    "--radius": `${r}px`,
    "--radius-sm": `${Math.max(0, r - 3)}px`,
    "--radius-btn": `${r}px`,
    "--radius-seg": `${r}px`,
    "--radius-dialog": `${r + 4}px`,
    ...(t.mono ? { "--sans": "var(--mono)", "--font-head": "var(--mono)" } : {}),
  };
}

let applied: string[] = [];

function clearCustom() {
  const root = document.documentElement;
  for (const k of applied) root.style.removeProperty(k);
  root.style.removeProperty("color-scheme");
  applied = [];
}

export function storedCustom(): CustomTheme {
  const t = load<unknown>("customTheme");
  return parseCustom(t) ?? structuredClone(DEFAULT_CUSTOM);
}

export function saveCustom(t: CustomTheme) {
  save("customTheme", t);
}

/** A custom theme from parsed JSON (an export file or localStorage), or null when it is not one. */
export function parseCustom(v: unknown): CustomTheme | null {
  if (!v || typeof v !== "object") return null;
  const o = v as Record<string, unknown>;
  const src = (o.colors && typeof o.colors === "object" ? o.colors : null) as Record<string, unknown> | null;
  if (!src) return null;
  const colors = { ...DEFAULT_CUSTOM.colors };
  for (const [k] of CUSTOM_KEYS) {
    const h = typeof src[k] === "string" ? hex6(src[k] as string) : null;
    if (h) colors[k] = h;
  }
  return {
    name: typeof o.name === "string" && o.name.trim() ? o.name.trim().slice(0, 60) : DEFAULT_CUSTOM.name,
    light: o.light === true,
    radius: typeof o.radius === "number" && Number.isFinite(o.radius) ? Math.min(16, Math.max(0, o.radius)) : DEFAULT_CUSTOM.radius,
    mono: o.mono === true,
    colors,
  };
}

/** The base colors of a preset theme, read from theme.css (for "Start from"). */
export function presetColors(id: string): Record<CustomKey, string> {
  const root = document.documentElement;
  const before = root.dataset.theme;
  const saved = applied.map((k) => [k, root.style.getPropertyValue(k)] as const);
  clearCustom();
  if (id === DEFAULT_THEME) delete root.dataset.theme;
  else root.dataset.theme = id;
  const cs = getComputedStyle(root);
  const out = { ...DEFAULT_CUSTOM.colors };
  for (const [k] of CUSTOM_KEYS) out[k] = hex6(cs.getPropertyValue(`--${k}`)) ?? out[k];
  // put back what was there
  if (before === undefined) delete root.dataset.theme;
  else root.dataset.theme = before;
  for (const [k, v] of saved) root.style.setProperty(k, v);
  applied = saved.map(([k]) => k);
  return out;
}

export function storedTheme(): string {
  const t = load<string>("theme");
  return t === CUSTOM || THEMES.some((p) => p.id === t) ? (t as string) : DEFAULT_THEME;
}

export function applyTheme(id: string, remember = true) {
  const root = document.documentElement;
  const t = id === CUSTOM || THEMES.some((p) => p.id === id) ? id : DEFAULT_THEME;
  clearCustom();
  if (t === DEFAULT_THEME) delete root.dataset.theme;
  else root.dataset.theme = t;
  if (t === CUSTOM) {
    const c = storedCustom();
    for (const [k, v] of Object.entries(customTokens(c))) {
      root.style.setProperty(k, v);
      applied.push(k);
    }
    root.style.colorScheme = c.light ? "light" : "dark";
  }
  if (remember) save("theme", t);
}
