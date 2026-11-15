// Hover text drawn by the app instead of the native tooltip (which looks like
// a browser's and ignores the theme). Any element with a `title` gets it: the
// title is moved to `data-tip` on first hover so the native one never shows.
// Icon-only buttons show their aria-label.

const DELAY = 450;

function tipText(el: Element): string | null {
  const t = el.getAttribute("title");
  if (t !== null) {
    el.removeAttribute("title");
    if (t) el.setAttribute("data-tip", t);
    else el.removeAttribute("data-tip");
  }
  const d = el.getAttribute("data-tip");
  if (d) return d;
  if (el instanceof HTMLButtonElement && !el.textContent?.trim()) return el.getAttribute("aria-label");
  return null;
}

export function installTooltips(): () => void {
  const tip = document.createElement("div");
  tip.className = "app-tip";
  tip.setAttribute("role", "tooltip");
  document.body.appendChild(tip);
  let target: Element | null = null;
  let timer = 0;

  function hide() {
    clearTimeout(timer);
    target = null;
    tip.classList.remove("on");
  }

  function place(el: Element) {
    const r = el.getBoundingClientRect();
    const t = tip.getBoundingClientRect();
    let x = r.left + r.width / 2 - t.width / 2;
    x = Math.min(Math.max(6, x), window.innerWidth - t.width - 6);
    let y = r.bottom + 6;
    if (y + t.height > window.innerHeight - 6) y = r.top - t.height - 6;
    tip.style.transform = `translate(${Math.round(x)}px, ${Math.round(Math.max(6, y))}px)`;
  }

  const over = (e: PointerEvent) => {
    const el = (e.target as Element | null)?.closest?.("[title], [data-tip], button[aria-label]") ?? null;
    if (el === target) return;
    hide();
    if (!el) return;
    const text = tipText(el);
    if (!text) return;
    target = el;
    timer = window.setTimeout(() => {
      if (target !== el || !el.isConnected) return;
      tip.textContent = text;
      tip.classList.add("on");
      place(el);
    }, DELAY);
  };

  const out = (e: PointerEvent) => {
    if (target && !(e.relatedTarget instanceof Node && target.contains(e.relatedTarget))) hide();
  };

  window.addEventListener("pointerover", over, true);
  window.addEventListener("pointerout", out, true);
  window.addEventListener("pointerdown", hide, true);
  window.addEventListener("keydown", hide, true);
  window.addEventListener("wheel", hide, { capture: true, passive: true });
  window.addEventListener("blur", hide);
  return () => {
    hide();
    tip.remove();
    window.removeEventListener("pointerover", over, true);
    window.removeEventListener("pointerout", out, true);
    window.removeEventListener("pointerdown", hide, true);
    window.removeEventListener("keydown", hide, true);
    window.removeEventListener("wheel", hide, { capture: true });
    window.removeEventListener("blur", hide);
  };
}
