//! Per-character storage (IMP-2.3): a slot-capped container held by a town
//! NPC, with no weight limit of its own.
//!
//! Three rules shape everything here. Transport is delta — one snapshot when
//! the container opens, then one message per changed slot — because
//! re-pushing 120 slots on every deposit is exactly what the cap was chosen
//! to avoid (SPK-2). Contents are resident only while open, so the map's size
//! tracks how many people stand at a storage NPC rather than how many are
//! online. And a move takes the `inventories` and `storages` write locks
//! together, the way `buy_item` takes gold and inventory, because that is the
//! whole of the duplication defence.

use crate::auth::{AuthService, ItemRow};
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::guild::{perms, GuildId, GUILD_STORAGE_SLOTS};
use onlinerpg_shared::inventory::{ItemInstance, PlayerInventory, STORAGE_SLOTS};

use super::inventory::{stack_into_bag, BagInsert};

/// One slot's contents after a change, for the delta messages.
type SlotChange = (u16, Option<ItemInstance>);

/// Which container a player currently has open. The deposit and withdrawal
/// messages are the same either way — only what they land in differs, which
/// is why a guild vault needed no new move protocol (IMP-4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenContainer {
    /// The player's own, anchored to the NPC they are standing at.
    Personal(PlayerId),
    Guild(GuildId),
}

/// Put `quantity` units into the container: onto a matching stack if the def
/// is stackable, otherwise one unit per free slot. Returns the slots that
/// changed, or `None` when there is not enough room — in which case nothing
/// was written.
fn place_in_storage(
    slots: &mut [Option<ItemInstance>],
    item_def_id: &str,
    enchant: i32,
    quantity: u32,
    stackable: bool,
    first_instance_id: u64,
) -> Option<Vec<SlotChange>> {
    if stackable {
        if let Some(index) = slots.iter().position(|slot| {
            slot.as_ref()
                .is_some_and(|i| i.item_def_id == item_def_id && i.enchant == enchant)
        }) {
            let existing = slots[index].as_mut().expect("found above");
            existing.quantity += quantity;
            return Some(vec![(index as u16, slots[index].clone())]);
        }
        let index = slots.iter().position(Option::is_none)?;
        slots[index] = Some(ItemInstance {
            instance_id: first_instance_id,
            item_def_id: item_def_id.to_string(),
            quantity,
            enchant,
        });
        return Some(vec![(index as u16, slots[index].clone())]);
    }

    // Non-stackable: one slot per unit, and all of them or none.
    let free: Vec<usize> = slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.is_none())
        .map(|(i, _)| i)
        .take(quantity as usize)
        .collect();
    if free.len() < quantity as usize {
        return None;
    }
    let mut changed = Vec::with_capacity(free.len());
    for (offset, index) in free.into_iter().enumerate() {
        slots[index] = Some(ItemInstance {
            instance_id: first_instance_id + offset as u64,
            item_def_id: item_def_id.to_string(),
            quantity: 1,
            enchant,
        });
        changed.push((index as u16, slots[index].clone()));
    }
    Some(changed)
}

impl super::GameState {
    /// Open the container this NPC keeps. Same gates as any other NPC
    /// service: an official NPC, reachable, within arm's reach.
    pub async fn open_storage(
        &self,
        auth_service: &std::sync::Arc<AuthService>,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
    ) {
        if let Err(reason) = self.validate_storage_npc(player_id, npc_player_id).await {
            return self.send_system_message(player_id, reason).await;
        }
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };

        // Only load when it is not already resident: re-opening must not
        // discard deposits that have not been flushed yet.
        if !self.storages.read().await.contains_key(player_id) {
            let auth = auth_service.clone();
            let Ok(Ok(rows)) =
                tokio::task::spawn_blocking(move || auth.load_storage(character_id)).await
            else {
                tracing::error!("Storage load failed for character {character_id}");
                return self
                    .send_system_message(player_id, "Your storage could not be opened")
                    .await;
            };
            let mut slots: Vec<Option<ItemInstance>> = vec![None; STORAGE_SLOTS];
            // Stored items carry no persistent instance id, so ids are minted
            // fresh on every load; the client only ever addresses slots.
            let first_id = self.reserve_instance_ids(rows.len() as u64).await;
            for (offset, (slot_index, row)) in rows.into_iter().enumerate() {
                let Some(slot) = slots.get_mut(usize::from(slot_index)) else {
                    // A row beyond the cap survives a cap reduction; drop it
                    // from the view rather than losing the whole container.
                    tracing::warn!(
                        "character {character_id} has a storage row at slot {slot_index}, \
                         beyond STORAGE_SLOTS"
                    );
                    continue;
                };
                *slot = Some(ItemInstance {
                    instance_id: first_id + offset as u64,
                    item_def_id: row.item_def_id,
                    quantity: row.quantity,
                    enchant: row.enchant,
                });
            }
            self.storages.write().await.insert(*player_id, slots);
        }
        self.open_storages
            .write()
            .await
            .insert(*player_id, OpenContainer::Personal(*npc_player_id));

