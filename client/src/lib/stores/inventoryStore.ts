import { derived, writable } from 'svelte/store'
import type {
  CharacterAttributes,
  EquipSlot,
  ItemInstance,
  PlayerInventory,
} from '../network/networkTypes'

export type { EquipSlot, ItemInstance, PlayerInventory }

const initialState: PlayerInventory = {
  bag: [],
  equipped: {},
}

export const inventoryStore = writable<PlayerInventory>({ ...initialState })

/** The local player's gold in the smallest currency unit (copper). */
export const playerGold = writable(0)

/** The local player's effective guard (base attribute + equipped-gear bonuses),
 *  computed server-side and pushed on join and after each equipment change.
 *  `null` until the first EffectiveStats arrives. */
export const playerGuard = writable<number | null>(null)

/** The local player's attributes with equipped bonuses folded in — what the
 *  server actually resolves against, not the base sheet. `null` until the
 *  first EffectiveStats arrives. */
export const effectiveAttributes = writable<CharacterAttributes | null>(null)

/** Carry cap in kg, already scaled by the hunger band. Never recomputed
 *  client-side: `str * 15` alone ignores the hunger and debuff multipliers
 *  the server enforces. `null` until the first EffectiveStats arrives. */
export const maxCarryWeight = writable<number | null>(null)

/** Item defs that act as a carried light source (mirrors shared TORCH_ITEM_IDS). */
const TORCH_ITEM_IDS = ['torch', 'worn_torch']

export function isTorchItemDefId(id: string | null | undefined): boolean {
  return id != null && TORCH_ITEM_IDS.includes(id)
}

/** True when the local player has a torch equipped in the off-hand slot. */
export const localTorchEquipped = derived(inventoryStore, (inv) => {
  const id = inv.equipped.off_hand?.item_def_id
  return isTorchItemDefId(id)
})

export function setInventory(inventory: PlayerInventory) {
  inventoryStore.set(inventory)
}

export function resetInventoryStore() {
  inventoryStore.set({ bag: [], equipped: {} })
  playerGold.set(0)
  playerGuard.set(null)
  effectiveAttributes.set(null)
  maxCarryWeight.set(null)
}
