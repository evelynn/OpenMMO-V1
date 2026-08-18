import { writable } from 'svelte/store'
import type { ItemInstance } from '../network/networkTypes'

/** Mirrors the server's `STORAGE_SLOTS`; shown, not enforced, here. */
export const STORAGE_SLOTS = 120

/** The container's slots, sparse by index. Filled by the one `StorageOpened`
 *  snapshot and then patched slot by slot — the server never re-sends the
 *  whole thing (SPK-2). Empty while closed. */
export const storageSlots = writable<(ItemInstance | null)[]>([])

export const storagePanelVisible = writable(false)

export function openStorage(slots: (ItemInstance | null)[]) {
  storageSlots.set(slots)
  storagePanelVisible.set(true)
}

export function applyStorageSlot(index: number, item: ItemInstance | null) {
  storageSlots.update((slots) => {
    const next = slots.slice()
    next[index] = item
    return next
  })
}

export function closeStorage() {
  storagePanelVisible.set(false)
  storageSlots.set([])
}
