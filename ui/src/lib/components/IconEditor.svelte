<script lang="ts">
  import { Check, Info, RefreshCw, Save, X } from "@lucide/svelte";
  import {
    backgroundCss,
    backgrounds,
    categoryNames,
    randomIconConfig,
    symbols,
    type IconConfig,
    type SymbolCategory,
  } from "../iconCatalog";
  import InstanceIcon from "./InstanceIcon.svelte";

  interface Props {
    initial: IconConfig;
    onSave: (config: IconConfig) => void;
    onClose: () => void;
  }

  let { initial, onSave, onClose }: Props = $props();

  // A copy, so Cancel leaves the caller's config alone.
  // svelte-ignore state_referenced_locally
  let config = $state<IconConfig>({ ...initial });

  const categories: SymbolCategory[] = ["loader", "modded", "vanilla"];
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div
    class="modal"
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-labelledby="icon-editor-title"
    onclick={(e) => e.stopPropagation()}
  >
    <header>
      <h2 id="icon-editor-title">Icon editor</h2>
      <button class="close" aria-label="Close" onclick={onClose}><X size={18} /></button>
    </header>

    <div class="body">
      <aside>
        <div class="preview">
          <InstanceIcon {config} size={132} />
          <div class="sizes">
            <InstanceIcon {config} size={40} />
            <InstanceIcon {config} size={30} />
            <InstanceIcon {config} size={20} />
          </div>
        </div>
        <button class="secondary" onclick={() => (config = randomIconConfig())}>
          <RefreshCw size={16} /> Randomize
        </button>
      </aside>

      <div class="options">
        <section>
          <h3>Background</h3>
          <div class="backgrounds">
            {#each backgrounds as bg (bg.id)}
              <button
                class="swatch"
                class:selected={config.background === bg.id}
                style:background-image={backgroundCss(bg.id)}
                title={bg.name}
                aria-label={bg.name}
                onclick={() => (config.background = bg.id)}
              >
                {#if config.background === bg.id}<Check size={20} />{/if}
              </button>
            {/each}
          </div>
        </section>

        {#each categories as category (category)}
          <section>
            <h3>{categoryNames[category]}</h3>
            <div class="symbols">
              {#each symbols.filter((s) => s.category === category) as symbol (symbol.id)}
                <button
                  class="symbol"
                  class:selected={config.symbol === symbol.id}
                  title={symbol.name}
                  aria-label={symbol.name}
                  onclick={() => (config.symbol = symbol.id)}
                >
                  <img src={symbol.url} alt="" draggable="false" loading="lazy" />
                </button>
              {/each}
            </div>
          </section>
        {/each}
      </div>
    </div>

    <footer>
      <span class="hint"><Info size={16} /> Mix and match a background and a symbol.</span>
      <button class="secondary" onclick={onClose}><X size={16} /> Cancel</button>
      <button class="primary" onclick={() => onSave({ ...config })}><Save size={16} /> Save icon</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: center;
    background: var(--overlay);
    backdrop-filter: var(--blur);
  }

  .modal {
    width: min(920px, calc(100vw - 48px));
    height: min(700px, calc(100vh - 48px));
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.4);
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 22px;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    font-size: 1.15rem;
  }

  .close {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--bg-button);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  aside {
    width: 240px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 22px;
    border-right: 1px solid var(--border);
  }

  .preview {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
  }

  .sizes {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .options {
    flex: 1;
    overflow-y: auto;
  }

  section {
    padding: 18px 22px;
    border-bottom: 1px solid var(--border);
  }

  section:last-child {
    border-bottom: none;
  }

  h3 {
    font-size: 1rem;
    color: var(--text-strong);
    margin-bottom: 12px;
  }

  .backgrounds {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .swatch {
    width: 64px;
    height: 64px;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
    color: #fff;
    border: 2px solid transparent;
  }

  .swatch.selected {
    border-color: var(--text-strong);
  }

  .swatch :global(svg) {
    filter: drop-shadow(0 1px 2px rgb(0 0 0 / 0.5));
  }

  .symbols {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(84px, 1fr));
    gap: 10px;
  }

  .symbol {
    aspect-ratio: 1;
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .symbol:hover {
    background: var(--bg-button);
  }

  .symbol.selected {
    border-color: var(--brand);
    background: var(--brand-highlight);
  }

  .symbol img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 22px;
    border-top: 1px solid var(--border);
  }

  .hint {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .primary,
  .secondary {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 38px;
    padding: 0 14px;
    border-radius: var(--radius-sm);
    font-weight: 600;
  }

  .primary {
    background: var(--brand-bg);
    color: var(--brand-text);
  }

  .secondary {
    background: var(--bg-button);
    border: 1px solid var(--border);
  }

  .secondary:hover {
    background: var(--bg-button-hover);
  }
</style>
