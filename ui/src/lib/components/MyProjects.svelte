<script lang="ts">
  import { CalendarDays, Download, Heart, RefreshCw } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { account, signInWithBrowser } from "../account.svelte";
  import { compactNumber, relativeTime } from "../format";

  /** Mirrors `onyx_core::modrinth::Project` (the fields we show). */
  interface Project {
    id: string;
    slug: string | null;
    title: string;
    description: string;
    projectType: string;
    status: string;
    requestedStatus: string | null;
    iconUrl: string | null;
    downloads: number;
    followers: number;
    updated: string;
    /** Set when the project is listed through one of the user's organizations. */
    organizationName: string | null;
  }

  const statusLabels: Record<string, string> = {
    approved: "Public",
    processing: "In review",
    draft: "Draft",
    unlisted: "Unlisted",
    private: "Private",
    rejected: "Rejected",
    withheld: "Withheld",
    archived: "Archived",
    scheduled: "Scheduled",
    unknown: "Unknown",
  };

  // Projects needing attention first.
  const statusOrder = [
    "rejected",
    "withheld",
    "processing",
    "draft",
    "scheduled",
    "private",
    "unlisted",
    "approved",
    "archived",
    "unknown",
  ];

  let projects = $state<Project[] | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let statusFilter = $state<string | null>(null);
  let typeFilter = $state<string | null>(null);
  /** `""` means personal projects; any other string is an organization name. */
  let ownerFilter = $state<string | null>(null);

  async function load(refresh: boolean) {
    loading = true;
    error = null;
    try {
      projects = await invoke<Project[]>("my_projects", { refresh });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Load once signed in; re-load if the account changes.
  let loadedFor: string | null = null;
  $effect(() => {
    const id = account.user?.id ?? null;
    if (id && id !== loadedFor) {
      loadedFor = id;
      load(false);
    }
    if (!id) {
      loadedFor = null;
      projects = null;
    }
  });

  const statuses = $derived(
    statusOrder.filter((s) => projects?.some((p) => p.status === s)),
  );
  const types = $derived([...new Set(projects?.map((p) => p.projectType) ?? [])].sort());
  const organizations = $derived(
    [...new Set(projects?.flatMap((p) => p.organizationName ?? []) ?? [])].sort(),
  );

  const visible = $derived(
    (projects ?? [])
      .filter((p) => !statusFilter || p.status === statusFilter)
      .filter((p) => !typeFilter || p.projectType === typeFilter)
      .filter((p) => ownerFilter === null || (p.organizationName ?? "") === ownerFilter)
      .toSorted(
        (a, b) =>
          statusOrder.indexOf(a.status) - statusOrder.indexOf(b.status) ||
          b.updated.localeCompare(a.updated),
      ),
  );
</script>

{#if account.status === "restoring"}
  <p class="note">Checking your Modrinth login...</p>
{:else if account.status !== "signed-in"}
  <div class="signed-out">
    <p>Sign in to Modrinth to see all of your projects, including drafts, private ones and ones in review.</p>
    <button class="primary" onclick={signInWithBrowser} disabled={account.status === "signing-in"}>
      {account.status === "signing-in" ? "Waiting for browser..." : "Sign in with Modrinth"}
    </button>
  </div>
{:else}
  <div class="toolbar">
    <div class="chips">
      <button class:active={statusFilter === null} onclick={() => (statusFilter = null)}>
        All {projects ? `(${projects.length})` : ""}
      </button>
      {#each statuses as status (status)}
        <button class:active={statusFilter === status} onclick={() => (statusFilter = status)}>
          {statusLabels[status] ?? status}
          ({projects?.filter((p) => p.status === status).length})
        </button>
      {/each}
    </div>
    {#if organizations.length > 0}
      <select bind:value={ownerFilter} aria-label="Owner">
        <option value={null}>All owners</option>
        <option value="">Personal</option>
        {#each organizations as organization (organization)}
          <option value={organization}>{organization}</option>
        {/each}
      </select>
    {/if}
    {#if types.length > 1}
      <select bind:value={typeFilter} aria-label="Project type">
        <option value={null}>All types</option>
        {#each types as type (type)}
          <option value={type}>{type}</option>
        {/each}
      </select>
    {/if}
    <button class="refresh" onclick={() => load(true)} disabled={loading} title="Refresh">
      <RefreshCw size={16} class={loading ? "spin" : ""} />
    </button>
  </div>

  {#if error}
    <p class="note error">{error}</p>
  {:else if projects && projects.length === 0}
    <p class="note">You don't have any projects on Modrinth yet.</p>
  {:else if !projects}
    <p class="note">Loading your projects...</p>
  {/if}

  <div class="list">
    {#each visible as project (project.id)}
      <article class="row">
        {#if project.iconUrl}
          <img class="icon" src={project.iconUrl} alt="" loading="lazy" />
        {:else}
          <div class="icon"></div>
        {/if}
        <div class="main">
          <div class="title">
            <h3>{project.title}</h3>
            <span class="badge" data-status={project.status}>
              {statusLabels[project.status] ?? project.status}
            </span>
            {#if project.status === "processing" && project.requestedStatus && project.requestedStatus !== "approved"}
              <span class="muted">then {statusLabels[project.requestedStatus] ?? project.requestedStatus}</span>
            {/if}
          </div>
          <p>{project.description || "No description"}</p>
          <span class="muted">
            <span class="type">{project.projectType}</span>
            {#if project.organizationName}· by {project.organizationName}{/if}
          </span>
        </div>
        <div class="stats">
          <span title="Downloads"><Download size={15} />{compactNumber(project.downloads)}</span>
          <span title="Followers"><Heart size={15} />{compactNumber(project.followers)}</span>
          <span title="Updated"><CalendarDays size={15} />{relativeTime(new Date(project.updated))}</span>
        </div>
      </article>
    {/each}
  </div>
{/if}

<style>
  .note {
    padding: 20px;
    text-align: center;
    color: var(--text-muted);
  }

  .error {
    color: #ff6b6b;
  }

  .signed-out {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    padding: 48px 20px;
    text-align: center;
    color: var(--text-muted);
  }

  .primary {
    padding: 10px 18px;
    border-radius: var(--radius-sm);
    background: var(--brand-bg);
    color: var(--brand-text);
    font-weight: 600;
  }

  .toolbar {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-bottom: 14px;
  }

  .chips {
    flex: 1;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chips button {
    padding: 6px 14px;
    border-radius: 999px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .chips button:hover {
    background: var(--bg-button);
  }

  .chips button.active {
    background: var(--brand-highlight);
    color: var(--brand);
  }

  select {
    height: 34px;
    padding: 0 10px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
    font: inherit;
    text-transform: capitalize;
  }

  .refresh {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--bg-button);
  }

  .refresh:hover {
    background: var(--bg-button-hover);
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .row {
    display: flex;
    gap: 16px;
    padding: 16px;
    background: var(--bg-raised);
    border-radius: var(--radius-lg);
  }

  .icon {
    width: 64px;
    height: 64px;
    flex-shrink: 0;
    border-radius: var(--radius);
    background: var(--skeleton);
    object-fit: cover;
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  h3 {
    font-size: 1.1rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  p {
    margin: 0;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .muted {
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  .type {
    text-transform: capitalize;
  }

  .badge {
    flex-shrink: 0;
    padding: 2px 9px;
    border-radius: 999px;
    font-size: 0.78rem;
    font-weight: 700;
    --badge: var(--text-muted);
    color: var(--badge);
    background: color-mix(in srgb, var(--badge) 16%, transparent);
  }

  .badge[data-status="approved"] {
    --badge: var(--brand);
  }

  .badge[data-status="processing"],
  .badge[data-status="scheduled"] {
    --badge: #f0a830;
  }

  .badge[data-status="rejected"],
  .badge[data-status="withheld"] {
    --badge: #ff6b6b;
  }

  .badge[data-status="unlisted"] {
    --badge: #5aa7ff;
  }

  .badge[data-status="private"] {
    --badge: #b48cff;
  }

  .stats {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
    flex-shrink: 0;
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .stats span {
    display: flex;
    align-items: center;
    gap: 6px;
  }
</style>
