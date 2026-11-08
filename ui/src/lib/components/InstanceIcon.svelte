<script lang="ts">
  import { Box } from "@lucide/svelte";

  let { size = 48 }: { size?: number } = $props();
</script>

<span class="icon" style:--size="{size}px">
  <Box size={size * 0.5} />
</span>

<style>
  .icon {
    width: var(--size);
    height: var(--size);
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border-radius: calc(var(--size) / 4);
    background: var(--bg-button);
    color: var(--text-muted);
  }
</style>
