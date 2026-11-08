<script lang="ts">
  import { Compass, House, Library, Plus, Settings, User } from "@lucide/svelte";
  import { account } from "../account.svelte";
  import { instances } from "../instances.svelte";
  import { activeAccount } from "../minecraft.svelte";
  import { nav, openInstance } from "../navigation.svelte";
  import { browse, runSearch } from "../pages/browseState.svelte";
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
    { id: "browse", label: "Browse", icon: Compass },
    { id: "library", label: "Library", icon: Library },
  ] as const;

  // Installing into an instance from Browse or opening one of its projects
  // still counts as being in that instance.
  const activeInstance = $derived(
    nav.page === "instance"
      ? nav.instanceId
      : nav.page === "browse" && browse.target
        ? browse.target.id
        : null,
  );
</script>

<nav class="rail" data-tauri-drag-region>
  {#each items as item (item.id)}
    {@const active = nav.page === item.id && !(item.id === "browse" && browse.target)}
    <button
      class="nav-button"
      class:active
      title={item.label}
      aria-label={item.label}
      aria-current={active ? "page" : undefined}
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

  <div class="divider"></div>

  {#each pinned as instance (instance.id)}
    <button
      class="instance"
      class:active={activeInstance === instance.id}
      title={instance.name}
      aria-label={instance.name}
      onclick={() => openInstance(instance.id)}
    >
      <InstanceIcon {instance} size={30} />
      {#if instances.games[instance.id]}
        <span class="running" class:preparing={instances.games[instance.id].phase === "preparing"}></span>
      {/if}
    </button>
  {/each}

  {#if pinned.length}
    <div class="divider"></div>
  {/if}

  <button class="nav-button" title="New instance" aria-label="New instance" onclick={onCreateInstance}>
    <Plus size={22} />
  </button>

  <div class="spacer" data-tauri-drag-region></div>

  <button class="nav-button" title="Settings" aria-label="Settings" onclick={onOpenSettings}>
    <Settings size={22} />
  </button>
  <button class="nav-button avatar" title="Accounts" aria-label="Accounts" onclick={onOpenAccount}>
    {#if minecraftAccount}
      <SkinHead skinUrl={minecraftAccount.skinUrl} size={28} />
    {:else if account.user?.avatarUrl}
      <img src={account.user.avatarUrl} alt="" />
    {:else}
      <User size={22} />
    {/if}
  </button>
</nav>

<style>
  .rail {
    width: var(--rail-width);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 0 12px;
    background: var(--bg-shell);
    overflow-y: auto;
    scrollbar-width: none;
  }

  .spacer {
    flex: 1;
    align-self: stretch;
  }

  button {
    flex-shrink: 0;
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text);
    transition:
      background 120ms,
      color 120ms;
  }

  button:hover {
    background: var(--bg-button);
    color: var(--text-strong);
  }

  button.active {
    background: var(--brand-highlight);
    color: var(--brand);
  }

  .divider {
    flex-shrink: 0;
    width: 32px;
    height: 1px;
    margin: 6px 0;
    background: var(--border);
  }

  .instance {
    position: relative;
  }

  .running {
    position: absolute;
    right: 6px;
    bottom: 6px;
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
    overflow: hidden;
  }

  .avatar img {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    object-fit: cover;
  }
</style>
