<script lang="ts">
  import {
    STORAGE_SLOTS,
    storagePanelVisible,
    storageSlots,
  } from '../stores/storageStore'
  import { inventoryStore } from '../stores/inventoryStore'
  import { networkManager } from '../network/socket'
  import { getItemDef } from '../data/itemDefs'
  import type { ItemInstance } from '../network/networkTypes'

  const visible = $derived($storagePanelVisible)
  const slots = $derived($storageSlots)
  const bag = $derived($inventoryStore.bag)
  const used = $derived(slots.filter(Boolean).length)

  const label = (item: ItemInstance) => {
    const name = getItemDef(item.item_def_id)?.name ?? item.item_def_id
    const enchant = item.enchant > 0 ? `+${item.enchant} ` : ''
    return item.quantity > 1
      ? `${enchant}${name} ×${item.quantity}`
      : `${enchant}${name}`
  }

  function close() {
    networkManager.sendCloseStorage()
    storagePanelVisible.set(false)
  }
</script>

{#if visible}
  <div class="storage-panel" aria-label="Storage">
    <div class="panel-header">
      <span class="panel-title">Storage</span>
      <span class="slot-count">{used}/{STORAGE_SLOTS}</span>
      <button class="close-btn" title="Close" onclick={close}>×</button>
    </div>

    <div class="columns">
      <div class="column">
        <div class="column-title">Stored — no weight limit</div>
        {#if used === 0}
          <div class="empty">Nothing stored.</div>
        {:else}
          {#each slots as item, index (index)}
            {#if item}
              <button
                class="row"
                title="Take one out"
                onclick={() => networkManager.sendStorageWithdraw(index, 1)}
              >
                <span class="slot-index">{index}</span>
                <span class="row-name">{label(item)}</span>
              </button>
            {/if}
          {/each}
        {/if}
      </div>

      <div class="column">
        <div class="column-title">Carried</div>
        {#if bag.length === 0}
          <div class="empty">Your bag is empty.</div>
        {:else}
          {#each bag as item (item.instance_id)}
            <button
              class="row"
              title="Put one in"
              onclick={() =>
                networkManager.sendStorageDeposit(item.instance_id, 1)}
            >
              <span class="row-name">{label(item)}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .storage-panel {
    position: absolute;
    top: 60px;
    right: 16px;
    width: 380px;
    max-height: 60vh;
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

  .slot-count {
    opacity: 0.7;
  }

  .close-btn {
    background: none;
    border: none;
    color: #e8e0cc;
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
  }

  .columns {
    display: flex;
    gap: 8px;
    padding: 8px;
    overflow-y: auto;
  }

  .column {
    flex: 1;
    min-width: 0;
  }

  .column-title {
    opacity: 0.7;
    margin-bottom: 4px;
  }

  .empty {
    opacity: 0.55;
    padding: 8px 0;
  }

  .row {
    display: flex;
    gap: 6px;
    width: 100%;
    text-align: left;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid #4a3f2a;
    border-radius: 4px;
    color: inherit;
    font: inherit;
    padding: 4px 6px;
    margin-bottom: 3px;
    cursor: pointer;
  }

  .row:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .slot-index {
    opacity: 0.5;
    min-width: 24px;
  }

  .row-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
