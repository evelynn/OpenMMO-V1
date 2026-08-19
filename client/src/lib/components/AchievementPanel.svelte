<script lang="ts">
  import { networkManager } from '../network/socket'
  import {
    allAchievements,
    unlockedAchievements,
    activeTitle,
    availableTitles,
  } from '../stores/achievementStore'
  import GoldAmount from './GoldAmount.svelte'

  const rows = $derived(
    allAchievements().map((def) => ({
      def,
      unlocked: $unlockedAchievements.has(def.id),
    }))
  )

  const earned = $derived(rows.filter((r) => r.unlocked).length)
</script>

<div class="achievements">
  <div class="summary">
    <span>{earned} / {rows.length}</span>
    <label>
      Title
      <select
        value={$activeTitle ?? ''}
        onchange={(e) =>
          networkManager.sendSetTitle(e.currentTarget.value || null)}
      >
        <option value="">none</option>
        {#each $availableTitles as title (title)}
          <option value={title}>{title}</option>
        {/each}
      </select>
    </label>
  </div>

  {#each rows as row (row.def.id)}
    <div class="row" class:locked={!row.unlocked}>
      <div class="head">
        <span class="name">{row.def.name}</span>
        {#if row.def.titleId}
          <span class="title-tag">&lt;{row.def.titleId}&gt;</span>
        {/if}
      </div>
      <div class="detail">
        {row.def.description}
        {#if row.def.rewardZeny}
          · <GoldAmount copper={row.def.rewardZeny} />
        {/if}
      </div>
    </div>
  {/each}
</div>

<style>
  .achievements {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
  }

  .summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    color: #e2b93b;
    font-size: 12px;
  }

  .summary select {
    background: rgba(0, 0, 0, 0.6);
    color: #e2b93b;
    border: 1px solid rgba(226, 185, 59, 0.4);
    border-radius: 4px;
    font-family: inherit;
    font-size: 11px;
  }

  .row {
    padding: 6px 8px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
  }

  .row.locked {
    opacity: 0.45;
  }

  .head {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 13px;
    color: #ddd;
  }

  .title-tag {
    color: #e2b93b;
  }

  .detail {
    font-size: 11px;
    color: #999;
  }
</style>
