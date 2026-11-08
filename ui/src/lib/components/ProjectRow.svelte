<script lang="ts">
  import { CalendarDays, Download, Heart } from "@lucide/svelte";
  import { compactNumber, relativeTime } from "../format";
  import type { Snippet } from "svelte";
  import type { SearchHit } from "../modrinth";

  let { hit, actions }: { hit: SearchHit; actions?: Snippet } = $props();
</script>

<article class="row">
  {#if hit.iconUrl}
    <img class="icon" src={hit.iconUrl} alt="" loading="lazy" />
  {:else}
    <div class="icon"></div>
  {/if}

  <div class="main">
    <div class="title">
      <h3>{hit.title}</h3>
      <span>by {hit.author}</span>
    </div>
    <p>{hit.description}</p>
    {#if hit.displayCategories.length}
      <div class="tags">
        {#each hit.displayCategories as tag (tag)}
          <span>{tag}</span>
        {/each}
      </div>
    {/if}
  </div>

  <div class="stats">
    {@render actions?.()}
    <span title="Downloads"><Download size={15} />{compactNumber(hit.downloads)}</span>
    <span title="Followers"><Heart size={15} />{compactNumber(hit.follows)}</span>
    <span title="Updated"><CalendarDays size={15} />{relativeTime(new Date(hit.dateModified))}</span>
  </div>
</article>

<style>
  .row {
    display: flex;
    gap: 16px;
    padding: 16px;
    background: var(--bg-raised);
    border-radius: var(--radius-lg);
  }

  .icon {
    width: 72px;
    height: 72px;
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
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  h3 {
    font-size: 1.1rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .title span {
    color: var(--text-muted);
    white-space: nowrap;
  }

  p {
    margin: 0;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .tags span {
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--bg-button);
    color: var(--text-muted);
    font-size: 0.8rem;
    text-transform: capitalize;
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
