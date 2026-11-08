<script lang="ts">
  import { isTauri } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { Copy, Minus, Square, X } from "@lucide/svelte";
  import { instanceById, instances } from "../instances.svelte";
  import { openInstance } from "../navigation.svelte";

  let { title }: { title: string } = $props();

  const win = isTauri() ? getCurrentWindow() : null;
  let maximized = $state(false);

  const games = $derived(Object.values(instances.games));
  const status = $derived.by(() => {
    if (games.length === 0) return "No instances running";
    if (games.length > 1) return `${games.length} instances running`;
    const name = instanceById(games[0].id)?.name ?? "Minecraft";
    return games[0].phase === "preparing" ? `Preparing ${name}` : name;
  });

  $effect(() => {
    if (!win) return;
    const sync = () => win.isMaximized().then((m) => (maximized = m));
    sync();
    const unlisten = win.onResized(sync);
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<header class="titlebar" data-tauri-drag-region>
  <h1 data-tauri-drag-region>{title}</h1>

  <button
    class="status"
    title="Running instances"
    disabled={games.length === 0}
    onclick={() => games[0] && openInstance(games[0].id)}
  >
    <span class="dot" class:running={games.length > 0}></span>
    {status}
  </button>

  <div class="controls">
    <button aria-label="Minimize" onclick={() => win?.minimize()}>
      <Minus size={18} />
    </button>
    <button aria-label={maximized ? "Restore" : "Maximize"} onclick={() => win?.toggleMaximize()}>
      {#if maximized}
        <Copy size={15} />
      {:else}
        <Square size={15} />
      {/if}
    </button>
    <button class="close" aria-label="Close" onclick={() => win?.close()}>
      <X size={18} />
    </button>
  </div>
</header>

<style>
  .titlebar {
    height: var(--titlebar-height);
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 16px;
    padding-left: 12px;
    background: var(--bg-shell);
  }

  h1 {
    flex: 1;
    font-size: 1.05rem;
    font-weight: 600;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 16px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 0.9rem;
    color: var(--text);
  }

  .status:disabled {
    cursor: default;
  }

  .status:not(:disabled):hover {
    background: var(--bg-button);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-muted);
  }

  .dot.running {
    background: var(--brand);
  }

  .controls {
    display: flex;
    align-self: stretch;
  }

  .controls button {
    width: 46px;
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }

  .controls button:hover {
    background: var(--bg-button);
    color: var(--text-strong);
  }

  .controls .close:hover {
    background: #e81123;
    color: #fff;
  }
</style>
