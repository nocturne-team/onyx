<script lang="ts">
  import { Check, Download, LoaderCircle, X } from "@lucide/svelte";
  import { canInstallInto, installKey, installToInstance, installs } from "../content.svelte";
  import { instances, loaderNames, type Instance } from "../instances.svelte";
  import type { SearchHit } from "../modrinth";
  import InstanceIcon from "./InstanceIcon.svelte";

  interface Props {
    hit: SearchHit;
    /** A specific version to install; otherwise the newest compatible one. */
    version?: { id: string; versionNumber: string; gameVersions: string[]; loaders: string[] };
    onClose: () => void;
  }

  let { hit, version, onClose }: Props = $props();

  /** Why `instance` can't take this, or null if it can. */
  function blocker(instance: Instance): string | null {
    if (!canInstallInto(instance, hit.projectType)) return "needs a mod loader";
    if (!version) return null;
    if (!version.gameVersions.includes(instance.gameVersion)) return `not for ${instance.gameVersion}`;
    const loaders = instance.loader === "quilt" ? ["quilt", "fabric"] : [instance.loader];
    if (hit.projectType === "mod" && !version.loaders.some((l) => loaders.includes(l))) {
      return `not for ${loaderNames[instance.loader]}`;
    }
    return null;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div
    class="modal"
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-labelledby="install-title"
    onclick={(e) => e.stopPropagation()}
  >
    <header>
      <h2 id="install-title">Install {hit.title}{version ? ` ${version.versionNumber}` : ""}</h2>
      <button class="close" aria-label="Close" onclick={onClose}><X size={18} /></button>
    </header>
    <div class="body">
      {#if instances.list.length === 0}
        <p class="muted">Create an instance first.</p>
      {/if}
      {#each instances.list as instance (instance.id)}
        {@const state = installs[installKey(instance.id, hit.projectId)]}
        {@const problem = blocker(instance)}
        {@const compatible = problem === null}
        <div class="row">
          <InstanceIcon {instance} size={40} />
          <div class="main">
            <strong>{instance.name}</strong>
            <span class="muted">
              {loaderNames[instance.loader]}
              {instance.gameVersion}
              {#if problem}· {problem}{/if}
            </span>
            {#if typeof state === "object"}<span class="error">{state.error}</span>{/if}
          </div>
          {#if state === "installed"}
            <span class="done"><Check size={16} /> Installed</span>
          {:else if state === "installing"}
            <button class="install" disabled><LoaderCircle size={16} class="spin" /> Installing</button>
          {:else}
            <button
              class="install"
              disabled={!compatible}
              onclick={() =>
                installToInstance(instance.id, hit.projectId, hit.projectType, version?.id ?? null)}
            >
              <Download size={16} /> Install
            </button>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    background: var(--overlay);
    backdrop-filter: var(--blur);
  }

  .modal {
    width: min(520px, calc(100vw - 48px));
    max-height: calc(100vh - 96px);
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
    gap: 12px;
    padding: 18px 22px;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    font-size: 1.15rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .close {
    width: 36px;
    height: 36px;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--bg-button);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px;
    overflow-y: auto;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px;
    border-radius: var(--radius);
  }

  .row:hover {
    background: var(--bg-button);
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  strong {
    color: var(--text-strong);
  }

  .muted {
    color: var(--text-muted);
    font-size: 0.88rem;
  }

  .error {
    color: #ff6b6b;
    font-size: 0.85rem;
  }

  .install,
  .done {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 12px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    white-space: nowrap;
  }

  .install {
    background: var(--brand-bg);
    color: var(--brand-text);
  }

  .install:disabled {
    background: var(--bg-button);
    color: var(--text-muted);
    cursor: default;
  }

  .done {
    color: var(--brand);
  }
</style>
