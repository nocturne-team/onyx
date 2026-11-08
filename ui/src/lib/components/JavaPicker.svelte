<script lang="ts">
  import { FolderOpen } from "@lucide/svelte";
  import { detectJava, pickJava, type JavaInstall } from "../java";

  interface Props {
    /** The chosen binary, or null for the automatic choice. */
    value: string | null;
    automaticLabel: string;
    /** Only offer detected installs of this major version. */
    major?: number;
    onChange: (path: string | null) => void;
  }

  let { value, automaticLabel, major, onChange }: Props = $props();

  let installs = $state<JavaInstall[]>([]);
  let error = $state<string | null>(null);
  detectJava().then((all) => (installs = all));

  const offered = $derived(installs.filter((j) => major === undefined || j.major === major));
  // A hand-picked path that detection doesn't know still needs an option.
  const extra = $derived(value && !offered.some((j) => j.path === value) ? value : null);

  async function browse() {
    error = null;
    try {
      const picked = await pickJava();
      if (!picked) return;
      if (major !== undefined && picked.major !== major) {
        error = `That's Java ${picked.major}, not Java ${major}.`;
        return;
      }
      if (!installs.some((j) => j.path === picked.path)) installs = [...installs, picked];
      onChange(picked.path);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="picker">
  <select
    value={value ?? ""}
    onchange={(e) => onChange(e.currentTarget.value || null)}
    aria-label="Java"
  >
    <option value="">{automaticLabel}</option>
    {#each offered as java (java.path)}
      <option value={java.path}>Java {java.version} · {java.path}</option>
    {/each}
    {#if extra}<option value={extra}>{extra}</option>{/if}
  </select>
  <button class="browse" onclick={browse} title="Pick a Java binary"><FolderOpen size={16} /> Browse</button>
</div>
{#if error}<p class="error">{error}</p>{/if}

<style>
  .picker {
    display: flex;
    gap: 8px;
  }

  select {
    flex: 1;
    min-width: 0;
    height: 36px;
    padding: 0 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
    font: inherit;
  }

  .browse {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 36px;
    padding: 0 12px;
    border-radius: var(--radius-sm);
    background: var(--bg-button);
    font-weight: 600;
    white-space: nowrap;
  }

  .browse:hover {
    background: var(--bg-button-hover);
  }

  .error {
    color: #ff6b6b;
    font-size: 0.88rem;
  }
</style>
