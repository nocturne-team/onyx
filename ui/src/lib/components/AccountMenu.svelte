<script lang="ts">
  import { Check, FolderKanban, KeyRound, LoaderCircle, LogOut, Plus, X } from "@lucide/svelte";
  import {
    addMinecraftAccount,
    cancelMinecraftLogin,
    minecraft,
    removeAccount,
    setActiveAccount,
  } from "../minecraft.svelte";
  import SkinHead from "./SkinHead.svelte";
  import {
    account,
    cancelSignIn,
    signInWithBrowser,
    signInWithToken,
    signOut,
  } from "../account.svelte";

  interface Props {
    onClose: () => void;
    onOpenProjects: () => void;
  }

  let { onClose, onOpenProjects }: Props = $props();

  let showToken = $state(false);
  let token = $state("");

  function submitToken(e: SubmitEvent) {
    e.preventDefault();
    signInWithToken(token).then(() => {
      if (account.status === "signed-in") token = "";
    });
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>

<div class="menu" role="dialog" aria-label="Accounts">
  <span class="section">Minecraft</span>
  {#each minecraft.accounts as mc (mc.id)}
    <div class="mc-row" class:active={mc.id === minecraft.active}>
      <button class="mc-account" onclick={() => setActiveAccount(mc.id)}>
        <SkinHead skinUrl={mc.skinUrl} size={28} />
        <span>{mc.name}</span>
        {#if mc.id === minecraft.active}<Check size={16} />{/if}
      </button>
      <button class="remove" title="Remove {mc.name}" onclick={() => removeAccount(mc.id)}>
        <X size={15} />
      </button>
    </div>
  {/each}
  {#if minecraft.signingIn}
    <div class="waiting">
      <LoaderCircle size={16} class="spin" />
      <span>Continue in your browser...</span>
      <button class="secondary" onclick={cancelMinecraftLogin}>Cancel</button>
    </div>
  {:else}
    <button class="item" onclick={addMinecraftAccount}>
      <Plus size={18} />
      {minecraft.accounts.length ? "Add another account" : "Sign in with Microsoft"}
    </button>
  {/if}
  {#if minecraft.error}
    <p class="error">{minecraft.error}</p>
  {/if}

  <div class="divider"></div>
  <span class="section">Modrinth</span>
  {#if account.status === "signed-in" && account.user}
    <div class="who">
      {#if account.user.avatarUrl}
        <img src={account.user.avatarUrl} alt="" />
      {/if}
      <div>
        <span class="muted">Signed in to Modrinth as</span>
        <strong>{account.user.username}</strong>
      </div>
    </div>
    <button class="item" onclick={onOpenProjects}><FolderKanban size={18} /> My projects</button>
    <button class="item" onclick={() => signOut()}><LogOut size={18} /> Sign out</button>
  {:else if account.status === "signing-in"}
    <p>Waiting for you to approve Onyx in your browser...</p>
    <button class="secondary" onclick={cancelSignIn}>Cancel</button>
  {:else}
    <p class="muted">See all of your projects, including drafts, private ones and ones in review.</p>
    <button class="primary" onclick={signInWithBrowser} disabled={account.status === "restoring"}>
      Sign in with Modrinth
    </button>

    {#if showToken}
      <form onsubmit={submitToken}>
        <input
          type="password"
          placeholder="Personal access token (mrp_...)"
          autocomplete="off"
          bind:value={token}
        />
        <button class="secondary" type="submit">Use token</button>
        <span class="muted small">
          Create one at modrinth.com/settings/pats with the Read user data and Read projects scopes.
        </span>
      </form>
    {:else}
      <button class="link" onclick={() => (showToken = true)}>
        <KeyRound size={15} /> Use a personal access token instead
      </button>
    {/if}
  {/if}

  {#if account.error}
    <p class="error">{account.error}</p>
  {/if}
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 5;
  }

  .menu {
    position: fixed;
    bottom: 12px;
    left: calc(var(--rail-width) + 4px);
    z-index: 6;
    width: 320px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.35);
  }

  .section {
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .divider {
    height: 1px;
    margin: 4px 0;
    background: var(--border);
  }

  .mc-row {
    display: flex;
    align-items: center;
    border-radius: var(--radius-sm);
  }

  .mc-row:hover,
  .mc-row.active {
    background: var(--bg-button);
  }

  .mc-account {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    font-weight: 600;
    color: var(--text-strong);
  }

  .mc-account span {
    flex: 1;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mc-account :global(svg) {
    color: var(--brand);
  }

  .remove {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    margin-right: 4px;
    border-radius: 50%;
    color: var(--text-muted);
    opacity: 0;
  }

  .mc-row:hover .remove {
    opacity: 1;
  }

  .remove:hover {
    background: var(--bg-button-hover);
    color: #ff6b6b;
  }

  .waiting {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 2px;
  }

  .waiting span {
    flex: 1;
  }

  p {
    margin: 0;
  }

  .muted {
    color: var(--text-muted);
  }

  .small {
    font-size: 0.82rem;
  }

  .who {
    display: flex;
    gap: 12px;
    align-items: center;
    padding-bottom: 6px;
  }

  .who img {
    width: 44px;
    height: 44px;
    border-radius: var(--radius);
  }

  .who div {
    display: flex;
    flex-direction: column;
    font-size: 0.9rem;
  }

  .who strong {
    color: var(--text-strong);
    font-size: 1.05rem;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    font-weight: 600;
  }

  .item:hover {
    background: var(--bg-button);
  }

  .primary,
  .secondary {
    padding: 9px 14px;
    border-radius: var(--radius-sm);
    font-weight: 600;
  }

  .primary {
    background: var(--brand-bg);
    color: var(--brand-text);
  }

  .primary:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .secondary {
    background: var(--bg-button);
  }

  .secondary:hover {
    background: var(--bg-button-hover);
  }

  .link {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .link:hover {
    color: var(--text-strong);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  input {
    height: 38px;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-strong);
    font: inherit;
    user-select: text;
  }

  input:focus {
    outline: none;
    border-color: var(--brand);
  }

  .error {
    color: #ff6b6b;
    font-size: 0.9rem;
  }
</style>
