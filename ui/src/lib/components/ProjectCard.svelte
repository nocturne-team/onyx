<script lang="ts" module>
  export interface CardProject {
    title: string;
    author: string;
    description: string;
    iconUrl?: string;
    bannerUrl?: string;
    /** Icon color, used to tint the banner when there's no gallery image. */
    color?: string;
    downloads: number;
    follows: number;
    updated: Date;
  }
</script>

<script lang="ts">
  import { CalendarDays, Download, Heart } from "@lucide/svelte";
  import { compactNumber, relativeTime } from "../format";

  /** No project means a loading skeleton. */
  let { project }: { project?: CardProject } = $props();
</script>

<article class="card" class:skeleton={!project}>
  <div class="head">
    {#if project?.iconUrl}
      <img class="icon" src={project.iconUrl} alt="" loading="lazy" />
    {:else}
      <div class="icon"></div>
    {/if}
    <div class="names">
      {#if project}
        <h3>{project.title}</h3>
        <span>by {project.author}</span>
      {:else}
        <div class="bar wide"></div>
        <div class="bar"></div>
      {/if}
    </div>
  </div>

  <div
    class="banner"
    style:background-image={project?.bannerUrl
      ? `url("${project.bannerUrl}")`
      : project?.color
        ? `linear-gradient(135deg, ${project.color}55, ${project.color}11)`
        : undefined}
  >
    {#if project}
      <div class="chips">
        <span><Download size={14} />{compactNumber(project.downloads)}</span>
        <span><Heart size={14} />{compactNumber(project.follows)}</span>
        <span><CalendarDays size={14} />{relativeTime(project.updated)}</span>
      </div>
    {/if}
  </div>

  <p class="description">
    {#if project}
      {project.description}
    {:else}
      <span class="bar wide"></span>
      <span class="bar"></span>
    {/if}
  </p>
</article>

<style>
  .card {
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    border-radius: var(--radius-lg);
    overflow: hidden;
    min-width: 0;
  }

  .head {
    display: flex;
    gap: 14px;
    align-items: center;
    padding: 16px;
  }

  .icon {
    width: 48px;
    height: 48px;
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    background: var(--skeleton);
    object-fit: cover;
  }

  .names {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }

  h3 {
    font-size: 1.05rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .names span {
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .banner {
    position: relative;
    aspect-ratio: 16 / 9;
    background: var(--skeleton) center / cover no-repeat;
  }

  .chips {
    position: absolute;
    right: 10px;
    bottom: 10px;
    display: flex;
    gap: 6px;
  }

  .chips span {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 9px;
    border-radius: var(--radius-sm);
    background: var(--chip-bg);
    backdrop-filter: var(--blur);
    color: var(--text-strong);
    font-size: 0.82rem;
  }

  /* Spacing is margin, not padding: padding on a line-clamped box shows
     part of the next line. */
  .description {
    margin: 14px 16px 18px;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    min-height: calc(3 * 1.4em);
  }

  .bar {
    display: block;
    height: 12px;
    width: 40%;
    border-radius: 6px;
    background: var(--skeleton);
    margin-bottom: 8px;
  }

  .bar.wide {
    width: 75%;
  }

  .skeleton .bar,
  .skeleton .icon,
  .skeleton .banner {
    animation: pulse 1.4s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.55;
    }
  }
</style>
