<script lang="ts">
  import { ChevronRight } from "@lucide/svelte";
  import InstanceCard from "../components/InstanceCard.svelte";
  import ProjectCard from "../components/ProjectCard.svelte";
  import { instances } from "../instances.svelte";
  import { nav, openProject } from "../navigation.svelte";
  import { search, toCardProject, type ProjectType } from "../modrinth";

  const PER_SECTION = 6;

  const section = (title: string, projectType: ProjectType) => ({
    title,
    // These barely change, so they're cached for an hour, on disk.
    results: search({ projectType, limit: PER_SECTION }, "long"),
  });

  const sections = [section("Popular packs", "modpack"), section("Popular mods", "mod")];

  // Instances are listed most recently played first.
  const recent = $derived(instances.list.filter((i) => i.lastPlayed).slice(0, 6));
</script>

<div class="page">
  <section>
    <h2>
      <button class="heading" onclick={() => (nav.page = "library")}>
        Jump back in <ChevronRight size={20} />
      </button>
    </h2>
    {#if recent.length}
      <div class="instances">
        {#each recent as instance (instance.id)}
          <InstanceCard {instance} />
        {/each}
      </div>
    {:else}
      <div class="empty">Instances you play will show up here.</div>
    {/if}
  </section>

  {#each sections as section (section.title)}
    <section>
      <h2>{section.title} <ChevronRight size={20} /></h2>
      {#await section.results}
        <div class="grid">
          {#each { length: PER_SECTION }, i (i)}
            <ProjectCard />
          {/each}
        </div>
      {:then results}
        <div class="grid">
          {#each results.hits as hit (hit.projectId)}
            <ProjectCard project={toCardProject(hit)} onclick={() => openProject(hit.projectId)} />
          {/each}
        </div>
      {:catch error}
        <div class="empty">Couldn't load {section.title.toLowerCase()}: {error}</div>
      {/await}
    </section>
  {/each}
</div>

<style>
  .page {
    padding: 20px 24px 32px;
    display: flex;
    flex-direction: column;
    gap: 28px;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 1.3rem;
    margin-bottom: 14px;
  }

  .heading {
    display: flex;
    align-items: center;
    gap: 6px;
    font: inherit;
  }

  .heading:hover {
    color: var(--text-strong);
  }

  .instances {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 16px;
  }

  .empty {
    padding: 28px;
    border: 1px dashed var(--border);
    border-radius: var(--radius-lg);
    color: var(--text-muted);
    text-align: center;
  }
</style>
