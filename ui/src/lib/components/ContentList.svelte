<script lang="ts">
  import {
    Compass,
    Download,
    FolderOpen,
    Inbox,
    LoaderCircle,
    Package,
    RefreshCw,
    Search,
    Trash2,
  } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    applyUpdate,
    checkUpdates,
    cachedContentInfo,
    identifyContent,
    installKey,
    installs,
    type KnownFile,
    type Update,
  } from "../content.svelte";
  import { instanceById, instances, type ContentFile, type ContentKind } from "../instances.svelte";
  import { cachedIcon } from "../modrinth";
  import { nav } from "../navigation.svelte";
  import { browse, runSearch } from "../pages/browseState.svelte";

  let { id }: { id: string } = $props();

  const labels: Record<ContentKind, { one: string; chip: string; projectType: "mod" | "resourcepack" | "shader" }> = {
    mods: { one: "Mod", chip: "Mods", projectType: "mod" },
    resourcepacks: { one: "Resource pack", chip: "Resource packs", projectType: "resourcepack" },
    shaderpacks: { one: "Shader", chip: "Shaders", projectType: "shader" },
  };

  const vanilla = $derived(instanceById(id)?.loader === "vanilla");
  // Vanilla can't load mods, so its mods folder isn't listed.
  const kinds = $derived(
    (["mods", "resourcepacks", "shaderpacks"] as ContentKind[]).filter((k) => k !== "mods" || !vanilla),
  );

  interface Item extends ContentFile {
    kind: ContentKind;
    /** "<folder>/<file name>", the key `identify_content` and updates use. */
    key: string;
  }

  let items = $state<Item[] | null>(null);
  /** Modrinth info by "<folder>/<file name>"; filled in after the list shows. */
  let known = $state<Record<string, KnownFile>>({});
  let error = $state<string | null>(null);
  /** Available updates by "<folder>/<file name>". */
  let updates = $state<Record<string, Update>>({});
  let updating = $state<Set<string>>(new Set());
  let filter = $state("");
  let shown = $state<ContentKind | "all">("all");

  // Updates need the network, so they're checked when the list opens and on
  // Refresh, not on every window focus.
  async function findUpdates() {
    try {
      const list = await checkUpdates(id);
      updates = Object.fromEntries(list.map((u) => [u.key, u]));
    } catch (e) {
      console.warn("couldn't check for updates", e);
    }
  }

  const pending = $derived((items ?? []).filter((m) => updates[m.key] && !updating.has(m.key)));

  async function runUpdates(targets: Item[]) {
    updating = new Set([...updating, ...targets.map((m) => m.key)]);
    for (const item of targets) {
      const available = updates[item.key];
      if (!available) continue;
      try {
        await applyUpdate(id, available);
        delete updates[available.key];
      } catch (e) {
        error = String(e);
      }
    }
    updating = new Set([...updating].filter((key) => !targets.some((m) => m.key === key)));
    await load();
    findUpdates();
  }

  async function load() {
    try {
      const lists = await Promise.all(
        kinds.map(async (kind) =>
          (await invoke<ContentFile[]>("list_content", { id, kind })).map(
            (file): Item => ({ ...file, kind, key: `${kind}/${file.fileName}` }),
          ),
        ),
      );
      items = lists.flat();
      error = null;
    } catch (e) {
      error = String(e);
      return;
    }
    // Last run's result first, so names and icons show right away.
    if (Object.keys(known).length === 0) {
      cachedContentInfo(id)
        .then((cached) => {
          if (Object.keys(known).length === 0) known = cached;
        })
        .catch(() => {});
    }
    identifyContent(id)
      .then((result) => (known = result))
      .catch((e) => console.warn("couldn't identify content", e));
  }

  function browseContent() {
    const instance = instanceById(id);
    if (!instance) return;
    browse.target = {
      id,
      name: instance.name,
      loader: instance.loader,
      gameVersion: instance.gameVersion,
    };
    // So Browse shows what's already here as installed.
    for (const file of Object.values(known)) installs[installKey(id, file.projectId)] = "installed";
    browse.projectType = shown === "all" ? (vanilla ? "resourcepack" : "mod") : labels[shown].projectType;
    browse.categories = [];
    browse.query = "";
    runSearch();
    nav.page = "browse";
  }

  // Reload once an install (e.g. a modpack) into this instance finishes.
  let wasBusy = false;
  $effect(() => {
    const busy = instances.games[id]?.phase === "preparing";
    if (wasBusy && !busy) load();
    wasBusy = busy;
  });

  $effect(() => {
    void kinds;
    load();
    findUpdates();
    // Pick up files dropped into the folders while Onyx is open.
    const onFocus = () => load();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  });

  async function toggle(item: Item) {
    try {
      await invoke("set_content_enabled", {
        id,
        kind: item.kind,
        fileName: item.fileName,
        enabled: !item.enabled,
      });
    } catch (e) {
      error = String(e);
    }
    load();
  }

  async function remove(item: Item) {
    try {
      await invoke("remove_content", { id, kind: item.kind, fileName: item.fileName });
    } catch (e) {
      error = String(e);
    }
    load();
  }

  const openFolder = () =>
    shown === "all"
      ? invoke("open_instance_folder", { id })
      : invoke("open_instance_subfolder", { id, folder: shown });

  // Only kinds that have something in them get a chip, like the Modrinth app.
  const chips = $derived(kinds.filter((k) => items?.some((m) => m.kind === k)));
  $effect(() => {
    if (shown !== "all" && items && !chips.includes(shown)) shown = "all";
  });

  const title = (m: Item) => known[m.key]?.title ?? m.name ?? m.fileName;
  const visible = $derived(
    (items ?? [])
      .filter((m) => shown === "all" || m.kind === shown)
      .filter((m) =>
        `${title(m)} ${m.fileName} ${m.modId ?? ""}`.toLowerCase().includes(filter.toLowerCase()),
      )
      .sort((a, b) => title(a).localeCompare(title(b), undefined, { sensitivity: "base" })),
  );
