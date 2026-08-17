import { writable } from 'svelte/store'

/** One contract as the board offers it (ServerMessage::QuestOffer). */
export interface QuestOffer {
  id: string
  name: string
  monster_id: string
  count: number
  min_level: number
  max_level: number
  reward_xp: number
  reward_zeny: number
  reward_item: string | null
  daily_limit: number
  daily_remaining: number
  /** Kills banked, or null when the contract is not accepted. */
  progress: number | null
}

/** Mirrors the server-side `MAX_ACCEPTED_QUESTS`. */
export const MAX_ACCEPTED_QUESTS = 5

/** The board the player last opened. */
export const questBoard = writable<{ boardId: string; quests: QuestOffer[] }>({
  boardId: '',
  quests: [],
})

/** Accepted contracts, keyed by quest id — what the HUD tracker renders.
 *  Progress arrives as single-row pushes, so this is the only place the two
 *  views agree. */
export const acceptedQuests = writable<
  Map<string, { name: string; progress: number; count: number }>
>(new Map())

export const questBoardVisible = writable(false)
export const questBoardLoading = writable(false)

export function applyQuestProgress(
  questId: string,
  progress: number,
  count: number
) {
  acceptedQuests.update((map) => {
    const entry = map.get(questId)
    map.set(questId, { name: entry?.name ?? questId, progress, count })
    return new Map(map)
  })
}

export function trackQuest(questId: string, name: string, count: number) {
  acceptedQuests.update((map) => {
    map.set(questId, { name, progress: 0, count })
    return new Map(map)
  })
}

export function untrackQuest(questId: string) {
  acceptedQuests.update((map) => {
    map.delete(questId)
    return new Map(map)
  })
}

export function resetQuestStores() {
  questBoard.set({ boardId: '', quests: [] })
  acceptedQuests.set(new Map())
  questBoardVisible.set(false)
  questBoardLoading.set(false)
}
