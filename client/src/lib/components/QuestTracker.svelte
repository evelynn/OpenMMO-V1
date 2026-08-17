<script lang="ts">
  import { acceptedQuests } from '../stores/questStore'

  // Always on screen, never an overlay: the tracker is what makes a contract
  // feel live while hunting (IMP-2.5).
  const tracked = $derived([...$acceptedQuests.entries()])
</script>

{#if tracked.length > 0}
  <div class="quest-tracker" aria-label="Active contracts">
    {#each tracked as [id, quest] (id)}
      <div class="tracked" class:done={quest.progress >= quest.count}>
        <span class="tracked-name">{quest.name}</span>
        <span class="tracked-count">{quest.progress}/{quest.count}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .quest-tracker {
    position: absolute;
    top: 120px;
    left: 16px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 5px 8px;
    background: rgba(20, 18, 14, 0.72);
    border-left: 2px solid #6b5a3a;
    border-radius: 0 4px 4px 0;
    color: #e8e0cc;
    font-size: 12px;
    pointer-events: none;
  }

  .tracked {
    display: flex;
    gap: 10px;
    justify-content: space-between;
  }

  .tracked.done .tracked-count {
    color: #8fd08f;
    font-weight: 600;
  }

  .tracked-count {
    color: #d8c98a;
  }
</style>
