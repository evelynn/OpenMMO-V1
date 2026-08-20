//! Guilds (IMP-4.1).
//!
//! A roster on its own is a chat room, so this ships with the two things that
//! give a guild a reason to outlive a raid: a shared vault, and ranks that
//! gate it. The vault reuses the storage container wholesale — the deposit
//! and withdrawal messages never learned there was a second kind of box.
//!
//! Membership is one indexed row per character, mirrored into `guild_of` so
//! "which guild is this player in" costs a hash lookup rather than a query.

use crate::auth::AuthService;
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::guild::{
    is_valid_guild_name, perms, GuildDeniedReason, GuildId, GuildMemberInfo, GuildRankInfo,
    GuildState, DEFAULT_RANK, GUILD_MAX_MEMBERS, LEADER_RANK,
};
use std::sync::Arc;
use tracing::{info, warn};

/// A character's guild and rank while they are online.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GuildMembership {
    pub guild_id: GuildId,
    pub rank_id: u8,
}

/// Run a blocking guild query off the async threads.
async fn db<T: Send + 'static>(
    op: impl FnOnce() -> Result<T, crate::auth::AuthError> + Send + 'static,
) -> Option<T> {
    match tokio::task::spawn_blocking(op).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(e)) => {
            warn!("guild query failed: {e}");
            None
        }
        Err(e) => {
            warn!("spawn_blocking panicked on a guild query: {e}");
            None
        }
    }
}

impl super::GameState {
    pub(crate) async fn load_guild_membership(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        character_id: i64,
    ) {
        let auth = Arc::clone(auth_service);
        let Some(Some((guild_id, rank_id))) = db(move || auth.guild_of(character_id)).await else {
            return;
        };
        self.guild_of
            .write()
            .await
            .insert(*player_id, GuildMembership { guild_id, rank_id });
    }

    pub(crate) async fn forget_guild_membership(&self, player_id: &PlayerId) {
        self.guild_of.write().await.remove(player_id);
    }

    pub(crate) async fn guild_membership(&self, player_id: &PlayerId) -> Option<GuildMembership> {
        self.guild_of.read().await.get(player_id).copied()
    }

    /// Whether the player's rank carries `permission`. Absent membership is a
    /// clean no, which is what makes this safe to call from the storage path.
    pub(super) async fn guild_permits(&self, player_id: &PlayerId, permission: u8) -> bool {
        let Some(membership) = self.guild_membership(player_id).await else {
            return false;
        };
        if membership.rank_id == LEADER_RANK {
            return true;
        }
        self.guild_rank_perms(membership.guild_id, membership.rank_id)
            .await
            .is_some_and(|bits| bits & permission != 0)
    }

    async fn guild_rank_perms(&self, guild_id: GuildId, rank_id: u8) -> Option<u8> {
        self.guild_ranks
            .read()
            .await
            .get(&guild_id)?
            .iter()
            .find(|(id, _, _)| *id == rank_id)
            .map(|(_, _, bits)| *bits)
    }

    /// Online members of `guild_id` with the vault open, so a delta reaches
    /// everyone looking at the same shelf.
    pub(super) async fn guild_viewers(&self, guild_id: GuildId) -> Vec<PlayerId> {
        let open = self.open_storages.read().await;
        self.guild_of
            .read()
            .await
            .iter()
            .filter(|(player_id, m)| {
                m.guild_id == guild_id
                    && matches!(
                        open.get(player_id),
                        Some(super::storage::OpenContainer::Guild(id)) if *id == guild_id
                    )
            })
            .map(|(player_id, _)| *player_id)
            .collect()
    }

    async fn online_guild_members(&self, guild_id: GuildId) -> Vec<PlayerId> {
        self.guild_of
            .read()
            .await
            .iter()
            .filter(|(_, m)| m.guild_id == guild_id)
            .map(|(player_id, _)| *player_id)
            .collect()
    }

    async fn deny(&self, player_id: &PlayerId, reason: GuildDeniedReason) {
        self.send_direct_message(player_id, ServerMessage::GuildDenied { reason })
            .await;
    }

