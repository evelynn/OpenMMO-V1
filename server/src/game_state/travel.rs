//! Paid travel between fixed town nodes (IMP-2.4).
//!
//! Two rules carry the design. Destinations come from a table, never from the
//! client, so crossing the world stays content rather than a menu. And a
//! dungeon is neither a destination nor an origin: the table is checked at
//! boot, and `floor_level != 0` refuses the departure, which is the one line
//! that stops "escape a dungeon by paying".
//!
//! The fare is burned. Paying it to the NPC would be a transfer between
//! pockets, and the point of a money sink is that the money stops existing.

use crate::travel_defs::{travel_node, travel_nodes};
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::messages::TravelOffer;

impl super::GameState {
    /// Answer the travel service. An empty `node_id` asks what is on offer;
    /// a named one asks to be taken there.
    pub async fn request_travel(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        node_id: &str,
    ) {
        if let Err(reason) = self.validate_travel_agent(player_id, npc_player_id).await {
            return self.deny_travel(player_id, reason).await;
        }
        if node_id.is_empty() {
            return self.send_travel_destinations(player_id).await;
        }
        let Some(node) = travel_node(node_id) else {
            return self.deny_travel(player_id, "No such destination").await;
        };

        let (level, floor_level, in_combat) = {
            let players = self.players.read().await;
            let Some(player) = players.get(player_id) else {
                return;
            };
            (
                player.level,
                player.floor_level,
                Self::in_combat(player) || player.health == 0,
            )
        };
        // The dungeon rule, from the departure side.
        if floor_level != 0 {
            return self
                .deny_travel(player_id, "You can only travel from the surface")
                .await;
        }
        if in_combat {
            return self
                .deny_travel(player_id, "Not while you are fighting")
                .await;
        }
        if level < node.min_level {
            return self
                .deny_travel(player_id, "You are not experienced enough for that road")
                .await;
        }

        // Charged under the gold guard so two requests cannot spend the same
        // coin, and burned rather than paid to anyone.
        if node.fare > 0 {
            let mut gold = self.player_gold.write().await;
            let Some(purse) = gold.get_mut(player_id) else {
                return;
            };
            if *purse < node.fare {
                drop(gold);
                return self
                    .deny_travel(player_id, "You cannot afford the fare")
                    .await;
            }
            *purse -= node.fare;
            let remaining = *purse;
            drop(gold);
            self.mark_dirty(player_id).await;
            self.send_direct_message(player_id, ServerMessage::GoldUpdate { gold: remaining })
                .await;
        }

        let position = crate::types::Position {
            x: node.x,
            y: node.y,
            z: node.z,
        };
        self.teleport_player(player_id, position, 0.0, 0).await;
        self.send_system_message(player_id, &format!("You arrive at {}.", node.name))
            .await;
    }

    async fn send_travel_destinations(&self, player_id: &PlayerId) {
        let level = {
            let players = self.players.read().await;
            players.get(player_id).map_or(1, |p| p.level)
        };
        let purse = self
            .player_gold
            .read()
            .await
            .get(player_id)
            .copied()
            .unwrap_or(0);
        let nodes = travel_nodes()
            .iter()
            .map(|node| TravelOffer {
                id: node.id.clone(),
                name: node.name.clone(),
                fare: node.fare,
                min_level: node.min_level,
                affordable: level >= node.min_level && purse >= node.fare,
            })
            .collect();
        self.send_direct_message(player_id, ServerMessage::TravelDestinations { nodes })
            .await;
    }

    async fn deny_travel(&self, player_id: &PlayerId, reason: &str) {
        self.send_direct_message(
            player_id,
            ServerMessage::TravelDenied {
                reason: reason.to_string(),
            },
        )
        .await;
    }

    /// An official NPC within reach, like every other town service.
    async fn validate_travel_agent(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
    ) -> Result<(), &'static str> {
        let players = self.players.read().await;
        let player = players.get(player_id).ok_or("Player not found")?;
        let npc = players.get(npc_player_id).ok_or("Nobody is there")?;
        if !npc.is_official_npc {
            return Err("Only townsfolk arrange travel");
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
