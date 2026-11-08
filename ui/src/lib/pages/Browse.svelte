<script lang="ts">
  import { Search } from "@lucide/svelte";
  import { ArrowLeft, Filter, Gamepad2, Lock, Tag, X } from "@lucide/svelte";
  import CategoryFilter from "../components/CategoryFilter.svelte";
  import InstallButton from "../components/InstallButton.svelte";
  import InstanceIcon from "../components/InstanceIcon.svelte";
  import ProjectRow from "../components/ProjectRow.svelte";
  import { gameVersions, instanceById, loaderNames, type Instance } from "../instances.svelte";
  import { openInstance } from "../navigation.svelte";
  import type { ProjectType, SearchIndex } from "../modrinth";
  import { browse, loadMore, runSearch } from "./browse.svelte";

  const types: { id: ProjectType; label: string }[] = [
    { id: "mod", label: "Mods" },
    { id: "modpack", label: "Modpacks" },
    { id: "resourcepack", label: "Resource packs" },
    { id: "shader", label: "Shaders" },
  ];

  const sorts: { id: SearchIndex; label: string }[] = [
    { id: "relevance", label: "Relevance" },
    { id: "downloads", label: "Downloads" },
    { id: "follows", label: "Follows" },
    { id: "newest", label: "Newest" },
    { id: "updated", label: "Recently updated" },
  ];

  if (browse.hits.length === 0 && !browse.error) runSearch();

  const loaderLabel = (loader: string) => loaderNames[loader as Instance["loader"]] ?? loader;

  // Modrinth tags loaders as categories; shaders use their own "loaders".
  const loaderOptions: Record<string, [string, string][]> = {
    mod: [
      ["fabric", "Fabric"],
      ["neoforge", "NeoForge"],
      ["forge", "Forge"],
      ["quilt", "Quilt"],
    ],
    modpack: [
      ["fabric", "Fabric"],
      ["neoforge", "NeoForge"],
      ["forge", "Forge"],
      ["quilt", "Quilt"],
    ],
    shader: [
      ["iris", "Iris"],
      ["optifine", "OptiFine"],
      ["canvas", "Canvas"],
      ["vanilla", "Vanilla (core shaders)"],
    ],
  };
  const releases = gameVersions()
    .then((all) => all.filter((v) => v.type === "release").map((v) => v.id))
    .catch(() => [] as string[]);

  let debounce: ReturnType<typeof setTimeout> | undefined;
  function onInput() {
    clearTimeout(debounce);
    debounce = setTimeout(runSearch, 300);
  }

  let showCategories = $state(false);

  function setCategories(selected: string[]) {
    browse.categories = selected;
    runSearch();
  }

  function setType(id: ProjectType) {
    if (browse.projectType === id) return;
    browse.projectType = id;
    // Each project type has its own categories.
    browse.categories = [];
    // Loader names differ between mods and shaders.
    if (!loaderOptions[id]?.some(([value]) => value === browse.loader)) browse.loader = "";
    runSearch();
  }

  /** Calls loadMore whenever the element scrolls into view. */
  function whenVisible(node: HTMLElement) {
    const observer = new IntersectionObserver(
      (entries) => entries.some((e) => e.isIntersecting) && loadMore(),
      { rootMargin: "400px" },
    );
    observer.observe(node);
    return { destroy: () => observer.disconnect() };
  }
</script>

