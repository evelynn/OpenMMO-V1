//! Guilds (IMP-4.1).
//!
//! A roster alone is a chat room, so the first release ships the two things
//! that give a guild a reason to persist: a shared storage and ranks that
//! actually gate it. Numbers live here so the server, the client and the
//! agent all read the same limits.

use serde::{Deserialize, Serialize};

pub type GuildId = i64;

/// Hard cap, deliberately not for sale. A guild that can buy its way past
/// thirty stops being the group you know and becomes a mailing list.
pub const GUILD_MAX_MEMBERS: usize = 30;

/// Twice a character's own storage: shared between up to thirty people, and
/// sized against the same delta transport SPK-2 measured.
pub const GUILD_STORAGE_SLOTS: usize = 240;

/// Ranks per guild, including the leader's. Five rather than RO's twenty:
/// every rank past the third is a permission nobody uses and a row everybody
/// pays for.
pub const GUILD_RANKS: usize = 5;

/// The rank a founder holds. Always every permission, never editable.
pub const LEADER_RANK: u8 = 0;
/// The rank an invitee lands on: able to contribute to the vault, not to
/// empty it. The rank below is a probation the leader can demote to, not
/// where everyone starts — a new member who can do nothing at all has no
/// reason to open the panel.
pub const DEFAULT_RANK: u8 = 3;

/// What a rank may do. A bitfield rather than a row per permission: five
/// flags fit in a byte and a guild's whole permission table is five bytes.
pub mod perms {
    pub const INVITE: u8 = 1 << 0;
    pub const KICK: u8 = 1 << 1;
    pub const STORAGE_DEPOSIT: u8 = 1 << 2;
    pub const STORAGE_WITHDRAW: u8 = 1 << 3;
    /// Reserved. Housing enforces no ownership at all today, so nothing
    /// checks this yet (doc 13 IMP-4.1 revision).
    pub const HOUSE_EDIT: u8 = 1 << 4;

    pub const ALL: u8 = INVITE | KICK | STORAGE_DEPOSIT | STORAGE_WITHDRAW | HOUSE_EDIT;
}

/// Default permissions for a fresh guild's five ranks, leader first.
///
/// Withdrawing is the one that can empty the guild, so it starts two ranks
/// higher than depositing — a new member can contribute before they can take.
pub const DEFAULT_RANK_PERMS: [u8; GUILD_RANKS] = [
    perms::ALL,
    perms::INVITE | perms::KICK | perms::STORAGE_DEPOSIT | perms::STORAGE_WITHDRAW,
    perms::INVITE | perms::STORAGE_DEPOSIT | perms::STORAGE_WITHDRAW,
    perms::STORAGE_DEPOSIT,
    0,
];

pub const DEFAULT_RANK_NAMES: [&str; GUILD_RANKS] =
    ["Leader", "Officer", "Veteran", "Member", "Recruit"];

/// Longest guild name accepted. Names are unique, so this also bounds what a
/// squatter can reserve.
pub const GUILD_NAME_MAX: usize = 24;

/// Whether `name` is acceptable: printable, trimmed, and not empty.
pub fn is_valid_guild_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && trimmed.len() == name.len()
        && trimmed.chars().count() <= GUILD_NAME_MAX
        && trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '\'' || c == '-')
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildMemberInfo {
    pub character_id: i64,
    pub name: String,
    pub rank_id: u8,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildRankInfo {
    pub rank_id: u8,
    pub name: String,
    pub perm_bits: u8,
}

/// Everything a member's panel draws. Sent whole on any change: a guild is
/// thirty rows, so a delta protocol would cost more to maintain than to send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildState {
    pub guild_id: GuildId,
    pub name: String,
    pub leader_character_id: i64,
    /// The viewer's own rank, so the client can grey out what it cannot do.
    pub your_rank_id: u8,
    pub members: Vec<GuildMemberInfo>,
    pub ranks: Vec<GuildRankInfo>,
}

/// Why the server refused a guild action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuildDeniedReason {
    NotInAGuild,
    AlreadyInAGuild,
    NameTaken,
    BadName,
    NoPermission,
    GuildFull,
    NotFound,
    /// The leader cannot leave a guild that still has members, and cannot be
    /// kicked at all.
    LeaderMustHandOver,
}

impl std::fmt::Display for GuildDeniedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotInAGuild => "not_in_a_guild",
            Self::AlreadyInAGuild => "already_in_a_guild",
            Self::NameTaken => "name_taken",
            Self::BadName => "bad_name",
            Self::NoPermission => "no_permission",
            Self::GuildFull => "guild_full",
            Self::NotFound => "not_found",
            Self::LeaderMustHandOver => "leader_must_hand_over",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_leader_rank_can_do_everything() {
        assert_eq!(DEFAULT_RANK_PERMS[LEADER_RANK as usize], perms::ALL);
    }

    /// A new member gives before they can take: the rank an invite lands on
    /// may deposit and may not withdraw.
    #[test]
    fn a_new_member_can_contribute_but_not_empty_the_vault() {
        let starting = DEFAULT_RANK_PERMS[DEFAULT_RANK as usize];
        assert_ne!(starting & perms::STORAGE_DEPOSIT, 0);
        assert_eq!(starting & perms::STORAGE_WITHDRAW, 0);
        assert_eq!(starting & perms::KICK, 0);
    }

    /// The bottom rank is a probation, so it can do nothing at all.
    #[test]
    fn the_bottom_rank_is_a_probation() {
        assert_eq!(DEFAULT_RANK_PERMS[GUILD_RANKS - 1], 0);
        assert!(
            (DEFAULT_RANK as usize) < GUILD_RANKS - 1,
            "nobody should start on probation"
        );
    }

    #[test]
    fn every_rank_has_a_name() {
        assert_eq!(DEFAULT_RANK_NAMES.len(), GUILD_RANKS);
        assert_eq!(DEFAULT_RANK_PERMS.len(), GUILD_RANKS);
    }

    #[test]
    fn a_name_must_be_trimmed_printable_and_short() {
        assert!(is_valid_guild_name("The Iron Circle"));
        assert!(is_valid_guild_name("O'Hara-Kin"));
        assert!(!is_valid_guild_name(""));
        assert!(!is_valid_guild_name("   "));
        assert!(!is_valid_guild_name(" padded"));
        assert!(!is_valid_guild_name("trailing "));
        assert!(!is_valid_guild_name(&"x".repeat(GUILD_NAME_MAX + 1)));
        assert!(!is_valid_guild_name("drop\ttable"));
    }

    /// Non-ASCII names are fine; the cap counts characters, not bytes, so a
    /// Korean guild name is not silently a third as long.
    #[test]
    fn the_name_cap_counts_characters() {
        let name = "가".repeat(GUILD_NAME_MAX);
        assert!(name.len() > GUILD_NAME_MAX, "the fixture must be multibyte");
        assert!(is_valid_guild_name(&name));
        assert!(!is_valid_guild_name(&"가".repeat(GUILD_NAME_MAX + 1)));
    }
}
