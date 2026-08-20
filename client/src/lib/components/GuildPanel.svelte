<script lang="ts">
  import { networkManager } from '../network/socket'
  import {
    guild,
    myGuildPerms,
    isGuildLeader,
    GUILD_PERMS,
    LEADER_RANK,
  } from '../stores/guildStore'

  let newName = $state('')
  let inviteName = $state('')

  const rankName = $derived(
    (rankId: number) =>
      $guild?.ranks.find((r) => r.rank_id === rankId)?.name ?? `Rank ${rankId}`
  )
</script>

<div class="guild-panel">
  {#if !$guild}
    <div class="found">
      <p class="hint">You are in no guild.</p>
      <input
        bind:value={newName}
        placeholder="Guild name"
        maxlength="24"
        onkeydown={(e) => e.stopPropagation()}
      />
      <button
        disabled={newName.trim().length === 0}
        onclick={() => {
          networkManager.sendCreateGuild(newName.trim())
          newName = ''
        }}>Found</button
      >
    </div>
  {:else}
    <div class="header">
      <span class="name">{$guild.name}</span>
      <span class="rank">{rankName($guild.your_rank_id)}</span>
    </div>

    <div class="actions">
      <button onclick={() => networkManager.sendOpenGuildStorage()}>
        Vault
      </button>
      <button onclick={() => networkManager.sendLeaveGuild()}>Leave</button>
    </div>

    {#if ($myGuildPerms & GUILD_PERMS.INVITE) !== 0}
      <div class="invite">
        <input
          bind:value={inviteName}
          placeholder="Character name"
          onkeydown={(e) => e.stopPropagation()}
        />
        <button
          disabled={inviteName.trim().length === 0}
          onclick={() => {
            networkManager.sendInviteToGuild(inviteName.trim())
            inviteName = ''
          }}>Invite</button
        >
      </div>
    {/if}

    <div class="roster">
      {#each $guild.members as m (m.character_id)}
        <div class="member" class:offline={!m.online}>
          <span class="member-name">{m.name}</span>
          <span class="member-rank">{rankName(m.rank_id)}</span>
          {#if $isGuildLeader && m.rank_id !== LEADER_RANK}
            <select
              value={m.rank_id}
              onchange={(e) =>
                networkManager.sendSetGuildRank(
                  m.character_id,
                  Number(e.currentTarget.value)
                )}
            >
              {#each $guild.ranks.filter((r) => r.rank_id !== LEADER_RANK) as r (r.rank_id)}
                <option value={r.rank_id}>{r.name}</option>
              {/each}
            </select>
          {/if}
          {#if ($myGuildPerms & GUILD_PERMS.KICK) !== 0 && m.rank_id !== LEADER_RANK}
            <button
              class="kick"
              onclick={() => networkManager.sendKickFromGuild(m.character_id)}
              >×</button
            >
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .guild-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px;
    font-family: 'Courier New', monospace;
  }

  .hint {
    margin: 0;
    color: #999;
    font-size: 12px;
  }

  .found,
  .invite,
  .actions {
    display: flex;
    gap: 6px;
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 4px 6px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.5);
    color: #ddd;
    font-family: inherit;
    font-size: 12px;
  }

  button,
  select {
    padding: 4px 10px;
    border: 1px solid rgba(226, 185, 59, 0.4);
    border-radius: 4px;
    background: rgba(226, 185, 59, 0.15);
    color: #e2b93b;
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }

  .name {
    color: #e2b93b;
    font-size: 14px;
    font-weight: bold;
  }

  .rank {
    color: #999;
    font-size: 11px;
  }

  .roster {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .member {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    font-size: 12px;
    color: #ddd;
  }

  .member.offline {
    opacity: 0.45;
  }

  .member-name {
    flex: 1;
  }

  .member-rank {
    color: #999;
    font-size: 11px;
  }

  .kick {
    padding: 0 6px;
    color: #ff9a8a;
    border-color: rgba(255, 154, 138, 0.4);
    background: rgba(255, 154, 138, 0.12);
  }
</style>
