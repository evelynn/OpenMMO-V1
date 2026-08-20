import { writable, derived } from 'svelte/store'

/** Mirrors shared `guild::perms`. */
export const GUILD_PERMS = {
  INVITE: 1 << 0,
  KICK: 1 << 1,
  STORAGE_DEPOSIT: 1 << 2,
  STORAGE_WITHDRAW: 1 << 3,
  HOUSE_EDIT: 1 << 4,
} as const

export const LEADER_RANK = 0

export interface GuildMemberInfo {
  character_id: number
  name: string
  rank_id: number
  online: boolean
}

export interface GuildRankInfo {
  rank_id: number
  name: string
  perm_bits: number
}

export interface GuildState {
  guild_id: number
  name: string
  leader_character_id: number
  your_rank_id: number
  members: GuildMemberInfo[]
  ranks: GuildRankInfo[]
}

/** The local player's guild, or null when they are in none. The server sends
 *  it whole on every change, so nothing here reconstructs state. */
export const guild = writable<GuildState | null>(null)

/** A pending invite, kept until answered. */
export const guildInvite = writable<{
  guildId: number
  guildName: string
  from: string
} | null>(null)

/** What the viewer's own rank permits, for greying out controls. The server
 *  re-checks every one of these — this only avoids offering what will fail. */
export const myGuildPerms = derived(guild, ($guild) => {
  if (!$guild) return 0
  if ($guild.your_rank_id === LEADER_RANK) return 0xff
  return (
    $guild.ranks.find((r) => r.rank_id === $guild.your_rank_id)?.perm_bits ?? 0
  )
})

export const isGuildLeader = derived(
  guild,
  ($guild) => $guild?.your_rank_id === LEADER_RANK
)

export function resetGuild() {
  guild.set(null)
  guildInvite.set(null)
}
