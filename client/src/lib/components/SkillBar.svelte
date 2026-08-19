<script lang="ts">
  import { networkManager } from '../network/socket'
  import { skillsStore } from '../stores/skillsStore'
  import { getSkillDef } from '../data/skillDefs'
  import {
    skillBar,
    skillCooldownUntil,
    activeCast,
    combatTargetId,
  } from '../stores/combatSkillStore'

  // A cooldown ring needs a clock, but nothing else on screen does — one
  // rAF loop while something is actually cooling, none while idle.
  let now = $state(performance.now())
  $effect(() => {
    const cooling = Object.values($skillCooldownUntil).some((t) => t > now)
    if (!cooling && !$activeCast) return
    let frame = requestAnimationFrame(function tick() {
      now = performance.now()
      frame = requestAnimationFrame(tick)
    })
    return () => cancelAnimationFrame(frame)
  })

  const slots = $derived(
    $skillBar.map((id) => {
      if (!id) return null
      const def = getSkillDef(id)
      if (!def) return null
      const level = $skillsStore.map[id]?.level ?? 0
      const until = $skillCooldownUntil[id] ?? 0
      const remaining = Math.max(0, until - now)
      const total = def.cooldownMs ?? 0
      return {
        id,
        def,
        level,
        remaining,
        fraction: total > 0 ? Math.min(1, remaining / total) : 0,
      }
    })
  )

  const castFraction = $derived.by(() => {
    const cast = $activeCast
    if (!cast) return 0
    const span = cast.endsAt - cast.startedAt
    if (span <= 0) return 1
    return Math.min(1, Math.max(0, (now - cast.startedAt) / span))
  })

  function use(index: number) {
    const slot = slots[index]
    if (!slot || slot.level === 0) return
    // The selected monster is the target; the server refuses anything else,
    // including nothing at all.
    networkManager.sendUseSkill(slot.id, $combatTargetId)
  }
</script>

{#if $activeCast}
  <div class="cast-bar" role="progressbar" aria-valuenow={castFraction * 100}>
    <div class="cast-fill" style="width: {castFraction * 100}%"></div>
    <span class="cast-label"
      >{getSkillDef($activeCast.skill)?.name ?? $activeCast.skill}</span
    >
  </div>
{/if}

<div class="skill-bar">
  {#each slots as slot, index (index)}
    <button
      class="skill-slot"
      class:locked={!slot || slot.level === 0}
      disabled={!slot || slot.level === 0}
      onclick={() => use(index)}
      title={slot
        ? `${slot.def.name} Lv.${slot.level} — ${slot.def.damageDice}, ${slot.def.range}m`
        : ''}
    >
      {#if slot}
        <span class="skill-name">{slot.def.name}</span>
        <span class="skill-level">{slot.level > 0 ? slot.level : '—'}</span>
        {#if slot.remaining > 0}
          <div class="cooldown" style="height: {slot.fraction * 100}%"></div>
          <span class="cooldown-text">{Math.ceil(slot.remaining / 1000)}</span>
        {/if}
      {/if}
    </button>
  {/each}
</div>

<style>
  .skill-bar {
    display: flex;
    gap: 4px;
  }

  .skill-slot {
    position: relative;
    width: 60px;
    height: 48px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    border: 1px solid rgba(226, 185, 59, 0.4);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.6);
    color: #e2b93b;
    font-family: inherit;
    font-size: 10px;
    cursor: pointer;
  }

  .skill-slot:hover:not(:disabled) {
    background: rgba(226, 185, 59, 0.2);
  }

  .skill-slot.locked {
    color: #666;
    border-color: rgba(255, 255, 255, 0.15);
    cursor: default;
  }

  .skill-name {
    line-height: 1.1;
    text-align: center;
  }

  .skill-level {
    font-weight: bold;
  }

  .cooldown {
    position: absolute;
    left: 0;
    bottom: 0;
    width: 100%;
    background: rgba(0, 0, 0, 0.65);
    pointer-events: none;
  }

  .cooldown-text {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
    font-weight: bold;
    color: #fff;
    pointer-events: none;
  }

  .cast-bar {
    position: relative;
    width: 220px;
    height: 14px;
    margin: 0 auto 6px;
    border: 1px solid rgba(226, 185, 59, 0.5);
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.7);
    overflow: hidden;
  }

  .cast-fill {
    height: 100%;
    background: rgba(226, 185, 59, 0.6);
  }

  .cast-label {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    color: #fff;
  }
</style>