        let slots = self
            .storages
            .read()
            .await
            .get(player_id)
            .cloned()
            .unwrap_or_default();
        self.send_direct_message(player_id, ServerMessage::StorageOpened { slots })
            .await;
    }

    pub async fn close_storage(&self, player_id: &PlayerId) {
        self.open_storages.write().await.remove(player_id);
        self.release_storage_if_clean(player_id).await;
    }

    /// Move `quantity` units of a bag item in. Both write locks are held
    /// across the re-check and both mutations, so a second request cannot
    /// bank the same units twice.
    pub async fn storage_deposit(&self, player_id: &PlayerId, instance_id: u64, quantity: u32) {
        let Some(container) = self.open_container(player_id).await else {
            return self.refuse_closed(player_id, quantity).await;
        };
        if quantity == 0 {
            return self.refuse_closed(player_id, quantity).await;
        }
        if !self
            .may_use_container(player_id, container, perms::STORAGE_DEPOSIT)
            .await
        {
            return self
                .send_system_message(player_id, "Your rank cannot put anything in")
                .await;
        }
        // Reserved outside the locks, like every other id-minting path.
        let first_instance_id = self.reserve_instance_ids(u64::from(quantity)).await;

        let moved = {
            let mut inventories = self.inventories.write().await;
            let mut storages = self.storages.write().await;
            let mut guild_storages = self.guild_storages.write().await;
            let (Some(inv), Some(slots)) = (
                inventories.get_mut(player_id),
                match container {
                    OpenContainer::Personal(_) => storages.get_mut(player_id),
                    OpenContainer::Guild(guild_id) => guild_storages.get_mut(&guild_id),
                },
            ) else {
                return;
            };
            let found = inv.bag.iter().position(|i| i.instance_id == instance_id);
            if let Some(index) = found {
                let taken = quantity.min(inv.bag[index].quantity);
                let def_id = inv.bag[index].item_def_id.clone();
                let enchant = inv.bag[index].enchant;
                let stackable = self.item_defs.stackable(&def_id);
                place_in_storage(slots, &def_id, enchant, taken, stackable, first_instance_id).map(
                    |changed| {
                        if inv.bag[index].quantity > taken {
                            inv.bag[index].quantity -= taken;
                        } else {
                            inv.bag.remove(index);
                        }
                        (changed, inv.clone())
                    },
                )
            } else {
                None
            }
        };

        self.finish_move(player_id, moved, "There is no room in your storage")
            .await;
    }

    /// Take `quantity` units out. The container has no weight of its own, so
    /// this is where carrying capacity is finally checked.
    pub async fn storage_withdraw(&self, player_id: &PlayerId, slot_index: u16, quantity: u32) {
        let Some(container) = self.open_container(player_id).await else {
            return self.refuse_closed(player_id, quantity).await;
        };
        if quantity == 0 {
            return self.refuse_closed(player_id, quantity).await;
        }
        if !self
            .may_use_container(player_id, container, perms::STORAGE_WITHDRAW)
            .await
        {
            return self
                .send_system_message(player_id, "Your rank cannot take anything out")
                .await;
        }
        // Reads player_characters and hunger, which rank below inventories —
        // take it before the write locks, never under them.
        let max_weight = self.max_carry_weight(player_id).await;
        let bag_instance_id = self.reserve_instance_ids(u64::from(quantity)).await;

        let moved = {
            let mut inventories = self.inventories.write().await;
            let mut storages = self.storages.write().await;
            let mut guild_storages = self.guild_storages.write().await;
            let (Some(inv), Some(slots)) = (
                inventories.get_mut(player_id),
                match container {
                    OpenContainer::Personal(_) => storages.get_mut(player_id),
                    OpenContainer::Guild(guild_id) => guild_storages.get_mut(&guild_id),
                },
            ) else {
                return;
            };
            let stored = slots.get(usize::from(slot_index)).cloned().flatten();
            if let Some(stored) = stored {
                let item_weight = self.item_defs.weight(&stored.item_def_id);
                let headroom = max_weight - self.calc_total_weight(inv);
                let affordable = if item_weight <= 0.0 {
                    stored.quantity
                } else {
                    ((headroom / item_weight) + 1e-3).floor().max(0.0) as u32
                };
                let taken = quantity.min(stored.quantity).min(affordable);
                if taken == 0 {
                    None
                } else {
                    let stackable = self.item_defs.stackable(&stored.item_def_id);
                    stack_into_bag(
                        &mut inv.bag,
                        BagInsert {
                            stackable,
                            item_def_id: &stored.item_def_id,
                            enchant: stored.enchant,
                            first_instance_id: bag_instance_id,
                            quantity: taken,
                        },
                    );
                    let slot = &mut slots[usize::from(slot_index)];
                    if stored.quantity > taken {
                        slot.as_mut().expect("read above").quantity -= taken;
                    } else {
                        *slot = None;
                    }
                    Some((vec![(slot_index, slot.clone())], inv.clone()))
                }
            } else {
                None
            }
        };

        self.finish_move(player_id, moved, "You cannot carry that")
            .await;
    }

    /// Close the container of anyone who walked away from their NPC. Rides
    /// the existing tick rather than adding one, like `tick_shop_holds`.
    pub async fn tick_storage_sessions(&self) {
        let open: Vec<(PlayerId, PlayerId)> = {
            let sessions = self.open_storages.read().await;
            if sessions.is_empty() {
                return;
            }
            sessions
                .iter()
                .filter_map(|(player_id, container)| match container {
                    // A guild vault is anchored to a guild, not a spot, so
                    // nothing to walk away from.
                    OpenContainer::Personal(npc) => Some((*player_id, *npc)),
                    OpenContainer::Guild(_) => None,
                })
                .collect()
        };
        for (player_id, npc_player_id) in open {
            if self
                .validate_storage_npc(&player_id, &npc_player_id)
                .await
                .is_err()
            {
                self.close_storage(&player_id).await;
                self.send_system_message(&player_id, "You step away from your storage")
                    .await;
            }
        }
    }

    /// Storage rows for the batch save, and the ids they belong to. Clears
    /// the dirty set the way the inventory flush does.
    pub(super) async fn take_dirty_storages(
        &self,
    ) -> (Vec<PlayerId>, Vec<(i64, Vec<(u16, ItemRow)>)>) {
        let dirty: Vec<PlayerId> = {
            let mut set = self.dirty_storages.write().await;
            if set.is_empty() {
                return (Vec::new(), Vec::new());
            }
            set.drain().collect()
        };
        let storages = self.storages.read().await;
        let characters = self.player_characters.read().await;
        let mut rows = Vec::with_capacity(dirty.len());
        for pid in &dirty {
            let (Some(slots), Some((character_id, _, _))) =
                (storages.get(pid), characters.get(pid))
            else {
                continue;
            };
            rows.push((*character_id, storage_rows(slots)));
        }
        (dirty, rows)
    }

    pub(super) async fn restore_dirty_storages(&self, ids: Vec<PlayerId>) {
        if !ids.is_empty() {
            self.dirty_storages.write().await.extend(ids);
        }
    }

    /// Every open container, for the shutdown snapshot.
    pub(super) async fn all_storage_rows(&self) -> Vec<(i64, Vec<(u16, ItemRow)>)> {
        let storages = self.storages.read().await;
        let characters = self.player_characters.read().await;
        storages
            .iter()
            .filter_map(|(pid, slots)| {
                let (character_id, _, _) = characters.get(pid)?;
                Some((*character_id, storage_rows(slots)))
            })
            .collect()
    }

    pub(super) async fn forget_storage(&self, player_id: &PlayerId) {
        self.storages.write().await.remove(player_id);
        self.dirty_storages.write().await.remove(player_id);
        self.open_storages.write().await.remove(player_id);
    }

    async fn open_container(&self, player_id: &PlayerId) -> Option<OpenContainer> {
        self.open_storages.read().await.get(player_id).copied()
    }

    /// Whether this player's rank allows `permission` on the open container.
    /// A personal container has no ranks — it is yours.
    async fn may_use_container(
        &self,
        player_id: &PlayerId,
        container: OpenContainer,
        permission: u8,
    ) -> bool {
        match container {
            OpenContainer::Personal(_) => true,
            OpenContainer::Guild(_) => self.guild_permits(player_id, permission).await,
        }
    }

    async fn refuse_closed(&self, player_id: &PlayerId, quantity: u32) {
        if quantity > 0 {
            self.send_system_message(player_id, "Your storage is not open")
                .await;
        }
    }

    /// Drop a closed container from memory once nothing is waiting to be
    /// written, so the resident set stays the people actually at an NPC.
    async fn release_storage_if_clean(&self, player_id: &PlayerId) {
        if self.dirty_storages.read().await.contains(player_id) {
            return;
        }
        self.storages.write().await.remove(player_id);
    }

    /// Mark whichever container the player has open as needing a flush.
    async fn mark_open_container_dirty(&self, player_id: &PlayerId) {
        match self.open_container(player_id).await {
            Some(OpenContainer::Guild(guild_id)) => {
                self.dirty_guild_storages.write().await.insert(guild_id);
            }
            _ => {
                self.dirty_storages.write().await.insert(*player_id);
            }
        }
    }

    /// Load a guild's vault into memory if it is not already there, sized to
    /// `GUILD_STORAGE_SLOTS`. Resident only while somebody has it open, the
    /// same rule personal storage follows.
    pub(super) async fn ensure_guild_storage_loaded(
        &self,
        auth_service: &std::sync::Arc<AuthService>,
        guild_id: GuildId,
    ) -> bool {
        if self.guild_storages.read().await.contains_key(&guild_id) {
            return true;
        }
        let auth = auth_service.clone();
        let Ok(Ok(rows)) =
            tokio::task::spawn_blocking(move || auth.load_guild_storage(guild_id)).await
        else {
            tracing::error!("Guild storage load failed for guild {guild_id}");
            return false;
        };
        let mut slots: Vec<Option<ItemInstance>> = vec![None; GUILD_STORAGE_SLOTS];
        let first_id = self.reserve_instance_ids(rows.len() as u64).await;
        for (offset, (slot_index, row)) in rows.into_iter().enumerate() {
            let Some(slot) = slots.get_mut(usize::from(slot_index)) else {
                tracing::warn!("guild {guild_id} has a storage row beyond GUILD_STORAGE_SLOTS");
                continue;
            };
            *slot = Some(ItemInstance {
                instance_id: first_id + offset as u64,
                item_def_id: row.item_def_id,
                quantity: row.quantity,
                enchant: row.enchant,
            });
        }
        self.guild_storages.write().await.insert(guild_id, slots);
        true
    }

    /// Guild vault rows for the batch save.
    pub(super) async fn take_dirty_guild_storages(&self) -> Vec<(GuildId, Vec<(u16, ItemRow)>)> {
        let dirty: Vec<GuildId> = {
            let mut set = self.dirty_guild_storages.write().await;
            if set.is_empty() {
                return Vec::new();
            }
            set.drain().collect()
        };
        let storages = self.guild_storages.read().await;
        dirty
            .into_iter()
            .filter_map(|guild_id| {
                let slots = storages.get(&guild_id)?;
                Some((guild_id, storage_rows(slots)))
            })
            .collect()
    }

    pub(super) async fn restore_dirty_guild_storages(&self, ids: Vec<GuildId>) {
        if !ids.is_empty() {
            self.dirty_guild_storages.write().await.extend(ids);
        }
    }

    /// Push the bag and the changed slots, and mark both for the batch save.
    async fn finish_move(
        &self,
        player_id: &PlayerId,
        moved: Option<(Vec<SlotChange>, PlayerInventory)>,
        refusal: &str,
    ) {
        let Some((changed, inventory)) = moved else {
            return self.send_system_message(player_id, refusal).await;
        };
        self.mark_inventory_dirty(player_id).await;
        self.mark_open_container_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, inventory).await;
        // A guild vault is shared, so every member looking at it sees the
        // same delta rather than a stale panel until they reopen it.
        let watchers = match self.open_container(player_id).await {
            Some(OpenContainer::Guild(guild_id)) => self.guild_viewers(guild_id).await,
            _ => vec![*player_id],
        };
        for (slot_index, item) in changed {
            for watcher in &watchers {
                self.send_direct_message(
                    watcher,
                    ServerMessage::StorageSlotChanged {
                        slot_index,
                        item: item.clone(),
                    },
                )
                .await;
            }
        }
    }

    /// An official NPC, on the player's floor, within reach — the same shape
    /// as `validate_trader`, which is what every NPC service gates on.
    async fn validate_storage_npc(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
    ) -> Result<(), &'static str> {
        let players = self.players.read().await;
        let player = players.get(player_id).ok_or("Player not found")?;
        let npc = players.get(npc_player_id).ok_or("Nobody is there")?;
        if !npc.is_official_npc {
            return Err("Only townsfolk keep storage");
        }
        let dist_sq = super::combat::reachable_dist_sq(
            player.position,
            player.floor_level,
            npc.position,
            npc.floor_level,
        )
        .ok_or("They are on another floor")?;
        if dist_sq > super::trading::MAX_TRADE_DISTANCE * super::trading::MAX_TRADE_DISTANCE {
            return Err("Too far away");
        }
        Ok(())
    }
}

fn storage_rows(slots: &[Option<ItemInstance>]) -> Vec<(u16, ItemRow)> {
    slots
        .iter()
        .enumerate()
        .filter_map(|(index, slot)| {
            let item = slot.as_ref()?;
            Some((
                index as u16,
                ItemRow {
                    item_def_id: item.item_def_id.clone(),
                    quantity: item.quantity,
                    equip_slot: None,
                    enchant: item.enchant,
                },
            ))
        })
        .collect()
}
