<script lang="ts">
  // One tool the app downloads and sets up itself (src-tauri install.rs):
  // Install, a progress bar with Cancel, then the result. A copy the app
  // installed can be removed again.
  import { onDestroy } from "svelte";
  import { app } from "$lib/state.svelte";
  import * as ipc from "$lib/ipc";

  let {
    tool,
    label,
    note = "",
    oninstalled,
  }: {
    tool: ipc.InstallTool;
    /** the button text, e.g. "Install ffmpeg with vid.stab 1.4" */
    label: string;
    /** where it comes from and its size, shown beside the button */
    note?: string;
    /** after an install or removal: the path now set (null after removal) */
    oninstalled?: (path: string | null) => void;
  } = $props();

  let busy = $state(false);
  let prog = $state<ipc.InstallProgress | null>(null);
  let msg = $state<{ text: string; err: boolean } | null>(null);
  let ours = $state<string | null>(null);
  $effect(() => {
    ipc.toolInstalled(tool).then((p) => (ours = p)).catch(() => {});
  });

  let un: (() => void) | null = null;
  ipc.onInstallProgress((p) => {
    if (p.tool === tool) prog = p;
  }).then((u) => (un = u));
  onDestroy(() => un?.());

  const mb = (b: number) => `${(b / 1048576).toFixed(0)} MB`;
  const status = $derived.by(() => {
    const p = prog;
    if (!p) return "Starting...";
    switch (p.phase) {
      case "find":
        return "Looking up the latest release...";
      case "download":
        return `Downloading ${mb(p.done)}${p.total ? ` of ${mb(p.total)}` : ""}`;
      case "unpack":
        return "Unpacking...";
      case "check":
        return "Checking that it runs...";
      case "brew":
        return p.text ?? "Homebrew is installing...";
      case "pip":
        return p.text ?? "Installing the Python packages...";
    }
  });
  const fraction = $derived(prog?.phase === "download" && prog.total ? prog.done / prog.total : null);

  async function install() {
    busy = true;
    msg = null;
    prog = null;
    try {
      const r = await ipc.toolInstall(tool);
      ours = await ipc.toolInstalled(tool).catch(() => null);
      await app.refreshTools();
      msg = { text: `Installed ${r.release}. comfywebm now uses ${r.path}.`, err: false };
      oninstalled?.(r.path);
    } catch (e) {
      const t = ipc.errText(e);
      msg = { text: t === "Canceled" ? "Canceled; nothing was changed." : `Not installed: ${t}`, err: t !== "Canceled" };
    } finally {
      busy = false;
      prog = null;
    }
  }

  async function remove() {
    msg = null;
    try {
      await ipc.toolRemove(tool);
      ours = null;
      await app.refreshTools();
      msg = { text: "Removed the copy comfywebm installed.", err: false };
      oninstalled?.(null);
    } catch (e) {
      msg = { text: ipc.errText(e), err: true };
    }
  }
</script>

<div class="inst">
  {#if busy}
    <div class="row">
      <div class="bar" class:indef={fraction === null}><div style:width="{fraction === null ? 100 : Math.round(fraction * 100)}%"></div></div>
      <button class="btn small" onclick={() => ipc.toolInstallCancel(tool)}>Cancel</button>
    </div>
    <span class="small muted mono line">{status}</span>
  {:else}
    <div class="row">
      <button class="btn small" onclick={install}>{ours ? label.replace(/^Install/, "Update") : label}</button>
      {#if ours}<button class="btn small" title={ours} onclick={remove}>Remove</button>{/if}
      {#if note}<span class="small muted">{note}</span>{/if}
    </div>
  {/if}
  {#if msg}<span class="small" class:err={msg.err}>{msg.text}</span>{/if}
</div>

<style>
  .inst {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .small {
    font-size: 11px;
  }
  .line {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .err {
    color: var(--red);
  }
  .bar {
    flex: 1;
    min-width: 120px;
    height: 6px;
    border-radius: 3px;
    background: var(--bg-chip);
    overflow: hidden;
  }
  .bar > div {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .bar.indef > div {
    opacity: 0.45;
    animation: pulse 1.2s ease-in-out infinite alternate;
  }
  @keyframes pulse {
    to {
      opacity: 0.9;
    }
  }
</style>
