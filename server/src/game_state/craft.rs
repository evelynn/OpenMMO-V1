//! Crafting (IMP-4.4), in two branches over one recipe table.
//!
//! **Commissioning an NPC** costs a fee, cannot fail, and cannot aim high:
//! you are buying certainty. **Working it yourself** at a fire costs nothing
//! but the materials, rolls `craft_success_bp`, may aim for a better piece,
//! and loses everything on a bad roll — the same bargain over-enchanting
//! offers.
//!
//! Materials are all-or-nothing: they are counted and taken under one write
//! lock, so a craft never eats half a recipe.

use super::inventory::{stack_into_bag, BagInsert};
use crate::types::PlayerId;
use onlinerpg_shared::craft::{craft_success_bp, CRAFT_BP_SCALE, MAX_CRAFT_OPTIONS};
use onlinerpg_shared::inventory::PlayerInventory;
use onlinerpg_shared::ServerMessage;
use rand::Rng;
use tracing::info;

/// How close a fire has to be to work at. The grill radius: if you can cook
/// at it, you can heat metal at it.
const FORGE_RADIUS: f32 = onlinerpg_shared::hunger::CAMPFIRE_GRILL_RADIUS;

/// XP a successful self-craft teaches, before the ambition bonus.
const CRAFT_XP: u64 = 40;

/// Units of `item_def_id` in the bag, at any enchant level.
fn bag_units(inv: &PlayerInventory, item_def_id: &str) -> u32 {
    inv.bag
        .iter()
        .filter(|item| item.item_def_id == item_def_id)
        .map(|item| item.quantity)
        .sum()
}

/// Take exactly `quantity` units, lowest enchant first. The caller must have
/// verified the bag covers it under the same lock.
fn take_units(inv: &mut PlayerInventory, item_def_id: &str, quantity: u32) {
    let mut left = quantity;
    while left > 0 {
        let Some(idx) = inv
            .bag
            .iter()
            .enumerate()
            .filter(|(_, item)| item.item_def_id == item_def_id)
            .min_by_key(|(_, item)| item.enchant)
            .map(|(idx, _)| idx)
        else {
            return;
        };
        let take = left.min(inv.bag[idx].quantity);
        inv.bag[idx].quantity -= take;
        if inv.bag[idx].quantity == 0 {
            inv.bag.remove(idx);
        }
        left -= take;
    }
}

