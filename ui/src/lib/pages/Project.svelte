<script lang="ts">
  import {
    ArrowLeft,
    BookOpen,
    Bug,
    CalendarDays,
    Code,
    Download,
    ExternalLink,
    Heart,
    MessageCircle,
    Scale,
  } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import InstallButton from "../components/InstallButton.svelte";
  import InstallPopup from "../components/InstallPopup.svelte";
  import { compactNumber, relativeTime } from "../format";
  import { renderMarkdown } from "../markdown";
  import {
    pageAsHit,
    projectPage,
    projectVersions,
    type ProjectPage,
    type ProjectVersion,
  } from "../modrinth";
  import { goBack } from "../navigation.svelte";

  let { id }: { id: string } = $props();

  let page = $state<ProjectPage | null>(null);
  let error = $state<string | null>(null);
  let tab = $state<"description" | "versions" | "gallery">("description");
  let versions = $state<ProjectVersion[] | null>(null);
  let versionsError = $state<string | null>(null);
  let loaderFilter = $state("");
  let gameFilter = $state("");
  let installing = $state<ProjectVersion | null>(null);

  $effect(() => {
    const current = id;
    page = null;
    error = null;
    versions = null;
    tab = "description";
    projectPage(current)
      .then((p) => {
        if (current === id) page = p;
      })
      .catch((e) => (error = String(e)));
  });

  $effect(() => {
    if (tab !== "versions" || versions) return;
    projectVersions(id)
      .then((v) => (versions = v))
      .catch((e) => (versionsError = String(e)));
  });

  const body = $derived(page ? renderMarkdown(page.body) : "");

  /** Every link in the description opens in the browser, never in the app. */
  function interceptLinks(e: MouseEvent) {
    const link = (e.target as HTMLElement).closest("a");
    if (!link) return;
    e.preventDefault();
    const href = link.getAttribute("href");
    if (href) openExternal(new URL(href, "https://modrinth.com/").href);
  }

  function openExternal(url: string) {
    invoke("open_external", { url }).catch((e) => console.warn("couldn't open link", e));
  }

  const links = $derived(
    page
      ? (
          [
            [page.sourceUrl, "Source", Code],
            [page.issuesUrl, "Issues", Bug],
            [page.wikiUrl, "Wiki", BookOpen],
            [page.discordUrl, "Discord", MessageCircle],
            ...page.donationUrls.map((d) => [d.url, d.platform, Heart] as const),
          ] as const
        ).filter(([url]) => url)
      : [],
  );

  const versionLoaders = $derived([...new Set(versions?.flatMap((v) => v.loaders) ?? [])].sort());
  const versionGames = $derived(
    [...new Set(versions?.flatMap((v) => v.gameVersions) ?? [])].sort((a, b) =>
      b.localeCompare(a, undefined, { numeric: true }),
    ),
  );
  const shownVersions = $derived(
    (versions ?? []).filter(
      (v) =>
        (!loaderFilter || v.loaders.includes(loaderFilter)) &&
        (!gameFilter || v.gameVersions.includes(gameFilter)),
    ),
  );

  /** "1.20.1 – 26.3" style summary of a long version list. */
  function gameRange(all: string[]): string {
    // Snapshots and pre-releases would make the ends of the range odd.
    const releases = all.filter((v) => /^\d+(\.\d+)+$/.test(v));
    const list = releases.length ? releases : all;
    if (list.length <= 3) return list.join(", ");
    return `${list[0]} – ${list[list.length - 1]}`;
  }
</script>

