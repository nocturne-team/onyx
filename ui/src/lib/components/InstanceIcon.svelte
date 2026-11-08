<script lang="ts">
  import { blockPixels, seedFromString } from "../blockIcon";
  import { customIconUrl, type Instance } from "../instances.svelte";

  interface Props {
    instance?: Instance;
    /** Previews in the create form: a seed or an image, without an instance. */
    seed?: number;
    src?: string | null;
    size?: number;
  }

  let { instance, seed, src, size = 48 }: Props = $props();

  const custom = $derived(src ?? (instance ? customIconUrl(instance) : null));
  const pixels = $derived(
    blockPixels(seed ?? instance?.iconSeed ?? seedFromString(instance?.id ?? "")),
  );
</script>

<span class="icon" style:--size="{size}px">
  {#if custom}
    <img src={custom} alt="" />
  {:else if instance?.icon && custom === undefined}
    <!-- Custom icon still loading; keep the space. -->
  {:else}
    <svg viewBox="0 0 8 8" shape-rendering="crispEdges" aria-hidden="true">
      {#each pixels as fill, i (i)}
        <rect x={i % 8} y={Math.floor(i / 8)} width="1" height="1" {fill} />
      {/each}
    </svg>
  {/if}
</span>

<style>
  .icon {
    width: var(--size);
    height: var(--size);
    flex-shrink: 0;
    display: block;
    overflow: hidden;
    border-radius: calc(var(--size) / 4);
    background: var(--bg-button);
  }

  img,
  svg {
    width: 100%;
    height: 100%;
    display: block;
  }

  img {
    object-fit: cover;
  }
</style>
