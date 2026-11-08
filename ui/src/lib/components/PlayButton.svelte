<script lang="ts">
  import { LoaderCircle, Play, Square, X } from "@lucide/svelte";
  import { instances, launch, stop } from "../instances.svelte";

  let { id, compact = false }: { id: string; compact?: boolean } = $props();

  const game = $derived(instances.games[id]);
  const percent = $derived.by(() => {
    const p = game?.progress;
    if (!p) return null;
    const total = p.totalBytes || p.totalFiles;
    const done = p.totalBytes ? p.doneBytes : p.doneFiles;
    return total ? Math.floor((done / total) * 100) : null;
  });
</script>

{#if game?.phase === "running"}
  <button class="stop" class:compact onclick={() => stop(id)} title="Stop">
    <Square size={16} fill="currentColor" />
    {#if !compact}Stop{/if}
  </button>
{:else if game?.phase === "preparing"}
  <button
    class="busy"
    class:compact
    onclick={() => stop(id)}
    title="{game.stage ?? 'Preparing'} (click to cancel)"
  >
    <span class="progress">
      <LoaderCircle size={16} class="spin" />
      {#if !compact}
        {percent === null ? (game.stage ?? "Preparing") : `${percent}%`}
      {/if}
    </span>
    <span class="cancel"><X size={16} />{#if !compact}Cancel{/if}</span>
  </button>
{:else}
  <button class="play" class:compact onclick={() => launch(id)} title="Play">
    <Play size={16} fill="currentColor" />
    {#if !compact}Play{/if}
  </button>
{/if}

<style>
  button {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-width: 108px;
    height: 40px;
    padding: 0 18px;
    border-radius: var(--radius);
    font-weight: 700;
    white-space: nowrap;
  }

  button.compact {
    min-width: 0;
    width: 40px;
    padding: 0;
  }

  .play {
    background: var(--brand-bg);
    color: var(--brand-text);
  }

  .play:hover {
    filter: brightness(1.1);
  }

  .stop {
    background: #c9363b;
    color: #fff;
  }

  .stop:hover {
    filter: brightness(1.1);
  }

  .busy {
    background: var(--bg-button);
    color: var(--text-strong);
  }

  .busy span {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .busy .cancel,
  .busy:hover .progress {
    display: none;
  }

  .busy:hover .cancel {
    display: flex;
  }

  .busy:hover {
    background: #c9363b;
    color: #fff;
  }
</style>
