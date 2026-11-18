// What makes the window behave like an app and not a web page: no WebView2
// context menu (Back, Refresh, Save as, Print, Inspect), no browser keys
// (Find, Print, Reload, zoom, history), no Tab walk through every control, no
// middle-click autoscroll, no dragging images out. Tauri 2 has no switch for
// WebView2's context menu or its accelerator keys, so they are stopped here.
//
// Text fields get the app's own menu (undo, cut, copy, paste, select all; see
// menu.svelte.ts) and keep their keys. A dev build keeps reload and the
// DevTools keys.
import { menu, textMenu } from "./menu.svelte";

/** Ctrl (or Cmd) + key, by KeyboardEvent.key in lower case. Ctrl+O and Ctrl+U are the app's own. */
const CTRL = new Set(["f", "g", "p", "r", "s", "j", "h", "n", "t", "w", "d", "e", "k", "l", "=", "+", "-", "0"]);
const PLAIN = new Set(["F1", "F3", "F5", "F6", "F7", "F12", "BrowserBack", "BrowserForward", "BrowserRefresh", "BrowserSearch", "BrowserHome"]);

const FOCUSABLE = 'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

function typing(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false;
  if (t.isContentEditable || t instanceof HTMLTextAreaElement) return true;
  return t instanceof HTMLInputElement && !["range", "checkbox", "radio", "button", "submit", "reset", "color", "file"].includes(t.type);
}

export function installAppFeel(): () => void {
  const dev = import.meta.env.DEV;

  // an element that opened its own menu has already called preventDefault
  const onMenu = (e: MouseEvent) => {
    if (e.defaultPrevented) return;
    e.preventDefault();
    const t = e.target;
    if (typing(t) && (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement)) menu.show(e.clientX, e.clientY, textMenu(t));
    else menu.hide();
  };

  const key = (e: KeyboardEvent) => {
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (dev && (k === "F5" || k === "F12" || (e.ctrlKey && k === "r") || (e.ctrlKey && e.shiftKey && (k === "i" || k === "j" || k === "c")))) return;
    // Tab walks focus through every button; inside dialogs it moves between
    // the dialog's own controls and wraps at the ends, so it never leaves the dialog
    if (k === "Tab") {
      const dlg = (e.target as Element | null)?.closest?.("[role=dialog]");
      if (!dlg) return e.preventDefault();
      const items = [...dlg.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
      if (!items.length) return e.preventDefault();
      const first = items[0];
      const last = items[items.length - 1];
      const at = document.activeElement;
      if (e.shiftKey && (at === first || at === dlg)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && at === last) {
        e.preventDefault();
        first.focus();
      }
      return;
    }
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
    if (t instanceof HTMLElement && !t.closest("[role=dialog]")) lastOutside = typing(t) ? t : null;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) {
      t.spellcheck = false;
      t.autocomplete = "off";
    }
  };

  // Ctrl+wheel zooms the page
  const wheel = (e: WheelEvent) => {
    if (e.ctrlKey) e.preventDefault();
  };

  // When a dialog closes, focus goes back to the text field that had it when
  // the dialog opened. Nothing else gets it back: outside dialogs focus is only
  // ever left over from a click (keys.ts drops it), and a button given focus
  // here would take Space and show its focus outline.
  let lastOutside: HTMLElement | null = null;
  const unfocus = (e: FocusEvent) => {
    // focus left the field for nothing (Escape, a click elsewhere): not restored later
    if (e.target === lastOutside && !e.relatedTarget) lastOutside = null;
  };
  const watch = new MutationObserver((records) => {
    const closed = records.some((r) =>
      [...r.removedNodes].some((n) => n instanceof Element && (n.matches("[role=dialog]") || n.querySelector("[role=dialog]"))),
    );
    const at = document.activeElement;
    if (closed && (!at || at === document.body) && lastOutside?.isConnected) lastOutside.focus({ preventScroll: true });
  });
  watch.observe(document.body, { childList: true, subtree: true });

  window.addEventListener("contextmenu", onMenu);
  window.addEventListener("keydown", key, true);
  window.addEventListener("pointerdown", down, true);
  window.addEventListener("mouseup", up, true);
  window.addEventListener("dragstart", drag, true);
  window.addEventListener("focusin", focus, true);
  window.addEventListener("focusout", unfocus, true);
  window.addEventListener("wheel", wheel, { capture: true, passive: false });
  return () => {
    watch.disconnect();
    window.removeEventListener("contextmenu", onMenu);
    window.removeEventListener("keydown", key, true);
    window.removeEventListener("pointerdown", down, true);
    window.removeEventListener("mouseup", up, true);
    window.removeEventListener("dragstart", drag, true);
    window.removeEventListener("focusin", focus, true);
    window.removeEventListener("focusout", unfocus, true);
    window.removeEventListener("wheel", wheel, { capture: true });
  };
}