impl super::GameState {
    /// Craft `recipe_id`. `npc_player_id` commissions; `None` is your own
    /// hands at a fire.
    pub async fn craft_item(
        &self,
        player_id: &PlayerId,
        recipe_id: &str,
        npc_player_id: Option<PlayerId>,
        options: u8,
    ) {
        if self
            .reject_if_defeated(player_id, "You can't work while defeated")
            .await
        {
            return;
        }
        let Some(recipe) = self.recipe_defs.get(recipe_id) else {
            return self
                .send_system_message(player_id, "You know no such recipe")
                .await;
        };
        if options > MAX_CRAFT_OPTIONS {
            return self
                .send_system_message(
                    player_id,
                    format!("You can aim at most {MAX_CRAFT_OPTIONS} above plain"),
                )
                .await;
        }

        let commissioned = match npc_player_id {
            Some(npc) => {
                if options > 0 {
                    return self
                        .send_system_message(
                            player_id,
                            "They make it their way, plain and sound - aim high yourself",
                        )
                        .await;
                }
                if let Err(reason) = self.validate_craft_npc(player_id, &npc).await {
                    return self.send_system_message(player_id, reason).await;
                }
                Some(npc)
            }
            None => {
                if !self.at_a_fire(player_id).await {
                    return self
                        .send_system_message(player_id, "You need a lit fire to work at")
                        .await;
                }
                None
            }
        };

        // The odds are the player's own; a commission never rolls.
        let (roll_bp, chance_bp) = if commissioned.is_some() {
            (0, CRAFT_BP_SCALE)
        } else {
            let dex = {
                let characters = self.player_characters.read().await;
                characters.get(player_id).map_or(10, |(_, _, a)| a.dex)
            };
            let level = self.skill_level(player_id, recipe.skill()).await;
            let chance = craft_success_bp(
                recipe.base_success_bp,
                dex,
                recipe.dex_k,
                level,
                recipe.skill_m,
                options,
                recipe.option_penalty_bp,
            );
            (rand::thread_rng().gen_range(0..CRAFT_BP_SCALE), chance)
        };
        let success = roll_bp < chance_bp;

        // The fee is charged before the materials are touched, so a wallet
        // that cannot cover it costs nothing.
        if let Some(npc) = commissioned {
            if recipe.npc_fee > 0 && !self.spend_copper(player_id, recipe.npc_fee).await {
                return self
                    .send_system_message(
                        player_id,
                        format!("They want {} copper for the work", recipe.npc_fee),
                    )
                    .await;
            }
            let cap = self
                .official_npc_def(&npc)
                .await
                .map_or(0, |def| def.wallet_cap);
            self.credit_npc_wallet(&npc, recipe.npc_fee, cap).await;
        }

        let materials = recipe.materials();
        let output = recipe.output.clone();
        let name = recipe.name.clone();
        let skill = recipe.skill();
        let stackable = self.item_defs.get(&output).is_some_and(|def| def.stackable);
        let instance_id = self.next_instance_id().await;

        let snapshot = {
            let mut inventories = self.inventories.write().await;
            let Some(inv) = inventories.get_mut(player_id) else {
                return;
            };
            // Counted and taken under one lock: never half a recipe.
            let short = materials
                .iter()
                .find(|(id, qty)| bag_units(inv, id) < *qty)
                .cloned();
            if let Some((id, qty)) = short {
                drop(inventories);
                if let Some(npc) = commissioned {
                    // They were paid before the bag was checked; give it back.
                    self.refund_commission(player_id, &npc, recipe_id).await;
                }
                let what = self.item_name(&id);
                return self
                    .send_system_message(player_id, format!("You need {qty} x {what}"))
                    .await;
            }
            for (id, qty) in &materials {
                take_units(inv, id, *qty);
            }
            if success {
                stack_into_bag(
                    &mut inv.bag,
                    BagInsert::one(stackable, &output, i32::from(options), instance_id),
                );
            }
            inv.clone()
        };

        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        self.send_direct_message(
            player_id,
            ServerMessage::CraftResult {
                recipe_id: recipe_id.to_string(),
                success,
                item_def_id: output.clone(),
                enchant: i32::from(options),
            },
        )
        .await;
        let made = self.item_name(&output);
        self.send_system_message(
            player_id,
            if !success {
                format!("{name} failed - the materials are spoiled")
            } else if options > 0 {
                format!("{name}: a {made} (+{options})")
            } else {
                format!("{name}: a {made}")
            },
        )
        .await;

        // Only your own hands teach you anything, and only when they work.
        if success && commissioned.is_none() {
            self.add_skill_xp(player_id, skill, CRAFT_XP * (1 + u64::from(options)))
                .await;
        }
        info!(
            "craft: {} {} {recipe_id}{}",
            self.player_name_of(player_id).await,
            if success { "made" } else { "spoiled" },
            if commissioned.is_some() {
                " (commissioned)"
            } else {
                ""
            }
        );
    }

    /// Hand a commission fee back when the bag turned out not to cover the
    /// recipe. The NPC's wallet gives up what it was just credited.
    async fn refund_commission(&self, player_id: &PlayerId, npc: &PlayerId, recipe_id: &str) {
        let Some(fee) = self.recipe_defs.get(recipe_id).map(|r| r.npc_fee) else {
            return;
        };
        if fee <= 0 {
            return;
        }
        {
            let mut gold = self.player_gold.write().await;
            if let Some(wallet) = gold.get_mut(npc) {
                *wallet = (*wallet - fee).max(0);
            }
        }
        self.award_copper(player_id, fee).await;
    }

    /// Whether a lit campfire is within working distance on this floor.
    async fn at_a_fire(&self, player_id: &PlayerId) -> bool {
        let Some((position, floor_level)) = ({
            let players = self.players.read().await;
            players.get(player_id).map(|p| (p.position, p.floor_level))
        }) else {
            return false;
        };
        self.nearby_campfire(&position, floor_level, FORGE_RADIUS)
            .await
            .is_some()
    }

    /// The NPC has to be an official one, in reach, and willing to craft.
    async fn validate_craft_npc(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
    ) -> Result<(), &'static str> {
        {
            let players = self.players.read().await;
            let player = players.get(player_id).ok_or("Player not found")?;
            let npc = players.get(npc_player_id).ok_or("Nobody is there")?;
            if !npc.is_official_npc {
                return Err("Only townsfolk take commissions");
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
        }
        match self.official_npc_def(npc_player_id).await {
            Some(def) if def.crafts() => Ok(()),
            Some(_) => Err("They do not make things"),
            None => Err("Nobody is there"),
        }
    }
}
