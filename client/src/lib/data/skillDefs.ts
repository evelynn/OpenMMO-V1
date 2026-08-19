import skillsJson from '../../../../data/skills.json'
import type { CombatSkillId } from '../network/networkTypes'

/** One combat skill, as authored in `data-src/skills.csv`. The client reads
 *  these to draw the bar and the tree; every one of them is also what the
 *  server judges a use against, so the two can never disagree. */
export interface SkillDefinition {
  id: CombatSkillId
  name: string
  maxLevel: number
  vctMs?: number
  fctMs?: number
  afterCastDelayMs?: number
  cooldownMs?: number
  range: number
  target: string
  costSatiation?: number
  damageDice: string
  damageBonusStat: string
  animClip: string
  requiresSkill?: CombatSkillId
  requiresSkillLevel?: number
}

const skillDefs = skillsJson as Record<string, SkillDefinition>

export function getSkillDef(id: string): SkillDefinition | undefined {
  return skillDefs[id]
}

/** Every combat skill, ordered so a prerequisite always precedes what it
 *  unlocks — the tree panel renders top to bottom in this order. */
export function allSkillDefs(): SkillDefinition[] {
  const defs = Object.values(skillDefs)
  const placed: SkillDefinition[] = []
  const remaining = [...defs]
  while (remaining.length > 0) {
    const next = remaining.findIndex(
      (d) => !d.requiresSkill || placed.some((p) => p.id === d.requiresSkill)
    )
    if (next === -1) {
      placed.push(...remaining)
      break
    }
    placed.push(...remaining.splice(next, 1))
  }
  return placed
}

export default skillDefs
