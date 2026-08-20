<script lang="ts">
  import {
    questBoard,
    questBoardLoading,
    questBoardVisible,
    acceptedQuests,
    type QuestOffer,
  } from '../stores/questStore'
  import { networkManager } from '../network/socket'
  import { getItemDef } from '../data/itemDefs'
  import { gameStore } from '../stores/gameStore'

  /** The only board that exists until the city service NPC lands (IMP-2.2). */
  const DEFAULT_BOARD = 'capital'

  const visible = $derived($questBoardVisible)
  const quests = $derived($questBoard.quests)
  const level = $derived($gameStore.currentPlayer?.level ?? 1)

  $effect(() => {
    if (!visible) return
    questBoardLoading.set(true)
    networkManager.sendOpenQuestBoard(DEFAULT_BOARD)
  })

  const inBand = (q: QuestOffer) => level >= q.min_level && level <= q.max_level
  const exhausted = (q: QuestOffer) =>
    q.daily_limit > 0 && q.daily_remaining === 0
  const accepted = (q: QuestOffer) => $acceptedQuests.has(q.id)
  const done = (q: QuestOffer) => {
    const entry = $acceptedQuests.get(q.id)
    return !!entry && entry.progress >= entry.count
  }

  function rewardLabel(q: QuestOffer) {
    const parts = [`${q.reward_xp} XP`, `${q.reward_zeny}c`]
    if (q.reward_item) {
      parts.push(getItemDef(q.reward_item)?.name ?? q.reward_item)
    }
    return parts.join(' · ')
  }
</script>

{#if visible}
  <div class="quest-panel" aria-label="Hunting board">
    <div class="panel-header">
      <span class="panel-title">Hunting Board</span>
      <button
        class="close-btn"
        title="Close"
        onclick={() => questBoardVisible.set(false)}>×</button
      >
    </div>

    {#if $questBoardLoading}
      <div class="empty">Loading…</div>
    {:else if quests.length === 0}
      <div class="empty">No contracts posted.</div>
    {:else}
      <div class="quest-rows">
        {#each quests as quest (quest.id)}
          <div class="quest-row" class:dimmed={!inBand(quest)}>
            <div class="quest-head">
              <span class="quest-name">{quest.name}</span>
              <span class="quest-band"
                >Lv {quest.min_level}-{quest.max_level}</span
              >
            </div>
            <div class="quest-target">
              {quest.target} ×{quest.count}
              {#if accepted(quest)}
                <span class="quest-progress"
                  >({$acceptedQuests.get(quest.id)?.progress ??
                    0}/{quest.count})</span
                >
              {/if}
            </div>
            <div class="quest-reward">{rewardLabel(quest)}</div>
            <div class="quest-foot">
              {#if quest.daily_limit > 0}
                <span class="quest-daily"
                  >Daily {quest.daily_remaining}/{quest.daily_limit}</span
                >
              {:else}
                <span class="quest-daily">Repeatable</span>
              {/if}
              <span class="row-actions">
                {#if done(quest)}
                  <button
                    class="row-btn"
                    title="Turn in — rewards arrive by mail"
                    onclick={() => networkManager.sendTurnInQuest(quest.id)}
                    >Turn in</button
                  >
                {:else if accepted(quest)}
                  <button
                    class="row-btn danger"
                    title="Abandon; banked kills are lost"
                    onclick={() => networkManager.sendAbandonQuest(quest.id)}
                    >Abandon</button
                  >
                {:else}
                  <button
                    class="row-btn"
                    disabled={!inBand(quest) || exhausted(quest)}
                    title={exhausted(quest)
                      ? 'Done for today'
                      : !inBand(quest)
                        ? 'Outside your level band'
                        : 'Accept'}
                    onclick={() => networkManager.sendAcceptQuest(quest.id)}
                    >Accept</button
                  >
                {/if}
              </span>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .quest-panel {
    position: absolute;
    top: 60px;
    right: 16px;
    width: 340px;
    max-height: 62vh;
    display: flex;
    flex-direction: column;
    background: rgba(20, 18, 14, 0.94);
    border: 1px solid #6b5a3a;
    border-radius: 6px;
    color: #e8e0cc;
    font-size: 13px;
    z-index: 45;
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-bottom: 1px solid #4a3f2a;
  }

  .panel-title {
    font-weight: 600;
    flex: 1;
  }

  .close-btn,
  .row-btn {
    background: none;
    border: 1px solid #6b5a3a;
    border-radius: 3px;
    color: #e8e0cc;
    cursor: pointer;
    padding: 1px 6px;
    font-size: 12px;
  }

  .close-btn:hover,
  .row-btn:hover:not(:disabled) {
    background: #3a3122;
  }

  .row-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .row-btn.danger:hover {
    background: #5a2222;
  }

  .empty {
    padding: 14px 10px;
    text-align: center;
    color: #a89878;
  }

  .quest-rows {
    overflow-y: auto;
  }

  .quest-row {
    padding: 6px 8px;
    border-bottom: 1px solid #332c1e;
  }

  .quest-row.dimmed {
    opacity: 0.55;
  }

  .quest-head {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .quest-name {
    font-weight: 600;
  }

  .quest-band,
  .quest-daily {
    color: #a89878;
    font-size: 11px;
  }

  .quest-target {
    color: #c9bfa5;
    margin-top: 2px;
  }

  .quest-progress {
    color: #d8c98a;
  }

  .quest-reward {
    margin-top: 2px;
    color: #d8c98a;
    font-size: 12px;
  }

  .quest-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 5px;
  }

  .row-actions {
    display: flex;
    gap: 4px;
  }
</style>
