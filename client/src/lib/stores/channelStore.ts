import { writable } from 'svelte/store'

/** Mirrors `shared/src/channel.rs`. */
export const CHANNEL_CAPACITY = 1000

export interface ChannelOccupancy {
  channel: number
  players: number
}

/** Which channel we are on, from `ChannelState`. Null until the server says. */
export const currentChannel = writable<number | null>(null)

/** Every channel and how full it is, refreshed on entry and each switch. */
export const channelOccupancy = writable<ChannelOccupancy[]>([])

export const resetChannels = () => {
  currentChannel.set(null)
  channelOccupancy.set([])
}
