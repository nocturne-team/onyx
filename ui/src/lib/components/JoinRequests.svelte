<script lang="ts">
  import { Check, X } from "@lucide/svelte";
  import { answerJoinRequest, invites } from "../invites.svelte";
</script>

{#if invites.requests.length}
  <div class="stack" role="status">
    {#each invites.requests as request (request.userId)}
      <div class="toast">
        {#if request.avatarUrl}
          <img src={request.avatarUrl} alt="" width="36" height="36" />
        {:else}
          <span class="avatar">{request.name.slice(0, 1).toUpperCase()}</span>
        {/if}
        <span class="text"><strong>{request.name}</strong> wants to join your game</span>
        <button class="accept" title="Let them join" onclick={() => answerJoinRequest(request, true)}>
          <Check size={16} />
        </button>
        <button class="decline" title="Decline" onclick={() => answerJoinRequest(request, false)}>
          <X size={16} />
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .stack {
    position: fixed;
    right: 20px;
    bottom: 20px;
    z-index: 15;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(360px, calc(100vw - 40px));
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.35);
  }

  img,
  .avatar {
    width: 36px;
    height: 36px;
    flex-shrink: 0;
    border-radius: 50%;
  }

  .avatar {
    display: grid;
    place-items: center;
    background: var(--bg-button);
    font-weight: 700;
  }

  .text {
    flex: 1;
    min-width: 0;
    font-size: 0.92rem;
  }

  strong {
    color: var(--text-strong);
  }

  button {
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border-radius: 50%;
  }

  .accept {
    background: var(--brand-bg);
    color: var(--brand-text);
  }

  .decline {
    background: var(--bg-button);
  }
</style>
