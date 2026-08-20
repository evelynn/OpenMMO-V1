<script lang="ts">
  import { guildInvite } from '../stores/guildStore'
  import { networkManager } from '../network/socket'

  function answer(accept: boolean) {
    const invite = $guildInvite
    if (!invite) return
    networkManager.sendRespondGuildInvite(invite.guildId, accept)
    guildInvite.set(null)
  }
</script>

{#if $guildInvite}
  <div class="guild-invite" role="dialog" aria-label="Guild invite">
    <div class="text">
      <strong>{$guildInvite.from}</strong> invites you to
      <strong>{$guildInvite.guildName}</strong>
    </div>
    <div class="buttons">
      <button class="accept" onclick={() => answer(true)}>Join</button>
      <button onclick={() => answer(false)}>Decline</button>
    </div>
  </div>
{/if}

<style>
  .guild-invite {
    position: fixed;
    top: 26%;
    left: 50%;
    transform: translateX(-50%);
    z-index: 1200;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 16px;
    border: 1px solid rgba(226, 185, 59, 0.6);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.85);
    color: #ddd;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    text-align: center;
  }

  .buttons {
    display: flex;
    gap: 8px;
    justify-content: center;
  }

  button {
    padding: 4px 14px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.08);
    color: #ddd;
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
  }

  button.accept {
    border-color: rgba(226, 185, 59, 0.6);
    background: rgba(226, 185, 59, 0.2);
    color: #e2b93b;
  }
</style>
