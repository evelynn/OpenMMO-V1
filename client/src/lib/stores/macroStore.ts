import { writable } from 'svelte/store'
import {
  characterPanelVisible,
  inventoryVisible,
  worldMapVisible,
} from './debugStore'
import { friendPanelVisible } from './friendStore'
import { mailPanelVisible } from './mailStore'
import { questBoardVisible } from './questStore'
import { SLASH_EMOTE_ANIMS } from './emoteStore'
import { networkManager } from '../network/socket'

export const MACRO_COUNT = 10

/** Panels a macro may open. Deliberately the panels only — nothing here
 *  performs an action, it just puts a window on screen. */
export const MACRO_PANELS = {
  inventory: inventoryVisible,
  character: characterPanelVisible,
  friends: friendPanelVisible,
  mail: mailPanelVisible,
  questBoard: questBoardVisible,
  worldMap: worldMapVisible,
} as const

export type MacroPanelId = keyof typeof MACRO_PANELS

/**
 * Everything a macro can do, and there is no fourth case: an emote, a line of
 * chat, or opening a panel. Attacking, casting and using items are absent by
 * construction, so no key can be bound to a combat action — the answer to
 * "people playing like bots" is the shape of this type, not a rule
 * (12_UX_SERVICES §3, IMP-4.6). Automating combat is what `agent-client` is
 * for, and that is the honest route.
 */
export type MacroAction =
  | { kind: 'emote'; emote: string }
  | { kind: 'say'; message: string }
  | { kind: 'panel'; panel: MacroPanelId }

/** Whether the macro bar's editor is on screen. */
export const macroPanelVisible = writable(false)

export const macros = writable<(MacroAction | null)[]>(
  new Array(MACRO_COUNT).fill(null)
)

/** localStorage key for the active character; null until a character loads. */
let storageKey: string | null = null

function persist(slots: (MacroAction | null)[]) {
  if (!storageKey) return
  try {
    localStorage.setItem(storageKey, JSON.stringify(slots))
  } catch {
    /* storage full or unavailable — macros just won't persist */
  }
}

/** A stored slot only survives if it still names something real: an emote the
 *  server accepts, a chat line that is not a command, or a known panel. */
function reviveMacro(raw: unknown): MacroAction | null {
  if (!raw || typeof raw !== 'object') return null
  const entry = raw as Record<string, unknown>
  if (entry.kind === 'emote' && typeof entry.emote === 'string') {
    return SLASH_EMOTE_ANIMS.has(entry.emote)
      ? { kind: 'emote', emote: entry.emote }
      : null
  }
  if (entry.kind === 'say' && typeof entry.message === 'string') {
    return isSayable(entry.message)
      ? { kind: 'say', message: entry.message }
      : null
  }
  if (entry.kind === 'panel' && typeof entry.panel === 'string') {
    return entry.panel in MACRO_PANELS
      ? { kind: 'panel', panel: entry.panel as MacroPanelId }
      : null
  }
  return null
}

/** A macro line is chat, not a command line: `/` would smuggle every slash
 *  command back into the macro bar, which is the one thing it must not hold. */
export function isSayable(message: string): boolean {
  const trimmed = message.trim()
  return trimmed.length > 0 && !trimmed.startsWith('/')
}

export function loadMacros(characterId: number) {
  storageKey = `macros:${characterId}`
  const next: (MacroAction | null)[] = new Array(MACRO_COUNT).fill(null)
  try {
    const raw = localStorage.getItem(storageKey)
    const parsed = raw ? JSON.parse(raw) : null
    if (Array.isArray(parsed)) {
      for (let i = 0; i < MACRO_COUNT; i++) next[i] = reviveMacro(parsed[i])
    }
  } catch {
    /* corrupt entry — fall back to empty */
  }
  macros.set(next)
}

export function assignMacro(index: number, action: MacroAction) {
  if (index < 0 || index >= MACRO_COUNT) return
  if (action.kind === 'say' && !isSayable(action.message)) return
  if (action.kind === 'emote' && !SLASH_EMOTE_ANIMS.has(action.emote)) return
  macros.update((slots) => {
    const next = [...slots]
    next[index] = action
    persist(next)
    return next
  })
}

export function clearMacro(index: number) {
  if (index < 0 || index >= MACRO_COUNT) return
  macros.update((slots) => {
    const next = [...slots]
    next[index] = null
    persist(next)
    return next
  })
}

export function resetMacros() {
  storageKey = null
  macros.set(new Array(MACRO_COUNT).fill(null))
}

/** Which slot an ALT+digit press means: 1..9 then 0 for the tenth. Returns
 *  null for every other key. */
export function macroSlotForKey(key: string): number | null {
  if (key.length !== 1 || key < '0' || key > '9') return null
  const digit = Number(key)
  return digit === 0 ? MACRO_COUNT - 1 : digit - 1
}

/** Run one slot. Nothing here reaches the combat path: the emote and the
 *  chat line go out as ordinary chat, and the panel is a local store. */
export function runMacro(action: MacroAction | null) {
  if (!action) return
  switch (action.kind) {
    case 'emote':
      networkManager.sendChatMessage(`/emote ${action.emote}`)
      break
    case 'say':
      if (isSayable(action.message))
        networkManager.sendChatMessage(action.message)
      break
    case 'panel':
      MACRO_PANELS[action.panel].update((v) => !v)
      break
  }
}
