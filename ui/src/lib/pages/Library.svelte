<script lang="ts">
  import { Plus } from "@lucide/svelte";
  import InstanceCard from "../components/InstanceCard.svelte";
  import MyProjects from "../components/MyProjects.svelte";
  import { instances } from "../instances.svelte";
  import { library, type LibraryTab } from "./library.svelte";

  const tabs: { id: LibraryTab; label: string }[] = [
    { id: "instances", label: "Instances" },
    { id: "projects", label: "My projects" },
  ];

  let { onCreateInstance }: { onCreateInstance: () => void } = $props();
</script>

<div class="page">
  <div class="tabs" role="tablist">
    {#each tabs as tab (tab.id)}
      <button
        role="tab"
        aria-selected={library.tab === tab.id}
        class:active={library.tab === tab.id}
        onclick={() => (library.tab = tab.id)}
      >
        {tab.label}
      </button>
    {/each}
  </div>

  {#if library.tab === "instances"}
    <div class="grid">
      {#each instances.list as instance (instance.id)}
        <InstanceCard {instance} />
      {/each}
      <button class="create" onclick={onCreateInstance}><Plus size={20} /> Create instance</button>
    </div>
  {:else}
    <MyProjects />
  {/if}
</div>

<style>
  .page {
    padding: 20px 24px 32px;
    display: flex;
    flex-direction: column;
    gap: 14px;
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

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
  }

  .create {
    min-height: 84px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border: 1px dashed var(--border);
    border-radius: var(--radius-lg);
    color: var(--text-muted);
    font-weight: 600;
  }

  .create:hover {
    color: var(--text-strong);
    border-color: var(--text-muted);
  }
</style>
