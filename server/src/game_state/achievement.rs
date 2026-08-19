//! Achievements and titles (IMP-3.7).
//!
//! One entry point: `bump`. A counter moves, and every achievement reading
//! that counter is compared right there. Nothing sweeps — walking 5,000
//! players against every achievement on a tick is precisely what the design
//! rules out, and an event already knows which counter it touched.

use crate::achievement_defs::{achievement_def, achievement_defs, is_known_title, Trigger};
use crate::auth::AuthService;
use onlinerpg_shared::messages::MailAttachment;
use onlinerpg_shared::{PlayerId, ServerMessage};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{info, warn};

/// One character's achievement state while they are online.
#[derive(Debug, Default, Clone)]
pub(crate) struct AchievementState {
    pub unlocked: HashSet<String>,
    /// counter key → value. Loaded at entry, written back with the batch save.
    pub counters: HashMap<String, u64>,
    pub active_title: Option<String>,
}

impl super::GameState {
    pub(crate) async fn load_achievements(
        &self,
        player_id: &PlayerId,
        unlocked: Vec<String>,
        counters: Vec<(String, u64)>,
        active_title: Option<String>,
    ) {
        let state = AchievementState {
            unlocked: unlocked.into_iter().collect(),
            counters: counters.into_iter().collect(),
            active_title: active_title.clone(),
        };
        self.achievements.write().await.insert(*player_id, state);
        if active_title.is_some() {
            self.set_player_title(player_id, active_title).await;
        }
    }

    pub(crate) async fn forget_achievements(&self, player_id: &PlayerId) {
        self.achievements.write().await.remove(player_id);
    }

    pub(crate) async fn achievement_list_msg(&self, player_id: &PlayerId) -> ServerMessage {
        let achievements = self.achievements.read().await;
        let state = achievements.get(player_id);
        let mut unlocked: Vec<String> = state
            .map(|s| s.unlocked.iter().cloned().collect())
            .unwrap_or_default();
        unlocked.sort();
        ServerMessage::AchievementList {
            unlocked,
            active_title: state.and_then(|s| s.active_title.clone()),
        }
    }

    pub(crate) async fn active_title(&self, player_id: &PlayerId) -> Option<String> {
        self.achievements
            .read()
            .await
            .get(player_id)
            .and_then(|s| s.active_title.clone())
    }

    /// Counters changed since the last flush, as `(character_id, key, value)`.
    pub(super) async fn dirty_counters(&self) -> Vec<(i64, String, u64)> {
        let dirty: Vec<PlayerId> = self.dirty_counters.write().await.drain().collect();
        if dirty.is_empty() {
            return Vec::new();
        }
        let characters = self.player_characters.read().await;
        let achievements = self.achievements.read().await;
        dirty
            .into_iter()
            .filter_map(|pid| {
                let (character_id, _, _) = characters.get(&pid)?;
                let state = achievements.get(&pid)?;
                Some(
                    state
                        .counters
                        .iter()
                        .map(move |(key, value)| (*character_id, key.clone(), *value)),
                )
            })
            .flatten()
            .collect()
    }

    /// Move `trigger`'s counter and unlock whatever that crosses.
    ///
    /// `arg` narrows the counter (a monster id, say): an event bumps both the
    /// broad counter and the narrow one, so "kill anything" and "kill orcs"
    /// both advance on one orc without either counting the other's kills.
    /// Deliberately takes no `AuthService`: every event that can move a
    /// counter would otherwise have to carry a database handle down the hot
    /// paths (movement, chat, combat). Crossings are queued and drained by a
    /// tick that does have one, the same shape `pending_discovery_saves`
    /// already uses (doc 13 IMP-3.7 revision).
    pub(crate) async fn bump(
        &self,
        player_id: &PlayerId,
        trigger: Trigger,
        arg: Option<&str>,
        amount: u64,
    ) {
        if amount == 0 {
            return;
        }
        let mut keys = vec![trigger.as_str().to_string()];
        if let Some(arg) = arg {
            keys.push(format!("{}:{arg}", trigger.as_str()));
        }

        let crossed = {
            let mut achievements = self.achievements.write().await;
            let Some(state) = achievements.get_mut(player_id) else {
                return;
            };
            let mut crossed: Vec<String> = Vec::new();
            for key in &keys {
                let entry = state.counters.entry(key.clone()).or_insert(0);
                *entry = if trigger.is_high_water() {
                    (*entry).max(amount)
                } else {
                    entry.saturating_add(amount)
                };
                let value = *entry;
                for def in achievement_defs() {
                    if &def.counter_key() != key || value < def.threshold {
                        continue;
                    }
                    if state.unlocked.contains(&def.id) {
                        continue;
                    }
                    // Marked here, under the same lock that moved the counter:
                    // two events landing together must not both mail a reward.
                    state.unlocked.insert(def.id.clone());
                    crossed.push(def.id.clone());
                }
            }
            crossed
        };
        self.dirty_counters.write().await.insert(*player_id);
        if !crossed.is_empty() {
            let mut pending = self.pending_unlocks.write().await;
            pending.extend(crossed.into_iter().map(|id| (*player_id, id)));
        }
    }

