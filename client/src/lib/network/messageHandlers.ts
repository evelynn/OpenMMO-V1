import { get } from 'svelte/store'
import {
  gameStore,
  updatePlayer,
  addChatMessage,
  addCombatMessage,
  addChatBubble,
  resetGameStore,
  isAdminUser,
  serverNotice,
} from '../stores/gameStore'
import type { GameState, LocalPlayer, RemotePlayer } from '../stores/gameStore'
import { MathUtils, Vector3 } from 'three'
import { remotePlayerManager } from '../managers/remotePlayerManager'
import { FishingAnimationName } from '../types/animations'
import {
  cancelPendingFishingSounds,
  playFishingSound,
} from '../managers/sfxManager'
import { FISHING_CAST_SWING_DELAY_MS } from '../data/combatTiming'
import { monsterManager } from '../managers/monsterManager'
import { housingManager } from '../managers/housingManager'
import { bridgeManager } from '../managers/bridgeManager'
import { objectManager } from '../managers/objectManager'
import { groundItemManager } from '../managers/groundItemManager'
import { channelOccupancy, currentChannel } from '../stores/channelStore'
import { dungeonManager, setDungeonInstance } from '../managers/dungeonManager'
import type { ItemInstance } from '../network/networkTypes'
import {
  openStorage as openStoragePanel,
  applyStorageSlot,
} from '../stores/storageStore'
import { showTravelOffers, type TravelOffer } from '../stores/travelStore'
import {
  setInventory,
  playerGold,
  playerGuard,
  effectiveAttributes,
  maxCarryWeight,
} from '../stores/inventoryStore'
import { hungerState, grilling, type HungerBand } from '../stores/hungerStore'
import { activeDebuffs, type ActiveDebuff } from '../stores/debuffStore'
import { debuffPresentation } from '../data/debuffPresentation'
import { campfireManager } from '../managers/campfireManager'
import { stallManager } from '../managers/stallManager'
import { tipHatManager } from '../managers/tipHatManager'
import { catchMessage } from './fishingMessages'
import type { SkillId } from '../stores/skillsStore'
import {
  skillsStore,
  applySkillXp,
  SKILL_DISPLAY_NAMES,
} from '../stores/skillsStore'
import {
  myFishing,
  applyFightUpdate,
  upsertBobber,
  markBobberBite,
  updateBobberFight,
  removeBobber,
} from '../stores/fishingStore'
import { getItemDef } from '../data/itemDefs'
import { getMonsterDef } from '../data/monsterDefs'
import { getSkillDef } from '../data/skillDefs'
import {
  activeCast,
  applySkillCooldowns,
  jobProgress,
} from '../stores/combatSkillStore'
import { applySkillLevel } from '../stores/skillsStore'
import {
  activeTitle,
  applyAchievementList,
  applyUnlock,
} from '../stores/achievementStore'
import { guild, guildInvite } from '../stores/guildStore'
import { guildChatEntry } from '../chat-format'
import {
  shopSession,
  applyDealUpdate,
  setMerchantDeals,
  wasShopRequested,
  pendingTradeOffer,
  type BuybackEntry,
} from '../stores/tradeStore'
import {
  partyRoster,
  applyPartyPositions,
  applyPartyVitals,
  resetPartyPositions,
  resetPartyStores,
  pendingPartyInvites,
  pendingPartySummons,
  SUMMON_TTL_MS,
  MAX_PENDING_PARTY_INVITES,
  type PartyMemberEntry,
  type PartyMemberPositionEntry,
  type PartyMemberVitalsEntry,
} from '../stores/partyStore'
import {
  applyFriendList,
  applyFriendsOnline,
  friendList,
  friendOnlineNoticeEnabled,
  pendingFriendRequests,
  resetFriendStores,
  MAX_PENDING_FRIEND_REQUESTS,
} from '../stores/friendStore'
import {
  mailList,
  mailLoading,
  unreadMail,
  resetMailStores,
  type MailEntry,
} from '../stores/mailStore'
import {
  questBoard,
  questBoardLoading,
  acceptedQuests,
  applyQuestProgress,
  trackQuest,
  untrackQuest,
  resetQuestStores,
  type QuestOffer,
} from '../stores/questStore'
import { enqueueConsent } from '../stores/consentQueue'
import { editorTreeDataManager } from '../stores/editorStore'
import { discoveredDungeonIds } from '../stores/dungeonStore'
import type { MonsterData } from '../types/Monster'
import { requestCameraReset } from '../stores/cameraStore'
import { setServerGameTime } from '../stores/timeStore'
import { combatController } from '../managers/combatController'
import {
  startMusicPerformance,
  stopMusicPerformance,
  fadeOutMusicPerformance,
  applyInteractionChange,
} from '../managers/musicPerformance'
import { refreshBardZone } from '../managers/bardZone'
import {
  emoteRequest,
  EMOTE_ANIMS,
  MUSIC_EMOTE_ANIM,
  SLASH_EMOTE_ANIMS,
} from '../stores/emoteStore'
import { whisperChatEntry, partyChatEntry } from '../chat-format'
import { fishing_cast_ms } from '../wasm/onlinerpg_shared'
import type { NetworkEvent } from './networkEvents'
import type {
  AccountCharacter,
  AuthSuccessPayload,
  CharacterAttributes,
  CharacterRollResult,
  ServerGroundItem,
  PositionCorrection,
  ServerMonster,
  ServerPlayer,
} from './networkTypes'

function mapBuyback(
  entries:
    | {
        entry_id: number
        item_def_id: string
        enchant: number
        price: number
      }[]
    | undefined
): BuybackEntry[] {
  return (entries ?? []).map((e) => ({
    entryId: e.entry_id,
    itemDefId: e.item_def_id,
    enchant: e.enchant,
    price: Number(e.price),
  }))
}

function toLocalPlayer(sp: ServerPlayer): LocalPlayer {
  return {
    ...sp,
    position: new Vector3(sp.position.x, sp.position.y, sp.position.z),
    rotation: sp.rotation ?? 0,
    maxHealth: sp.max_health,
    characterClass: sp.class,
    gender: sp.gender,
  }
}

function toRemotePlayer(sp: ServerPlayer): RemotePlayer {
  return {
    id: sp.id,
    name: sp.name,
    level: sp.level,
    health: sp.health,
    maxHealth: sp.max_health,
    characterClass: sp.class,
    gender: sp.gender,
    torchOn: sp.torch_on,
    mainHand: sp.main_hand ?? null,
    costumeHead: sp.cosmetics?.costume_head ?? null,
    title: sp.cosmetics?.title ?? null,
    floorLevel: sp.floor_level ?? 0,
    isOfficialNpc: sp.is_official_npc ?? false,
  }
}

