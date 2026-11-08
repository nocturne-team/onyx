<script lang="ts">
  import { LoaderCircle, Play, Plus, X } from "@lucide/svelte";
  import { installModpack } from "../content.svelte";
  import {
    createInstance,
    instanceById,
    instances,
    loaderNames,
    loaderVersions,
  } from "../instances.svelte";
  import { fitLabels, invites, joinInvite, type InviteOffer } from "../invites.svelte";
  import { openInstance } from "../navigation.svelte";
  import InstanceIcon from "./InstanceIcon.svelte";

  let { offer }: { offer: InviteOffer } = $props();

  const invite = $derived(offer.invite);
  const candidates = $derived(
    offer.candidates
      .map((c) => ({ ...c, instance: instanceById(c.id) }))
      .filter((c) => c.instance),
  );
  let picked = $state<string | null>(null);
  const selected = $derived(picked ?? candidates[0]?.id ?? null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  function close() {
    invites.offer = null;
  }

  async function join(id: string) {
    if (!invite) return;
    if (instances.games[id]) {
      // Already running: the game can't be told to join from outside.
      await navigator.clipboard.writeText(invite.address).catch(() => {});
      error = `That instance is already running. The address ${invite.address} is on your clipboard; use Multiplayer → Direct Connection.`;
      return;
    }
    busy = true;
    error = null;
    try {
      openInstance(id);
      close();
      await joinInvite(id, invite.address);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  /** No fitting instance: make one, then join with it. */
  async function create() {
    if (!invite) return;
    busy = true;
    error = null;
    try {
      if (invite.pack) {
        // Installs in the background; joining waits until the user presses Play.
        const instance = await installModpack(invite.pack.projectId);
        openInstance(instance.id);
        close();
        return;
      }
      let loaderVersion: string | null = null;
      if (invite.loader !== "vanilla") {
        const versions = await loaderVersions(invite.loader, invite.gameVersion);
        loaderVersion = (versions.find((v) => v.stable) ?? versions[0])?.version ?? null;
        if (!loaderVersion) {
          throw `${loaderNames[invite.loader]} has no build for ${invite.gameVersion}.`;
        }
      }
      const name =
        invite.loader === "vanilla"
          ? invite.gameVersion
          : `${loaderNames[invite.loader]} ${invite.gameVersion}`;
      const instance = await createInstance(
        name,
        invite.gameVersion,
        invite.loader,
        loaderVersion,
        null,
        null,
      );
      await join(instance.id);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !busy && close()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={() => !busy && close()}>
  <div
    class="modal"
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-labelledby="join-title"
    onclick={(e) => e.stopPropagation()}
  >
    <header>
      <h2 id="join-title">{invite ? "Join a game" : "Invite link"}</h2>
      <button class="close" aria-label="Close" onclick={close} disabled={busy}><X size={18} /></button>
    </header>
    <div class="body">
      {#if !invite}
        <p class="error">{offer.error}</p>
      {:else}
        <p>
          <strong>{invite.address}</strong>
          <span class="muted">
            · {loaderNames[invite.loader]}
            {invite.gameVersion}{#if invite.pack}, modpack{/if}
          </span>
        </p>
        <p class="muted small">
          Only join people you trust. The server sees your Minecraft name.
        </p>

        {#if candidates.length}
          <div class="list">
            {#each candidates as c (c.id)}
              {@const instance = c.instance!}
              <button
                class="row"
                class:selected={selected === c.id}
                onclick={() => (picked = c.id)}
                ondblclick={() => join(c.id)}
              >
                <InstanceIcon {instance} size={40} />
                <span class="main">
                  <strong>{instance.name}</strong>
                  <span class="muted">
                    {loaderNames[instance.loader]}
                    {instance.gameVersion} · {fitLabels[c.fit]}
                  </span>
                </span>
              </button>
            {/each}
          </div>
        {:else}
          <p class="muted">
            None of your instances run {invite.pack ? "this modpack" : invite.gameVersion}.
          </p>
        {/if}

        {#if error}<p class="error">{error}</p>{/if}

        <div class="actions">
          <button class="secondary" disabled={busy} onclick={create}>
            <Plus size={16} />
            {invite.pack ? "Install the modpack" : `New ${loaderNames[invite.loader]} instance`}
          </button>
          <button
            class="primary"
            disabled={busy || !selected}
            onclick={() => selected && join(selected)}
          >
            {#if busy}<LoaderCircle size={16} class="spin" />{:else}<Play size={16} />{/if}
            Join
          </button>
        </div>
      {/if}
    </div>
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
    gap: 10px;
    padding: 16px 22px 20px;
    overflow-y: auto;
  }

  strong {
    color: var(--text-strong);
  }

  .muted {
    color: var(--text-muted);
  }

  .small {
    font-size: 0.85rem;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0 -8px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px;
    border-radius: var(--radius);
    text-align: left;
    background: none;
    border: 1px solid transparent;
  }

  .row:hover {
    background: var(--bg-button);
  }

  .row.selected {
    background: var(--brand-highlight);
    border-color: var(--brand);
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .main .muted {
    font-size: 0.88rem;
  }

  .error {
    color: #ff6b6b;
    font-size: 0.9rem;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }

  .actions button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 36px;
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
  }

  .actions button:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
