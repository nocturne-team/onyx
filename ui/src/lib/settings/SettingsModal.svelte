<script lang="ts">
  import JavaPicker from "../components/JavaPicker.svelte";
  import MemorySetting from "../components/MemorySetting.svelte";
  import {
    Coffee,
    Gamepad2,
    Gauge,
    Languages,
    Moon,
    Paintbrush,
    Settings,
    Shield,
    Sun,
    X,
  } from "@lucide/svelte";
  import { appInfo, settings, updateSettings, type Theme } from "./store.svelte";

  let { onClose }: { onClose: () => void } = $props();

  const sections = [
    { id: "appearance", label: "Appearance", icon: Paintbrush },
    { id: "language", label: "Language", icon: Languages },
    { id: "privacy", label: "Privacy", icon: Shield },
    { id: "java", label: "Java installations", icon: Coffee },
    { id: "instance", label: "Default instance options", icon: Gamepad2 },
    { id: "resources", label: "Resource management", icon: Gauge },
  ] as const;

  const themes: { id: Theme; label: string; preview: "dark" | "light" | "oled" }[] = [
    { id: "dark", label: "Dark", preview: "dark" },
    { id: "light", label: "Light", preview: "light" },
    { id: "oled", label: "OLED", preview: "oled" },
    { id: "system", label: "Sync with system", preview: "dark" },
  ];

  // Speed cap presets in KiB/s, picked with a slider.
  const speedSteps = [256, 512, 1024, 2048, 5120, 10240, 25600, 51200];
  // svelte-ignore state_referenced_locally
  let speedIndex = $state(nearestSpeed(settings.downloadLimitKib ?? 5120));
  // svelte-ignore state_referenced_locally
  let concurrentDraft = $state(settings.maxConcurrentDownloads);

  function nearestSpeed(kib: number) {
    let best = 0;
    speedSteps.forEach((step, i) => {
      if (Math.abs(step - kib) < Math.abs(speedSteps[best] - kib)) best = i;
    });
    return best;
  }

  function speedLabel(kib: number) {
    return kib >= 1024 ? `${(kib / 1024).toFixed(kib % 1024 === 0 ? 0 : 1)} MB/s` : `${kib} KB/s`;
  }

  let active = $state<(typeof sections)[number]["id"]>("appearance");
  const info = appInfo();

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div
    class="modal"
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-labelledby="settings-title"
    onclick={(e) => e.stopPropagation()}
  >
    <header>
      <h2 id="settings-title"><Settings size={20} /> Settings</h2>
      <button class="close" aria-label="Close settings" onclick={onClose}><X size={18} /></button>
    </header>

    <div class="body">
      <nav>
        {#each sections as section (section.id)}
          <button class:active={active === section.id} onclick={() => (active = section.id)}>
            <section.icon size={18} />
            {section.label}
          </button>
        {/each}

        <div class="about">
          {#await info then { version, os }}
            <span>Onyx {version}</span>
            <span>{os}</span>
          {/await}
        </div>
      </nav>

      <div class="content">
        {#if active === "appearance"}
          <h3>Color theme</h3>
          <p class="hint">Select your preferred color theme for Onyx.</p>
          <div class="themes">
            {#each themes as theme (theme.id)}
              <button
                class="theme"
                class:selected={settings.theme === theme.id}
                aria-pressed={settings.theme === theme.id}
                onclick={() => updateSettings({ theme: theme.id })}
              >
                <div class="preview" data-theme={theme.preview}>
                  <div class="mini-card">
                    <div class="mini-icon"></div>
                    <div class="mini-lines"><span></span><span></span></div>
                  </div>
                </div>
                <div class="theme-label">
                  <span class="radio"></span>
                  {theme.label}
                  {#if theme.id === "dark"}<Moon size={15} />{/if}
                  {#if theme.id === "light"}<Sun size={15} />{/if}
                </div>
              </button>
            {/each}
          </div>

          <div class="toggle-row">
            <div>
              <h3>Advanced rendering</h3>
              <p class="hint">
                Enables blur and other effects that may cause performance issues without
                hardware-accelerated rendering.
              </p>
            </div>
            <button
              class="switch"
              role="switch"
              aria-label="Advanced rendering"
              aria-checked={settings.advancedRendering}
              onclick={() => updateSettings({ advancedRendering: !settings.advancedRendering })}
            >
              <span></span>
            </button>
          </div>
        {:else if active === "java"}
          <h3>Java installations</h3>
          <p class="hint">
            Onyx downloads Mojang's Java for each Minecraft version on its own. Pick an installed Java
            here to use it instead.
          </p>
          {#each [25, 21, 17, 8] as major (major)}
            <div class="java-row">
              <h4>Java {major}</h4>
              <JavaPicker
                {major}
                value={settings.javaOverrides[major] ?? null}
                automaticLabel="Automatic (Mojang's Java {major})"
                onChange={(path) => {
                  const next = { ...settings.javaOverrides };
                  if (path) next[major] = path;
                  else delete next[major];
                  updateSettings({ javaOverrides: next });
                }}
              />
            </div>
          {/each}
        {:else if active === "instance"}
          <h3>Memory</h3>
          <p class="hint">Maximum memory for instances that don't set their own.</p>
          {#await info then { totalMemoryMb }}
            <MemorySetting
              value={settings.defaultMemoryMb}
              fallbackLabel="Mojang's recommendation for each version"
              totalMb={totalMemoryMb}
              onChange={(value) => updateSettings({ defaultMemoryMb: value })}
            />
          {/await}
        {:else if active === "resources"}
          <h3>Maximum concurrent downloads</h3>
          <p class="hint">
            How many files Onyx downloads at the same time when installing games, mods and modpacks.
          </p>
          <div class="slider">
            <input
              type="range"
              min="1"
              max="32"
              bind:value={concurrentDraft}
              onchange={() => updateSettings({ maxConcurrentDownloads: concurrentDraft })}
              aria-label="Maximum concurrent downloads"
            />
            <strong>{concurrentDraft}</strong>
          </div>

          <div class="toggle-row spaced">
            <div>
              <h3>Limit download speed</h3>
              <p class="hint">
                Caps the total speed of all downloads, so installs don't take over your connection.
              </p>
            </div>
            <button
              class="switch"
              role="switch"
              aria-label="Limit download speed"
              aria-checked={settings.downloadLimitKib !== null}
              onclick={() =>
                updateSettings({
                  downloadLimitKib:
                    settings.downloadLimitKib === null ? speedSteps[speedIndex] : null,
                })}
            >
              <span></span>
            </button>
          </div>
          {#if settings.downloadLimitKib !== null}
            <div class="slider">
              <input
                type="range"
                min="0"
                max={speedSteps.length - 1}
                bind:value={speedIndex}
                onchange={() => updateSettings({ downloadLimitKib: speedSteps[speedIndex] })}
                aria-label="Download speed limit"
              />
              <strong>{speedLabel(speedSteps[speedIndex])}</strong>
            </div>
          {/if}
        {:else if active === "privacy"}
          <div class="toggle-row">
            <div>
              <h3>Discord rich presence</h3>
              <p class="hint">
                Shows the instance you're playing on your Discord profile. Turn it off and Onyx
                won't appear there at all.
              </p>
            </div>
            <button
              class="switch"
              role="switch"
              aria-label="Discord rich presence"
              aria-checked={settings.discordRichPresence}
              onclick={() => updateSettings({ discordRichPresence: !settings.discordRichPresence })}
            >
              <span></span>
            </button>
          </div>
        {:else}
          <p class="hint">Not built yet.</p>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    background: var(--overlay);
    backdrop-filter: var(--blur);
  }

  .modal {
    width: min(920px, calc(100vw - 48px));
    height: min(640px, calc(100vh - 48px));
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.4);
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 22px;
    border-bottom: 1px solid var(--border);
  }

  header h2 {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 1.15rem;
  }

  .close {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--bg-button);
  }

  .close:hover {
    background: var(--bg-button-hover);
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  nav {
    width: 270px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px;
    border-right: 1px solid var(--border);
  }

  nav button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    text-align: left;
  }

  nav button:hover {
    background: var(--bg-button);
  }

  nav button.active {
    background: var(--brand-highlight);
    color: var(--brand);
  }

  .about {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    padding: 0 12px;
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .content {
    flex: 1;
    overflow-y: auto;
    padding: 18px 22px 24px;
  }

  h3 {
    font-size: 1.05rem;
  }

  .hint {
    margin: 4px 0 14px;
    color: var(--text-muted);
  }

  .themes {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 12px;
    margin-bottom: 28px;
  }

  .theme {
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--bg-button);
    border: 2px solid transparent;
    text-align: left;
  }

  .theme.selected {
    border-color: var(--brand);
  }

  .preview {
    padding: 24px 20px;
    background: var(--bg);
  }

  .mini-card {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--bg-raised);
  }

  .mini-icon {
    width: 26px;
    height: 26px;
    border-radius: 6px;
    background: var(--bg-button);
  }

  .mini-lines {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .mini-lines span {
    height: 6px;
    border-radius: 3px;
    background: var(--text-muted);
  }

  .mini-lines span:last-child {
    width: 60%;
  }

  .theme-label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    font-weight: 600;
  }

  .radio {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--text-muted);
    flex-shrink: 0;
  }

  .selected .radio {
    border-color: var(--brand);
    background: radial-gradient(var(--brand) 40%, transparent 45%);
  }

  .java-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 16px;
  }

  .java-row h4 {
    font-size: 0.95rem;
    color: var(--text-strong);
  }

  .toggle-row {
    display: flex;
    gap: 24px;
    align-items: center;
    justify-content: space-between;
  }

  .toggle-row.spaced {
    margin-top: 28px;
  }

  .slider {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .slider input {
    flex: 1;
    accent-color: var(--brand);
  }

  .slider strong {
    min-width: 72px;
    text-align: right;
  }

  .switch {
    width: 50px;
    height: 28px;
    flex-shrink: 0;
    border-radius: 999px;
    background: var(--bg-button);
    position: relative;
    transition: background 150ms;
  }

  .switch span {
    position: absolute;
    top: 4px;
    left: 4px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--text-strong);
    transition: transform 150ms;
  }

  .switch[aria-checked="true"] {
    background: var(--brand-bg);
  }

  .switch[aria-checked="true"] span {
    transform: translateX(22px);
  }
</style>
