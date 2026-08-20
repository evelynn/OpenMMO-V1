import npcsJson from '../../../../data/npcs.json'
import { getMerchantByNpcName } from './merchantDefs'

/** One row of the NPC registry (data/npcs.json). Trading fields are
 *  optional — an empty/absent wishlist means the NPC does not trade as a
 *  resident (economy phase 3: finite salary-funded wallet, buys only its
 *  wishlist, sells from its real inventory). */
export interface NpcDefinition {
  id: string
  npcName: string
  /** Semicolon-separated item def ids. */
  wishlist?: string
  wishlistRatePercent?: number
  salaryPerDay?: number
  walletCap?: number
  /** Copper per contracted hour; absent/0 = not for hire (IMP-4.5). */
  hireRatePerHour?: number
}

const npcDefs = npcsJson as Record<string, NpcDefinition>

const byNpcName = new Map(
  Object.values(npcDefs).map((def) => [def.npcName, def])
)

export function getNpcTraderByNpcName(
  npcName: string
): NpcDefinition | undefined {
  const def = byNpcName.get(npcName)
  return def?.wishlist ? def : undefined
}

/** What interactions an NPC supports, derived from the same game data the
 *  server reads (doc/ECONOMY.md "거래 진입 UI"): all LLM NPCs can talk,
 *  merchants and resident traders can trade. */
export interface NpcCapabilities {
  talk: boolean
  trade: boolean
  /** Copper per hour this NPC charges for a companion contract; 0 = never. */
  hireRatePerHour: number
  /** Trade is the click default for merchants; talk for everyone else. */
  defaultAction: 'talk' | 'trade'
  /** Portrait/def id for UI assets, when the NPC trades. */
  traderId?: string
}

export function getNpcCapabilities(npcName: string): NpcCapabilities {
  const merchant = getMerchantByNpcName(npcName)
  const trader = getNpcTraderByNpcName(npcName)
  return {
    talk: true,
    trade: Boolean(merchant || trader),
    hireRatePerHour: byNpcName.get(npcName)?.hireRatePerHour ?? 0,
    defaultAction: merchant ? 'trade' : 'talk',
    traderId: merchant?.id ?? trader?.id,
  }
}

export default npcDefs