function emitCurrentPlayerDamageInfo(
  playerId: number,
  damage: number,
  hit: boolean,
  currentHealth: number,
  delayMs: number
) {
  const emit = () => {
    const state = get(gameStore)
    if (state.currentPlayer?.id !== playerId) return

    updatePlayer(playerId, {
      lastDamageInfo: {
        damage,
        hit,
        currentHealth,
        trigger: (state.currentPlayer.lastDamageInfo?.trigger ?? 0) + 1,
      },
    })
  }

  if (delayMs > 0) {
    globalThis.setTimeout(emit, delayMs)
  } else {
    emit()
  }
}

/** Resolve object interaction for a remote player: find nearest placement, snap position/rotation. */
async function applyObjectInteraction(
  playerId: number,
  objectType: string,
  wx: number,
  wz: number
) {
  // Pickup and the emotes are animations, not placed objects: they happen
  // wherever the player is standing, so the placement search can only ever
  // find nothing. Skipping them drops two awaits and a scan of every cached
  // region before the clip starts.
  if (objectType === 'pickup' || EMOTE_ANIMS.has(objectType)) {
    remotePlayerManager.handleInteraction(playerId, objectType, 0)
    return
  }

  await objectManager.fetchCatalog()
  const def = objectManager.getCatalogEntry(objectType)
  const anim = def?.interaction ?? objectType
  const offsetY = def?.interactOffset?.y ?? 0
  const placement = await objectManager.findNearestPlacementAsync(
    objectType,
    wx,
    wz
  )
  const pos = placement
    ? { x: placement.x, y: placement.y, z: placement.z }
    : undefined
  // Placements store degrees (the mesh converts on the way in); a player's
  // rotation is radians everywhere else, so a bed at 270° laid the sleeper
  // out crosswise.
  const rot = placement ? MathUtils.degToRad(placement.rotation) : undefined
  remotePlayerManager.handleInteraction(playerId, anim, offsetY, pos, rot)
}

/** Spawn a remote player's visual, apply any object interaction, and store it in game state. */
function addRemotePlayerToState(state: GameState, sp: ServerPlayer) {
  remotePlayerManager.initPlayer(sp.id, sp.position, sp.rotation)
  if (sp.object_type) {
    applyObjectInteraction(sp.id, sp.object_type, sp.position.x, sp.position.z)
  }
  state.otherPlayers.set(sp.id, toRemotePlayer(sp))
  refreshBardZone(state.otherPlayers)
}

/** Remove a remote player's visual and store entry. */
function removeRemotePlayerFromState(state: GameState, playerId: number) {
  remotePlayerManager.removePlayer(playerId)
  state.otherPlayers.delete(playerId)
  refreshBardZone(state.otherPlayers)
  // A leaving player's FishingEnded may never arrive; drop their bobber.
  removeBobber(playerId)
}

export type MessageEvents = {
  authSuccess: NetworkEvent<(payload: AuthSuccessPayload) => void>
  authError: NetworkEvent<(message: string) => void>
  joinSuccess: NetworkEvent<() => void>
  characterCreated: NetworkEvent<(character: AccountCharacter) => void>
  characterStatsRolled: NetworkEvent<(result: CharacterRollResult) => void>
  characterDeleted: NetworkEvent<(characterId: number) => void>
  characterError: NetworkEvent<(message: string) => void>
  kicked: NetworkEvent<(reason: string) => void>
  playerRespawned: NetworkEvent<(playerId: number) => void>
  interactionRejected: NetworkEvent<(reason: string) => void>
  positionCorrected: NetworkEvent<(c: PositionCorrection) => void>
}

function isSelfPlayer(playerId: number): boolean {
  return get(gameStore).currentPlayer?.id === playerId
}

/// Who did it, for a chat line: "You" for us, their name for anyone else.
function actorName(playerId: number): string {
  const state = get(gameStore)
  if (state.currentPlayer?.id === playerId) return 'You'
  return state.otherPlayers.get(playerId)?.name ?? 'Someone'
}

/// One chat line for a ground item changing hands. Silent unless a player
/// did it (actorId set) and the item is known.
function announceGroundItem(
  actorId: number | null | undefined,
  itemDefId: string | undefined,
  verb: string,
  quantity = 1
) {
  if (actorId == null || !itemDefId) return
  const name = getItemDef(itemDefId)?.name ?? itemDefId
  const amount = quantity > 1 ? ` x${quantity}` : ''
  addChatMessage({
    text: `${actorName(actorId)} ${verb} ${name}${amount}.`,
    sender: 'system',
  })
}

