<script lang="ts">
  import { User } from "@lucide/svelte";

  /** Renders the face (plus hat layer) from a 64x64 Minecraft skin texture. */
  let { skinUrl, size = 32 }: { skinUrl: string | null; size?: number } = $props();
</script>

{#if skinUrl}
  <span
    class="head"
    style:--size="{size}px"
    style:background-image="url({JSON.stringify(skinUrl)}), url({JSON.stringify(skinUrl)})"
  ></span>
{:else}
  <span class="head placeholder" style:--size="{size}px"><User size={size * 0.6} /></span>
{/if}

<style>
  .head {
    width: var(--size);
    height: var(--size);
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border-radius: 4px;
    image-rendering: pixelated;
    /* Each skin pixel is size/8 px. The hat (x 40, y 8) is drawn over the face (x 8, y 8). */
    background-size: calc(var(--size) * 8);
    background-position:
      calc(var(--size) * -5) calc(var(--size) * -1),
      calc(var(--size) * -1) calc(var(--size) * -1);
    background-repeat: no-repeat;
  }

  .placeholder {
    background: var(--bg-button);
    color: var(--text-muted);
  }
</style>
