<script lang="ts">
  interface Props {
    /** Megabytes, or null for "use the fallback". */
    value: number | null;
    /** What null means, e.g. "Mojang's recommendation" or "the default (4 GB)". */
    fallbackLabel: string;
    /** Physical memory in MB, if known; caps the slider. */
    totalMb: number | null;
    onChange: (value: number | null) => void;
  }

  let { value, fallbackLabel, totalMb, onChange }: Props = $props();

  // Leave some memory for the OS; without a known total, allow up to 16 GB.
  const max = $derived(totalMb ? Math.max(1024, Math.floor((totalMb * 0.75) / 512) * 512) : 16384);
  // svelte-ignore state_referenced_locally
  let draft = $state(value ?? 4096);
  $effect(() => {
    if (value !== null) draft = value;
  });

  const gb = (mb: number) => `${(mb / 1024).toFixed(mb % 1024 === 0 ? 0 : 1)} GB`;
</script>

<div class="memory">
  <label class="check">
    <input
      type="checkbox"
      checked={value === null}
      onchange={(e) => onChange(e.currentTarget.checked ? null : Math.min(draft, max))}
    />
    Use {fallbackLabel}
  </label>
  {#if value !== null}
    <div class="slider">
      <input
        type="range"
        min="1024"
        {max}
        step="512"
        bind:value={draft}
        onchange={() => onChange(draft)}
        aria-label="Maximum memory"
      />
      <strong>{gb(draft)}</strong>
    </div>
    <p class="hint">
      1 GB – {gb(max)}{totalMb ? ` (this PC has ${gb(Math.round(totalMb / 512) * 512)})` : ""}.
      More isn't always faster; 4–8 GB suits most modpacks.
    </p>
  {/if}
</div>

<style>
  .memory {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
  }

  .slider {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  input[type="range"] {
    flex: 1;
    accent-color: var(--brand);
  }

  strong {
    min-width: 56px;
    text-align: right;
    color: var(--text-strong);
  }

  .hint {
    color: var(--text-muted);
    font-size: 0.88rem;
  }
</style>