    /// Rebuild the guild's state from the database and push it to whichever
    /// members are online. One guild is thirty rows, so a whole-state resend
    /// costs less than a delta protocol would cost to keep correct.
    pub(crate) async fn push_guild_state(
        &self,
        auth_service: &Arc<AuthService>,
        guild_id: GuildId,
    ) {
        let auth = Arc::clone(auth_service);
        let Some(Some(rows)) = db(move || auth.load_guild(guild_id)).await else {
            return;
        };
        self.guild_ranks
            .write()
            .await
            .insert(guild_id, rows.ranks.clone());

        let online = self.online_guild_members(guild_id).await;
        let online_ids: std::collections::HashSet<i64> = {
            let characters = self.player_characters.read().await;
            online
                .iter()
                .filter_map(|pid| characters.get(pid).map(|(id, _, _)| *id))
                .collect()
        };
        let members: Vec<GuildMemberInfo> = rows
            .members
            .iter()
            .map(|(character_id, name, rank_id)| GuildMemberInfo {
                character_id: *character_id,
                name: name.clone(),
                rank_id: *rank_id,
                online: online_ids.contains(character_id),
            })
            .collect();
        let ranks: Vec<GuildRankInfo> = rows
            .ranks
            .iter()
            .map(|(rank_id, name, perm_bits)| GuildRankInfo {
                rank_id: *rank_id,
                name: name.clone(),
                perm_bits: *perm_bits,
            })
            .collect();

        for player_id in online {
            let your_rank_id = self
                .guild_membership(&player_id)
                .await
                .map_or(DEFAULT_RANK, |m| m.rank_id);
            self.send_direct_message(
                &player_id,
                ServerMessage::GuildUpdated {
                    guild: Some(GuildState {
                        guild_id,
                        name: rows.name.clone(),
                        leader_character_id: rows.leader_character_id,
                        your_rank_id,
                        members: members.clone(),
                        ranks: ranks.clone(),
                    }),
                },
            )
            .await;
        }
    }

    pub async fn create_guild(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        name: String,
    ) {
        if !is_valid_guild_name(&name) {
            return self.deny(player_id, GuildDeniedReason::BadName).await;
        }
        if self.guild_membership(player_id).await.is_some() {
            return self
                .deny(player_id, GuildDeniedReason::AlreadyInAGuild)
                .await;
        }
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth = Arc::clone(auth_service);
        let created = name.clone();
        // The UNIQUE constraints are the real arbiter: two founders racing on
        // the same name both reach here, and exactly one insert survives.
        let Some(guild_id) = db(move || auth.create_guild(&created, character_id)).await else {
            return self.deny(player_id, GuildDeniedReason::NameTaken).await;
        };
        self.guild_of.write().await.insert(
            *player_id,
            GuildMembership {
                guild_id,
                rank_id: LEADER_RANK,
            },
        );
        info!("Character {character_id} founded guild '{name}' ({guild_id})");
        self.push_guild_state(auth_service, guild_id).await;
    }

    pub async fn invite_to_guild(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        name: String,
    ) {
        let Some(membership) = self.guild_membership(player_id).await else {
            return self.deny(player_id, GuildDeniedReason::NotInAGuild).await;
        };
        if !self.guild_permits(player_id, perms::INVITE).await {
            return self.deny(player_id, GuildDeniedReason::NoPermission).await;
        }
        if self.guild_size(auth_service, membership.guild_id).await >= GUILD_MAX_MEMBERS {
            return self.deny(player_id, GuildDeniedReason::GuildFull).await;
        }
        let Some(target) = self.player_id_by_name(&name).await else {
            return self.deny(player_id, GuildDeniedReason::NotFound).await;
        };
        if self.guild_membership(&target).await.is_some() {
            return self
                .deny(player_id, GuildDeniedReason::AlreadyInAGuild)
                .await;
        }

        let auth = Arc::clone(auth_service);
        let guild_id = membership.guild_id;
        let Some(Some(rows)) = db(move || auth.load_guild(guild_id)).await else {
            return;
        };
        self.guild_invites
            .write()
            .await
            .insert(target, membership.guild_id);
        let from = self.player_name_of(player_id).await;
        self.send_direct_message(
            &target,
            ServerMessage::GuildInvite {
                guild_id: membership.guild_id,
                guild_name: rows.name,
                from,
            },
        )
        .await;
    }

