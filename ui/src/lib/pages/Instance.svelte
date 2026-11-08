<script lang="ts">
  import {
    Clock,
    FolderOpen,
    Gamepad2,
    ImageUp,
    ArrowUpCircle,
    Package,
    Palette,
    RefreshCw,
    ScrollText,
    SlidersHorizontal,
    Tag,
    Trash2,
    TriangleAlert,
  } from "@lucide/svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { tick } from "svelte";
  import InstanceIcon from "../components/InstanceIcon.svelte";
  import ContentList from "../components/ContentList.svelte";
  import PlayButton from "../components/PlayButton.svelte";
  import { relativeTime } from "../format";
  import IconEditor from "../components/IconEditor.svelte";
  import MemorySetting from "../components/MemorySetting.svelte";
  import { appInfo, settings } from "../settings/store.svelte";
  import { checkPackUpdate, updateModpack, type PackUpdate } from "../content.svelte";
  import { defaultIconConfig, randomIconConfig, type IconConfig } from "../iconCatalog";
  import {
    deleteInstance,
    instanceById,
    instances,
    loaderNames,
    pickIconFile,
    type ContentKind,
    setInstanceIcon,
    setInstanceIconConfig,
    setInstanceMemory,
  } from "../instances.svelte";
  import { nav } from "../navigation.svelte";

  let { id }: { id: string } = $props();

  const instance = $derived(instanceById(id));
  const game = $derived(instances.games[id]);
  const error = $derived(instances.errors[id]);

  let tab = $state<"content" | "logs" | "options">("content");
  let optionsError = $state<string | null>(null);
  const info = appInfo();
  let contentKind = $state<ContentKind>("mods");
  // Vanilla can't load mods, so it starts on resource packs.
  $effect(() => {
    if (instance?.loader === "vanilla" && contentKind === "mods") contentKind = "resourcepacks";
  });
  const contentKinds = $derived(
    (
      [
        ["mods", "Mods"],
        ["resourcepacks", "Resource packs"],
        ["shaderpacks", "Shaders"],
      ] as [ContentKind, string][]
    ).filter(([kind]) => kind !== "mods" || instance?.loader !== "vanilla"),
  );
  let confirmingDelete = $state(false);
  let iconMenu = $state(false);
  let iconError = $state<string | null>(null);

  async function uploadIcon() {
    iconMenu = false;
    const path = await pickIconFile();
    if (!path) return;
    try {
      await setInstanceIcon(id, path);
      iconError = null;
    } catch (e) {
      iconError = String(e);
    }
  }

  let editingIcon = $state(false);

  let packUpdate = $state<PackUpdate | null>(null);
  let packError = $state<string | null>(null);
  // Re-check whenever the installed pack version changes (e.g. after updating).
  $effect(() => {
    const version = instance?.pack?.versionId;
    packUpdate = null;
    if (!version) return;
    checkPackUpdate(id)
      .then((update) => (packUpdate = update))
      .catch((e) => console.warn("couldn't check for a modpack update", e));
  });

  async function runPackUpdate() {
    if (!packUpdate) return;
    packError = null;
    try {
      await updateModpack(id, packUpdate.versionId);
      packUpdate = null;
    } catch (e) {
      packError = String(e);
    }
  }

  async function saveIconConfig(config: IconConfig) {
    iconMenu = false;
    editingIcon = false;
    try {
      await setInstanceIconConfig(id, config);
      iconError = null;
    } catch (e) {
      iconError = String(e);
    }
  }
  let deleteError = $state<string | null>(null);

  // The log is kept outside Svelte's reactivity (it can be thousands of lines)
  // and written to the <pre> directly.
  let logEl = $state<HTMLPreElement>();
  let lines: string[] = [];
  let follow = true;

  function render() {
    if (!logEl) return;
    logEl.textContent = lines.join("\n");
    if (follow) logEl.scrollTop = logEl.scrollHeight;
  }

  $effect(() => {
    if (!isTauri()) return;
    const current = id;
    lines = [];
    let pending = false;
    invoke<string[]>("game_log", { id: current }).then((log) => {
      lines = log.concat(lines);
      render();
    });
    const unlisten = listen<[string, string]>("instance:log", ({ payload: [from, line] }) => {
      if (from !== current) return;
      lines.push(line);
      if (lines.length > 5000) lines.splice(0, lines.length - 5000);
      // Batch renders: games can log hundreds of lines per second.
      if (!pending) {
        pending = true;
        requestAnimationFrame(() => {
          pending = false;
          render();
        });
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  });

  $effect(() => {
    if (tab === "logs") tick().then(render);
  });

  function onScroll() {
    if (!logEl) return;
    follow = logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 40;
  }

  async function remove() {
    try {
      await deleteInstance(id);
      nav.page = "library";
    } catch (e) {
      deleteError = String(e);
    }
  }

  function playTime(secs: number): string {
    if (secs < 60) return "less than a minute";
    const hours = Math.floor(secs / 3600);
    const minutes = Math.floor((secs % 3600) / 60);
    return hours ? `${hours}h ${minutes}m` : `${minutes}m`;
  }
</script>

{#if !instance}
  <p class="note">
    {instances.loaded ? "This instance doesn't exist anymore." : "Loading..."}
  </p>
{:else}
  <div class="page">
    <header>
      <div class="icon-wrap">
        <button class="icon-button-edit" title="Change icon" onclick={() => (iconMenu = !iconMenu)}>
          <InstanceIcon {instance} size={64} />
        </button>
        {#if iconMenu}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="menu-backdrop" onclick={() => (iconMenu = false)}></div>
          <div class="icon-menu">
            <button onclick={uploadIcon}><ImageUp size={16} /> Upload image</button>
            <button
              onclick={() => {
                iconMenu = false;
                editingIcon = true;
              }}><Palette size={16} /> Customize</button
            >
            <button onclick={() => saveIconConfig(randomIconConfig())}>
              <RefreshCw size={16} /> Randomize
            </button>
          </div>
        {/if}
      </div>
      <div class="title">
        <h2>{instance.name}</h2>
        <div class="meta">
          <span>
            <Tag size={15} />
            {loaderNames[instance.loader]}
            {instance.gameVersion}{#if instance.loaderVersion}<span class="muted">
                ({instance.loaderVersion})</span
              >{/if}
          </span>
          <span class="dot"></span>
          <span>
            <Clock size={15} />
            {instance.lastPlayed
              ? `Played ${relativeTime(new Date(instance.lastPlayed * 1000))}`
              : "Never played"}
          </span>
          {#if instance.pack}
            <span class="dot"></span>
            <span><Package size={15} /> Modpack {instance.pack.versionNumber}</span>
          {/if}
          {#if instance.playedSecs > 0}
            <span class="dot"></span>
            <span>{playTime(instance.playedSecs)} total</span>
          {/if}
        </div>
      </div>
      <div class="actions">
        <PlayButton {id} />
        <button
          class="icon-button"
          title="Open folder"
          onclick={() => invoke("open_instance_folder", { id })}
        >
          <FolderOpen size={20} />
        </button>
        <button
          class="icon-button danger"
          title="Delete instance"
          onclick={() => (confirmingDelete = true)}
        >
          <Trash2 size={20} />
        </button>
      </div>
    </header>

    {#if game?.phase === "preparing" && game.stage}
      <div class="banner">
        {game.stage}
        {#if game.progress && game.progress.totalFiles}
          <span class="muted">
            {game.progress.doneFiles} / {game.progress.totalFiles} files,
            {Math.round(game.progress.doneBytes / 1048576)} / {Math.round(
              game.progress.totalBytes / 1048576,
            )} MB
          </span>
        {/if}
      </div>
    {/if}

    {#if packUpdate && !game}
      <div class="banner update">
        <ArrowUpCircle size={18} />
        <span>Modpack update available: <strong>{packUpdate.versionNumber}</strong></span>
        <span class="muted">Your own added mods and worlds are kept.</span>
        <button class="update-button" onclick={runPackUpdate}>Update</button>
      </div>
    {/if}
    {#if packError}
      <div class="banner error"><TriangleAlert size={18} /> {packError}</div>
    {/if}

    {#if iconError}
      <div class="banner error"><TriangleAlert size={18} /> {iconError}</div>
    {/if}

    {#if error}
      <div class="banner error"><TriangleAlert size={18} /> {error}</div>
    {/if}

    {#if confirmingDelete}
      <div class="banner confirm">
        <span>Delete <strong>{instance.name}</strong> and all of its worlds, mods and settings?</span>
        <button class="secondary" onclick={() => (confirmingDelete = false)}>Cancel</button>
        <button class="danger-button" onclick={remove}>Delete</button>
        {#if deleteError}<span class="error-text">{deleteError}</span>{/if}
      </div>
    {/if}

    <nav class="tabs">
      <button class:active={tab === "content"} onclick={() => (tab = "content")}>
        <Gamepad2 size={17} /> Content
      </button>
      <button class:active={tab === "logs"} onclick={() => (tab = "logs")}>
        <ScrollText size={17} /> Logs
      </button>
      <button class:active={tab === "options"} onclick={() => (tab = "options")}>
        <SlidersHorizontal size={17} /> Options
      </button>
    </nav>

    {#if tab === "content"}
      <div class="kinds">
        {#each contentKinds as [kind, label] (kind)}
          <button class:active={contentKind === kind} onclick={() => (contentKind = kind)}>{label}</button>
        {/each}
      </div>
      <ContentList {id} kind={contentKind} />
    {:else if tab === "options"}
      <section class="options">
        <h3>Memory</h3>
        <p class="muted">Takes effect the next time the game starts.</p>
        {#await info then { totalMemoryMb }}
          <MemorySetting
            value={instance.maxMemoryMb}
            fallbackLabel={settings.defaultMemoryMb
              ? `the default from Settings (${(settings.defaultMemoryMb / 1024).toFixed(1)} GB)`
              : "the default from Settings (Mojang's recommendation)"}
            totalMb={totalMemoryMb}
            onChange={(value) =>
              setInstanceMemory(id, value).catch((e) => (optionsError = String(e)))}
          />
        {/await}
        {#if optionsError}<p class="error-text">{optionsError}</p>{/if}
      </section>
    {:else}
      <pre class="log" bind:this={logEl} onscroll={onScroll}></pre>
    {/if}
  </div>
{/if}

{#if editingIcon && instance}
  <IconEditor
    initial={instance.iconConfig ?? defaultIconConfig(instance.id)}
    onClose={() => (editingIcon = false)}
    onSave={saveIconConfig}
  />
{/if}

<style>
  .note {
    padding: 40px;
    text-align: center;
    color: var(--text-muted);
  }

  .page {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px 24px 24px;
  }

  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding-bottom: 18px;
    border-bottom: 1px solid var(--border);
  }

  .icon-wrap {
    position: relative;
  }

  .icon-button-edit {
    display: block;
    border-radius: 16px;
  }

  .icon-button-edit:hover {
    filter: brightness(1.15);
  }

  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 4;
  }

  .icon-menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 5;
    display: flex;
    flex-direction: column;
    min-width: 180px;
    padding: 6px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.35);
  }

  .icon-menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    font-weight: 600;
  }

  .icon-menu button:hover {
    background: var(--bg-button);
  }

  .title {
    flex: 1;
    min-width: 0;
  }

  h2 {
    font-size: 1.5rem;
    color: var(--text-strong);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    color: var(--text-muted);
  }

  .meta span {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .meta .dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--text-muted);
  }

  .actions {
    display: flex;
    gap: 8px;
  }

  .icon-button {
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
    background: var(--bg-button);
  }

  .icon-button:hover {
    background: var(--bg-button-hover);
  }

  .icon-button.danger:hover {
    color: #ff6b6b;
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-radius: var(--radius);
    background: var(--bg-raised);
  }

  .banner.update {
    background: var(--brand-highlight);
    color: var(--text-strong);
  }

  .banner.update :global(svg) {
    color: var(--brand);
  }

  .banner.update .muted {
    flex: 1;
  }

  .update-button {
    height: 34px;
    padding: 0 14px;
    border-radius: var(--radius-sm);
    background: var(--brand-bg);
    color: var(--brand-text);
    font-weight: 600;
  }

  .banner.error {
    color: #ff8a8a;
    background: color-mix(in srgb, #ff6b6b 12%, var(--bg-raised));
  }

  .banner.confirm span:first-child {
    flex: 1;
  }

  .muted {
    color: var(--text-muted);
  }

  .secondary,
  .danger-button {
    height: 34px;
    padding: 0 14px;
    border-radius: var(--radius-sm);
    font-weight: 600;
  }

  .secondary {
    background: var(--bg-button);
  }

  .danger-button {
    background: #c9363b;
    color: #fff;
  }

  .error-text {
    color: #ff6b6b;
  }

  .tabs {
    display: flex;
    gap: 4px;
    align-self: flex-start;
    padding: 4px;
    border-radius: var(--radius);
    background: var(--bg-raised);
  }

  .tabs button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 14px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    color: var(--text-muted);
  }

  .tabs button:hover {
    color: var(--text-strong);
  }

  .tabs button.active {
    background: var(--brand-highlight);
    color: var(--brand);
  }

  .kinds {
    display: flex;
    gap: 6px;
  }

  .kinds button {
    padding: 5px 12px;
    border-radius: 999px;
    font-weight: 600;
    font-size: 0.9rem;
    color: var(--text-muted);
  }

  .kinds button:hover {
    background: var(--bg-button);
  }

  .kinds button.active {
    background: var(--bg-button);
    color: var(--text-strong);
  }


  .options {
    max-width: 640px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 18px 20px;
    border-radius: var(--radius-lg);
    background: var(--bg-raised);
  }

  .options h3 {
    color: var(--text-strong);
  }

  .log {
    flex: 1;
    min-height: 200px;
    margin: 0;
    padding: 14px;
    overflow: auto;
    border-radius: var(--radius);
    background: var(--bg-shell);
    font-family: ui-monospace, "JetBrains Mono", monospace;
    font-size: 0.8rem;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
</style>
