<script lang="ts">
  import { loaderNames, type Instance } from "../instances.svelte";
  import { openInstance } from "../navigation.svelte";
  import InstanceIcon from "./InstanceIcon.svelte";
  import PlayButton from "./PlayButton.svelte";

  let { instance }: { instance: Instance } = $props();
</script>

<div class="card">
  <button class="open" onclick={() => openInstance(instance.id)}>
    <InstanceIcon {instance} size={56} />
    <span class="text">
      <strong>{instance.name}</strong>
      <span class="muted">{loaderNames[instance.loader]} {instance.gameVersion}</span>
    </span>
  </button>
  <div class="play"><PlayButton id={instance.id} compact /></div>
</div>

<style>
  .card {
    position: relative;
    background: var(--bg-raised);
    border-radius: var(--radius-lg);
  }

  .open {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px;
    padding-right: 64px;
    text-align: left;
    border-radius: var(--radius-lg);
  }

  .open:hover {
    background: var(--bg-button);
  }

  .text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  strong {
    color: var(--text-strong);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .muted {
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .play {
    position: absolute;
    right: 14px;
    top: 50%;
    translate: 0 -50%;
  }
</style>