<div class="page">
  <button class="back" onclick={goBack}><ArrowLeft size={18} /> Back</button>

  {#if error}
    <p class="note error">Couldn't load this project: {error}</p>
  {:else if !page}
    <p class="note">Loading...</p>
  {:else}
    <header>
      {#if page.iconUrl}
        <img class="icon" src={page.iconUrl} alt="" />
      {:else}
        <div class="icon"></div>
      {/if}
      <div class="title">
        <h2>{page.title}</h2>
        {#if page.author}<span class="muted">by {page.author}</span>{/if}
        <p>{page.description}</p>
        <div class="stats">
          <span title="Downloads"><Download size={15} /> {compactNumber(page.downloads)}</span>
          <span title="Followers"><Heart size={15} /> {compactNumber(page.followers)}</span>
          <span title="Updated">
            <CalendarDays size={15} /> Updated {relativeTime(new Date(page.updated))}
          </span>
        </div>
      </div>
      <div class="install"><InstallButton hit={pageAsHit(page)} /></div>
    </header>

    <div class="layout">
      <div class="main">
        <nav class="tabs">
          <button class:active={tab === "description"} onclick={() => (tab = "description")}>
            Description
          </button>
          <button class:active={tab === "versions"} onclick={() => (tab = "versions")}>Versions</button>
          {#if page.gallery.length}
            <button class:active={tab === "gallery"} onclick={() => (tab = "gallery")}>
              Gallery ({page.gallery.length})
            </button>
          {/if}
        </nav>

        {#if tab === "description"}
          <!-- Sanitized by renderMarkdown (DOMPurify). -->
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="markdown" onclick={interceptLinks}>{@html body}</div>
        {:else if tab === "versions"}
          {#if versionsError}
            <p class="note error">{versionsError}</p>
          {:else if !versions}
            <p class="note">Loading versions...</p>
          {:else}
            <div class="filters">
              <select bind:value={loaderFilter} aria-label="Loader">
                <option value="">All loaders</option>
                {#each versionLoaders as loader (loader)}<option value={loader}>{loader}</option>{/each}
              </select>
              <select bind:value={gameFilter} aria-label="Game version">
                <option value="">All game versions</option>
                {#each versionGames as game (game)}<option value={game}>{game}</option>{/each}
              </select>
              <span class="muted">{shownVersions.length} versions</span>
            </div>
            <div class="versions">
              {#each shownVersions.slice(0, 100) as version (version.id)}
                <div class="version">
                  <span class="type" data-type={version.versionType}>{version.versionType[0].toUpperCase()}</span>
                  <div class="version-main">
                    <strong>{version.name || version.versionNumber}</strong>
                    <span class="muted">{version.versionNumber}</span>
                  </div>
                  <span class="muted loaders">{version.loaders.join(", ")}</span>
                  <span class="muted games">{gameRange(version.gameVersions)}</span>
                  <span class="muted">{relativeTime(new Date(version.datePublished))}</span>
                  {#if page.projectType !== "modpack"}
                    <button class="version-install" title="Install this version" onclick={() => (installing = version)}>
                      <Download size={16} />
                    </button>
                  {:else}
                    <span></span>
                  {/if}
                </div>
              {/each}
              {#if shownVersions.length > 100}
                <p class="note">Showing the newest 100. Filter to narrow it down.</p>
              {/if}
            </div>
          {/if}
        {:else}
          <div class="gallery">
            {#each page.gallery as image (image.url)}
              <figure>
                <img src={image.url} alt={image.title ?? ""} loading="lazy" />
                {#if image.title}<figcaption>{image.title}</figcaption>{/if}
              </figure>
            {/each}
          </div>
        {/if}
      </div>

      <aside>
        <section>
          <h3>Compatibility</h3>
          {#if page.loaders.length}
            <div class="chips">
              {#each page.loaders as loader (loader)}<span>{loader}</span>{/each}
            </div>
          {/if}
          {#if page.gameVersions.length}
            <p class="muted">Minecraft {gameRange(page.gameVersions)}</p>
          {/if}
        </section>
        {#if links.length}
          <section>
            <h3>Links</h3>
            {#each links as [url, label, Icon] (url)}
              <button class="link" onclick={() => openExternal(url!)}>
                <Icon size={16} />
                {label}
                <ExternalLink size={13} />
              </button>
            {/each}
          </section>
        {/if}
        {#if page.license}
          <section>
            <h3>License</h3>
            <p class="muted license">
              <Scale size={15} />
              {page.license.name || page.license.id.replace("LicenseRef-", "")}
            </p>
          </section>
        {/if}
      </aside>
    </div>
  {/if}
</div>

{#if installing && page}
  <InstallPopup hit={pageAsHit(page)} version={installing} onClose={() => (installing = null)} />
{/if}

<style>
  .page {
    padding: 16px 24px 32px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .back {
    align-self: flex-start;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 999px;
    background: var(--bg-button);
    font-weight: 600;
  }

  .back:hover {
    background: var(--bg-button-hover);
  }

  .note {
    padding: 20px;
    text-align: center;
    color: var(--text-muted);
  }

  .error {
    color: #ff6b6b;
  }

  header {
    display: flex;
    gap: 18px;
    align-items: flex-start;
  }

  .icon {
    width: 96px;
    height: 96px;
    flex-shrink: 0;
    border-radius: var(--radius-lg);
    background: var(--skeleton);
    object-fit: cover;
  }

  .title {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  h2 {
    font-size: 1.6rem;
    color: var(--text-strong);
  }

  .muted {
    color: var(--text-muted);
  }

  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    margin-top: 4px;
    color: var(--text-muted);
  }

  .stats span {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .layout {
    display: flex;
    gap: 20px;
    align-items: flex-start;
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
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
    padding: 7px 14px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    color: var(--text-muted);
  }

  .tabs button.active {
    background: var(--brand-highlight);
    color: var(--brand);
  }

  .markdown {
    padding: 20px 24px;
    border-radius: var(--radius-lg);
    background: var(--bg-raised);
    line-height: 1.6;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .markdown :global(img) {
    max-width: 100%;
    height: auto;
  }

  .markdown :global(a) {
    color: var(--brand);
    cursor: pointer;
  }

  .markdown :global(h1),
  .markdown :global(h2),
  .markdown :global(h3) {
    color: var(--text-strong);
    margin: 1em 0 0.4em;
  }

  .markdown :global(p),
  .markdown :global(ul),
  .markdown :global(ol) {
    margin: 0.6em 0;
  }

  .markdown :global(ul),
  .markdown :global(ol) {
    padding-left: 1.4em;
  }

  .markdown :global(code) {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--bg-button);
    font-size: 0.9em;
  }

  .markdown :global(pre) {
    padding: 12px;
    border-radius: var(--radius-sm);
    background: var(--bg-shell);
    overflow-x: auto;
  }

  .markdown :global(table) {
    border-collapse: collapse;
  }

  .markdown :global(td),
  .markdown :global(th) {
    padding: 4px 10px;
    border: 1px solid var(--border);
  }

  .markdown :global(hr) {
    border: none;
    border-top: 1px solid var(--border);
  }

  .filters {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  select {
    height: 34px;
    padding: 0 10px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
    font: inherit;
  }

  .versions {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .version {
    display: grid;
    grid-template-columns: 30px minmax(0, 2fr) minmax(0, 1fr) minmax(0, 1fr) auto 34px;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--bg-raised);
    font-size: 0.92rem;
  }

  .version-main {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .version-main > *,
  .loaders,
  .games {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .loaders {
    text-transform: capitalize;
  }

  .version-install {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--bg-button);
  }

  .version-install:hover {
    background: var(--brand-bg);
    color: var(--brand-text);
  }

  .type {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    font-weight: 700;
    --badge: var(--brand);
    color: var(--badge);
    background: color-mix(in srgb, var(--badge) 16%, transparent);
  }

  .type[data-type="beta"] {
    --badge: #f0a830;
  }

  .type[data-type="alpha"] {
    --badge: #ff6b6b;
  }

  .gallery {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
  }

  figure {
    margin: 0;
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--bg-raised);
  }

  figure img {
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    display: block;
  }

  figcaption {
    padding: 8px 12px;
    font-weight: 600;
  }

  aside {
    width: 260px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  aside section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 16px;
    border-radius: var(--radius-lg);
    background: var(--bg-raised);
  }

  aside h3 {
    font-size: 0.95rem;
    color: var(--text-strong);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chips span {
    padding: 2px 10px;
    border-radius: 999px;
    background: var(--bg-button);
    font-size: 0.85rem;
    text-transform: capitalize;
  }

  .link {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    color: var(--text);
    font-weight: 600;
  }

  .link:hover {
    color: var(--brand);
  }

  .license {
    display: flex;
    align-items: center;
    gap: 6px;
  }
</style>
