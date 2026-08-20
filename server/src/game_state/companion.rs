//! Companion contracts (IMP-4.5).
//!
//! Hiring an NPC buys a *fact*, not obedience: the server records who is
//! contracted to whom until when, charges the fee up front, and tells both
//! sides. What the NPC does about it is its own agent's decision, taken
//! through the same protocol any player uses — which is the point of
//! constraint (c). No new action was needed on the NPC's side; `follow`
//! already exists.
//!
//! Contracts live in memory. A restart ends every one of them and refunds
//! nothing, because the fee was prepaid — see doc 13 IMP-4.5.

use super::GameState;
use crate::types::PlayerId;
use onlinerpg_shared::ServerMessage;
use std::collections::HashMap;
use tracing::info;

/// Longest contract a single hire may buy. Game hours, and the game clock
/// runs far faster than the wall clock, so this is minutes of real play.
pub const MAX_CONTRACT_HOURS: u8 = 8;

#[derive(Debug, Clone, Copy)]
pub(super) struct CompanionContract {
    pub employer: PlayerId,
    pub expires_at: i64,
}

pub(super) type Contracts = HashMap<PlayerId, CompanionContract>;

impl GameState {
    /// Hire `npc_player_id` for `hours`. Refuses — with a line saying why —
    /// an NPC that is not for hire, one already under contract, a nonsense
    /// duration, or a wallet that cannot cover the fee.
    pub async fn hire_companion(&self, player_id: &PlayerId, npc_player_id: &PlayerId, hours: u8) {
        if hours == 0 || hours > MAX_CONTRACT_HOURS {
            return self
                .send_system_message(
                    player_id,
                    format!("A contract runs 1 to {MAX_CONTRACT_HOURS} hours"),
                )
                .await;
        }
        let Some(def) = self.official_npc_def(npc_player_id).await else {
            return self
                .send_system_message(player_id, "There is nobody there to hire")
                .await;
        };
        if !def.hireable() {
            return self
                .send_system_message(
                    player_id,
                    format!("{} does not take work like that", def.npc_name),
                )
                .await;
        }
        let now = crate::auth::unix_now();
        // Re-checked under the write lock below; this is the cheap refusal
        // that avoids charging anyone.
        if self.companion_contract(npc_player_id).await.is_some() {
            return self
                .send_system_message(
                    player_id,
                    format!("{} is already working for someone", def.npc_name),
                )
                .await;
        }

        let fee = def.hire_rate_per_hour * i64::from(hours);
        if !self.spend_copper(player_id, fee).await {
            return self
                .send_system_message(player_id, format!("You cannot cover the {fee} copper fee"))
                .await;
        }

        let expires_at = now + i64::from(hours) * 3600;
        let taken = {
            let mut contracts = self.companions.write().await;
            // Someone else may have signed them between the check and here.
            if contracts
                .get(npc_player_id)
                .is_some_and(|c| c.expires_at > now)
            {
                true
            } else {
                contracts.insert(
                    *npc_player_id,
                    CompanionContract {
                        employer: *player_id,
                        expires_at,
                    },
                );
                false
            }
        };
        if taken {
            self.award_copper(player_id, fee).await;
            return self
                .send_system_message(
                    player_id,
                    format!("{} was hired a moment before you", def.npc_name),
                )
                .await;
        }

        // The fee is a transfer, not a burn: the NPC salary economy already
        // exists and `walletCap` bounds it. Anything over the cap is lost,
        // exactly as an over-cap salary payment is.
        self.credit_npc_wallet(npc_player_id, fee, def.wallet_cap)
            .await;
        info!(
            "companion: {} hired for {} hour(s) at {} copper",
            def.npc_name, hours, fee
        );
        self.push_companion_contract(npc_player_id, player_id, expires_at)
            .await;
    }

    /// Credit an NPC's own wallet without the player-facing gold popups —
    /// the same clamp `tick_npc_salaries` uses, so a hire cannot push a
    /// wallet past its cap any more than a salary can.
    async fn credit_npc_wallet(&self, npc_player_id: &PlayerId, amount: i64, cap: i64) {
        let mut gold = self.player_gold.write().await;
        let wallet = gold.entry(*npc_player_id).or_insert(0);
        let before = *wallet;
        *wallet = (before + amount).min(cap.max(before));
    }

    pub(super) async fn companion_contract(
        &self,
        npc_player_id: &PlayerId,
    ) -> Option<CompanionContract> {
        let now = crate::auth::unix_now();
        self.companions
            .read()
            .await
            .get(npc_player_id)
            .copied()
            .filter(|c| c.expires_at > now)
    }

    async fn push_companion_contract(
        &self,
        npc_player_id: &PlayerId,
        employer_id: &PlayerId,
        expires_at: i64,
    ) {
        let msg = ServerMessage::CompanionContract {
            npc_player_id: *npc_player_id,
            employer_id: *employer_id,
            expires_at,
        };
        self.send_direct_message(employer_id, msg.clone()).await;
        self.send_direct_message(npc_player_id, msg).await;
    }

    /// Drop expired contracts and tell both sides. Rides the 8-second time
    /// tick rather than adding one — the map holds tens of rows, not one per
    /// player.
    pub async fn tick_companion_contracts(&self) {
        let now = crate::auth::unix_now();
        let expired: Vec<(PlayerId, PlayerId)> = {
            let mut contracts = self.companions.write().await;
            let done: Vec<(PlayerId, PlayerId)> = contracts
                .iter()
                .filter(|(_, c)| c.expires_at <= now)
                .map(|(npc, c)| (*npc, c.employer))
                .collect();
            for (npc, _) in &done {
                contracts.remove(npc);
            }
            done
        };
        for (npc, employer) in expired {
            self.push_companion_contract(&npc, &employer, 0).await;
        }
    }

    /// Either side leaving ends the contract: an NPC cannot follow someone
    /// who logged out, and a contract nobody can see is not a contract.
    pub(crate) async fn end_companion_contracts_for(&self, player_id: &PlayerId) {
        let ended: Vec<(PlayerId, PlayerId)> = {
            let mut contracts = self.companions.write().await;
            let done: Vec<(PlayerId, PlayerId)> = contracts
                .iter()
                .filter(|(npc, c)| *npc == player_id || c.employer == *player_id)
                .map(|(npc, c)| (*npc, c.employer))
                .collect();
            for (npc, _) in &done {
                contracts.remove(npc);
            }
            done
        };
        for (npc, employer) in ended {
            self.push_companion_contract(&npc, &employer, 0).await;
        }
    }
}
