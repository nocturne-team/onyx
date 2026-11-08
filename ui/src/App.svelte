<script lang="ts">
  import AccountMenu from "./lib/components/AccountMenu.svelte";
  import Rail from "./lib/components/Rail.svelte";
  import Titlebar from "./lib/components/Titlebar.svelte";
  import SettingsModal from "./lib/settings/SettingsModal.svelte";
  import Home from "./lib/pages/Home.svelte";
  import Browse from "./lib/pages/Browse.svelte";
  import Library from "./lib/pages/Library.svelte";
  import Instance from "./lib/pages/Instance.svelte";
  import Project from "./lib/pages/Project.svelte";
  import CreateInstanceModal from "./lib/components/CreateInstanceModal.svelte";
  import JoinModal from "./lib/components/JoinModal.svelte";
  import JoinRequests from "./lib/components/JoinRequests.svelte";
  import { loadSettings, settings } from "./lib/settings/store.svelte";
  import { nav, recordNavigation } from "./lib/navigation.svelte";
  import { library } from "./lib/pages/libraryState.svelte";
  import { restoreSession } from "./lib/account.svelte";
  import { initInstances } from "./lib/instances.svelte";
  import { resolveTheme } from "./lib/themes";
  import { loadMinecraftAccounts } from "./lib/minecraft.svelte";
  import { initInvites, invites } from "./lib/invites.svelte";

  let settingsOpen = $state(false);
  let accountOpen = $state(false);
  let createOpen = $state(false);
  let prefersDark = $state(window.matchMedia("(prefers-color-scheme: dark)").matches);

  loadSettings().catch((e) => console.error("failed to load settings", e));
  restoreSession();
  initInstances().catch((e) => console.error("failed to load instances", e));
  loadMinecraftAccounts().catch((e) => console.error("failed to load Minecraft accounts", e));
  initInvites();

  // Feeds the titlebar's back/forward arrows.
  $effect(recordNavigation);

  function openMyProjects() {
    library.tab = "projects";
    nav.page = "library";
    accountOpen = false;
  }

  $effect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => (prefersDark = query.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  });

  $effect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolveTheme(settings.theme, prefersDark);
    if (settings.accent) root.dataset.accent = settings.accent;
    else delete root.dataset.accent;
    root.dataset.advanced = String(settings.advancedRendering);
  });
</script>

<div class="shell">
  <Titlebar />
  <div class="body">
    <Rail
      onOpenSettings={() => (settingsOpen = true)}
      onOpenAccount={() => (accountOpen = !accountOpen)}
      onCreateInstance={() => (createOpen = true)}
    />
    <main>
      {#if nav.page === "home"}
        <Home />
      {:else if nav.page === "browse"}
        <Browse />
      {:else if nav.page === "library"}
        <Library onCreateInstance={() => (createOpen = true)} />
      {:else if nav.page === "project" && nav.projectId}
        <Project id={nav.projectId} />
      {:else if nav.instanceId}
        {#key nav.instanceId}
          <Instance id={nav.instanceId} />
        {/key}
      {/if}
    </main>
  </div>
</div>

{#if accountOpen}
  <AccountMenu onClose={() => (accountOpen = false)} onOpenProjects={openMyProjects} />
{/if}

{#if createOpen}
  <CreateInstanceModal onClose={() => (createOpen = false)} />
{/if}

{#if invites.offer}
  {#key invites.offer}
    <JoinModal offer={invites.offer} />
  {/key}
{/if}

<JoinRequests />

{#if settingsOpen}
  <SettingsModal onClose={() => (settingsOpen = false)} />
{/if}

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  main {
    flex: 1;
    min-width: 0;
    /* `scroll`, not `auto`: when a page starts short and grows past the
       window (Home while loading), WebKitGTK adds the scrollbar without
       reflowing, then reflows on some later restyle and every card shifts.
       Only the thumb is styled, so nothing shows when there's nothing to
       scroll. */
    overflow-y: scroll;
    background: var(--bg);
    border-top: 1px solid var(--border);
    border-left: 1px solid var(--border);
    border-top-left-radius: var(--radius-lg);
  }
</style>
