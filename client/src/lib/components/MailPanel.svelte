<script lang="ts">
  import {
    mailList,
    mailLoading,
    mailPanelVisible,
    MAX_MAILBOX,
    type MailEntry,
  } from '../stores/mailStore'
  import { networkManager } from '../network/socket'
  import { getItemDef } from '../data/itemDefs'

  const visible = $derived($mailPanelVisible)
  const mail = $derived($mailList)

  // The list is pulled, not pushed: opening the panel is what asks for it.
  $effect(() => {
    if (!visible) return
    mailLoading.set(true)
    networkManager.sendOpenMailbox()
  })

  const itemName = (defId: string) => getItemDef(defId)?.name ?? defId

  function attachmentLabel(entry: MailEntry) {
    const parts = entry.items.map((item) =>
      item.quantity > 1
        ? `${itemName(item.item_def_id)} ×${item.quantity}`
        : itemName(item.item_def_id)
    )
    if (entry.gold > 0) parts.unshift(`${entry.gold}c`)
    return parts.join(', ')
  }

  const daysLeft = (entry: MailEntry) =>
    Math.max(0, Math.ceil((entry.expires_at - Date.now() / 1000) / 86400))
</script>

{#if visible}
  <div class="mail-panel" aria-label="Mailbox">
    <div class="panel-header">
      <span class="panel-title">Mailbox</span>
      <span class="mail-count">{mail.length}/{MAX_MAILBOX}</span>
      <button
        class="close-btn"
        title="Close"
        onclick={() => mailPanelVisible.set(false)}>×</button
      >
    </div>

    {#if $mailLoading}
      <div class="empty">Loading…</div>
    {:else if mail.length === 0}
      <div class="empty">
        No mail.<br />
        <span class="hint">Rewards arrive here when your bag is full.</span>
      </div>
    {:else}
      <div class="mail-rows">
        {#each mail as entry (entry.id)}
          <div class="mail-row">
            <div class="mail-head">
              <span class="mail-subject">{entry.subject}</span>
              <span class="mail-sender">{entry.sender}</span>
            </div>
            {#if entry.body}
              <div class="mail-body">{entry.body}</div>
            {/if}
            {#if entry.items.length > 0 || entry.gold > 0}
              <div class="mail-attachments">{attachmentLabel(entry)}</div>
            {/if}
            <div class="mail-foot">
              <span class="mail-expiry">{daysLeft(entry)}d left</span>
              <span class="row-actions">
                <button
                  class="row-btn"
                  title="Claim everything in this letter"
                  onclick={() => networkManager.sendClaimMail(entry.id)}
                  >Claim</button
                >
                <button
                  class="row-btn danger"
                  title="Delete, attachments included"
                  onclick={() => networkManager.sendDeleteMail(entry.id)}
                  >×</button
                >
              </span>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .mail-panel {
    position: absolute;
    top: 60px;
    right: 16px;
    width: 320px;
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

  .mail-count {
    color: #a89878;
    font-size: 11px;
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
  .row-btn:hover {
    background: #3a3122;
  }

  .row-btn.danger:hover {
    background: #5a2222;
  }

  .empty {
    padding: 14px 10px;
    text-align: center;
    color: #a89878;
  }

  .hint {
    font-size: 11px;
    color: #7d7057;
  }

  .mail-rows {
    overflow-y: auto;
  }

  .mail-row {
    padding: 6px 8px;
    border-bottom: 1px solid #332c1e;
  }

  .mail-head {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .mail-subject {
    font-weight: 600;
  }

  .mail-sender {
    color: #a89878;
    font-size: 11px;
  }

  .mail-body {
    color: #c9bfa5;
    margin-top: 2px;
  }

  .mail-attachments {
    margin-top: 3px;
    color: #d8c98a;
    font-size: 12px;
  }

  .mail-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 5px;
  }

  .mail-expiry {
    color: #7d7057;
    font-size: 11px;
  }

  .row-actions {
    display: flex;
    gap: 4px;
  }
</style>
