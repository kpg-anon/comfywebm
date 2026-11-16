// In-app updates through @tauri-apps/plugin-updater. The release script
// (scripts/release.ps1) publishes latest.json on the GitHub release with the
// version, the CHANGELOG section as notes, and the signed NSIS installer.
import { check as checkUpdate, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { load, save } from "./persist";
import { canSelfUpdate } from "./ipc";

export const RELEASES_URL = "https://github.com/kpg-anon/comfywebm/releases/latest";

export type UpdatePhase = "idle" | "checking" | "none" | "available" | "downloading" | "restarting" | "error";

class Updater {
  phase = $state<UpdatePhase>("idle");
  /** the newer version offered, e.g. "1.0.1" */
  version = $state("");
  /** release notes (markdown: ### headings, - bullets, **bold**, `code`, links) */
  notes = $state("");
  date = $state("");
  /** 0..100 while downloading */
  progress = $state(0);
  error = $state("");
  /** the notice was closed with Later; Settings still shows the update */
  dismissed = $state(false);
  /** false for a Linux .deb/.rpm install: the notice links the release page instead */
  selfUpdate = $state(true);
  /** Settings > Look > Updates: check when the app starts (per machine) */
  auto = $state(load<boolean>("updateCheck") ?? true);
  private update: Update | null = null;

  setAuto(on: boolean) {
    this.auto = on;
    save("updateCheck", on);
  }

  /**
   * Asks the release endpoint for a newer version. A check at start (`manual`
   * false) stays silent when it fails, as it does offline or in a dev build;
   * a manual check shows the error.
   */
  async check(manual = false) {
    if (this.phase === "checking" || this.phase === "downloading" || this.phase === "restarting") return;
    this.phase = "checking";
    this.error = "";
    try {
      const u = await checkUpdate();
      if (u) {
        this.selfUpdate = await canSelfUpdate().catch(() => true);
        this.update = u;
        this.version = u.version;
        this.notes = (u.body ?? "").trim();
        this.date = u.date ?? "";
        this.dismissed = false;
        this.phase = "available";
      } else {
        this.phase = manual ? "none" : "idle";
      }
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
      this.phase = manual ? "error" : "idle";
    }
  }

  /** Downloads the installer (its signature is checked), runs it and restarts the app. */
  async install() {
    const u = this.update;
    if (!u) return;
    this.phase = "downloading";
    this.progress = 0;
    let total = 0;
    let got = 0;
    try {
      await u.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") {
          got += ev.data.chunkLength;
          this.progress = total ? Math.min(100, Math.round((got / total) * 100)) : 0;
        } else if (ev.event === "Finished") this.progress = 100;
      });
      this.phase = "restarting";
      await relaunch();
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
      this.phase = "error";
    }
  }

  /** Dev only: shows the notice with sample notes, without a release. */
  preview(notes: string) {
    this.update = null;
    this.version = "1.0.1";
    this.notes = notes;
    this.dismissed = false;
    this.phase = "available";
  }
}

export const updater = new Updater();

// ---- release notes: a small markdown subset, as CHANGELOG.md uses it ----

export type Seg = { t: "text" | "bold" | "code" | "link"; v: string; href?: string };
export type Block = { kind: "heading"; text: string } | { kind: "bullet"; segs: Seg[] } | { kind: "para"; segs: Seg[] };

function inline(s: string): Seg[] {
  const out: Seg[] = [];
  const re = /\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\(([^)]+)\)|(https?:\/\/[^\s)]+)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(s))) {
    if (m.index > last) out.push({ t: "text", v: s.slice(last, m.index) });
    if (m[1] !== undefined) out.push({ t: "bold", v: m[1] });
    else if (m[2] !== undefined) out.push({ t: "code", v: m[2] });
    else if (m[3] !== undefined) out.push({ t: "link", v: m[3], href: m[4] });
    else out.push({ t: "link", v: m[5], href: m[5] });
    last = re.lastIndex;
  }
  if (last < s.length) out.push({ t: "text", v: s.slice(last) });
  return out;
}

/**
 * Notes as blocks. A bullet's wrapped lines (indented continuation lines, as
 * CHANGELOG.md is written) are joined to it. Returns the blocks and the link
 * from a trailing "---" + "Full changelog: <url>" footer, if there is one.
 */
export function parseNotes(body: string): { blocks: Block[]; changelogUrl: string } {
  const [main, ...foot] = body.split(/\r?\n-{3,}\s*\r?\n/);
  const changelogUrl = (foot.join("\n").match(/https?:\/\/\S+/) ?? [""])[0];
  const blocks: Block[] = [];
  let text: string[] | null = null;
  let kind: "bullet" | "para" = "para";
  const flush = () => {
    if (text) blocks.push({ kind, segs: inline(text.join(" ")) });
    text = null;
  };
  for (const raw of (main ?? "").split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) {
      flush();
      continue;
    }
    const h = line.match(/^#{1,6}\s+(.*)$/);
    const b = line.match(/^[-*]\s+(.*)$/);
    if (h) {
      flush();
      blocks.push({ kind: "heading", text: h[1] });
    } else if (b) {
      flush();
      kind = "bullet";
      text = [b[1]];
    } else if (text && /^\s/.test(raw)) {
      text.push(line);
    } else {
      flush();
      kind = "para";
      text = [line];
    }
  }
  flush();
  return { blocks, changelogUrl };
}
