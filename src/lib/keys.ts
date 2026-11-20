// Global keyboard shortcuts (USAGE.md, "Keys") and URL paste.
import { app } from "./state.svelte";
import { splits } from "./splits.svelte";

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

/** The key list shown by KeysDialog (? or F1). Keep in sync with onKey below and README "Keys". */
export const KEY_HELP: { group: string; keys: [string, string][] }[] = [
  {
    group: "Playback",
    keys: [
      ["Space", "play / pause (in Splits: the preview)"],
      ["Left / Right", "seek 1 s"],
      ["Shift + Left / Shift + Right", "seek 5 s"],
      [", / .", "one frame back / forward (or the mouse wheel over the picture)"],
      ["L", "loop the selected range"],
      ["M", "mute / unmute the preview (the speaker at the bottom right of the picture)"],
    ],
  },
  {
    group: "Ranges",
    keys: [
      ["I / O", "set the start / end of the range at the playhead"],
      ["Shift + I / Shift + O", "jump to the start / end of the selected range"],
      ["N", "add a range at the playhead"],
      ["Delete", "remove the selected range"],
      ["Ctrl + Z", "undo a range, order or crop change"],
      ["Ctrl + Shift + Z / Ctrl + Y", "redo"],
      ["H", "the history: every edit, click one to go back to it"],
      ["Ctrl + wheel", "zoom the timeline (wheel pans while zoomed)"],
      ["Drag a range", "move it, keeping its length (a click seeks)"],
      ["Alt + drag", "move a range or its edge without snapping"],
      ["[ / ]", "previous / next scene cut (after Find cuts)"],
    ],
  },
  {
    group: "Crop",
    keys: [
      ["C", "crop mode on / off"],
      ["Enter or Esc", "leave crop mode (the crop is kept)"],
      ["Arrows", "move the crop 1 px (Shift: 10 px)"],
      ["Alt + arrows", "resize the crop"],
    ],
  },
  {
    group: "App",
    keys: [
      ["Ctrl + O", "open a file"],
      ["Ctrl + U", "open a web link"],
      ["Ctrl + E", "encode"],
      ["Ctrl + Enter", "add to the queue"],
      ["P", "sample: encode 3 s at the playhead"],
      ["S", "save the current frame as PNG (source picture with the crop)"],
      ["Shift + S", "save the current frame as the encode sees it: filters, adjustments, rotation and output size"],
      ["Q", "switch between the editor and the queue"],
      ["B", "switch between the editor and the batch"],
      ["? / F1", "this list"],
    ],
  },
];

export type KeyActions = { openFile: () => void; openUrl: (url?: string) => void };

const URL_RE = /^https?:\/\/\S+$/i;

export function installKeys(actions: KeyActions): () => void {
  // A mouse click leaves focus on what was clicked (a button, checkbox,
  // slider, the video, the timeline); it would then take Space, Enter and the
  // arrows, and a key press would show its focus outline. Tab only moves focus
  // inside dialogs (appfeel.ts), so outside them nothing needs focus after a
  // click: it is dropped. Inside a dialog focus goes to the dialog itself, so
  // its own keys (Escape) still reach it. Text fields keep focus.
  // The focused element is used, not the event target: a click on a button's
  // icon or label has the <span> as target.
  // A native <select> must never be blurred on click: in WebView2 its list
  // opens on mouse-down and the click (mouse-up) that follows would close it
  // before an option can be picked. The app uses Select.svelte instead, whose
  // open list takes the keyboard itself.
  let pointer = false;
  const onPointer = () => (pointer = true);
  const dropFocus = () => {
    if (!pointer) return;
    const el = document.activeElement;
    if (!(el instanceof HTMLElement) || el === document.body || isTextField(el)) return;
    const dlg = el.closest<HTMLElement>("[role=dialog]");
    if (dlg === el) return;
    if (dlg) dlg.focus({ preventScroll: true });
    else el.blur();
  };

  const onKey = (e: KeyboardEvent) => {
    pointer = false;
    // F1 is prevented by appfeel.ts (WebView2 help), so it is handled before the check below
    if (e.key === "F1" && !app.settingsOpen && !app.urlOpen && !app.sample) {
      app.keysOpen = !app.keysOpen;
      return;
    }
    if (e.defaultPrevented) return;
    if (isTextField(e.target)) {
      if (e.key === "Escape") (e.target as HTMLElement).blur();
      // Ctrl+E is not left to WebView2 (appfeel.ts no longer blocks it)
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "e") e.preventDefault();
      return;
    }
    if (app.settingsOpen || app.urlOpen || app.sample || app.keysOpen) return; // dialogs handle their own keys
    if (e.key === "?") {
      app.keysOpen = true;
      e.preventDefault();
      return;
    }
    if (isPressable(e.target) && (e.key === " " || e.key === "Enter")) {
      // in a dialog, Tab and Space or Enter press buttons; outside one a
      // focused button is left over from a click and must not take the key
      if ((e.target as Element).closest("[role=dialog]")) return;
      (e.target as HTMLElement).blur();
      e.preventDefault();
      if (e.key === "Enter" && !app.cropMode && !app.pointsMode) return;
    }

    const ctrl = e.ctrlKey || e.metaKey;
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    let handled = true;

    if (ctrl) {
      if (k === "z" && !e.shiftKey) app.undo();
      else if ((k === "z" && e.shiftKey) || k === "y") app.redo();
      else if (k === "o") actions.openFile();
      else if (k === "u") actions.openUrl();
      else if (k === "e" && !e.shiftKey) void app.enqueue(true);
      else if (e.key === "Enter") void app.enqueue(false);
      else handled = false;
    } else if (app.view === "splits" && k === " ") {
      splits.togglePlay?.();
    } else if (app.view !== "editor" && k !== "q" && k !== "b") {
      // the editor is hidden: its keys do nothing
      handled = false;
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
        // frame step on , and . (and < and >, the same keys with Shift on US layouts)
        case ",":
        case "<":
          app.stepFrame(-1);
          break;
        case "[":
          app.jumpCut(-1);
          break;
        case "]":
          app.jumpCut(1);
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
          void app.exportFrame(e.shiftKey);
          break;
        case "Delete":
        case "Backspace":
          if (app.segments.length > 1 && app.sel >= 0) app.removeRange();
          else handled = false;
          break;
        case "l":
          app.toggleLoop();
          break;
        case "m":
          app.toggleMute();
          break;
        case "h":
          if (app.media) app.historyOpen = !app.historyOpen;
          break;
        case "q":
          app.setView(app.view === "queue" ? "editor" : "queue");
          break;
        case "b":
          app.setView(app.view === "batch" ? "editor" : "batch");
          break;
        case "c":
          if (app.media && !app.audioOnly) app.cropMode = !app.cropMode;
          break;
        case "Enter":
          if (app.pointsMode) void app.trackPoints();
          else if (app.cropMode) app.cropMode = false;
          else handled = false;
          break;
        case "Escape":
          if (app.pointsMode) app.cancelPoints();
          else if (app.followMode) app.cancelFollow();
          else if (app.cropMode) app.cropMode = false;
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
  // pointerup: focus came with the mouse-down. click: a click on a <label>
  // focuses its checkbox only then.
  window.addEventListener("pointerup", dropFocus, true);
  window.addEventListener("click", dropFocus);
  window.addEventListener("keydown", onKey);
  return () => {
    window.removeEventListener("pointerdown", onPointer, true);
    window.removeEventListener("paste", onPaste);
    window.removeEventListener("pointerup", dropFocus, true);
    window.removeEventListener("click", dropFocus);
    window.removeEventListener("keydown", onKey);
  };
}
