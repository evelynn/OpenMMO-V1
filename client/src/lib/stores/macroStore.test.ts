import { describe, expect, it, beforeEach, vi } from 'vitest'
import { get } from 'svelte/store'
import {
  MACRO_COUNT,
  assignMacro,
  clearMacro,
  isSayable,
  loadMacros,
  macroSlotForKey,
  macros,
  resetMacros,
  runMacro,
  type MacroAction,
} from './macroStore'
import { inventoryVisible } from './debugStore'

// vitest runs in node here, so the store's only browser dependency gets a
// stand-in rather than a jsdom environment for the whole suite.
const kept = new Map<string, string>()
globalThis.localStorage = {
  getItem: (k: string) => kept.get(k) ?? null,
  setItem: (k: string, v: string) => void kept.set(k, v),
  removeItem: (k: string) => void kept.delete(k),
  clear: () => kept.clear(),
  key: (i: number) => [...kept.keys()][i] ?? null,
  get length() {
    return kept.size
  },
}

const sent: string[] = []
vi.mock('../network/socket', () => ({
  networkManager: { sendChatMessage: (m: string) => sent.push(m) },
}))

beforeEach(() => {
  sent.length = 0
  localStorage.clear()
  resetMacros()
})

/** The point of IMP-4.6: there is no fourth kind, so no key can be bound to
 *  a combat action. These must stay compile errors — `npm run check` is what
 *  actually enforces it. */
describe('the macro union', () => {
  it('has no case for attacking, casting or using an item', () => {
    const attack = {
      kind: 'attack',
      monsterId: 'm1',
    } as unknown as MacroAction
    // @ts-expect-error a macro is never an attack
    const _attack: MacroAction = { kind: 'attack', monsterId: 'm1' }
    // @ts-expect-error a macro is never a skill cast
    const _skill: MacroAction = { kind: 'use_skill', skill: 'power_strike' }
    // @ts-expect-error a macro is never an item use
    const _item: MacroAction = { kind: 'use_item', instanceId: 3 }
    void _attack
    void _skill
    void _item
    // And one that slips past the type at runtime still does nothing.
    runMacro(attack)
    expect(sent).toEqual([])
  })
})

describe('assignment', () => {
  it('keeps command lines out of a chat macro', () => {
    assignMacro(0, { kind: 'say', message: '/spawnmob goblin' })
    expect(get(macros)[0]).toBeNull()
    expect(isSayable('/anything')).toBe(false)
    expect(isSayable('   ')).toBe(false)
  })

  it('refuses an emote the server would not accept', () => {
    assignMacro(0, { kind: 'emote', emote: 'not_an_emote' })
    expect(get(macros)[0]).toBeNull()
  })

  it('stores and clears a slot', () => {
    assignMacro(1, { kind: 'panel', panel: 'inventory' })
    expect(get(macros)[1]).toEqual({ kind: 'panel', panel: 'inventory' })
    clearMacro(1)
    expect(get(macros)[1]).toBeNull()
  })

  it('ignores an index outside the bar', () => {
    assignMacro(MACRO_COUNT, { kind: 'emote', emote: 'clap' })
    expect(get(macros).filter(Boolean)).toEqual([])
  })
})

describe('running a slot', () => {
  it('sends an emote and a chat line as ordinary chat', () => {
    runMacro({ kind: 'emote', emote: 'clap' })
    runMacro({ kind: 'say', message: 'well met' })
    expect(sent).toEqual(['/emote clap', 'well met'])
  })

  it('toggles a panel without touching the network', () => {
    inventoryVisible.set(false)
    runMacro({ kind: 'panel', panel: 'inventory' })
    expect(get(inventoryVisible)).toBe(true)
    expect(sent).toEqual([])
    inventoryVisible.set(false)
  })
})

describe('persistence', () => {
  it('survives a reload per character and drops entries that went stale', () => {
    loadMacros(7)
    assignMacro(0, { kind: 'emote', emote: 'clap' })
    resetMacros()
    loadMacros(7)
    expect(get(macros)[0]).toEqual({ kind: 'emote', emote: 'clap' })

    localStorage.setItem(
      'macros:8',
      JSON.stringify([
        { kind: 'emote', emote: 'gone_from_the_server' },
        { kind: 'say', message: '/give sword' },
        { kind: 'panel', panel: 'nowhere' },
      ])
    )
    loadMacros(8)
    expect(get(macros).filter(Boolean)).toEqual([])
  })
})

describe('the ALT row', () => {
  it('runs 1..9 then 0 for the tenth slot', () => {
    expect(macroSlotForKey('1')).toBe(0)
    expect(macroSlotForKey('9')).toBe(8)
    expect(macroSlotForKey('0')).toBe(MACRO_COUNT - 1)
    expect(macroSlotForKey('m')).toBeNull()
  })
})
