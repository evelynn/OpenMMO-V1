<script lang="ts">
  import { achievementToasts } from '../stores/achievementStore'

  const SHOW_MS = 6000

  // Each toast clears itself; the queue is a plain list so several unlocking
  // together stack instead of overwriting one another.
  $effect(() => {
    if ($achievementToasts.length === 0) return
    const timer = setTimeout(
      () => achievementToasts.update((list) => list.slice(1)),
      SHOW_MS
    )
    return () => clearTimeout(timer)
  })
</script>

{#if $achievementToasts.length > 0}
  <div class="achievement-toast" role="status">
    {#each $achievementToasts.slice(0, 3) as def (def.id)}
      <div class="entry">
        <span class="banner">Achievement</span>
        <span class="name">{def.name}</span>
        <span class="detail">{def.description}</span>
        {#if def.titleId}
          <span class="title">Title unlocked: &lt;{def.titleId}&gt;</span>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .achievement-toast {
    position: fixed;
    top: 18%;
    left: 50%;
    transform: translateX(-50%);
    z-index: 1200;
    display: flex;
    flex-direction: column;
    gap: 6px;
    pointer-events: none;
  }

  .entry {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 240px;
    padding: 8px 14px;
    border: 1px solid rgba(226, 185, 59, 0.6);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.82);
    text-align: center;
    font-family: 'Courier New', monospace;
  }

  .banner {
    color: #e2b93b;
    font-size: 10px;
    letter-spacing: 2px;
    text-transform: uppercase;
  }

  .name {
    color: #fff;
    font-size: 15px;
    font-weight: bold;
  }

  .detail {
    color: #bbb;
    font-size: 11px;
  }

  .title {
    color: #e2b93b;
    font-size: 11px;
  }
</style>
