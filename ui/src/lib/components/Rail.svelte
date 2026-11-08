<script lang="ts">
  import { House, Library, Plus, Search, Settings, User } from "@lucide/svelte";
  import { account } from "../account.svelte";
  import { instances } from "../instances.svelte";
  import { activeAccount } from "../minecraft.svelte";
  import { nav, openInstance } from "../navigation.svelte";
  import { browse, runSearch } from "../pages/browse.svelte";
  import InstanceIcon from "./InstanceIcon.svelte";
  import SkinHead from "./SkinHead.svelte";

  interface Props {
    onOpenSettings: () => void;
    onOpenAccount: () => void;
    onCreateInstance: () => void;
  }

  let { onOpenSettings, onOpenAccount, onCreateInstance }: Props = $props();

  const pinned = $derived(instances.list.slice(0, 5));
  const minecraftAccount = $derived(activeAccount());

  const items = [
    { id: "home", label: "Home", icon: House },
    { id: "browse", label: "Browse", icon: Search },
    { id: "library", label: "Library", icon: Library },
  ] as const;
</script>

<nav class="rail" data-tauri-drag-region>
  <button
    class="avatar"
    title="Accounts"
    aria-label="Accounts"
    onclick={onOpenAccount}
  >
    {#if minecraftAccount}
      <SkinHead skinUrl={minecraftAccount.skinUrl} size={30} />
    {:else if account.user?.avatarUrl}
      <img src={account.user.avatarUrl} alt="" />
    {:else}
      <User size={22} />
    {/if}
  </button>

  {#each items as item (item.id)}
    <button
      class="nav-button"
      class:active={nav.page === item.id}
      title={item.label}
      aria-label={item.label}
      aria-current={nav.page === item.id ? "page" : undefined}
      onclick={() => {
        // The rail's Browse is the general one, not "install into X".
        if (item.id === "browse" && browse.target) {
          browse.target = null;
          runSearch();
        }
        nav.page = item.id;
      }}
    >
      <item.icon size={22} />
    </button>
  {/each}

  {#if pinned.length}
    <div class="divider"></div>
    {#each pinned as instance (instance.id)}
      <button
        class="instance"
        class:active={nav.page === "instance" && nav.instanceId === instance.id}
        title={instance.name}
        aria-label={instance.name}
        onclick={() => openInstance(instance.id)}
      >
        <InstanceIcon {instance} size={36} />
        {#if instances.games[instance.id]}
          <span class="running" class:preparing={instances.games[instance.id].phase === "preparing"}></span>
        {/if}
      </button>
    {/each}
  {/if}

  <div class="spacer" data-tauri-drag-region></div>

  <button class="new-instance" title="New instance" aria-label="New instance" onclick={onCreateInstance}>
    <Plus size={24} />
  </button>
  <button class="nav-button" title="Settings" aria-label="Settings" onclick={onOpenSettings}>
    <Settings size={22} />
  </button>
</nav>

<style>
  .rail {
    width: var(--rail-width);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 12px 0;
    background: var(--bg-shell);
  }

  .spacer {
    flex: 1;
    align-self: stretch;
  }

  button {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
    transition:
      background 120ms,
      color 120ms;
  }

  .divider {
    width: 32px;
    height: 1px;
    margin: 4px 0;
    background: var(--border);
  }

  .instance {
    position: relative;
  }

  .instance:hover,
  .instance.active {
    background: var(--bg-button);
  }

  .running {
    position: absolute;
    right: 4px;
    bottom: 4px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--brand);
    border: 2px solid var(--bg-shell);
  }

  .running.preparing {
    background: #f0a830;
  }

  .avatar {
    background: var(--bg-raised);
    color: var(--text-muted);
    margin-bottom: 8px;
    overflow: hidden;
  }

  .avatar:hover {
    color: var(--text-strong);
  }

  .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .nav-button {
    color: var(--text-muted);
  }

  .nav-button:hover {
    background: var(--bg-button);
    color: var(--text-strong);
  }

  .nav-button.active {
    background: var(--bg-button);
    color: var(--text-strong);
  }

  .new-instance {
    background: var(--brand-bg);
    color: var(--brand-text);
  }

  .new-instance:hover {
    filter: brightness(1.1);
  }
</style>