    pub async fn respond_guild_invite(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        guild_id: GuildId,
        accept: bool,
    ) {
        let invited = self.guild_invites.write().await.remove(player_id);
        if invited != Some(guild_id) || !accept {
            return;
        }
        if self.guild_membership(player_id).await.is_some() {
            return self
                .deny(player_id, GuildDeniedReason::AlreadyInAGuild)
                .await;
        }
        // Re-checked on acceptance, not only on invite: thirty people can be
        // invited to a guild with one seat left.
        if self.guild_size(auth_service, guild_id).await >= GUILD_MAX_MEMBERS {
            return self.deny(player_id, GuildDeniedReason::GuildFull).await;
        }
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth = Arc::clone(auth_service);
        if db(move || auth.add_guild_member(guild_id, character_id, DEFAULT_RANK))
            .await
            .is_none()
        {
            return self
                .deny(player_id, GuildDeniedReason::AlreadyInAGuild)
                .await;
        }
        self.guild_of.write().await.insert(
            *player_id,
            GuildMembership {
                guild_id,
                rank_id: DEFAULT_RANK,
            },
        );
        self.push_guild_state(auth_service, guild_id).await;
    }

    async fn guild_size(&self, auth_service: &Arc<AuthService>, guild_id: GuildId) -> usize {
        let auth = Arc::clone(auth_service);
        db(move || auth.load_guild(guild_id))
            .await
            .flatten()
            .map_or(0, |rows| rows.members.len())
    }

    pub async fn kick_from_guild(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        character_id: i64,
    ) {
        let Some(membership) = self.guild_membership(player_id).await else {
            return self.deny(player_id, GuildDeniedReason::NotInAGuild).await;
        };
        if !self.guild_permits(player_id, perms::KICK).await {
            return self.deny(player_id, GuildDeniedReason::NoPermission).await;
        }
        let auth = Arc::clone(auth_service);
        let guild_id = membership.guild_id;
        let Some(Some(rows)) = db(move || auth.load_guild(guild_id)).await else {
            return;
        };
        if rows.leader_character_id == character_id {
            return self
                .deny(player_id, GuildDeniedReason::LeaderMustHandOver)
                .await;
        }
        if !rows.members.iter().any(|(id, _, _)| *id == character_id) {
            return self.deny(player_id, GuildDeniedReason::NotFound).await;
        }
        self.detach_member(auth_service, guild_id, character_id)
            .await;
        self.push_guild_state(auth_service, guild_id).await;
    }

    pub async fn leave_guild(&self, auth_service: &Arc<AuthService>, player_id: &PlayerId) {
        let Some(membership) = self.guild_membership(player_id).await else {
            return self.deny(player_id, GuildDeniedReason::NotInAGuild).await;
        };
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth = Arc::clone(auth_service);
        let guild_id = membership.guild_id;
        let Some(Some(rows)) = db(move || auth.load_guild(guild_id)).await else {
            return;
        };
        let is_leader = rows.leader_character_id == character_id;
        if is_leader && rows.members.len() > 1 {
            return self
                .deny(player_id, GuildDeniedReason::LeaderMustHandOver)
                .await;
        }

        self.detach_member(auth_service, guild_id, character_id)
            .await;
        if is_leader {
            // The last one out takes the guild with them; the cascades clear
            // the ranks and the vault.
            let auth = Arc::clone(auth_service);
            db(move || auth.delete_guild(guild_id)).await;
            self.guild_ranks.write().await.remove(&guild_id);
            self.guild_storages.write().await.remove(&guild_id);
            self.dirty_guild_storages.write().await.remove(&guild_id);
        } else {
            self.push_guild_state(auth_service, guild_id).await;
        }
        self.send_direct_message(player_id, ServerMessage::GuildUpdated { guild: None })
            .await;
    }

    /// Remove the row, drop the runtime mirror, and close the vault if they
    /// had it open — a former member must not keep a window onto it.
    async fn detach_member(
        &self,
        auth_service: &Arc<AuthService>,
        guild_id: GuildId,
        character_id: i64,
    ) {
        let auth = Arc::clone(auth_service);
        db(move || auth.remove_guild_member(character_id)).await;
        let departing: Vec<PlayerId> = {
            let characters = self.player_characters.read().await;
            self.guild_of
                .read()
                .await
                .iter()
                .filter(|(pid, m)| {
                    m.guild_id == guild_id
                        && characters
                            .get(pid)
                            .is_some_and(|(id, _, _)| *id == character_id)
                })
                .map(|(pid, _)| *pid)
                .collect()
        };
        for pid in departing {
            self.guild_of.write().await.remove(&pid);
            if matches!(
                self.open_storages.read().await.get(&pid),
                Some(super::storage::OpenContainer::Guild(_))
            ) {
                self.open_storages.write().await.remove(&pid);
            }
            self.send_direct_message(&pid, ServerMessage::GuildUpdated { guild: None })
                .await;
        }
    }