<div class="page">
  {#if browse.target}
    {@const target = browse.target}
    <header class="target">
      <button class="back" title="Back to {target.name}" onclick={() => openInstance(target.id)}>
        <ArrowLeft size={20} />
      </button>
      <InstanceIcon instance={instanceById(target.id)} size={52} />
      <div>
        <h2>{target.name}</h2>
        <div class="meta">
          <span>Installing content</span>
          <span class="dot"></span>
          <span><Gamepad2 size={15} /> Minecraft {target.gameVersion}</span>
          <span class="dot"></span>
          <span><Tag size={15} /> {loaderLabel(target.loader)}</span>
        </div>
      </div>
    </header>
  {/if}

  <div class="toolbar">
    <label class="search">
      <Search size={18} />
      <input
        type="search"
        placeholder="Search {types.find((t) => t.id === browse.projectType)?.label.toLowerCase()}..."
        bind:value={browse.query}
        oninput={onInput}
      />
    </label>
    <select bind:value={browse.index} onchange={runSearch} aria-label="Sort by">
      {#each sorts as sort (sort.id)}
        <option value={sort.id}>Sort: {sort.label}</option>
      {/each}
    </select>
    <button
      class="category-toggle"
      class:active={showCategories || browse.categories.length > 0}
      aria-expanded={showCategories}
      onclick={() => (showCategories = !showCategories)}
    >
      <Filter size={16} />
      Categories{browse.categories.length ? ` (${browse.categories.length})` : ""}
    </button>
    {#if browse.categories.length}
      <button class="clear" title="Clear categories" onclick={() => setCategories([])}>
        <X size={16} />
      </button>
    {/if}
  </div>

  {#if !browse.target}
    <div class="filters">
      {#if loaderOptions[browse.projectType]}
        <select bind:value={browse.loader} onchange={runSearch} aria-label="Loader">
          <option value="">Any loader</option>
          {#each loaderOptions[browse.projectType] as [value, label] (value)}
            <option {value}>{label}</option>
          {/each}
        </select>
      {/if}
      {#await releases then list}
        <select bind:value={browse.gameVersion} onchange={runSearch} aria-label="Game version">
          <option value="">Any version</option>
          {#each list as version (version)}<option value={version}>{version}</option>{/each}
        </select>
      {/await}
    </div>
  {/if}
  {#if browse.target}
    <div class="locked">
      <span title="Only showing versions for this instance"><Lock size={13} /> {browse.target.gameVersion}</span>
      {#if browse.projectType === "mod"}
        <span title="Only showing mods for this loader"><Lock size={13} /> {loaderLabel(browse.target.loader)}</span>
      {/if}
    </div>
  {/if}

  <div class="tabs" role="tablist">
    {#each types.filter((t) => !browse.target || t.id !== "modpack") as type (type.id)}
      <button
        role="tab"
        aria-selected={browse.projectType === type.id}
        class:active={browse.projectType === type.id}
        onclick={() => setType(type.id)}
      >
        {type.label}
      </button>
    {/each}
  </div>

  {#if showCategories}
    <CategoryFilter
      projectType={browse.projectType}
      selected={browse.categories}
      onChange={setCategories}
    />
  {/if}

  {#if browse.error}
    <div class="note">Search failed: {browse.error}</div>
  {:else if !browse.loading && browse.hits.length === 0}
    <div class="note">No results.</div>
  {/if}

  <div class="results">
    {#each browse.hits as hit (hit.projectId)}
      <ProjectRow {hit}>
        {#snippet actions()}
          <InstallButton {hit} />
        {/snippet}
      </ProjectRow>
    {/each}
  </div>

  {#if browse.hits.length > 0 && browse.hits.length < browse.totalHits}
    <div class="more" use:whenVisible>{browse.loading ? "Loading..." : ""}</div>
  {:else if browse.loading}
    <div class="more">Loading...</div>
  {/if}
</div>

<style>
  .page {
    padding: 20px 24px 32px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .toolbar {
    display: flex;
    gap: 10px;
  }

  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text-muted);
  }

  .search:focus-within {
    border-color: var(--brand);
  }

  input {
    flex: 1;
    height: 42px;
    border: none;
    outline: none;
    background: none;
    color: var(--text-strong);
    font: inherit;
  }

  select {
    padding: 0 12px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text);
    font: inherit;
  }

  .category-toggle,
  .clear {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-weight: 600;
    color: var(--text);
  }

  .clear {
    padding: 0 12px;
  }

  .category-toggle.active {
    border-color: var(--brand);
    color: var(--brand);
  }

  .tabs {
    display: flex;
    gap: 6px;
  }

  .tabs button {
    padding: 6px 14px;
    border-radius: 999px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .tabs button:hover {
    background: var(--bg-button);
  }

  .tabs button.active {
    background: var(--brand-highlight);
    color: var(--brand);
  }

  .target {
    display: flex;
    align-items: center;
    gap: 14px;
    padding-bottom: 6px;
  }

  .back {
    width: 44px;
    height: 44px;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--bg-button);
  }

  .back:hover {
    background: var(--bg-button-hover);
  }

  .target h2 {
    font-size: 1.4rem;
    color: var(--text-strong);
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

  .filters {
    display: flex;
    gap: 8px;
  }

  .filters select {
    height: 34px;
  }

  .locked {
    display: flex;
    gap: 6px;
  }

  .locked span {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  .results {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .note,
  .more {
    padding: 20px;
    text-align: center;
    color: var(--text-muted);
  }
</style>
