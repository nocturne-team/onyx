<script lang="ts">
  import DOMPurify from "dompurify";
  import { categories, type Category } from "../modrinth";

  interface Props {
    projectType: string;
    selected: string[];
    onChange: (selected: string[]) => void;
  }

  let { projectType, selected, onChange }: Props = $props();

  const all = categories().catch(() => [] as Category[]);

  // Modrinth's tag icons are SVG from the API; strip anything but drawing markup.
  const icons = new Map<string, string>();
  function icon(category: Category) {
    let svg = icons.get(category.icon);
    if (svg === undefined) {
      svg = DOMPurify.sanitize(category.icon, {
        USE_PROFILES: { svg: true },
        FORBID_TAGS: ["style", "foreignObject", "a", "use", "image"],
        FORBID_ATTR: ["style"],
      });
      icons.set(category.icon, svg);
    }
    return svg;
  }

  function groups(list: Category[]) {
    const byHeader = new Map<string, Category[]>();
    for (const category of list) {
      if (category.projectType !== projectType) continue;
      const group = byHeader.get(category.header) ?? [];
      group.push(category);
      byHeader.set(category.header, group);
    }
    for (const group of byHeader.values()) group.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
    // Same group order as modrinth.com; unknown headers go last.
    const rank = (h: string) => {
      const i = headerOrder.indexOf(h);
      return i === -1 ? headerOrder.length : i;
    };
    return [...byHeader].sort(([a], [b]) => rank(a) - rank(b));
  }

  const headerOrder = ["categories", "features", "resolutions", "performance impact"];
  const acronyms: Record<string, string> = { pbr: "PBR", gui: "GUI" };

  const label = (name: string) => {
    if (acronyms[name]) return acronyms[name];
    const words = name.replace(/-/g, " ");
    return words.charAt(0).toUpperCase() + words.slice(1);
  };

  function toggle(name: string) {
    onChange(selected.includes(name) ? selected.filter((n) => n !== name) : [...selected, name]);
  }
</script>

{#await all then list}
  <div class="groups">
    {#each groups(list) as [header, group] (header)}
      <div class="group">
        <h4>{label(header)}</h4>
        <div class="chips">
          {#each group as category (category.name)}
            <button
              class:active={selected.includes(category.name)}
              aria-pressed={selected.includes(category.name)}
              onclick={() => toggle(category.name)}
            >
              {#if category.icon}
                <span class="icon">{@html icon(category)}</span>
              {/if}
              {label(category.name)}
            </button>
          {/each}
        </div>
      </div>
    {:else}
      <p class="empty">No categories for this type.</p>
    {/each}
  </div>
{/await}

<style>
  .groups {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 16px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  h4 {
    margin: 0 0 6px;
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chips button {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 11px;
    border-radius: 999px;
    background: var(--bg-button);
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--text);
  }

  .chips button:hover {
    background: var(--bg-button-hover);
  }

  .chips button.active {
    background: var(--brand-highlight);
    color: var(--brand);
  }

  .icon {
    display: grid;
    width: 15px;
    height: 15px;
  }

  .icon :global(svg) {
    width: 100%;
    height: 100%;
  }

  .empty {
    color: var(--text-muted);
  }
</style>
