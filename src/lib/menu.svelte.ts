// The app's own right-click menu. WebView2's menu (with Back, Reload, Inspect,
// "Import passwords"...) never shows: appfeel.ts opens this one for text
// fields, and any element can open it with its own items via showMenu.
import { readText } from "@tauri-apps/plugin-clipboard-manager";

export type MenuItem =
  | {
      label: string;
      action: () => void;
      disabled?: boolean;
      /** a key shown on the right, for example "Ctrl+C" */
      keys?: string;
      danger?: boolean;
    }
  | "sep";

class Menu {
  open = $state(false);
  x = $state(0);
  y = $state(0);
  items = $state<MenuItem[]>([]);
  /** the element that had focus when the menu opened; it gets it back */
  owner: HTMLElement | null = null;

  show(x: number, y: number, items: MenuItem[]) {
    if (!items.some((i) => i !== "sep")) return;
    const at = document.activeElement;
    this.owner = at instanceof HTMLElement && at !== document.body ? at : null;
    this.x = x;
    this.y = y;
    this.items = items;
    this.open = true;
  }

  hide() {
    this.open = false;
  }
}

export const menu = new Menu();

/** Opens the menu at the pointer for a contextmenu event (or a click). */
export function showMenu(e: MouseEvent, items: MenuItem[]) {
  e.preventDefault();
  e.stopPropagation();
  menu.show(e.clientX, e.clientY, items);
}

const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
const mod = isMac ? "Cmd" : "Ctrl";

/** Undo, cut, copy, paste, delete and select all for a text field. */
export function textMenu(el: HTMLInputElement | HTMLTextAreaElement): MenuItem[] {
  const ro = el.readOnly || el.disabled;
  // number fields have no selection API; treat them as having a selection
  let sel = true;
  try {
    if (el.selectionStart !== null && el.selectionEnd !== null) sel = el.selectionEnd > el.selectionStart;
  } catch {
    sel = true;
  }
  const empty = el.value === "";
  const run = (cmd: string) => () => {
    el.focus();
    document.execCommand(cmd);
  };
  return [
    { label: "Undo", keys: `${mod}+Z`, disabled: ro, action: run("undo") },
    { label: "Redo", keys: isMac ? "Cmd+Shift+Z" : "Ctrl+Y", disabled: ro, action: run("redo") },
    "sep",
    { label: "Cut", keys: `${mod}+X`, disabled: ro || !sel, action: run("cut") },
    { label: "Copy", keys: `${mod}+C`, disabled: !sel, action: run("copy") },
    { label: "Paste", keys: `${mod}+V`, disabled: ro, action: () => void paste(el) },
    { label: "Delete", disabled: ro || !sel, action: run("delete") },
    "sep",
    {
      label: "Select all",
      keys: `${mod}+A`,
      disabled: empty,
      action: () => {
        el.focus();
        el.select();
      },
    },
  ];
}

/** Pastes plain text at the cursor, as Ctrl+V would (an `input` event, undoable). The text is read through
 * the clipboard plugin: WebView2 holds navigator.clipboard.readText for a permission it never grants here. */
async function paste(el: HTMLInputElement | HTMLTextAreaElement) {
  let text = "";
  try {
    text = await readText();
  } catch {
    return;
  }
  el.focus();
  if (!text) return;
  // single-line fields take the first line, as the browser's own paste does
  if (el instanceof HTMLInputElement) text = text.replace(/\r?\n[\s\S]*$/, "");
  document.execCommand("insertText", false, text);
}
