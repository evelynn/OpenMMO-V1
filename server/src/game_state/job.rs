//! Job advancement (IMP-8.1): novice → a class → that class awakened.
//!
//! The gate is **Job level**, not base level. That is what makes this fit a
//! game whose base curve doubles every level and stalls around 16 — job XP
//! rides the same kills on a polynomial curve, so it keeps paying out where
//! the base curve has stopped (09 #28, re-judged).
//!
//! Nothing resets. RO sends Job level back to 1 on advancement; here job XP
//! *is* the skill-point payout schedule, so resetting it would contradict the
//! points already spent.

use crate::auth::AuthService;
use onlinerpg_shared::character::JobTier;
use onlinerpg_shared::messages::JobAdvanceDeniedReason;
use onlinerpg_shared::skills::skill_level_from_xp;
use onlinerpg_shared::{CharacterClass, PlayerId, ServerMessage};
use std::sync::Arc;
use tracing::{info, warn};

/// Max HP granted by an advancement. The first pays the difference between
/// the novice die and the job's; the second pays a full die. Past levels are
/// not re-rolled — the die only governs what comes next.
fn hp_bonus(from: CharacterClass, to: CharacterClass, tier: JobTier) -> u32 {
    match tier {
        JobTier::First => u32::from(to.hit_die().saturating_sub(from.hit_die())),
        _ => u32::from(to.hit_die()),
    }
}

impl super::GameState {
    /// The townsfolk gate saving and storage already use: somebody is there,
    /// they are official, and they are close enough on the same floor.
    async fn npc_is_within_reach(&self, player_id: &PlayerId, npc_player_id: &PlayerId) -> bool {
        let players = self.players.read().await;
        let (Some(player), Some(npc)) = (players.get(player_id), players.get(npc_player_id)) else {
            return false;
        };
        if !npc.is_official_npc {
            return false;
        }
        let Some(dist_sq) = super::combat::reachable_dist_sq(
            player.position,
            player.floor_level,
            npc.position,
            npc.floor_level,
        ) else {
            return false;
        };
        dist_sq <= super::trading::MAX_TRADE_DISTANCE * super::trading::MAX_TRADE_DISTANCE
    }

    /// The class the player is wearing and how far up the ladder they are.
    async fn job_state_of(&self, player_id: &PlayerId) -> Option<(CharacterClass, JobTier)> {
        let class = self.players.read().await.get(player_id).map(|p| p.class)?;
        let tier = self.job_tiers.read().await.get(player_id).copied()?;
        Some((class, tier))
    }

    /// Job level: the second growth axis, read off the same curve skill
    /// points pay out on.
    pub(crate) async fn job_level(&self, player_id: &PlayerId) -> u32 {
        let progress = self.job_progress.read().await;
        skill_level_from_xp(progress.get(player_id).map_or(0, |entry| entry.xp))
    }

    pub(crate) async fn advance_job(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        wanted: CharacterClass,
    ) {
        if !self.npc_is_within_reach(player_id, npc_player_id).await {
            self.deny_job(player_id, JobAdvanceDeniedReason::NoOneToAsk)
                .await;
            return;
        }

        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let Some((current_class, tier)) = self.job_state_of(player_id).await else {
            return;
        };

        let Some(next) = tier.next() else {
            self.deny_job(player_id, JobAdvanceDeniedReason::AlreadyAdvanced)
                .await;
            return;
        };
        // Checked before the class, so "come back at job 10" is what a novice
        // hears rather than a complaint about the class they picked.
        if let Some(required) = tier.required_job_level() {
            if self.job_level(player_id).await < required {
                self.deny_job(player_id, JobAdvanceDeniedReason::JobLevelTooLow)
                    .await;
                return;
            }
        }
        match next {
            JobTier::First if !wanted.is_first_job() => {
                self.deny_job(player_id, JobAdvanceDeniedReason::NotAFirstJob)
                    .await;
                return;
            }
            JobTier::Second if wanted != current_class => {
                self.deny_job(player_id, JobAdvanceDeniedReason::ClassMismatch)
                    .await;
                return;
            }
            _ => {}
        }

        let bonus = hp_bonus(current_class, wanted, next);
        let max_hp = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                return;
            };
            player.class = wanted;
            player.max_health = player.max_health.saturating_add(bonus);
            // The advancement is a moment of renewal, and arriving at a new
            // class on three hit points would only send the player to a bed.
            player.health = player.max_health;
            player.max_health
        };

        let auth = Arc::clone(auth_service);
        if let Err(e) = tokio::task::spawn_blocking(move || {
            auth.advance_character_job(character_id, wanted, next, max_hp)
        })
        .await
        .unwrap_or_else(|e| {
            warn!("spawn_blocking panicked recording a job advancement: {e}");
            Ok(())
        }) {
            warn!("Failed to persist job advancement for {character_id}: {e}");
        }

        self.job_tiers.write().await.insert(*player_id, next);
        self.dirty_players.write().await.insert(*player_id);
        self.send_direct_message(
            player_id,
            ServerMessage::JobAdvanced {
                character_class: wanted,
                job_tier: next.as_u8(),
                max_hp,
            },
        )
        .await;
        // High-water, so the second advancement clears the first's row too if
        // the first somehow never landed.
        self.bump(
            player_id,
            crate::achievement_defs::Trigger::JobTier,
            None,
            u64::from(next.as_u8()),
        )
        .await;
        if next == JobTier::First {
            self.mail_job_starter_kit(auth_service, character_id, wanted)
                .await;
        }
        info!(
            "Character {character_id} advanced to {} ({next:?})",
            wanted.as_str()
        );
    }

    /// A class's starter gear is handed out at creation, which a novice
    /// predates. Post it instead of pushing it into a bag that may be full.
    async fn mail_job_starter_kit(
        &self,
        auth_service: &Arc<AuthService>,
        character_id: i64,
        class: CharacterClass,
    ) {
        let items: Vec<_> = crate::auth::class_starter_items(&class)
            .iter()
            .map(
                |(item_def_id, quantity, _)| onlinerpg_shared::messages::MailAttachment {
                    item_def_id: (*item_def_id).to_string(),
                    quantity: *quantity,
                    enchant: 0,
                },
            )
            .collect();
        if items.is_empty() {
            return;
        }
        self.deliver_mail(
            auth_service,
            character_id,
            super::mail::Letter {
                sender: "Guild Registrar".to_string(),
                subject: format!("Welcome, {}", class.as_str()),
                body: "The tools of your trade.".to_string(),
                gold: 0,
                items,
            },
        )
        .await;
    }

    async fn deny_job(&self, player_id: &PlayerId, reason: JobAdvanceDeniedReason) {
        self.send_direct_message(player_id, ServerMessage::JobAdvanceDenied { reason })
            .await;
    }
}
