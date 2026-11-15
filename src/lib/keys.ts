// Global keyboard shortcuts (PLAN.md "Keys", plus Amendments 1 and 2) and URL paste.
import { app } from "./state.svelte";

const NOT_TEXT = new Set(["checkbox", "radio", "button", "submit", "reset", "color", "file"]);

/** True while focus is in a field that uses the keyboard itself. */
function isTextField(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  if (el.isContentEditable) return true;
  if (el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) return true;
  return el instanceof HTMLInputElement && !NOT_TEXT.has(el.type);
}

/** Buttons and checkboxes keep their own Space and Enter. */
function isPressable(el: EventTarget | null): boolean {
  return (
    el instanceof HTMLButtonElement ||
    (el instanceof HTMLInputElement && (el.type === "checkbox" || el.type === "radio"))
  );
}

export type KeyActions = { openFile: () => void; openUrl: (url?: string) => void };

const URL_RE = /^https?:\/\/\S+$/i;

export function installKeys(actions: KeyActions): () => void {
  // After a mouse click, a button, checkbox, select or slider keeps focus and
  // would swallow Space and the arrows; drop focus so the shortcuts work.
  let pointer = false;
  const onPointer = () => (pointer = true);
  const blurAfterPointer = (e: Event) => {
    const t = e.target;
    if (!pointer) return;
    if (
      t instanceof HTMLButtonElement ||
      t instanceof HTMLSelectElement ||
      (t instanceof HTMLInputElement && ["checkbox", "radio", "range"].includes(t.type))
    )
      t.blur();
  };

  const onKey = (e: KeyboardEvent) => {
    pointer = false;
    if (e.defaultPrevented) return;
    if (isTextField(e.target)) {
      if (e.key === "Escape") (e.target as HTMLElement).blur();
      return;
    }
    if (app.settingsOpen || app.urlOpen || app.sample) return; // dialogs handle their own keys
    if (isPressable(e.target) && (e.key === " " || e.key === "Enter")) return;

    const ctrl = e.ctrlKey || e.metaKey;
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    let handled = true;

    if (ctrl) {
      if (k === "o") actions.openFile();
      else if (k === "u") actions.openUrl();
      else if (e.key === "Enter") void app.enqueue(true);
      else handled = false;
    } else if (e.key.startsWith("Arrow") && app.cropMode && app.crop) {
      const step = e.shiftKey ? 10 : 1;
      const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
      const dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
      app.nudgeCrop(dx, dy, e.altKey);
    } else {
      switch (k) {
        case " ":
          app.togglePlay();
          break;
        case "ArrowLeft":
          app.seek(app.current - (e.shiftKey ? 5 : 1));
          break;
        case "ArrowRight":
          app.seek(app.current + (e.shiftKey ? 5 : 1));
          break;
        case ",":
        case "<":
          app.stepFrame(-1);
          break;
        case ".":
        case ">":
          app.stepFrame(1);
          break;
        case "i":
          if (e.shiftKey) app.seek(app.start);
          else app.markIn();
          break;
        case "o":
          if (e.shiftKey) app.seek(app.end);
          else app.markOut();
          break;
        case "n":
          app.addRange();
          break;
        case "p":
          void app.runSample();
          break;
        case "s":
          void app.exportFrame();
          break;
        case "Delete":
        case "Backspace":
          if (app.segments.length > 1 && app.sel >= 0) app.removeRange();
          else handled = false;
          break;
        case "l":
          app.toggleLoop();
          break;
        case "c":
          if (app.media) app.cropMode = !app.cropMode;
          break;
        case "Escape":
          if (app.cropMode) app.cropMode = false;
          else handled = false;
          break;
        default:
          handled = false;
      }
    }
    if (handled) e.preventDefault();
  };

  // pasting a URL while no text field has focus opens the URL dialog with it
  const onPaste = (e: ClipboardEvent) => {
    if (isTextField(e.target) || app.settingsOpen || app.urlOpen) return;
    const text = e.clipboardData?.getData("text/plain")?.trim() ?? "";
    if (URL_RE.test(text)) {
      e.preventDefault();
      actions.openUrl(text);
    }
  };

  window.addEventListener("pointerdown", onPointer, true);
  window.addEventListener("paste", onPaste);
  window.addEventListener("click", blurAfterPointer);
  window.addEventListener("change", blurAfterPointer);
  window.addEventListener("keydown", onKey);
  return () => {
    window.removeEventListener("pointerdown", onPointer, true);
    window.removeEventListener("paste", onPaste);
    window.removeEventListener("click", blurAfterPointer);
    window.removeEventListener("change", blurAfterPointer);
    window.removeEventListener("keydown", onKey);
  };
}
