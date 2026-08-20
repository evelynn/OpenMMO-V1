//! World channels (IMP-7.1).
//!
//! One server process holds several parallel copies of the world. Characters,
//! storage, guilds and mail are shared across all of them — a channel splits
//! **who sees whom**, nothing else. Switching channels keeps your character.
//!
//! The number exists because [`tools/spike-socket-load.sh`] measured it: a
//! thousand real connections are comfortable, five thousand connect and then
//! cannot stay.

use serde::{Deserialize, Serialize};

/// How many players one channel holds before it refuses more.
pub const CHANNEL_CAPACITY: u16 = 1_000;

/// Channels the server runs. Total capacity is this times the cap.
pub const DEFAULT_CHANNEL_COUNT: u8 = 4;

/// Which copy of the world a player is in. `0` is the first channel; players
/// see it as "Channel 1".
pub type ChannelId = u8;

/// Occupancy of one channel, for the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelOccupancy {
    pub channel: ChannelId,
    pub players: u16,
}

impl ChannelOccupancy {
    pub fn is_full(&self) -> bool {
        self.players >= CHANNEL_CAPACITY
    }
}

/// Why a channel switch was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelDeniedReason {
    /// That channel is at capacity.
    Full,
    /// No such channel on this server.
    NoSuchChannel,
    /// Already there.
    SameChannel,
    /// Underground: a dungeon floor is walked per instance, and moving
    /// channels mid-descent would strand the monsters that belong to it.
    InDungeon,
}

impl std::fmt::Display for ChannelDeniedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Full => "that channel is full",
            Self::NoSuchChannel => "no such channel",
            Self::SameChannel => "you are already there",
            Self::InDungeon => "not while you are underground",
        };
        f.write_str(text)
    }
}

/// The least crowded channel with room, preferring `near` when it has room —
/// which is how a player lands beside their party rather than wherever the
/// counter happens to point.
pub fn best_channel(occupancy: &[ChannelOccupancy], near: Option<ChannelId>) -> Option<ChannelId> {
    if let Some(wanted) = near {
        if let Some(entry) = occupancy.iter().find(|o| o.channel == wanted) {
            if !entry.is_full() {
                return Some(wanted);
            }
        }
    }
    occupancy
        .iter()
        .filter(|o| !o.is_full())
        .min_by_key(|o| (o.players, o.channel))
        .map(|o| o.channel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occ(pairs: &[(ChannelId, u16)]) -> Vec<ChannelOccupancy> {
        pairs
            .iter()
            .map(|&(channel, players)| ChannelOccupancy { channel, players })
            .collect()
    }

    #[test]
    fn the_emptiest_channel_wins_and_ties_go_to_the_lowest() {
        let o = occ(&[(0, 40), (1, 5), (2, 5)]);
        assert_eq!(best_channel(&o, None), Some(1));
    }

    #[test]
    fn a_party_pulls_you_to_their_channel_unless_it_is_full() {
        let o = occ(&[(0, 900), (1, 10)]);
        assert_eq!(
            best_channel(&o, Some(0)),
            Some(0),
            "busy is not full — go where your party is"
        );

        let o = occ(&[(0, CHANNEL_CAPACITY), (1, 10)]);
        assert_eq!(
            best_channel(&o, Some(0)),
            Some(1),
            "a full channel hands you to the emptiest one instead"
        );
    }

    #[test]
    fn a_server_with_every_channel_full_admits_nobody() {
        let o = occ(&[(0, CHANNEL_CAPACITY), (1, CHANNEL_CAPACITY)]);
        assert_eq!(best_channel(&o, None), None);
        assert_eq!(best_channel(&o, Some(1)), None);
    }
}
