<script lang="ts">
  import { networkManager } from '../network/socket'
  import { skillsStore } from '../stores/skillsStore'
  import { allSkillDefs } from '../data/skillDefs'
  import { jobProgress } from '../stores/combatSkillStore'
  import { skill_xp_for_level } from '../wasm/onlinerpg_shared'

  const rows = $derived(
    allSkillDefs().map((def) => {
      const level = $skillsStore.map[def.id]?.level ?? 0
      const requiredLevel = def.requiresSkill
        ? ($skillsStore.map[def.requiresSkill]?.level ?? 0)
        : 0
      const locked =
        !!def.requiresSkill && requiredLevel < (def.requiresSkillLevel ?? 1)
      return {
        def,
        level,
        locked,
        maxed: level >= def.maxLevel,
        canLearn:
          !locked && level < def.maxLevel && $jobProgress.skillPoints > 0,
      }
    })
  )

  // Job XP toward the next point, on the same curve the server pays out on.
  const towardNextPoint = $derived.by(() => {
    const xp = $jobProgress.jobXp
    let level = 0
    while (Number(skill_xp_for_level(level + 1)) <= xp) level++
    const start = Number(skill_xp_for_level(level))
    const next = Number(skill_xp_for_level(level + 1))
    return next > start ? ((xp - start) / (next - start)) * 100 : 0
  })
</script>

<div class="skill-tree">
  <div class="job-line">
    <span class="points">{$jobProgress.skillPoints} skill point(s)</span>
    <div
      class="job-track"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(towardNextPoint)}
    >
      <span class="job-fill" style={`width: ${towardNextPoint}%`}></span>
    </div>
  </div>

  {#each rows as row (row.def.id)}
    <div class="skill-row" class:locked={row.locked}>
      <div class="skill-head">
        <span class="skill-name">{row.def.name}</span>
        <span class="skill-level">{row.level} / {row.def.maxLevel}</span>
      </div>
      <div class="skill-detail">
        {row.def.damageDice} · {row.def.range}m · {row.def.costSatiation ?? 0} satiation
        {#if row.locked}
          <br />needs {row.def.requiresSkill} Lv.{row.def.requiresSkillLevel}
        {/if}
      </div>
      <button
        class="learn"
        disabled={!row.canLearn}
        onclick={() => networkManager.sendLearnSkill(row.def.id)}
      >
        {row.maxed ? 'maxed' : 'learn'}
      </button>
    </div>
  {/each}
</div>

<style>
  .skill-tree {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px;
  }

  .job-line {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .points {
    color: #e2b93b;
    font-size: 12px;
    white-space: nowrap;
  }

  .job-track {
    flex: 1;
    height: 6px;
    background: rgba(255, 255, 255, 0.12);
    border-radius: 3px;
    overflow: hidden;
  }

  .job-fill {
    display: block;
    height: 100%;
    background: #e2b93b;
  }

  .skill-row {
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-areas: 'head learn' 'detail learn';
    gap: 2px 8px;
    align-items: center;
    padding: 6px 8px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
  }

  .skill-row.locked {
    opacity: 0.5;
  }

  .skill-head {
    grid-area: head;
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 13px;
    color: #ddd;
  }

  .skill-level {
    color: #e2b93b;
  }

  .skill-detail {
    grid-area: detail;
    font-size: 11px;
    color: #999;
  }

  .learn {
    grid-area: learn;
    padding: 4px 10px;
    border: 1px solid rgba(226, 185, 59, 0.4);
    border-radius: 4px;
    background: rgba(226, 185, 59, 0.15);
    color: #e2b93b;
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
  }

  .learn:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
