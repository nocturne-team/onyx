<script lang="ts">
  import { Compass, FolderOpen, Inbox, Package, RefreshCw, Search, Trash2 } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { identifyContent, installKey, installs, type KnownFile } from "../content.svelte";
  import { instanceById, instances, type ModFile } from "../instances.svelte";
  import { nav } from "../navigation.svelte";
  import { browse, runSearch } from "../pages/browse.svelte";

  let { id }: { id: string } = $props();

  let mods = $state<ModFile[] | null>(null);
  /** Modrinth info by "mods/<file name>"; filled in after the list shows. */
  let known = $state<Record<string, KnownFile>>({});
  let error = $state<string | null>(null);
  let filter = $state("");

  async function load() {
    try {
      mods = await invoke<ModFile[]>("list_mods", { id });
      error = null;
    } catch (e) {
      error = String(e);
      return;
    }
    identifyContent(id)
      .then((result) => (known = result))
      .catch((e) => console.warn("couldn't identify mods", e));
  }

  const info = (mod: ModFile) => known[`mods/${mod.fileName}`];

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
    browse.projectType = "mod";
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
    load();
    // Pick up jars dropped into the folder while Onyx is open.
    const onFocus = () => load();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  });

  async function toggle(mod: ModFile) {
    try {
      await invoke("set_mod_enabled", { id, fileName: mod.fileName, enabled: !mod.enabled });
    } catch (e) {
      error = String(e);
    }
    load();
  }

  async function remove(mod: ModFile) {
    try {
      await invoke("remove_mod", { id, fileName: mod.fileName });
    } catch (e) {
      error = String(e);
    }
    load();
  }

  const openFolder = () => invoke("open_instance_subfolder", { id, folder: "mods" });

  const visible = $derived(
    (mods ?? []).filter((m) =>
      `${m.name ?? ""} ${m.fileName} ${m.modId ?? ""}`.toLowerCase().includes(filter.toLowerCase()),
    ),
  );
</script>

{#if error}<p class="error">{error}</p>{/if}

{#if mods && mods.length === 0}
  <div class="empty">
    <Inbox size={48} />
    <h3>No mods installed</h3>
    <p class="muted">Find mods on Modrinth, or drop .jar files into the mods folder.</p>
    <div class="buttons">
      <button class="secondary" onclick={openFolder}><FolderOpen size={17} /> Open mods folder</button>
      <button class="primary" onclick={browseContent}><Compass size={17} /> Browse content</button>
    </div>
  </div>
{:else if mods}
  <div class="toolbar">
    <div class="search">
      <Search size={16} />
      <input placeholder="Search {mods.length} {mods.length === 1 ? 'mod' : 'mods'}..." bind:value={filter} />
    </div>
    <button class="icon-button" title="Refresh" onclick={load}><RefreshCw size={17} /></button>
    <button class="secondary" onclick={openFolder}><FolderOpen size={17} /> Open folder</button>
    <button class="primary" onclick={browseContent}><Compass size={17} /> Browse content</button>
  </div>

  <div class="list">
    {#each visible as mod (mod.fileName)}
      <div class="row" class:disabled={!mod.enabled}>
        {#if info(mod)?.iconUrl}
          <img class="icon" src={info(mod)?.iconUrl} alt="" loading="lazy" />
        {:else}
          <span class="icon"><Package size={20} /></span>
        {/if}
        <div class="main">
          <strong>{info(mod)?.title ?? mod.name ?? mod.fileName}</strong>
          <span class="muted">{mod.version ? `${mod.version} · ` : ""}{mod.fileName}</span>
        </div>
        <button
          class="switch"
          class:on={mod.enabled}
          role="switch"
          aria-checked={mod.enabled}
          aria-label={mod.enabled ? "Disable" : "Enable"}
          title={mod.enabled ? "Disable" : "Enable"}
          onclick={() => toggle(mod)}
        ><span></span></button>
        <button class="icon-button danger" title="Remove" onclick={() => remove(mod)}>
          <Trash2 size={17} />
        </button>
      </div>
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
    padding: 60px 20px;
    text-align: center;
    color: var(--text-muted);
  }

  .empty h3 {
    color: var(--text-strong);
    font-size: 1.3rem;
  }

  .muted {
    color: var(--text-muted);
    font-size: 0.88rem;
  }

  .toolbar {
    display: flex;
    gap: 8px;
  }

  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 12px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
  }

  .search input {
    flex: 1;
    background: none;
    border: none;
    outline: none;
    color: var(--text-strong);
    font: inherit;
  }

  .secondary {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 14px;
    border-radius: var(--radius-sm);
    background: var(--bg-button);
    font-weight: 600;
  }

  .buttons {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }

  .primary {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 14px;
    border-radius: var(--radius-sm);
    background: var(--brand-bg);
    color: var(--brand-text);
    font-weight: 600;
  }

  .secondary:hover,
  .icon-button:hover {
    background: var(--bg-button-hover);
  }

  .icon-button {
    width: 38px;
    height: 38px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--bg-button);
  }

  .icon-button.danger:hover {
    color: #ff6b6b;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--bg-raised);
    border-radius: var(--radius);
  }

  .row.disabled .main,
  .row.disabled .icon {
    opacity: 0.5;
  }

  .icon {
    width: 40px;
    height: 40px;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--bg-button);
    color: var(--text-muted);
    object-fit: cover;
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .main strong {
    color: var(--text-strong);
  }

  .main > * {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .switch {
    width: 40px;
    height: 22px;
    flex-shrink: 0;
    padding: 3px;
    border-radius: 999px;
    background: var(--bg-button-hover);
    transition: background 120ms;
  }

  .switch span {
    display: block;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    transition: translate 120ms;
  }

  .switch.on {
    background: var(--brand-bg);
  }

  .switch.on span {
    translate: 18px 0;
  }
</style>
