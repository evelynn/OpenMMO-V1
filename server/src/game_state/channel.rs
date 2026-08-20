//! Channel assignment and switching (IMP-7.1).
//!
//! A channel splits who sees whom and nothing else: the same character, bag,
//! storage, guild and mail follow you across. That is why this module holds
//! one map and no persistence — there is nothing about a channel worth
//! surviving a restart.
//!
//! The channel is **not** a `Player` field. `Player` is full at msgpack's
//! 15-element fixarray boundary, and a client has no use for anyone else's
//! channel number because the area-of-interest filter never shows them
//! another channel's players in the first place.

use super::GameState;
use crate::types::PlayerId;
use onlinerpg_shared::channel::{
    best_channel, ChannelDeniedReason, ChannelId, ChannelOccupancy, CHANNEL_CAPACITY,
};
use onlinerpg_shared::ServerMessage;
use tracing::info;

impl GameState {
    /// A player's channel. Anyone unknown counts as channel 0, so a lookup
    /// that races registration shows the first channel rather than vanishing.
    pub(crate) fn channel_of(&self, player_id: &PlayerId) -> ChannelId {
        self.player_channels
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(player_id)
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn occupancy(&self) -> Vec<ChannelOccupancy> {
        let channels = self
            .player_channels
            .read()
            .unwrap_or_else(|e| e.into_inner());
        (0..self.channel_count)
            .map(|channel| ChannelOccupancy {
                channel,
                players: channels.values().filter(|c| **c == channel).count() as u16,
            })
            .collect()
    }

    /// Put a joining player on a channel, preferring one a party member is
    /// already on — a party split across channels cannot fight together.
    /// `None` when every channel is full.
    pub(crate) async fn assign_channel(&self, player_id: &PlayerId) -> Option<ChannelId> {
        let party_channel = {
            let members = self.other_party_members(player_id).await;
            members.first().map(|id| self.channel_of(id))
        };
        let chosen = best_channel(&self.occupancy(), party_channel)?;
        self.player_channels
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(*player_id, chosen);
        Some(chosen)
    }

    pub(crate) fn forget_channel(&self, player_id: &PlayerId) {
        self.player_channels
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(player_id);
    }

    /// Tell one player where they are and how full the world is.
    pub(crate) async fn push_channel_state(&self, player_id: &PlayerId) {
        let msg = ServerMessage::ChannelState {
            yours: self.channel_of(player_id),
            channels: self.occupancy(),
        };
        self.send_direct_message(player_id, msg).await;
    }

    /// Move a player to `channel`. Refused when it is full, absent, the one
    /// they are on, or when they are underground — a dungeon floor's monsters
    /// belong to the instance being walked, and leaving mid-descent would
    /// strand them.
    pub async fn switch_channel(&self, player_id: &PlayerId, channel: ChannelId) {
        let deny = |reason| async move {
            self.send_direct_message(player_id, ServerMessage::ChannelDenied { reason })
                .await;
        };

        if channel >= self.channel_count {
            return deny(ChannelDeniedReason::NoSuchChannel).await;
        }
        if channel == self.channel_of(player_id) {
            return deny(ChannelDeniedReason::SameChannel).await;
        }
        let underground = {
            let players = self.players.read().await;
            players.get(player_id).is_some_and(|p| p.floor_level < 0)
        };
        if underground {
            return deny(ChannelDeniedReason::InDungeon).await;
        }
        let full = self
            .occupancy()
            .iter()
            .any(|o| o.channel == channel && o.players >= CHANNEL_CAPACITY);
        if full {
            return deny(ChannelDeniedReason::Full).await;
        }

        // Everyone who could see them is about to stop being able to, and
        // the new channel has to be told someone arrived. Both are the same
        // events a login and a logout already send.
        self.announce_channel_departure(player_id).await;
        self.player_channels
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(*player_id, channel);
        self.announce_channel_arrival(player_id).await;

        info!("Player {player_id} moved to channel {}", channel + 1);
        self.push_channel_state(player_id).await;
    }

    /// Tell the channel being left that this player is gone, and take away
    /// what they could see there. Same messages a logout sends — to the
    /// people watching, someone leaving a channel *is* a logout.
    async fn announce_channel_departure(&self, player_id: &PlayerId) {
        let nearby = self
            .player_ids_within(player_id, super::EVENT_DELIVERY_RADIUS)
            .await;
        self.send_direct_message_to_players_except(
            &nearby,
            ServerMessage::PlayerLeft {
                player_id: *player_id,
            },
            Some(player_id),
        )
        .await;
        // And the mover loses sight of everyone they were watching.
        for other in nearby {
            if other == *player_id {
                continue;
            }
            self.send_direct_message(player_id, ServerMessage::PlayerLeft { player_id: other })
                .await;
        }
    }

    /// The arrival half: the new channel sees them, and they see it.
    async fn announce_channel_arrival(&self, player_id: &PlayerId) {
        let Some(player) = self.players.read().await.get(player_id).cloned() else {
            return;
        };
        let nearby = self
            .player_ids_within(player_id, super::EVENT_DELIVERY_RADIUS)
            .await;
        self.send_direct_message_to_players_except(
            &nearby,
            ServerMessage::PlayerJoined {
                player: player.clone(),
            },
            Some(player_id),
        )
        .await;
        let others: Vec<_> = {
            let players = self.players.read().await;
            nearby
                .iter()
                .filter(|id| **id != *player_id)
                .filter_map(|id| players.get(id).cloned())
                .collect()
        };
        for other in others {
            self.send_direct_message(player_id, ServerMessage::PlayerJoined { player: other })
                .await;
        }
    }
}
