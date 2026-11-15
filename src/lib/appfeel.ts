// What makes the window behave like an app and not a web page: no WebView2
// context menu (Back, Refresh, Save as, Print, Inspect), no browser keys
// (Find, Print, Reload, zoom, history), no Tab walk through every control, no
// middle-click autoscroll, no dragging images out. Tauri 2 has no switch for
// WebView2's context menu or its accelerator keys, so they are stopped here.
//
// Text fields keep their own context menu (cut, copy, paste) and keys.
// A dev build keeps reload and the DevTools keys.

/** Ctrl (or Cmd) + key, by KeyboardEvent.key in lower case. Ctrl+O and Ctrl+U are the app's own. */
const CTRL = new Set(["f", "g", "p", "r", "s", "j", "h", "n", "t", "w", "d", "e", "k", "l", "=", "+", "-", "0"]);
const PLAIN = new Set(["F1", "F3", "F5", "F6", "F7", "F12", "BrowserBack", "BrowserForward", "BrowserRefresh", "BrowserSearch", "BrowserHome"]);

function typing(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false;
  if (t.isContentEditable || t instanceof HTMLTextAreaElement) return true;
  return t instanceof HTMLInputElement && !["range", "checkbox", "radio", "button", "submit", "reset", "color", "file"].includes(t.type);
}

export function installAppFeel(): () => void {
  const dev = import.meta.env.DEV;

  const menu = (e: MouseEvent) => {
    if (!typing(e.target)) e.preventDefault();
  };

  const key = (e: KeyboardEvent) => {
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (dev && (k === "F5" || k === "F12" || (e.ctrlKey && k === "r") || (e.ctrlKey && e.shiftKey && (k === "i" || k === "j" || k === "c")))) return;
    // Tab walks focus through every button; inside dialogs it still moves between fields
    if (k === "Tab" && !(e.target as Element | null)?.closest?.("[role=dialog]")) return e.preventDefault();
    if (PLAIN.has(k)) return e.preventDefault();
    // Alt+Home: home page. Alt+Left/Right (history) are not stopped here:
    // keys.ts uses them (seek, crop resize), skips events that are already
    // defaultPrevented, and prevents them itself.
    if (e.altKey && k === "Home") return e.preventDefault();
    if ((e.ctrlKey || e.metaKey) && !e.altKey) {
      if (CTRL.has(k) && !(k === "s" && typing(e.target))) return e.preventDefault();
      if (k === "a" && !typing(e.target)) return e.preventDefault();
    }
  };

  const down = (e: PointerEvent) => {
    if (e.button === 1) e.preventDefault(); // middle click: autoscroll
  };
  // mouse back/forward buttons navigate history in a webview
  const up = (e: MouseEvent) => {
    if (e.button === 3 || e.button === 4) e.preventDefault();
  };

  const drag = (e: DragEvent) => {
    if (!typing(e.target)) e.preventDefault();
  };

  // no spelling underline or autofill list on text fields
  const focus = (e: FocusEvent) => {
    const t = e.target;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) {
      t.spellcheck = false;
      t.autocomplete = "off";
    }
  };

  // Ctrl+wheel zooms the page
  const wheel = (e: WheelEvent) => {
    if (e.ctrlKey) e.preventDefault();
  };

  window.addEventListener("contextmenu", menu);
  window.addEventListener("keydown", key, true);
  window.addEventListener("pointerdown", down, true);
  window.addEventListener("mouseup", up, true);
  window.addEventListener("dragstart", drag, true);
  window.addEventListener("focusin", focus, true);
  window.addEventListener("wheel", wheel, { capture: true, passive: false });
  return () => {
    window.removeEventListener("contextmenu", menu);
    window.removeEventListener("keydown", key, true);
    window.removeEventListener("pointerdown", down, true);
    window.removeEventListener("mouseup", up, true);
    window.removeEventListener("dragstart", drag, true);
    window.removeEventListener("focusin", focus, true);
    window.removeEventListener("wheel", wheel, { capture: true });
  };
}
