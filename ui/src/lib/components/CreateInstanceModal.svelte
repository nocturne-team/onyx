<script lang="ts">
  import {
    ArrowLeft,
    Boxes,
    Check,
    Compass,
    PackageOpen,
    Plus,
    Palette,
    RefreshCw,
    Search,
    Upload,
    X,
  } from "@lucide/svelte";
  import { importMrpack } from "../content.svelte";
  import {
    createInstance,
    gameVersions,
    imagePreview,
    pickIconFile,
    loaderGameVersions,
    loaderNames,
    loaderVersions,
    type Instance,
    type LoaderVersion,
  } from "../instances.svelte";
  import { nav, openInstance } from "../navigation.svelte";
  import { browse, runSearch } from "../pages/browseState.svelte";
  import { randomIconConfig, type IconConfig } from "../iconCatalog";
  import IconEditor from "./IconEditor.svelte";
  import InstanceIcon from "./InstanceIcon.svelte";

  let { onClose }: { onClose: () => void } = $props();

  let step = $state<"type" | "custom">("type");
  let query = $state("");
  let name = $state("");
  let gameVersion = $state<string | null>(null);
  let showSnapshots = $state(false);
  let loader = $state<Instance["loader"]>("vanilla");
  /** Game versions the selected loader supports; `null` for vanilla. */
  let supported = $state<Set<string> | null>(null);
  let loaderChoice = $state<"stable" | "latest" | "other">("stable");
  let available = $state<LoaderVersion[]>([]);
  let otherVersion = $state<string | null>(null);
  let loaderError = $state<string | null>(null);
  let creating = $state(false);
  let iconConfig = $state<IconConfig>(randomIconConfig());
  let editingIcon = $state(false);
  let iconPath = $state<string | null>(null);
  let iconPreview = $state<string | null>(null);

  async function uploadIcon() {
    const path = await pickIconFile();
    if (!path) return;
    const preview = await imagePreview(path);
    if (!preview) {
      error = "Icons must be PNG, JPEG, WebP or GIF images under 4 MB";
      return;
    }
    iconPath = path;
    iconPreview = preview;
  }

  function randomizeIcon() {
    iconPath = null;
    iconPreview = null;
    iconConfig = randomIconConfig();
  }
  let error = $state<string | null>(null);

  const versions = gameVersions();
  versions.then((all) => {
    gameVersion ??= all.find((v) => v.type === "release")?.id ?? null;
  });

  const loaders: Instance["loader"][] = ["vanilla", "fabric", "neoforge", "forge", "quilt"];
  const supportedLoaders = new Set<Instance["loader"]>([
    "vanilla",
    "fabric",
    "neoforge",
    "forge",
    "quilt",
  ]);
  const placeholder = $derived(
    gameVersion ? `${loaderNames[loader]} ${gameVersion}` : loaderNames[loader],
  );

  // The supported game versions for the picked loader.
  $effect(() => {
    const current = loader;
    supported = null;
    loaderError = null;
    if (current === "vanilla") return;
    loaderGameVersions(current)
      .then((set) => {
        if (loader === current) supported = set;
      })
      .catch((e) => (loaderError = `Couldn't load ${loaderNames[current]} versions: ${e}`));
  });

  // Loader versions for the picked game version.
  $effect(() => {
    const current = loader;
    const game = gameVersion;
    available = [];
    otherVersion = null;
    if (current === "vanilla" || !game) return;
    loaderVersions(current, game)
      .then((list) => {
        if (loader !== current || gameVersion !== game) return;
        available = list;
        otherVersion = list[0]?.version ?? null;
      })
      .catch((e) => (loaderError = `Couldn't load ${loaderNames[current]} versions: ${e}`));
  });

  // Keep the picked game version valid when the loader changes.
  $effect(() => {
    if (!supported || !gameVersion || supported.has(gameVersion)) return;
    versions.then((all) => {
      gameVersion = all.find((v) => v.type === "release" && supported?.has(v.id))?.id ?? null;
    });
  });

  const chosenLoaderVersion = $derived.by(() => {
    if (loader === "vanilla") return null;
    if (loaderChoice === "other") return otherVersion;
    if (loaderChoice === "latest") return available[0]?.version ?? null;
    return (available.find((v) => v.stable) ?? available[0])?.version ?? null;
  });

  function shown(v: { id: string; type: string }) {
    if (v.type !== "release" && !(showSnapshots && v.type === "snapshot")) return false;
    return !supported || supported.has(v.id);
  }

  function searchInstead(e: SubmitEvent) {
    e.preventDefault();
    browse.query = query;
    browse.target = null;
    browse.projectType = "modpack";
    browse.categories = [];
    runSearch();
    nav.page = "browse";
    onClose();
  }

  async function uploadPack() {
    error = null;
    try {
      const instance = await importMrpack();
      if (!instance) return;
      openInstance(instance.id);
      onClose();
    } catch (e) {
      error = String(e);
    }
  }

  function browsePacks() {
    browse.target = null;
    browse.query = "";
    browse.projectType = "modpack";
    browse.categories = [];
    runSearch();
    nav.page = "browse";
    onClose();
  }

  async function create() {
    if (!gameVersion) return;
    creating = true;
    error = null;
    try {
      const instance = await createInstance(
        name.trim() || placeholder,
        gameVersion,
        loader,
        chosenLoaderVersion,
        iconPath ? null : iconConfig,
        iconPath,
      );
      openInstance(instance.id);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      creating = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !editingIcon && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div
    class="modal"
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-labelledby="create-title"
    onclick={(e) => e.stopPropagation()}
  >
    <header>
      <h2 id="create-title">Create instance</h2>
      <button class="close" aria-label="Close" onclick={onClose}><X size={18} /></button>
    </header>

    {#if step === "type"}
      <div class="body">
        <form onsubmit={searchInstead}>
          <label class="label" for="create-search">Already know what you want to play?</label>
          <div class="search">
            <Search size={18} />
            <input id="create-search" placeholder="Search modpacks..." bind:value={query} />
          </div>
        </form>

        <div class="or"><span>or</span></div>

        <span class="label">Choose instance type</span>
        {#if error}<p class="error">{error}</p>{/if}
        <div class="types">
          <button class="type" onclick={() => (step = "custom")}>
            <span class="type-icon"><Boxes size={22} /></span>
            <span>
              <strong>Custom setup</strong>
              <span class="muted">Start from scratch by picking a loader and game version.</span>
            </span>
          </button>
          <button class="type" onclick={browsePacks}>
            <span class="type-icon"><Compass size={22} /></span>
            <span>
              <strong>Start from a modpack</strong>
              <span class="muted">Pick a modpack on Modrinth and we'll install its latest version.</span>
            </span>
          </button>
          <button class="type" onclick={uploadPack}>
            <span class="type-icon"><Upload size={22} /></span>
            <span>
              <strong>Upload a modpack</strong>
              <span class="muted">Install a modpack from an .mrpack file on your device.</span>
            </span>
          </button>
          <button class="type" disabled>
            <span class="type-icon"><PackageOpen size={22} /></span>
            <span>
              <strong>Import instance <em>Soon</em></strong>
              <span class="muted">Import an instance from Prism, CurseForge, or similar.</span>
            </span>
          </button>
        </div>
      </div>
    {:else}
      <div class="body">
        <div class="icon-row">
          <InstanceIcon config={iconConfig} src={iconPreview} size={112} />
          <div class="icon-buttons">
            <button class="secondary" onclick={uploadIcon}><Upload size={16} /> Upload</button>
            <button class="secondary" onclick={randomizeIcon}><RefreshCw size={16} /> Randomize</button>
            <button class="secondary" onclick={() => (editingIcon = true)}><Palette size={16} /> Customize</button>
          </div>
        </div>

        <label class="label" for="create-name">Name</label>
        <input id="create-name" class="text" {placeholder} bind:value={name} maxlength="100" />

        <span class="label">Loader</span>
        <div class="chips">
          {#each loaders as option (option)}
            <button
              class="chip"
              class:active={loader === option}
              disabled={!supportedLoaders.has(option)}
              title={supportedLoaders.has(option) ? undefined : "Coming soon"}
              onclick={() => (loader = option)}
            >
              {#if loader === option}<Check size={16} />{/if}
              {loaderNames[option]}
            </button>
          {/each}
        </div>

        <div class="version-label">
          <label class="label" for="create-version">Game version</label>
          <label class="toggle">
            <input type="checkbox" bind:checked={showSnapshots} /> Show snapshots
          </label>
        </div>
        {#await versions}
          <select id="create-version" class="text" disabled><option>Loading versions...</option></select>
        {:then all}
          <select id="create-version" class="text" bind:value={gameVersion}>
            {#each all.filter(shown) as version (version.id)}
              <option value={version.id}>{version.id}</option>
            {/each}
          </select>
        {:catch e}
          <p class="error">Couldn't load Minecraft versions: {e}</p>
        {/await}

        {#if loader !== "vanilla"}
          <span class="label">Loader version</span>
          <div class="chips">
            {#each [["stable", "Stable"], ["latest", "Latest"], ["other", "Other"]] as [value, label] (value)}
              <button
                class="chip"
                class:active={loaderChoice === value}
                onclick={() => (loaderChoice = value as typeof loaderChoice)}
              >
                {#if loaderChoice === value}<Check size={16} />{/if}
                {label}
              </button>
            {/each}
          </div>
          {#if loaderChoice === "other"}
            <select class="text" bind:value={otherVersion} disabled={!available.length}>
              {#each available as v (v.version)}
                <option value={v.version}>{v.version}{v.stable ? "" : " (beta)"}</option>
              {/each}
            </select>
          {:else if chosenLoaderVersion}
            <p class="muted">{loaderNames[loader]} Loader {chosenLoaderVersion}</p>
          {:else if gameVersion && !loaderError}
            <p class="muted">Loading loader versions...</p>
          {/if}
          {#if loaderError}<p class="error">{loaderError}</p>{/if}
        {/if}

        {#if error}<p class="error">{error}</p>{/if}

        <div class="actions">
          <button class="secondary" onclick={() => (step = "type")}>
            <ArrowLeft size={16} /> Back
          </button>
          <button
            class="primary"
            onclick={create}
            disabled={!gameVersion || creating || (loader !== "vanilla" && !chosenLoaderVersion)}
          >
            <Plus size={16} /> Create instance
          </button>
        </div>
      </div>
    {/if}
  </div>
</div>

{#if editingIcon}
  <IconEditor
    initial={iconConfig}
    onClose={() => (editingIcon = false)}
    onSave={(config) => {
      iconConfig = config;
      iconPath = null;
      iconPreview = null;
      editingIcon = false;
    }}
  />
{/if}

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
    width: min(560px, calc(100vw - 48px));
    max-height: calc(100vh - 48px);
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
    padding: 18px 22px;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    font-size: 1.15rem;
  }

  .close {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--bg-button);
  }

  .close:hover {
    background: var(--bg-button-hover);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 20px 22px 22px;
    overflow-y: auto;
  }

  .label {
    font-weight: 600;
    color: var(--text-strong);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 42px;
    padding: 0 12px;
    background: var(--bg-button);
    border: 2px solid transparent;
    border-radius: var(--radius);
    color: var(--text-muted);
  }

  .search:focus-within {
    border-color: var(--brand);
  }

  .search input {
    flex: 1;
    background: none;
    border: none;
    outline: none;
    color: var(--text-strong);
    font: inherit;
  }

  .or {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--text-muted);
    font-size: 0.85rem;
    margin: 4px 0;
  }

  .or::before,
  .or::after {
    content: "";
    flex: 1;
    height: 1px;
    background: var(--border);
  }

  .types {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .type {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px;
    text-align: left;
    background: var(--bg-button);
    border-radius: var(--radius);
  }

  .type:not(:disabled):hover {
    background: var(--bg-button-hover);
  }

  .type:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .type > span:last-child {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .type strong {
    color: var(--text-strong);
  }

  .type em {
    margin-left: 6px;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--bg-raised);
    font-size: 0.75rem;
    font-style: normal;
    color: var(--text-muted);
  }

  .type-icon {
    width: 46px;
    height: 46px;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
  }

  .muted {
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .icon-row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 6px;
  }

  .icon-buttons {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .text {
    height: 40px;
    padding: 0 12px;
    background: var(--bg-button);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-strong);
    font: inherit;
    margin-bottom: 8px;
  }

  .text:focus {
    outline: none;
    border-color: var(--brand);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 8px;
  }

  .chip {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-button);
    font-weight: 600;
  }

  .chip:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .chip.active {
    border-color: var(--brand);
    background: var(--brand-highlight);
    color: var(--brand);
  }

  .version-label {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }

  .primary,
  .secondary {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 38px;
    padding: 0 14px;
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
    border: 1px solid var(--border);
  }

  .error {
    color: #ff6b6b;
    font-size: 0.9rem;
  }
</style>