    /// Hand out whatever `bump` queued. Runs on the 250 ms hunger tick, so an
    /// unlock is announced within a frame or two of earning it.
    pub async fn tick_achievements(&self, auth_service: &Arc<AuthService>) {
        let pending: Vec<(PlayerId, String)> = {
            let mut queue = self.pending_unlocks.write().await;
            if queue.is_empty() {
                return;
            }
            std::mem::take(&mut *queue)
        };
        for (player_id, achievement_id) in pending {
            self.award_achievement(auth_service, &player_id, &achievement_id)
                .await;
        }
    }

    /// Persist the unlock, mail whatever it carries, and tell the player.
    async fn award_achievement(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        achievement_id: &str,
    ) {
        let Some(def) = achievement_def(achievement_id) else {
            return;
        };
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };

        // The DB row is the durable guard against a double reward: the
        // in-memory set covers this session, this covers a reconnect.
        let auth = Arc::clone(auth_service);
        let id = def.id.clone();
        let first_time =
            tokio::task::spawn_blocking(move || auth.unlock_achievement(character_id, &id))
                .await
                .unwrap_or_else(|e| {
                    warn!("spawn_blocking panicked recording an achievement: {e}");
                    Ok(false)
                })
                .unwrap_or(false);
        if !first_time {
            return;
        }

        if def.reward_zeny > 0 || def.reward_item.is_some() {
            let items = def
                .reward_item
                .as_ref()
                .map(|item| {
                    vec![MailAttachment {
                        item_def_id: item.clone(),
                        quantity: 1,
                        enchant: 0,
                    }]
                })
                .unwrap_or_default();
            self.deliver_mail(
                auth_service,
                character_id,
                super::mail::Letter {
                    sender: "Chronicler".to_string(),
                    subject: def.name.clone(),
                    body: def.description.clone(),
                    gold: def.reward_zeny,
                    items,
                },
            )
            .await;
        }

        info!("Character {character_id} unlocked '{}'", def.id);
        self.send_direct_message(
            player_id,
            ServerMessage::AchievementUnlocked {
                achievement_id: def.id.clone(),
                title: def.title_id.clone(),
            },
        )
        .await;
    }

    /// Show a title, or none. A title the character has not earned is
    /// refused: the client offers only unlocked ones, so anything else is a
    /// crafted message.
    pub(crate) async fn set_title(&self, player_id: &PlayerId, title: Option<String>) {
        let accepted = {
            let mut achievements = self.achievements.write().await;
            let Some(state) = achievements.get_mut(player_id) else {
                return;
            };
            match &title {
                None => true,
                Some(title) => {
                    is_known_title(title)
                        && achievement_defs().iter().any(|def| {
                            def.title_id.as_deref() == Some(title.as_str())
                                && state.unlocked.contains(&def.id)
                        })
                }
            }
        };
        if !accepted {
            warn!("Player {player_id} asked for an unearned title");
            return;
        }
        {
            let mut achievements = self.achievements.write().await;
            if let Some(state) = achievements.get_mut(player_id) {
                state.active_title = title.clone();
            }
        }
        self.mark_dirty(player_id).await;
        self.set_player_title(player_id, title.clone()).await;
        self.send_direct_message(player_id, ServerMessage::TitleSet { title })
            .await;
    }
}
