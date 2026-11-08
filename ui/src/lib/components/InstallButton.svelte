<script lang="ts">
  import { Check, Download, LoaderCircle } from "@lucide/svelte";
  import { installKey, installModpack, installToInstance, installs } from "../content.svelte";
  import type { SearchHit } from "../modrinth";
  import { openInstance } from "../navigation.svelte";
  import { browse } from "../pages/browseState.svelte";
  import InstallPopup from "./InstallPopup.svelte";

  let { hit }: { hit: SearchHit } = $props();

  let popupOpen = $state(false);
  let packBusy = $state(false);
  let packError = $state<string | null>(null);

  const targetState = $derived(
    browse.target ? installs[installKey(browse.target.id, hit.projectId)] : undefined,
  );

  async function click() {
    if (hit.projectType === "modpack") {
      packBusy = true;
      packError = null;
      try {
        openInstance((await installModpack(hit.projectId)).id);
      } catch (e) {
        packError = String(e);
      } finally {
        packBusy = false;
      }
    } else if (browse.target) {
      installToInstance(browse.target.id, hit.projectId, hit.projectType);
    } else {
      popupOpen = true;
    }
  }
</script>

<!-- Rows and cards around this open the project page on click; keep
     install clicks (and the popup) from reaching them. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<span class="wrap" onclick={(e) => e.stopPropagation()}>
{#if targetState === "installed"}
  <span class="button done"><Check size={16} /> Installed</span>
{:else if targetState === "installing" || packBusy}
  <button class="button" disabled><LoaderCircle size={16} class="spin" /> Installing</button>
{:else}
  <button
    class="button"
    onclick={click}
    title={typeof targetState === "object" ? targetState.error : (packError ?? undefined)}
    class:failed={typeof targetState === "object" || packError}
  >
    <Download size={16} /> Install
  </button>
{/if}

{#if popupOpen}
  <InstallPopup {hit} onClose={() => (popupOpen = false)} />
{/if}
</span>

<style>
  .wrap {
    display: contents;
  }

  .button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 14px;
    border-radius: var(--radius-sm);
    background: var(--brand-bg);
    color: var(--brand-text);
    font-weight: 600;
    white-space: nowrap;
  }

  .button:disabled {
    background: var(--bg-button);
    color: var(--text-strong);
    cursor: default;
  }

  .button.failed {
    background: #c9363b;
  }

  .done {
    background: none;
    color: var(--brand);
  }
</style>
