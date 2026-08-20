import { writable } from 'svelte/store'
import type { SkillId, Skills } from '../network/networkTypes'

export type { SkillId, Skills }

/** Player-facing skill names (mirrors shared `SkillId::display_name`). */
export const SKILL_DISPLAY_NAMES: Record<SkillId, string> = {
  fishing: 'Fishing',
  trading: 'Trading',
  crafting: 'Crafting',
  power_strike: 'Power Strike',
  cleave: 'Cleave',
  flame_dart: 'Flame Dart',
}

/** The local player's trained skills, pushed by the server on join
 *  (`SkillsUpdate`) and advanced by `SkillXpGained`. Empty map until the
 *  first skill is trained — panels render nothing for an empty map. */
export const skillsStore = writable<Skills>({ map: {} })

/** A combat skill's level after a point was spent. Levels only, no XP:
 *  they are bought, not trained (IMP-3.2). */
export function applySkillLevel(skill: SkillId, level: number) {
  skillsStore.update((skills) => ({
    map: { ...skills.map, [skill]: { level, xp: skills.map[skill]?.xp ?? 0 } },
  }))
}

export function applySkillXp(
  skill: SkillId,
  totalXp: number,
  newLevel: number
) {
  skillsStore.update((skills) => ({
    map: { ...skills.map, [skill]: { level: newLevel, xp: totalXp } },
  }))
}

export function resetSkillsStore() {
  skillsStore.set({ map: {} })
}