export function handleServerMessage(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  raw: any,
  events: MessageEvents,
  disconnect: () => void
) {
  if (typeof raw === 'string') {
    return
  }

  const type = Object.keys(raw)[0]
  const data = raw[type]

  switch (type) {
    case 'AuthSuccess': {
      const characters = (data.characters as AccountCharacter[]) ?? []
      events.authSuccess.emit({
        accountName: data.account_name,
        characters,
      })
      break
    }

    case 'AuthError': {
      console.warn('Authentication error:', data.message)
      events.authError.emit(data.message)
      break
    }

    case 'JoinSuccess': {
      const serverPlayer: ServerPlayer = data.player
      console.log('Join successful, received player data:', serverPlayer)
      isAdminUser.set(data.is_admin === true)
      const player = toLocalPlayer(serverPlayer)
      gameStore.update((state) => ({
        ...state,
        currentPlayer: player,
      }))
      // Players who logged out inside a dungeon reconnect there.
      dungeonManager.syncFromFloorLevel(
        serverPlayer.floor_level ?? 0,
        serverPlayer.position.x,
        serverPlayer.position.z
      )
      events.joinSuccess.emit()
      break
    }

    case 'CharacterCreated': {
      const character: AccountCharacter = data.character
      events.characterCreated.emit(character)
      break
    }

    case 'CharacterStatsRolled': {
      const attributes: CharacterAttributes = data.attributes
      events.characterStatsRolled.emit({
        attributes,
        maxHp: data.max_hp,
      })
      break
    }

    case 'CharacterDeleted': {
      events.characterDeleted.emit(data.character_id)
      break
    }

    case 'CharacterError': {
      events.characterError.emit(data.message)
      break
    }

    case 'PlayerJoined': {
      const serverPlayer: ServerPlayer = data.player
      const player = toLocalPlayer(serverPlayer)
      let joinedName: string | null = null
      gameStore.update((state) => {
        if (!state.currentPlayer) {
          console.log('Setting current player from PlayerJoined:', player)
          return { ...state, currentPlayer: player }
        } else if (serverPlayer.id !== state.currentPlayer.id) {
          addRemotePlayerToState(state, serverPlayer)
          joinedName = serverPlayer.name
        }
        return state
      })
      if (joinedName) {
        addChatMessage({
          text: `${joinedName} joined the game`,
          sender: 'system',
        })
      }
      break
    }

    case 'PlayerAppeared': {
      const serverPlayer: ServerPlayer = data.player
      gameStore.update((state) => {
        if (serverPlayer.id !== state.currentPlayer?.id) {
          addRemotePlayerToState(state, serverPlayer)
        }
        return state
      })
      break
    }

    case 'PlayerLeft': {
      stopMusicPerformance(data.player_id)
      let leftName: string | null = null
      gameStore.update((state) => {
        const player = state.otherPlayers.get(data.player_id)
        removeRemotePlayerFromState(state, data.player_id)
        if (player) {
          leftName = player.name
        }
        return state
      })
      if (leftName) {
        addChatMessage({ text: `${leftName} left the game`, sender: 'system' })
      }
      break
    }

    case 'PlayerDisappeared': {
      // Out of earshot by distance: their tune fades rather than cuts.
      fadeOutMusicPerformance(data.player_id)
      gameStore.update((state) => {
        removeRemotePlayerFromState(state, data.player_id)
        return state
      })
      break
    }

    case 'PlayerMoved': {
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        break
      }
      const deckY = bridgeManager.findDeckYAt(
        data.position.x,
        data.position.z,
        null
      )
      remotePlayerManager.setTargetPosition(
        data.player_id,
        {
          x: data.position.x,
          y: deckY ?? data.position.y,
          z: data.position.z,
        },
        data.rotation,
        data.sprinting === true
      )
      const existing = state.otherPlayers.get(data.player_id)
      if (existing && existing.floorLevel !== data.floor_level) {
        updatePlayer(data.player_id, { floorLevel: data.floor_level })
      }
      break
    }

    case 'PositionCorrected': {
      // No id to match: it only ever goes to the player it corrects.
      events.positionCorrected.emit({
        x: data.position.x,
        y: data.position.y,
        z: data.position.z,
        rotation: data.rotation,
      })
      break
    }

    case 'PlayerTeleported': {
      const state = get(gameStore)
      if (state.currentPlayer && state.currentPlayer.id === data.player_id) {
        // Through the store, not a bare mutation: subscribers that live
        // across a teleport (HUD widgets) otherwise keep the old position.
        gameStore.update((s) => {
          s.currentPlayer?.position.set(
            data.position.x,
            data.position.y,
            data.position.z
          )
          return s
        })
        dungeonManager.syncFromFloorLevel(
          data.floor_level ?? 0,
          data.position.x,
          data.position.z
        )
        requestCameraReset()
        // Any teleport settles the summon toast — an accepted one succeeded,
        // and one surviving the player's own departure would mislead.
        pendingPartySummons.set([])
        break
      }
      const tpDeckY = bridgeManager.findDeckYAt(
        data.position.x,
        data.position.z,
        null
      )
      remotePlayerManager.teleportPlayer(
        data.player_id,
        tpDeckY !== null ? { ...data.position, y: tpDeckY } : data.position,
        data.rotation
      )
      break
    }

    case 'ChatMessage': {
      const state = get(gameStore)
      const isLocal = state.currentPlayer?.id === data.player_id
      const playerName = isLocal
        ? state.currentPlayer?.name
        : (state.otherPlayers.get(data.player_id)?.name ?? 'Unknown')
      addChatMessage({
        text: data.message,
        sender: isLocal ? 'local' : 'remote',
        name: playerName,
      })
      addChatBubble(data.player_id, data.message)
      break
    }

    case 'WhisperMessage': {
      // No chat bubble — a whisper is private.
      const own = get(gameStore).currentPlayer?.name
      addChatMessage(whisperChatEntry(data.from, data.to, data.message, own))
      break
    }

    case 'PartyChatMessage':
      // No chat bubble — the party channel is private to the party.
      addChatMessage(partyChatEntry(data.from, data.message))
      break

    case 'SystemMessage':
      addChatMessage({ text: data.message, sender: 'system' })
      break

    case 'TravelDestinations':
      showTravelOffers(data.nodes as TravelOffer[])
      break

    case 'TravelDenied':
      addChatMessage({ text: data.reason, sender: 'system' })
      break

    case 'StorageOpened':
      openStoragePanel(data.slots as (ItemInstance | null)[])
      break

    case 'StorageSlotChanged':
      applyStorageSlot(
        Number(data.slot_index),
        (data.item ?? null) as ItemInstance | null
      )
      break

    case 'SavePointSet':
      addChatMessage({
        text: 'This is where you will return.',
        sender: 'system',
      })
      break

    case 'PartyInviteReceived':
      enqueueConsent(
        pendingPartyInvites,
        MAX_PENDING_PARTY_INVITES,
        (invite) => invite.inviterId === data.inviter_id,
        {
          inviterId: data.inviter_id,
          inviterName: data.inviter_name,
          offeredAt: Date.now(),
        }
      )
      break

    case 'PartyInviteResult':
      addChatMessage({ text: data.message, sender: 'system' })
      break

    case 'PartySummonReceived': {
      // Replace any same-caster entry (always stale: the ack-only cast never
      // re-sends for a live one) and age out the dead. No cap — distinct
      // casters bound the queue at the party size.
      const now = Date.now()
      pendingPartySummons.update((queue) => [
        ...queue.filter(
          (s) =>
            now - s.offeredAt < SUMMON_TTL_MS && s.casterId !== data.caster_id
        ),
        {
          casterId: data.caster_id,
          casterName: data.caster_name,
          offeredAt: now,
        },
      ])
      break
    }

    case 'PartyState': {
      const members = data.members as PartyMemberEntry[]
      const joined = members.length > 0
      partyRoster.set(joined ? { leaderId: data.leader_id, members } : null)
      if (joined) {
        pendingPartyInvites.set([])
      } else {
        resetPartyPositions()
      }
      // A summons only lives while its caster shares the roster — one from
      // someone who left can only ever be answered with "faded".
      const rosterIds = new Set(members.map((m) => m.id))
      pendingPartySummons.update((queue) =>
        queue.filter((summon) => rosterIds.has(summon.casterId))
      )
      break
    }

    case 'PartyVitals':
      applyPartyVitals(data.members as PartyMemberVitalsEntry[])
      break

    case 'FriendList':
      applyFriendList(
        (
          data.friends as {
            character_id: number
            name: string
            level: number
          }[]
        ).map((f) => ({
          characterId: f.character_id,
          name: f.name,
          level: f.level,
        }))
      )
      break

    case 'FriendsOnline': {
      const announced = applyFriendsOnline(
        data.friends as { character_id: number; level: number }[],
        get(friendList)
      )
      if (get(friendOnlineNoticeEnabled)) {
        for (const name of announced) {
          addChatMessage({
            text: `Friend: ${name} is online.`,
            sender: 'system',
          })
        }
      }
      break
    }

    case 'FriendRequestReceived':
      enqueueConsent(
        pendingFriendRequests,
        MAX_PENDING_FRIEND_REQUESTS,
        (request) => request.requesterId === data.requester_id,
        {
          requesterId: data.requester_id,
          requesterName: data.requester_name,
          offeredAt: Date.now(),
        }
      )
      break

    case 'PartyPositions':
      applyPartyPositions(
        data.members as PartyMemberPositionEntry[],
        get(gameStore).currentPlayer?.id,
        get(partyRoster) !== null
      )
      break

    case 'GameState':
      // A join snapshot starts a fresh session: any party membership died
      // with the old one (in-memory, disconnect = leave), and the server
      // cannot re-send what no longer exists.
      resetPartyStores()
      // Friendships persist, but this session's roster arrives as its own
      // FriendList; anything held from the old one is stale.
      resetFriendStores()
      // Mail is per character; a fresh session refetches the badge from the
      // join snapshot and the list only when the panel is opened.
      resetMailStores()
      resetQuestStores()
      gameStore.update((state) => {
        state.otherPlayers.clear()
        remotePlayerManager.reset()
        // A list, not a map: player ids are numeric and the wasm serializer
        // rejects non-string map keys (see ServerMessage::GameState).
        const serverPlayers = data.players as ServerPlayer[]
        serverPlayers.forEach((serverPlayer) => {
          if (serverPlayer.id !== state.currentPlayer?.id) {
            const player = toRemotePlayer(serverPlayer)
            remotePlayerManager.initPlayer(
              serverPlayer.id,
              serverPlayer.position,
              serverPlayer.rotation
            )
            if (serverPlayer.object_type) {
              applyObjectInteraction(
                serverPlayer.id,
                serverPlayer.object_type,
                serverPlayer.position.x,
                serverPlayer.position.z
              )
            }
            state.otherPlayers.set(serverPlayer.id, player)
          }
        })
        refreshBardZone(state.otherPlayers)
        return state
      })

      monsterManager.reset()
      if (data.monsters) {
        Object.values(data.monsters as Record<string, ServerMonster>).forEach(
          (monster) => {
            monsterManager.spawnWithId(
              monster.id,
              monster.monster_type as MonsterData['type'],
              monster.position,
              monster.owner_id,
              monster.health,
              monster.max_health,
              monster.floor_level,
              monster.aggressive
            )
          }
        )
      }

      groundItemManager.reset()
      if (data.ground_items) {
        ;(data.ground_items as ServerGroundItem[]).forEach((item) => {
          groundItemManager.spawn(item)
        })
      }

      campfireManager.reset()
      if (data.campfires) {
        for (const campfire of data.campfires) campfireManager.spawn(campfire)
      }
      stallManager.reset()
      if (data.stalls) {
        for (const stall of data.stalls) stallManager.spawn(stall)
      }
      tipHatManager.reset()
      if (data.tip_hats) {
        for (const hat of data.tip_hats) tipHatManager.spawn(hat)
      }
      break

    case 'GameTimeSync': {
      setServerGameTime({
        year: data.datetime.year,
        month: data.datetime.month,
        day: data.datetime.day,
        hour: data.datetime.hour,
        minute: data.datetime.minute,
        isNight: data.is_night,
      })
      break
    }

    case 'MonsterSpawned': {
      const monster: ServerMonster = data.monster
      monsterManager.spawnWithId(
        monster.id,
        monster.monster_type as MonsterData['type'],
        monster.position,
        monster.owner_id,
        monster.health,
        monster.max_health,
        monster.floor_level,
        monster.aggressive
      )
      break
    }

    case 'SpawnMonsterRequest': {
      // Server asks us to spawn a monster near the local player; pick a valid
      // grassland spot away from water/towns and request it.
      monsterManager.tryAmbientSpawn(data.monster_type)
      break
    }

    case 'NoSpawnZones':
      monsterManager.setNoSpawnZones(data.zones ?? [])
      break

    case 'MonsterAssigned': {
      const assigned: ServerMonster = data.monster
      // May be a reassignment of a monster we already track (dungeon
      // owner handover): update the owner and (re)create our brain.
      monsterManager.adoptOwnership(
        assigned.id,
        assigned.monster_type as MonsterData['type'],
        assigned.position,
        assigned.owner_id,
        assigned.health,
        assigned.max_health,
        assigned.floor_level,
        assigned.aggressive
      )
      break
    }

    case 'MonsterMoved':
      monsterManager.updateMonsterFromNetwork(
        data.monster_id,
        data.position,
        data.rotation,
        data.state,
        data.target_position
      )
      break

    case 'MonsterRemoved':
      monsterManager.remove(data.monster_id)
      break

    case 'MonsterDead':
      monsterManager.handleMonsterDead(
        data.monster_id,
        data.dropped_weapon_item_def_id
      )
      break

    case 'PlayerAttacked': {
      remotePlayerManager.handleAttack(data.player_id)

      const gameState = get(gameStore)
      const isLocalAttacker = gameState.currentPlayer?.id === data.player_id
      const attackerName = isLocalAttacker
        ? 'You'
        : gameState.otherPlayers.get(data.player_id)?.name || 'Unknown'

      addCombatMessage({
        text: data.hit
          ? `rolled ${data.roll}: HIT for ${data.damage} damage!`
          : `rolled ${data.roll}: MISSED!`,
        sender: isLocalAttacker ? 'local' : 'remote',
        name: attackerName,
        hit: data.hit,
      })

      monsterManager.handleMonsterAttacked(
        data.monster_id,
        data.player_id,
        data.hit,
        data.damage
      )
      break
    }

    case 'PlayerAttackRejected': {
      // The server sees a target we don't: stop the auto-attack loop instead
      // of swinging at it once per cooldown forever.
      if (
        data.reason === 'invalid_target' &&
        combatController.targetMonsterId === data.monster_id
      ) {
        combatController.cancelCombat()
      }
      const reasonText: Record<string, string> = {
        invalid_target: 'target is gone',
        out_of_range: 'too far away',
        attacker_dead: 'you are dead',
      }
      addCombatMessage({
        text: `attack rejected: ${reasonText[data.reason] ?? data.reason}`,
        sender: 'local',
        name: 'You',
        hit: false,
      })
      break
    }

    case 'MonsterProvoked':
      monsterManager.handleMonsterProvoked(data.monster_id, data.player_id)
      break

    case 'MonsterAttackedPlayer': {
      const gameState = get(gameStore)
      const isCurrentPlayer = gameState.currentPlayer?.id === data.player_id
      const monster = monsterManager.monsters.get(data.monster_id)
      if (monster?.ownerId !== gameState.currentPlayer?.id) {
        monsterManager.handleMonsterAttackStarted(data.monster_id, 250)
      }

      if (isCurrentPlayer) {
        emitCurrentPlayerDamageInfo(
          data.player_id,
          data.damage,
          data.hit,
          data.current_health,
          monsterManager.getMonsterAttackDamageTextDelayMs(data.monster_id)
        )
      }

      updatePlayer(data.player_id, {
        health: data.current_health,
      })

      const monsterTargetName = isCurrentPlayer
        ? 'You'
        : (gameState.otherPlayers.get(data.player_id)?.name ?? 'Unknown')
      addCombatMessage({
        text: data.hit
          ? `rolled ${data.roll}: HIT ${monsterTargetName} for ${data.damage} damage!`
          : `rolled ${data.roll}: MISSED!`,
        sender: 'system',
        name: 'Monster',
        hit: data.hit,
      })
      break
    }

    case 'PlayerDead': {
      console.log('Player dead:', data.player_id)
      const gameState = get(gameStore)
      const isDeadCurrentPlayer = gameState.currentPlayer?.id === data.player_id
      const deadPlayerName = isDeadCurrentPlayer
        ? 'You'
        : (gameState.otherPlayers.get(data.player_id)?.name ?? 'Unknown')
      addCombatMessage({
        text: `${deadPlayerName === 'You' ? 'You have' : deadPlayerName + ' has'} been slain!`,
        sender: 'system',
      })

      if (!isDeadCurrentPlayer) {
        remotePlayerManager.handleDead(data.player_id)
      }
      break
    }

    case 'Kicked': {
      console.warn('Kicked from server:', data.reason)
      events.kicked.emit(data.reason)
      resetGameStore()
      monsterManager.reset()
      remotePlayerManager.reset()
      disconnect()
      break
    }

    case 'ServerNotice': {
      serverNotice.set(data.message ?? null)
      break
    }

    case 'PlayerRespawned': {
      const serverPlayer: ServerPlayer = data.player
      console.log('Player respawned:', serverPlayer.id)
      const gameState = get(gameStore)
      const isCurrentPlayerRespawned =
        gameState.currentPlayer?.id === serverPlayer.id

      if (isCurrentPlayerRespawned) {
        const respawnPosition = new Vector3(
          serverPlayer.position.x,
          serverPlayer.position.y,
          serverPlayer.position.z
        )
        updatePlayer(serverPlayer.id, {
          position: respawnPosition,
          health: serverPlayer.health,
          maxHealth: serverPlayer.max_health,
        })
        // Death exits the dungeon: respawn is always on the surface.
        dungeonManager.syncFromFloorLevel(
          serverPlayer.floor_level ?? 0,
          serverPlayer.position.x,
          serverPlayer.position.z
        )
        requestCameraReset()
        addChatMessage({ text: 'You have been revived.', sender: 'system' })
      } else {
        updatePlayer(serverPlayer.id, {
          health: serverPlayer.health,
          maxHealth: serverPlayer.max_health,
        })
        addChatMessage({
          text: `${serverPlayer.name} has been revived.`,
          sender: 'system',
        })
        remotePlayerManager.handleRespawn(
          serverPlayer.id,
          serverPlayer.position,
          serverPlayer.rotation
        )
      }
      events.playerRespawned.emit(serverPlayer.id)
      break
    }

    case 'PlayerHealthUpdate': {
      const gameState = get(gameStore)
      const isCurrentPlayer = gameState.currentPlayer?.id === data.player_id

      let regenInfo = undefined
      if (isCurrentPlayer && gameState.currentPlayer) {
        const diff = data.health - gameState.currentPlayer.health
        if (diff > 0) {
          const prevTrigger =
            gameState.currentPlayer.lastRegenInfo?.trigger ?? 0
          regenInfo = {
            damage: diff,
            hit: true,
            trigger: prevTrigger + 1,
          }
        }
      }

      updatePlayer(data.player_id, {
        health: data.health,
        maxHealth: data.max_health,
        ...(isCurrentPlayer ? { lastRegenInfo: regenInfo } : {}),
      })
      break
    }

    case 'PlayerTorchToggled': {
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        break
      }
      updatePlayer(data.player_id, { torchOn: data.enabled })
      break
    }

    case 'PlayerMainHandChanged': {
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        break
      }
      updatePlayer(data.player_id, { mainHand: data.item_def_id ?? null })
      break
    }

    case 'PlayerCosmeticsChanged': {
      if (get(gameStore).currentPlayer?.id === data.player_id) break
      updatePlayer(data.player_id, {
        costumeHead: data.cosmetics?.costume_head ?? null,
        title: data.cosmetics?.title ?? null,
      })
      break
    }

    case 'PlayerMusicStarted': {
      const isMe = isSelfPlayer(data.player_id)
      startMusicPerformance(data.player_id, data.track, isMe, data.elapsed_secs)
      // Our own /play_music went to the server unresolved; its reply names
      // the track and is what strikes up our emote.
      if (isMe) emoteRequest.set(MUSIC_EMOTE_ANIM)
      const who = isMe
        ? null
        : (get(gameStore).otherPlayers.get(data.player_id)?.name ?? 'Someone')
      addChatMessage({
        text: who
          ? `${who} plays "${data.track}".`
          : `You play "${data.track}".`,
        sender: 'system',
      })
      break
    }

    case 'PlayerInteractionChanged': {
      // Leaving the strum ends the tune, for the performer too.
      applyInteractionChange(data.player_id, data.object_type ?? null)
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        // Our own /emote went to the server unresolved; this broadcast is
        // its reply, the way PlayerMusicStarted starts /play_music.
        if (data.object_type && SLASH_EMOTE_ANIMS.has(data.object_type)) {
          emoteRequest.set(data.object_type)
        }
        break
      }
      const ft: string | null = data.object_type ?? null
      if (ft) {
        const rp = remotePlayerManager.players.get(data.player_id)
        const wx = rp?.position.x ?? 0
        const wz = rp?.position.z ?? 0
        applyObjectInteraction(data.player_id, ft, wx, wz)
      } else {
        remotePlayerManager.handleStopInteraction(data.player_id)
      }
      break
    }

    case 'InteractionRejected': {
      // The event only cancels an in-flight interaction animation, so the
      // refusal would otherwise be silent. Reasons are sentences except the
      // machine codes mapped here (same pattern as PlayerAttackRejected).
      const reasonText: Record<string, string> = {
        occupied: 'Someone is already using it.',
      }
      addChatMessage({
        text: reasonText[data.reason] ?? data.reason,
        sender: 'system',
      })
      events.interactionRejected.emit(data.reason)
      break
    }

    case 'DungeonChestOpened': {
      // No items + no gold = re-open of a chest already claimed tonight;
      // the lid still swings, showing an empty box.
      dungeonManager.markTreasureChestOpened(data.entrance_id)
      const empty = (data.item_def_ids as string[]).length === 0 && !data.gold
      addChatMessage({
        text: empty
          ? 'The treasure chest is empty.'
          : `${actorName(data.player_id)} opened the treasure chest! (+${data.gold} gold)`,
        sender: 'system',
      })
      break
    }

    case 'DungeonPropsState':
      dungeonManager.setPropsState(
        data.entrance_id,
        data.depth,
        data.broken,
        data.opened
      )
      break

    case 'DungeonPropBroken':
      dungeonManager.markPropBroken(data.entrance_id, data.depth, data.prop_id)
      break

    case 'DungeonPropOpened':
      dungeonManager.markPropOpened(data.entrance_id, data.depth, data.prop_id)
      break

    case 'DungeonDoorToggled':
      dungeonManager.applyDoorToggle(
        data.entrance_id,
        data.depth,
        data.door_id,
        data.is_open
      )
      break

    case 'DungeonDoorsState':
      dungeonManager.applyDoorsSnapshot(data.entrance_id, data.doors)
      break

    case 'DungeonDiscoveries':
      discoveredDungeonIds.set(new Set(data.entrance_ids as string[]))
      break

    case 'HouseSpawned':
      housingManager.handleRemoteHouseSpawned(data.house)
      break

    case 'HouseUpdated':
      housingManager.handleRemoteHouseSpawned(data.house)
      break

    case 'TreeTilesInvalidated': {
      const treeDataManager = get(editorTreeDataManager)
      if (treeDataManager) void treeDataManager.refreshTiles(data.tiles ?? [])
      break
    }

    case 'HouseRemoved':
      housingManager.handleRemoteHouseRemoved(data.house_id)
      break

    case 'DoorToggled':
      housingManager.handleDoorToggled(
        data.house_id,
        data.room_index,
        data.wall_dir,
        data.segment_index,
        data.is_open
      )
      break

    case 'InventoryState':
    case 'InventoryUpdated':
      setInventory(data.inventory)
      break

    case 'GroundItemSpawned': {
      const item = data.item as ServerGroundItem
      groundItemManager.spawn(item, { animateSpawn: true })
      // Only what a hand put down: loot announces itself by landing.
      announceGroundItem(
        item.dropped_by,
        item.item_def_id,
        'dropped',
        item.quantity
      )
      break
    }

    case 'GroundItemAppeared':
      groundItemManager.spawn(data.item as ServerGroundItem)
      break

    case 'GroundItemRemoved': {
      // Read the pile before the removal drops it — who looted what matters
      // in a party, where one bag takes the drop everybody fought for.
      const taken =
        data.picked_up_by != null
          ? groundItemManager.items.get(data.instance_id)
          : undefined
      groundItemManager.remove(data.instance_id)
      // Self currency pickups: the server's system line reports the payout.
      const selfCurrency =
        taken != null &&
        getItemDef(taken.itemDefId)?.category === 'currency' &&
        isSelfPlayer(data.picked_up_by)
      if (!selfCurrency) {
        announceGroundItem(
          data.picked_up_by,
          taken?.itemDefId,
          'picked up',
          taken?.quantity
        )
      }
      break
    }

    case 'GroundItemQuantityChanged': {
      const pile = groundItemManager.items.get(data.instance_id)
      groundItemManager.setQuantity(data.instance_id, data.quantity)
      // The picker already got the server's took-X-left-Y system line.
      if (pile && !isSelfPlayer(data.picked_up_by)) {
        announceGroundItem(
          data.picked_up_by,
          pile.itemDefId,
          'picked up',
          pile.quantity - data.quantity
        )
      }
      break
    }

    case 'ShopState': {
      const session = {
        merchantPlayerId: data.merchant_player_id,
        merchantName: data.merchant_name,
        catalog: data.catalog ?? [],
        sellRatePercent: data.sell_rate_percent,
        wishlist: data.wishlist ?? [],
        stock: (data.stock ?? []).map(
          (entry: { item_def_id: string; quantity: number }) => ({
            itemDefId: entry.item_def_id,
            quantity: entry.quantity,
          })
        ),
        buyback: mapBuyback(data.buyback),
      }
      setMerchantDeals(data.merchant_player_id, data.active_deals ?? [])
      // Open directly only when the player asked for this shop (or it's a
      // refresh of the one already on screen). An NPC-pushed open_trade is
      // an *offer*: the window covers much of the screen, so it just shows
      // a small accept/decline toast instead of hijacking the view.
      const current = get(shopSession)
      if (
        wasShopRequested(data.merchant_player_id) ||
        current?.merchantPlayerId === data.merchant_player_id
      ) {
        shopSession.set(session)
      } else {
        pendingTradeOffer.set({ session, offeredAt: Date.now() })
      }
      break
    }

    case 'GoldUpdate':
      playerGold.set(Number(data.gold))
      break

    case 'EffectiveStats':
      playerGuard.set(Number(data.guard))
      effectiveAttributes.set(data.attributes as CharacterAttributes)
      maxCarryWeight.set(Number(data.max_carry_weight))
      break

    case 'GoldGained': {
      const state = get(gameStore)
      const playerId = state.currentPlayer?.id
      if (playerId) {
        updatePlayer(playerId, {
          lastGoldInfo: {
            amount: Number(data.amount),
            trigger: (state.currentPlayer?.lastGoldInfo?.trigger ?? 0) + 1,
          },
        })
      }
      break
    }

    case 'TradeError':
      addChatMessage({ text: data.message, sender: 'system' })
      break

    case 'DealUpdated':
      applyDealUpdate(
        data.merchant_player_id,
        data.item_def_id,
        data.kind,
        data.modifier_pct,
        data.expires_in_secs
      )
      break

    case 'BuybackUpdated':
      shopSession.update((session) =>
        session && session.merchantPlayerId === data.merchant_player_id
          ? { ...session, buyback: mapBuyback(data.buyback) }
          : session
      )
      break

    case 'QuestBoard': {
      questBoard.set({
        boardId: data.board_id,
        quests: data.quests as QuestOffer[],
      })
      questBoardLoading.set(false)
      // The board is the only message carrying names for accepted contracts,
      // so the tracker learns them here.
      acceptedQuests.update((map) => {
        for (const quest of data.quests as QuestOffer[]) {
          if (quest.progress === null) continue
          map.set(quest.id, {
            name: quest.name,
            progress: quest.progress,
            count: quest.count,
          })
        }
        return new Map(map)
      })
      break
    }

    case 'QuestAccepted': {
      const offer = get(questBoard).quests.find((q) => q.id === data.quest_id)
      trackQuest(data.quest_id, offer?.name ?? data.quest_id, data.count)
      addCombatMessage({
        text: `Contract accepted: ${offer?.name ?? data.quest_id}.`,
        sender: 'local',
      })
      break
    }

    case 'QuestProgress': {
      applyQuestProgress(data.quest_id, data.progress, data.count)
      if (data.progress >= data.count) {
        const name =
          get(acceptedQuests).get(data.quest_id)?.name ?? data.quest_id
        addCombatMessage({
          text: `${name} is ready to turn in.`,
          sender: 'local',
        })
      }
      break
    }

    case 'QuestCompleted': {
      const name = get(acceptedQuests).get(data.quest_id)?.name ?? data.quest_id
      untrackQuest(data.quest_id)
      if (data.rewarded) {
        addCombatMessage({
          text: `${name} complete — the reward is in your mailbox.`,
          sender: 'local',
        })
      }
      break
    }

    case 'SkillCastStarted': {
      if (data.player_id === get(gameStore).currentPlayer?.id) {
        activeCast.set({
          skill: data.skill,
          startedAt: performance.now(),
          endsAt: performance.now() + data.cast_ms,
        })
      }
      break
    }

    case 'SkillCastCancelled': {
      if (data.player_id === get(gameStore).currentPlayer?.id) {
        activeCast.set(null)
      }
      break
    }

    case 'SkillResult': {
      const isMine = data.player_id === get(gameStore).currentPlayer?.id
      if (isMine) activeCast.set(null)
      const name = getSkillDef(data.skill)?.name ?? data.skill
      if (isMine) {
        addCombatMessage({
          text: data.hit
            ? `${name} hits for ${data.damage}.`
            : `${name} misses.`,
          sender: 'local',
        })
      }
      break
    }

    case 'SkillRejected': {
      // The server owns every one of these decisions, so it also owns the
      // explanation — the client has nothing of its own to say here.
      activeCast.set(null)
      const name = getSkillDef(data.skill)?.name ?? data.skill
      addCombatMessage({
        text: `${name}: ${String(data.reason).replace(/_/g, ' ')}.`,
        sender: 'local',
      })
      break
    }

    case 'SkillCooldowns':
      applySkillCooldowns(data.cooldowns ?? [])
      break

    case 'SkillPointsUpdate':
      jobProgress.set({
        jobXp: Number(data.job_xp),
        skillPoints: data.skill_points,
      })
      break

    case 'SkillLearned': {
      applySkillLevel(data.skill, data.level)
      jobProgress.update((job) => ({ ...job, skillPoints: data.skill_points }))
      const name = getSkillDef(data.skill)?.name ?? data.skill
      addCombatMessage({
        text: `${name} is now level ${data.level}.`,
        sender: 'local',
      })
      break
    }

    case 'GuildUpdated':
      guild.set(data.guild ?? null)
      break

    case 'DungeonInstance':
      setDungeonInstance(data.entrance_id, data.party_seed)
      break

    case 'ChannelState':
      currentChannel.set(data.yours)
      channelOccupancy.set(data.channels)
      break

    case 'ChannelDenied':
      addChatMessage({
        text: `Channel switch refused: ${data.reason}`,
        sender: 'system',
      })
      break

    case 'CraftResult':
      // The server also sends a system line; this is the store-side hook for
      // anything that wants to react to a craft (IMP-4.4).
      break

    case 'CompanionContract': {
      const npc = get(gameStore).otherPlayers.get(data.npc_player_id)
      const who = npc?.name ?? 'Your companion'
      addChatMessage({
        text:
          data.expires_at === 0
            ? `${who} is no longer under contract.`
            : `${who} is with you until the contract runs out.`,
        sender: 'system',
      })
      break
    }

    case 'GuildInvite':
      guildInvite.set({
        guildId: data.guild_id,
        guildName: data.guild_name,
        from: data.from,
      })
      break

    case 'GuildChatMessage':
      addChatMessage(guildChatEntry(data.sender, data.message))
      break

    case 'GuildDenied':
      addCombatMessage({
        text: `Guild: ${String(data.reason).replace(/_/g, ' ')}.`,
        sender: 'local',
      })
      break

    case 'AchievementList':
      applyAchievementList(data.unlocked ?? [], data.active_title ?? null)
      break

    case 'AchievementUnlocked': {
      applyUnlock(data.achievement_id)
      break
    }

    case 'TitleSet':
      activeTitle.set(data.title ?? null)
      break

    case 'MvpBonus': {
      // Deliberately not "you got the kill": the biggest contributor and the
      // one who landed the last blow can be different people (IMP-2.8).
      const boss = getMonsterDef(data.monster_type)?.name ?? data.monster_type
      const item = data.item_def_id
        ? ` ${getItemDef(data.item_def_id)?.name ?? data.item_def_id} is in your mailbox.`
        : ''
      addCombatMessage({
        text: `MVP of ${boss} — +${data.xp} XP.${item}`,
        sender: 'local',
      })
      break
    }

    case 'MailUnread': {
      unreadMail.set(data.count)
      break
    }

    case 'MailList': {
      mailList.set(data.mail as MailEntry[])
      mailLoading.set(false)
      unreadMail.set(0)
      break
    }

    case 'MailUpdated': {
      const state = data.state as 'Claimed' | 'Deleted' | 'ClaimBlocked'
      if (state === 'ClaimBlocked') {
        addCombatMessage({
          text: 'Your bag cannot hold that letter — claiming is all or nothing.',
          sender: 'local',
        })
        break
      }
      mailList.update((list) =>
        list.filter((entry) => entry.id !== data.mail_id)
      )
      break
    }

    case 'XpGained': {
      const gameState = get(gameStore)
      const previousPlayer = gameState.currentPlayer
      const previousLevel =
        previousPlayer && previousPlayer.id === data.player_id
          ? previousPlayer.level
          : null
      const isCurrentPlayer = previousPlayer?.id === data.player_id
      const newTotalXp = Number(data.total_xp)
      const xpLost = Number(data.xp_lost ?? 0)
      // Concurrent kill shares can leave the server out of XP order, so a late
      // notice may carry an older total. Keep the gain message, but never roll
      // the displayed XP or level backwards on it.
      const isStaleGain =
        xpLost === 0 &&
        isCurrentPlayer &&
        newTotalXp < (previousPlayer?.totalXp ?? 0)

      let regenInfo = undefined
      if (isCurrentPlayer && previousPlayer) {
        const diff = data.current_hp - previousPlayer.health
        if (diff > 0) {
          const prevTrigger = previousPlayer.lastRegenInfo?.trigger ?? 0
          regenInfo = {
            damage: diff,
            hit: true,
            trigger: prevTrigger + 1,
          }
        }
      }

      updatePlayer(data.player_id, {
        ...(isStaleGain ? {} : { level: data.new_level, totalXp: newTotalXp }),
        health: data.current_hp,
        maxHealth: data.max_hp,
        ...(isCurrentPlayer ? { lastRegenInfo: regenInfo } : {}),
      })
      if (data.xp_amount > 0) {
        const multPct = data.xp_mult_pct ?? 100
        const gap =
          multPct === 100
            ? ''
            : multPct < 100
              ? ` (level gap: ${multPct}% of full)`
              : ` (level gap bonus: ${multPct}%)`
        addCombatMessage({
          text: `You gained ${data.xp_amount} XP.${gap}`,
          sender: 'local',
        })
      } else if (previousLevel !== null) {
        if (xpLost > 0) {
          addCombatMessage({
            text: `Death penalty: You lost ${xpLost} XP.`,
            sender: 'local',
          })
        } else {
          addCombatMessage({ text: 'Death penalty applied.', sender: 'local' })
        }
      }
      if (isStaleGain) {
        break
      }
      if (data.leveled_up) {
        addCombatMessage({
          text: `Level up! You are now level ${data.new_level}.`,
          sender: 'local',
        })
      } else if (previousLevel !== null && data.new_level < previousLevel) {
        addCombatMessage({
          text: `Level down. You are now level ${data.new_level}.`,
          sender: 'local',
        })
      }
      break
    }

    case 'SkillsUpdate':
      skillsStore.set(data.skills)
      break

    case 'FishingCasted': {
      // The float spends the swing + flight in the air; it splashes down
      // (and first renders) on the same schedule as the splash sound.
      upsertBobber(
        data.player_id,
        data.position,
        FISHING_CAST_SWING_DELAY_MS + fishing_cast_ms()
      )
      if (isSelfPlayer(data.player_id)) {
        myFishing.set({ phase: 'casting' })
        // Whoosh on the visible swing; splash one flight time (CAST_MS) later.
        playFishingSound('cast', FISHING_CAST_SWING_DELAY_MS)
        playFishingSound(
          'splash',
          FISHING_CAST_SWING_DELAY_MS + fishing_cast_ms()
        )
        addCombatMessage({ text: 'You cast your line.', sender: 'local' })
      } else {
        // Interact state ignores late moves; apply the server-computed facing.
        remotePlayerManager.handleInteraction(
          data.player_id,
          FishingAnimationName.CAST,
          0,
          undefined,
          data.rotation
        )
      }
      break
    }

    case 'FishingBite': {
      markBobberBite(data.player_id)
      if (isSelfPlayer(data.player_id)) {
        myFishing.set({ phase: 'bite' })
        playFishingSound('plop')
        addCombatMessage({
          text: 'Something bites! Hook it!',
          sender: 'local',
        })
      }
      break
    }

    case 'FishingFight': {
      updateBobberFight(
        data.player_id,
        data.bobber,
        data.fish_state,
        data.stamina_pct
      )
      if (isSelfPlayer(data.player_id)) {
        applyFightUpdate(data.fish_state, data.tension_pct, data.stamina_pct)
      }
      break
    }

    case 'FishingEnded': {
      removeBobber(data.player_id)
      const isSelf = isSelfPlayer(data.player_id)
      if (!isSelf) remotePlayerManager.handleStopInteraction(data.player_id)
      // Bystander celebration: everyone in radius hears about a trophy.
      if (!isSelf && data.outcome?.Caught?.trophy) {
        const { item_def_id, size_cm } = data.outcome.Caught
        const who = actorName(data.player_id)
        const fishName = getItemDef(item_def_id)?.name ?? item_def_id
        addCombatMessage({
          text: `${who} landed a trophy ${fishName} — ${size_cm} cm!`,
          sender: 'local',
        })
      }
      if (isSelf) {
        myFishing.set({ phase: 'idle' })
        cancelPendingFishingSounds()
        const outcome = data.outcome
        if (outcome === 'Escaped') {
          playFishingSound('snap')
          addCombatMessage({ text: 'The fish got away.', sender: 'local' })
        } else if (outcome === 'Aborted') {
          addCombatMessage({ text: 'You reel in your line.', sender: 'local' })
        } else if (outcome?.Caught) {
          playFishingSound('catch')
          const { item_def_id, size_cm, trophy } = outcome.Caught
          addCombatMessage({
            text: catchMessage(
              getItemDef(item_def_id),
              item_def_id,
              size_cm,
              trophy
            ),
            sender: 'local',
          })
        }
      }
      break
    }

    case 'FishingError':
      addCombatMessage({ text: data.message, sender: 'local' })
      break

    case 'SkillXpGained': {
      const skillId = data.skill as SkillId
      applySkillXp(skillId, Number(data.total_xp), data.new_level)
      const skillName = SKILL_DISPLAY_NAMES[skillId] ?? skillId
      addCombatMessage({
        text: `You gained ${data.xp_amount} ${skillName} XP.`,
        sender: 'local',
      })
      if (data.leveled_up) {
        addCombatMessage({
          text: `${skillName} is now level ${data.new_level}!`,
          sender: 'local',
        })
      }
      break
    }

    // Direct to the owner only; the multipliers are server-computed.
    case 'HungerUpdate': {
      const prev = get(hungerState)
      const band = data.state as HungerBand
      hungerState.set({
        satiation: data.satiation,
        band,
        moveMult: data.move_mult,
        attackMult: data.attack_mult,
        carryMult: data.carry_mult,
      })
      if (prev && prev.band !== band) {
        addCombatMessage({ text: HUNGER_BAND_MESSAGES[band], sender: 'local' })
      }
      break
    }

    // Direct to the owner only: the full active list (doc/DEBUFF.md).
    case 'DebuffUpdate': {
      const now = Date.now()
      const prevIds = new Set(get(activeDebuffs).map((d) => d.id))
      const next: ActiveDebuff[] = (
        data.debuffs as { id: string; remaining_ms: number }[]
      ).map((d) => ({ id: d.id, until: now + Number(d.remaining_ms) }))
      activeDebuffs.set(next)
      const nextIds = new Set(next.map((d) => d.id))
      for (const id of nextIds) {
        if (!prevIds.has(id)) {
          addCombatMessage({
            text: debuffPresentation(id).applied,
            sender: 'local',
          })
        }
      }
      for (const id of prevIds) {
        if (!nextIds.has(id)) {
          addCombatMessage({
            text: debuffPresentation(id).expired,
            sender: 'local',
          })
        }
      }
      break
    }

    case 'CampfireSpawned':
    case 'CampfireAppeared':
      campfireManager.spawn(data.campfire)
      break

    case 'CampfireRemoved':
      campfireManager.remove(data.campfire_id)
      break

    case 'StallPlaced':
    case 'StallAppeared':
      stallManager.spawn(data.stall)
      break

    case 'StallRemoved':
      stallManager.remove(data.stall_id)
      break

    case 'TipHatPlaced':
    case 'TipHatAppeared':
      tipHatManager.spawn(data.tip_hat)
      break

    case 'TipHatRemoved':
      tipHatManager.remove(data.tip_hat_id)
      break

    case 'GrillStarted':
      grilling.set(true)
      break

    case 'GrillEnded':
      grilling.set(false)
      if (data.grilled_item_def_id == null) {
        addCombatMessage({
          text: 'Your grilling was interrupted.',
          sender: 'local',
        })
      }
      break
  }
}

const HUNGER_BAND_MESSAGES: Record<HungerBand, string> = {
  Normal: 'Your stomach settles. You can sprint and recover normally.',
  Hungry: 'Your stomach growls. You can no longer sprint.',
  Weak: 'You are weak with hunger. You need to eat.',
}
