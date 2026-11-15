<script lang="ts">
  // Every key, opened with ? or F1 or the keyboard button in the header.
  import { app } from "$lib/state.svelte";
  import { KEY_HELP } from "$lib/keys";

  const close = () => (app.keysOpen = false);
</script>

<div
  class="backdrop"
  role="presentation"
  onpointerdown={(e) => {
    if (e.target === e.currentTarget) close();
  }}
>
  <div
    class="dialog keys"
    role="dialog"
    aria-modal="true"
    aria-labelledby="keys-title"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape" || e.key === "?" || e.key === "F1") {
        e.preventDefault();
        close();
      }
    }}
  >
    <h2 id="keys-title">Keys</h2>
    <div class="cols">
      {#each KEY_HELP as g (g.group)}
        <section>
          <h3 class="head-text">{g.group}</h3>
          {#each g.keys as [k, what] (k)}
            <div class="line">
              <span class="k">
                {#each k.split(" / ") as part, i (i)}{#if i}<span class="muted">/</span>{/if}<kbd>{part}</kbd>{/each}
              </span>
              <span class="what">{what}</span>
            </div>
          {/each}
        </section>
      {/each}
    </div>
    <div class="muted small">Keys do nothing while a text field has focus. Pasting a link opens it in the Open URL dialog.</div>
  </div>
</div>

<style>
  .keys {
    width: min(760px, calc(100vw - 40px));
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px 24px;
  }
  h3 {
    margin: 0 0 6px;
  }
  .line {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 2px 0;
    font-size: 12px;
  }
  .k {
    flex: none;
    width: 150px;
    display: inline-flex;
    flex-wrap: wrap;
    gap: 3px;
    align-items: baseline;
  }
  .what {
    color: var(--text-secondary);
  }
  .small {
    font-size: 11px;
  }
</style>