</script>

{#if error}<p class="error">{error}</p>{/if}

{#if items && items.length === 0}
  <div class="empty">
    <Inbox size={56} strokeWidth={1.5} />
    <h3>No content installed</h3>
    <p class="muted">Browse projects on Modrinth or add files to the instance folder to get started.</p>
    <div class="buttons">
      <button class="secondary" onclick={openFolder}><FolderOpen size={17} /> Open folder</button>
      <button class="primary" onclick={browseContent}><Compass size={17} /> Browse content</button>
    </div>
  </div>
{:else if items}
  <div class="toolbar">
    <div class="search">
      <Search size={17} />
      <input
        placeholder="Search {items.length} {items.length === 1 ? 'project' : 'projects'}..."
        bind:value={filter}
      />
    </div>
    <button class="secondary" onclick={openFolder}><FolderOpen size={17} /> Open folder</button>
    <button class="primary" onclick={browseContent}><Compass size={17} /> Browse content</button>
  </div>

  <div class="filters">
    <div class="chips">
      <button class:active={shown === "all"} onclick={() => (shown = "all")}>All</button>
      {#each chips as kind (kind)}
        <button class:active={shown === kind} onclick={() => (shown = kind)}>{labels[kind].chip}</button>
      {/each}
    </div>
    {#if pending.length > 0}
      <button class="link brand" onclick={() => runUpdates(pending)}>
        <Download size={16} /> Update all ({pending.length})
      </button>
    {/if}
    <button
      class="link"
      title="Refresh and check for updates"
      onclick={() => {
        load();
        findUpdates();
      }}><RefreshCw size={16} /> Refresh</button
    >
  </div>

  <div class="table">
    <div class="row head">
      <span>Project</span>
      <span>Version</span>
      <span class="right">Actions</span>
    </div>
    {#each visible as item (item.key)}
      {@const info = known[item.key]}
      {@const available = updates[item.key]}
      <div class="row" class:disabled={!item.enabled}>
        <div class="project">
          {#if info?.iconUrl}
            <img class="icon" src={cachedIcon(info.iconUrl)} alt="" loading="lazy" />
          {:else}
            <span class="icon"><Package size={22} /></span>
          {/if}
          <div class="text">
            <strong>{title(item)}</strong>
            <span class="muted">
              {labels[item.kind].one}{info ? "" : " · Local file"}
            </span>
          </div>
        </div>
        <div class="text version">
          <strong>{item.version ?? info?.versionNumber ?? "Unknown"}</strong>
          <span class="muted">{item.fileName}{item.isDir ? "/" : ""}</span>
        </div>
        <div class="actions">
          {#if updating.has(item.key)}
            <span class="updating"><LoaderCircle size={18} class="spin" /></span>
          {:else if available}
            <button
              class="icon-button brand"
              title="Update {available.current} to {available.latest.versionNumber}"
              aria-label="Update"
              onclick={() => runUpdates([item])}
            >
              <Download size={18} />
            </button>
          {/if}
          <button
            class="switch"
            class:on={item.enabled}
            role="switch"
            aria-checked={item.enabled}
            aria-label={item.enabled ? "Disable" : "Enable"}
            title={item.enabled ? "Disable" : "Enable"}
            onclick={() => toggle(item)}
          ><span></span></button>
          <button class="icon-button danger" title="Remove" aria-label="Remove" onclick={() => remove(item)}>
            <Trash2 size={18} />
          </button>
        </div>
      </div>
    {:else}
      <p class="none muted">Nothing matches "{filter}".</p>
    {/each}
  </div>
{/if}

<style>
  .error {
    color: #ff6b6b;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 80px 20px;
    text-align: center;
    color: var(--text-muted);
  }

  .empty h3 {
    margin-top: 8px;
    color: var(--text-strong);
    font-size: 1.5rem;
  }

  .muted {
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .buttons {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }

  .toolbar {
    display: flex;
    gap: 8px;
  }

  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    padding: 0 14px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
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

  .secondary,
  .primary {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 40px;
    padding: 0 16px;
    border-radius: var(--radius);
    font-weight: 600;
    white-space: nowrap;
  }

  .secondary {
    background: var(--bg-raised);
    border: 1px solid var(--border);
    color: var(--text-strong);
  }

  .secondary:hover {
    background: var(--bg-button);
  }

  .primary {
    background: var(--brand-bg);
    color: var(--brand-text);
  }

  .primary:hover {
    filter: brightness(1.1);
  }

  .filters {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .chips {
    flex: 1;
    display: flex;
    gap: 6px;
  }

  .chips button {
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 0.9rem;
    font-weight: 500;
    color: var(--text);
  }

  .chips button:hover {
    background: var(--bg-button);
  }

  .chips button.active {
    border-color: var(--brand);
    background: var(--brand-highlight);
    color: var(--brand);
  }

  .link {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
    font-size: 0.92rem;
    color: var(--text);
  }

  .link:hover {
    color: var(--text-strong);
  }

  .link.brand {
    color: var(--brand);
  }

  .link.brand:hover {
    filter: brightness(1.15);
  }

  .table {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-raised);
    overflow: hidden;
  }

  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto;
    align-items: center;
    gap: 16px;
    padding: 10px 16px;
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .row.head {
    padding: 12px 16px;
    font-weight: 600;
    color: var(--text);
  }

  .right {
    text-align: right;
  }

  .row.disabled .project,
  .row.disabled .version {
    opacity: 0.5;
  }

  .project {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .icon {
    width: 48px;
    height: 48px;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-button);
    color: var(--text-muted);
    object-fit: cover;
  }

  .text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .text > * {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .text strong {
    color: var(--text-strong);
    font-weight: 600;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .icon-button {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text);
  }

  .icon-button:hover {
    background: var(--bg-button);
  }

  .icon-button.brand {
    color: var(--brand);
  }

  .icon-button.danger:hover {
    color: #ff6b6b;
  }

  .updating {
    width: 36px;
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }

  .none {
    padding: 20px 16px;
    border-top: 1px solid var(--border);
  }

  .switch {
    width: 46px;
    height: 24px;
    flex-shrink: 0;
    padding: 3px;
    border-radius: 999px;
    background: var(--bg-button-hover);
    transition: background 120ms;
  }

  .switch span {
    display: block;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    transition: translate 120ms;
  }

  .switch.on {
    background: var(--brand);
  }

  .switch.on span {
    translate: 22px 0;
  }
</style>
