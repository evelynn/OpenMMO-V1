import { writable, derived } from 'svelte/store'
import achievementsJson from '../../../../data/achievements.json'

/** One achievement, as authored in `data-src/achievements.csv`. The client
 *  reads the same table the server judges against, so a panel entry and an
 *  unlock can never describe different things. */
export interface AchievementDefinition {
  id: string
  name: string
  description: string
  trigger: string
  triggerArg?: string
  threshold: number
  titleId?: string
  rewardItem?: string
  rewardZeny?: number
}

const defs = achievementsJson as Record<string, AchievementDefinition>

export function allAchievements(): AchievementDefinition[] {
  return Object.values(defs).sort(
    (a, b) =>
      a.trigger.localeCompare(b.trigger) ||
      a.threshold - b.threshold ||
      a.id.localeCompare(b.id)
  )
}

/** Ids the local character has unlocked, pushed on entry and on each unlock. */
export const unlockedAchievements = writable<Set<string>>(new Set())

/** The title showing beside the local player's name; null for none. */
export const activeTitle = writable<string | null>(null)

/** Recently unlocked, for the toast. Cleared by the toast itself. */
export const achievementToasts = writable<AchievementDefinition[]>([])

/** Titles the character may choose from — only those actually earned. */
export const availableTitles = derived(unlockedAchievements, ($unlocked) =>
  allAchievements()
    .filter((a) => a.titleId && $unlocked.has(a.id))
    .map((a) => a.titleId as string)
)

export function applyAchievementList(unlocked: string[], title: string | null) {
  unlockedAchievements.set(new Set(unlocked))
  activeTitle.set(title)
}

export function applyUnlock(achievementId: string) {
  unlockedAchievements.update((set) => new Set(set).add(achievementId))
  const def = defs[achievementId]
  if (def) achievementToasts.update((list) => [...list, def])
}

export function resetAchievements() {
  unlockedAchievements.set(new Set())
  activeTitle.set(null)
  achievementToasts.set([])
}
