<script lang="ts">
  import { backgroundCss, defaultIconConfig, symbolUrl, type IconConfig } from "../iconCatalog";
  import { customIconUrl, type Instance } from "../instances.svelte";

  interface Props {
    instance?: Instance;
    /** Previews (create form, icon editor): a built-in icon or an image, without an instance. */
    config?: IconConfig;
    src?: string | null;
    size?: number;
  }

  let { instance, config, src, size = 48 }: Props = $props();

  const custom = $derived(src ?? (instance ? customIconUrl(instance) : null));
  const builtIn = $derived(
    config ?? instance?.iconConfig ?? defaultIconConfig(instance?.id ?? ""),
  );
</script>

<span class="icon" style:--size="{size}px">
  {#if custom}
    <img src={custom} alt="" />
  {:else if instance?.icon && custom === undefined}
    <!-- Custom icon still loading; keep the space. -->
  {:else}
    <span class="built-in" style:background-image={backgroundCss(builtIn.background)}>
      {#if symbolUrl(builtIn.symbol)}
        <img src={symbolUrl(builtIn.symbol)} alt="" draggable="false" />
      {/if}
    </span>
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

  .built-in {
    display: block;
    width: 100%;
    height: 100%;
  }

  img {
    width: 100%;
    height: 100%;
    display: block;
    object-fit: cover;
    user-select: none;
  }
</style>
