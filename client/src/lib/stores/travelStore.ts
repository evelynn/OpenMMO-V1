import { writable } from 'svelte/store'

/** One destination as the server offers it. `affordable` already folds in the
 *  level gate and the purse, so the panel never has to guess what the server
 *  would refuse. */
export interface TravelOffer {
  id: string
  name: string
  fare: number
  min_level: number
  affordable: boolean
}

/** Filled by TravelDestinations; empty while the panel is closed. */
export const travelOffers = writable<TravelOffer[]>([])
export const travelPanelVisible = writable(false)
/** The NPC arranging the trip, so choosing a destination can name them. */
export const travelAgentId = writable<number | null>(null)

export function showTravelOffers(offers: TravelOffer[]) {
  travelOffers.set(offers)
  travelPanelVisible.set(true)
}

export function closeTravel() {
  travelPanelVisible.set(false)
  travelOffers.set([])
  travelAgentId.set(null)
}
