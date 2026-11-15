// Theme presets. Each id other than comfy-dark maps to a
// :root[data-theme="id"] block in theme.css. The choice is a per-machine UI
// preference, kept in localStorage and applied before the first paint
// (+layout.svelte).
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
  { id: "paper", name: "Paper", what: "Light", swatches: ["#f6f4ef", "#3b6fd8", "#2f8f5b", "#d49a14"] },
];

export const DEFAULT_THEME = "comfy-dark";

export function storedTheme(): string {
  const t = load<string>("theme");
  return THEMES.some((p) => p.id === t) ? (t as string) : DEFAULT_THEME;
}

export function applyTheme(id: string, remember = true) {
  const t = THEMES.some((p) => p.id === id) ? id : DEFAULT_THEME;
  if (t === DEFAULT_THEME) delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
  if (remember) save("theme", t);
}
