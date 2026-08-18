<script lang="ts">
  import {
    closeTravel,
    travelAgentId,
    travelOffers,
    travelPanelVisible,
  } from '../stores/travelStore'
  import { networkManager } from '../network/socket'

  const visible = $derived($travelPanelVisible)
  const offers = $derived($travelOffers)

  function go(nodeId: string) {
    const agent = $travelAgentId
    if (agent === null) return
    networkManager.sendRequestTravel(agent, nodeId)
    closeTravel()
  }
</script>

{#if visible}
  <div class="travel-panel" aria-label="Travel">
    <div class="panel-header">
      <span class="panel-title">Travel</span>
      <button class="close-btn" title="Close" onclick={closeTravel}>×</button>
    </div>

    {#if offers.length === 0}
      <div class="empty">Nowhere to go from here.</div>
    {:else}
      {#each offers as offer (offer.id)}
        <button
          class="row"
          disabled={!offer.affordable}
          title={offer.affordable
            ? `Travel to ${offer.name}`
            : `Needs level ${offer.min_level} and ${offer.fare}c`}
          onclick={() => go(offer.id)}
        >
          <span class="row-name">{offer.name}</span>
          <span class="row-fare"
            >{offer.fare > 0 ? `${offer.fare}c` : 'free'}</span
          >
        </button>
      {/each}
      <div class="hint">Dungeons are walked to, never travelled to.</div>
    {/if}
  </div>
{/if}

<style>
  .travel-panel {
    position: absolute;
    top: 60px;
    right: 16px;
    width: 260px;
    max-height: 60vh;
    overflow-y: auto;
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

  .close-btn {
    background: none;
    border: none;
    color: #e8e0cc;
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
  }

  .empty,
  .hint {
    opacity: 0.55;
    padding: 8px;
  }

  .row {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    width: calc(100% - 16px);
    margin: 4px 8px;
    padding: 5px 6px;
    text-align: left;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid #4a3f2a;
    border-radius: 4px;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .row:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
  }

  .row:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .row-fare {
    opacity: 0.75;
  }
</style>
