<script lang="ts">
  import { isTauri } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    ChevronLeft,
    ChevronRight,
    Compass,
    Copy,
    House,
    Library,
    Minus,
    Square,
    X,
  } from "@lucide/svelte";
  import logo from "../../assets/logo.svg";
  import { instanceById, instances, type Instance } from "../instances.svelte";
  import {
    canGoBack,
    canGoForward,
    goBack,
    historyBack,
    historyForward,
    nav,
    openInstance,
  } from "../navigation.svelte";
  import { browse } from "../pages/browseState.svelte";
  import type { Page } from "../nav";
  import type { ProjectType } from "../modrinth";
  import InstanceIcon from "./InstanceIcon.svelte";

  const win = isTauri() ? getCurrentWindow() : null;
  let maximized = $state(false);

  const games = $derived(Object.values(instances.games));
  const status = $derived.by(() => {
    if (games.length === 0) return "No instances running";
    if (games.length > 1) return `${games.length} instances running`;
    const name = instanceById(games[0].id)?.name ?? "Minecraft";
    return games[0].phase === "preparing" ? `Preparing ${name}` : name;
  });

  const discover: Record<ProjectType, string> = {
    mod: "Discover mods",
    modpack: "Discover modpacks",
    resourcepack: "Discover resource packs",
    shader: "Discover shaders",
  };

  interface Crumb {
    label: string;
    icon?: typeof House;
    instance?: Instance;
    imageUrl?: string | null;
    onclick?: () => void;
  }

  function instanceCrumb(id: string | null, onclick?: () => void): Crumb[] {
    const instance = instanceById(id);
    return instance ? [{ label: instance.name, instance, onclick }] : [];
  }

  function pageCrumbs(page: Page, instanceId: string | null, link: boolean): Crumb[] {
    switch (page) {
      case "home":
        return [{ label: "Home", icon: House, onclick: link ? goBack : undefined }];
      case "library":
        return [{ label: "Library", icon: Library, onclick: link ? goBack : undefined }];
      case "browse":
        if (browse.target) {
          return [
            ...instanceCrumb(browse.target.id, () => openInstance(browse.target!.id)),
            { label: "Discover content", icon: Compass, onclick: link ? goBack : undefined },
          ];
        }
        return [
          {
            label: discover[browse.projectType] ?? "Discover",
            icon: Compass,
            onclick: link ? goBack : undefined,
          },
        ];
      case "instance":
        return instanceCrumb(instanceId, link ? goBack : undefined);
      case "project":
        return [];
    }
  }

  const crumbs = $derived.by((): Crumb[] => {
    if (nav.page !== "project") return pageCrumbs(nav.page, nav.instanceId, false);
    const back = nav.back ?? { page: "browse" as Page, instanceId: null };
    return [
      ...pageCrumbs(back.page, back.instanceId, true),
      { label: nav.project?.title ?? "Project", imageUrl: nav.project?.iconUrl },
    ];
  });

  $effect(() => {
    if (!win) return;
    const sync = () => win.isMaximized().then((m) => (maximized = m));
    sync();
    const unlisten = win.onResized(sync);
    return () => {
      unlisten.then((f) => f());
    };
  });

  // Mouse back/forward buttons, like a browser.
  function onMouseUp(e: MouseEvent) {
    if (e.button === 3) historyBack();
    if (e.button === 4) historyForward();
  }
</script>

<svelte:window onmouseup={onMouseUp} />

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <img src={logo} alt="" width="26" height="26" />
    <span data-tauri-drag-region>Onyx</span>
  </div>

  <div class="arrows">
    <button aria-label="Back" title="Back" disabled={!canGoBack()} onclick={historyBack}>
      <ChevronLeft size={18} />
    </button>
    <button aria-label="Forward" title="Forward" disabled={!canGoForward()} onclick={historyForward}>
      <ChevronRight size={18} />
    </button>
  </div>

  <nav class="crumbs" aria-label="Location" data-tauri-drag-region>
    {#each crumbs as crumb, i (i)}
      {@const last = i === crumbs.length - 1}
      {#if i > 0}
        <ChevronRight size={16} class="separator" />
      {/if}
      <svelte:element
        this={crumb.onclick && !last ? "button" : "span"}
        class="crumb"
        class:current={last}
        role={crumb.onclick && !last ? undefined : "presentation"}
        onclick={crumb.onclick && !last ? crumb.onclick : undefined}
        data-tauri-drag-region={crumb.onclick && !last ? undefined : true}
      >
        {#if crumb.instance}
          <InstanceIcon instance={crumb.instance} size={20} />
        {:else if crumb.imageUrl}
          <img class="crumb-image" src={crumb.imageUrl} alt="" />
        {:else if crumb.icon}
          <crumb.icon size={18} />
        {/if}
        <span class="crumb-label">{crumb.label}</span>
      </svelte:element>
    {/each}
  </nav>

  <button
    class="status"
    title="Running instances"
    disabled={games.length === 0}
    onclick={() => games[0] && openInstance(games[0].id)}
  >
    <span class="dot" class:running={games.length > 0}></span>
    {status}
  </button>

  <div class="controls">
    <button aria-label="Minimize" onclick={() => win?.minimize()}>
      <Minus size={18} />
    </button>
    <button aria-label={maximized ? "Restore" : "Maximize"} onclick={() => win?.toggleMaximize()}>
      {#if maximized}
        <Copy size={15} />
      {:else}
        <Square size={15} />
      {/if}
    </button>
    <button class="close" aria-label="Close" onclick={() => win?.close()}>
      <X size={18} />
    </button>
  </div>
</header>

<style>
  .titlebar {
    height: var(--titlebar-height);
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding-left: 12px;
    background: var(--bg-shell);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 1.35rem;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: var(--text-strong);
  }

  .brand img {
    display: block;
    border-radius: 6px;
  }

  .arrows {
    display: flex;
    gap: 4px;
  }

  .arrows button {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border: 1px solid var(--border);
    border-radius: 50%;
    color: var(--text);
  }

  .arrows button:not(:disabled):hover {
    background: var(--bg-button);
    color: var(--text-strong);
  }

  .arrows button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .crumbs {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: 6px;
    color: var(--text-muted);
  }

  .crumbs :global(.separator) {
    flex-shrink: 0;
  }

  .crumb {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font-size: 1rem;
    font-weight: 500;
    color: var(--text);
  }

  .crumb.current {
    color: var(--text-strong);
  }

  button.crumb:hover .crumb-label {
    text-decoration: underline;
  }

  .crumb-label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .crumb-image {
    width: 20px;
    height: 20px;
    border-radius: 5px;
    object-fit: cover;
    flex-shrink: 0;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 16px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 0.9rem;
    color: var(--text);
    white-space: nowrap;
  }

  .status:disabled {
    cursor: default;
  }

  .status:not(:disabled):hover {
    background: var(--bg-button);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-muted);
  }

  .dot.running {
    background: var(--brand);
  }

  .controls {
    display: flex;
    align-self: stretch;
  }

  .controls button {
    width: 46px;
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }

  .controls button:hover {
    background: var(--bg-button);
    color: var(--text-strong);
  }

  .controls .close:hover {
    background: #e81123;
    color: #fff;
  }
</style>