    pub async fn set_guild_rank(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        character_id: i64,
        rank_id: u8,
    ) {
        let Some(membership) = self.guild_membership(player_id).await else {
            return self.deny(player_id, GuildDeniedReason::NotInAGuild).await;
        };
        // Ranks are the leader's to hand out: an officer who could promote
        // could promote themselves.
        if membership.rank_id != LEADER_RANK
            || rank_id == LEADER_RANK
            || usize::from(rank_id) >= onlinerpg_shared::guild::GUILD_RANKS
        {
            return self.deny(player_id, GuildDeniedReason::NoPermission).await;
        }
        let auth = Arc::clone(auth_service);
        db(move || auth.set_guild_member_rank(character_id, rank_id)).await;
        self.remirror_rank(character_id, rank_id).await;
        self.push_guild_state(auth_service, membership.guild_id)
            .await;
    }

    pub async fn transfer_guild_leadership(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        character_id: i64,
    ) {
        let Some(membership) = self.guild_membership(player_id).await else {
            return self.deny(player_id, GuildDeniedReason::NotInAGuild).await;
        };
        if membership.rank_id != LEADER_RANK {
            return self.deny(player_id, GuildDeniedReason::NoPermission).await;
        }
        let Some(mine) = self.character_id_of(player_id).await else {
            return;
        };
        let guild_id = membership.guild_id;
        let auth = Arc::clone(auth_service);
        let Some(Some(rows)) = db(move || auth.load_guild(guild_id)).await else {
            return;
        };
        if !rows.members.iter().any(|(id, _, _)| *id == character_id) {
            return self.deny(player_id, GuildDeniedReason::NotFound).await;
        }

        let auth = Arc::clone(auth_service);
        db(move || {
            auth.set_guild_leader(guild_id, character_id)?;
            auth.set_guild_member_rank(character_id, LEADER_RANK)?;
            // The outgoing leader keeps a rank they can still work from.
            auth.set_guild_member_rank(mine, 1)
        })
        .await;
        self.remirror_rank(character_id, LEADER_RANK).await;
        self.remirror_rank(mine, 1).await;
        self.push_guild_state(auth_service, guild_id).await;
    }

    /// Update the in-memory rank of whichever online player is that character.
    async fn remirror_rank(&self, character_id: i64, rank_id: u8) {
        let target: Option<PlayerId> = {
            let characters = self.player_characters.read().await;
            characters
                .iter()
                .find(|(_, (id, _, _))| *id == character_id)
                .map(|(pid, _)| *pid)
        };
        if let Some(pid) = target {
            if let Some(m) = self.guild_of.write().await.get_mut(&pid) {
                m.rank_id = rank_id;
            }
        }
    }

    pub async fn open_guild_storage(&self, auth_service: &Arc<AuthService>, player_id: &PlayerId) {
        let Some(membership) = self.guild_membership(player_id).await else {
            return self.deny(player_id, GuildDeniedReason::NotInAGuild).await;
        };
        if !self
            .ensure_guild_storage_loaded(auth_service, membership.guild_id)
            .await
        {
            return;
        }
        self.open_storages.write().await.insert(
            *player_id,
            super::storage::OpenContainer::Guild(membership.guild_id),
        );
        let slots = self
            .guild_storages
            .read()
            .await
            .get(&membership.guild_id)
            .cloned()
            .unwrap_or_default();
        self.send_direct_message(player_id, ServerMessage::StorageOpened { slots })
            .await;
    }

    /// One line to every member who is online. Built once and sent to the
    /// list, the way party chat is — never a query per member.
    pub async fn send_guild_chat(&self, player_id: &PlayerId, message: String) {
        let Some(membership) = self.guild_membership(player_id).await else {
            return self.deny(player_id, GuildDeniedReason::NotInAGuild).await;
        };
        let sender = self.player_name_of(player_id).await;
        let members = self.online_guild_members(membership.guild_id).await;
        self.send_direct_message_to_players(
            &members,
            ServerMessage::GuildChatMessage { sender, message },
        )
        .await;
    }
}
