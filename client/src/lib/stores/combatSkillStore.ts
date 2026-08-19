import { writable, derived } from 'svelte/store'
import type { CombatSkillId } from '../network/networkTypes'

/** Job progress: XP toward the next skill point, and the points not yet
 *  spent. Both are the server's numbers — the client never advances them. */
export const jobProgress = writable<{ jobXp: number; skillPoints: number }>({
  jobXp: 0,
  skillPoints: 0,
})

/** Absolute times (performance-clock ms) each skill comes off cooldown. The
 *  server sends what remains; storing a deadline rather than a countdown
 *  keeps the bar honest without a ticking store. */
export const skillCooldownUntil = writable<Record<string, number>>({})

/** The cast in flight, if any: what the local player is casting and when the
 *  bar should be full. Cleared by the server's result or cancel — never by
 *  the client deciding for itself that the cast is done. */
export const activeCast = writable<{
  skill: CombatSkillId
  startedAt: number
  endsAt: number
} | null>(null)

/** The monster the player is currently fighting, mirrored out of
 *  `CombatController` so the skill bar aims at the same thing a swing does.
 *  A skill still needs a target the server accepts — this only saves the
 *  player from picking one twice. */
export const combatTargetId = writable<string | null>(null)

/** Skills the bar offers, in the order they were learned into the slots. */
export const skillBar = writable<(CombatSkillId | null)[]>([
  'power_strike',
  'cleave',
  'flame_dart',
])

export const hasSkillPoints = derived(
  jobProgress,
  ($job) => $job.skillPoints > 0
)

export function applySkillCooldowns(
  cooldowns: { skill: string; remaining_ms: number }[]
) {
  const now = performance.now()
  const next: Record<string, number> = {}
  for (const entry of cooldowns) next[entry.skill] = now + entry.remaining_ms
  skillCooldownUntil.set(next)
}

export function resetCombatSkills() {
  jobProgress.set({ jobXp: 0, skillPoints: 0 })
  skillCooldownUntil.set({})
  activeCast.set(null)
  combatTargetId.set(null)
}
