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
  import { loadSettings, settings } from "./lib/settings/store.svelte";
  import { pageTitles } from "./lib/nav";
  import { nav } from "./lib/navigation.svelte";
  import { library } from "./lib/pages/library.svelte";
  import { browse } from "./lib/pages/browse.svelte";
  import { restoreSession } from "./lib/account.svelte";
  import { initInstances, instanceById } from "./lib/instances.svelte";
  import { loadMinecraftAccounts } from "./lib/minecraft.svelte";

  let settingsOpen = $state(false);
  let accountOpen = $state(false);
  let createOpen = $state(false);
  let prefersDark = $state(window.matchMedia("(prefers-color-scheme: dark)").matches);

  loadSettings().catch((e) => console.error("failed to load settings", e));
  restoreSession();
  initInstances().catch((e) => console.error("failed to load instances", e));
  loadMinecraftAccounts().catch((e) => console.error("failed to load Minecraft accounts", e));

  const title = $derived(
    nav.page === "instance"
      ? (instanceById(nav.instanceId)?.name ?? "Instance")
      : nav.page === "project"
        ? "Project"
        : nav.page === "browse" && browse.target
        ? `${browse.target.name} › Browse content`
        : pageTitles[nav.page],
  );

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
    root.dataset.theme =
      settings.theme === "system" ? (prefersDark ? "dark" : "light") : settings.theme;
    root.dataset.advanced = String(settings.advancedRendering);
  });
</script>

<div class="shell">
  <Rail
    onOpenSettings={() => (settingsOpen = true)}
    onOpenAccount={() => (accountOpen = !accountOpen)}
    onCreateInstance={() => (createOpen = true)}
  />
  <div class="column">
    <Titlebar {title} />
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

{#if settingsOpen}
  <SettingsModal onClose={() => (settingsOpen = false)} />
{/if}

<style>
  .shell {
    display: flex;
    height: 100%;
  }

  .column {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  main {
    flex: 1;
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
