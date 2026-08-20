//! Hunting-board contracts (IMP-2.5) and their daily limit (IMP-2.6).
//!
//! Progress is event-driven end to end: a kill advances the counters of the
//! same recipients that share its XP, and nothing walks the player table on a
//! tick. Rewards go out by mail, so a full bag never costs the payout.

use crate::auth::AuthService;
use crate::quest_defs::{QuestKey, MAX_ACCEPTED_QUESTS};
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::messages::{MailAttachment, QuestOffer};
use std::sync::Arc;
use tracing::{info, warn};

use super::auth_db;
use super::mail::Letter;

/// UTC day number. The reset key for the daily limit — deliberately not the
/// game clock, whose day is three real hours (`time.rs`), which would let a
/// "daily" contract run eight times a day.
pub(crate) fn utc_day_key(now_secs: i64) -> i64 {
    now_secs.div_euclid(86_400)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl super::GameState {
    /// Seed a character's accepted contracts at login. Rows kept only for
    /// their daily tally (progress 0) are left out of the runtime vector.
    pub async fn load_quest_progress(&self, player_id: &PlayerId, rows: &[crate::auth::QuestRow]) {
        let mut accepted: Vec<(QuestKey, u16)> = rows
            .iter()
            .filter(|row| row.progress > 0)
            .filter_map(|row| Some((self.quest_defs.key(&row.quest_id)?, row.progress)))
            .collect();
        accepted.truncate(MAX_ACCEPTED_QUESTS);
        if accepted.is_empty() {
            return;
        }
        self.quest_progress
            .write()
            .await
            .insert(*player_id, accepted);
    }

    pub(super) async fn drop_quest_progress(&self, player_id: &PlayerId) {
        self.quest_progress.write().await.remove(player_id);
        self.dirty_quests.write().await.remove(player_id);
    }

    /// One board's contracts with this character's state folded in.
    pub async fn open_quest_board(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        board_id: &str,
    ) {
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth = auth_service.clone();
        let Ok(rows) = auth_db(move || auth.load_quests(character_id)).await else {
            warn!("Quest load failed for character {character_id}");
            return;
        };
        let today = utc_day_key(now_secs());
        let accepted = self.quest_progress.read().await;
        let mine = accepted.get(player_id);

        let quests: Vec<QuestOffer> = self
            .quest_defs
            .on_board(board_id)
            .map(|def| {
                let key = self.quest_defs.key(&def.id);
                let progress = key.and_then(|key| {
                    mine.and_then(|list| {
                        list.iter()
                            .find(|(entry, _)| *entry == key)
                            .map(|(_, progress)| *progress)
                    })
                });
                let spent = rows
                    .iter()
                    .find(|row| row.quest_id == def.id && row.day_key == today)
                    .map_or(0, |row| row.day_count);
                QuestOffer {
                    id: def.id.clone(),
                    name: def.name.clone(),
                    target: def.target_label(),
                    count: def.count,
                    min_level: def.min_level,
                    max_level: def.max_level,
                    reward_xp: def.reward_xp,
                    reward_zeny: def.reward_zeny,
                    reward_item: def.reward_item.clone(),
                    daily_limit: def.daily_limit,
                    daily_remaining: def.daily_limit.saturating_sub(spent),
                    progress,
                }
            })
            .collect();
        drop(accepted);

        self.send_direct_message(
            player_id,
            ServerMessage::QuestBoard {
                board_id: board_id.to_string(),
                quests,
            },
        )
        .await;
    }

    /// Take a contract. Refused outside the level band, past the accept cap,
    /// or when today's completions are already spent.
    pub async fn accept_quest(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        quest_id: &str,
    ) {
        let Some(def) = self.quest_defs.by_id(quest_id) else {
            return;
        };
        let Some(key) = self.quest_defs.key(quest_id) else {
            return;
        };
        let level = {
            let players = self.players.read().await;
            players.get(player_id).map_or(1, |p| p.level)
        };
        if level < def.min_level || level > def.max_level {
            return self
                .send_system_message(
                    player_id,
                    format!(
                        "{} is for levels {}-{}.",
                        def.name, def.min_level, def.max_level
                    ),
                )
                .await;
        }
        if def.daily_limit > 0 && self.daily_remaining(auth_service, player_id, def).await == 0 {
            return self
                .send_system_message(player_id, format!("{} is done for today.", def.name))
                .await;
        }

        {
            let mut progress = self.quest_progress.write().await;
            let list = progress.entry(*player_id).or_default();
            if list.iter().any(|(entry, _)| *entry == key) {
                return;
            }
            if list.len() >= MAX_ACCEPTED_QUESTS {
                drop(progress);
                return self
                    .send_system_message(
                        player_id,
                        format!("You can hold only {MAX_ACCEPTED_QUESTS} contracts."),
                    )
                    .await;
            }
            list.push((key, 0));
        }
        self.mark_quests_dirty(player_id).await;
        self.send_direct_message(
            player_id,
            ServerMessage::QuestAccepted {
                quest_id: quest_id.to_string(),
                count: def.count,
            },
        )
        .await;
    }

    pub async fn abandon_quest(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        quest_id: &str,
    ) {
        let Some(key) = self.quest_defs.key(quest_id) else {
            return;
        };
        {
            let mut progress = self.quest_progress.write().await;
            let Some(list) = progress.get_mut(player_id) else {
                return;
            };
            let before = list.len();
            list.retain(|(entry, _)| *entry != key);
            if list.len() == before {
                return;
            }
        }
        if let Some(character_id) = self.character_id_of(player_id).await {
            let auth = auth_service.clone();
            let quest = quest_id.to_string();
            let _ = auth_db(move || auth.abandon_quest(character_id, &quest)).await;
        }
        self.send_direct_message(
            player_id,
            ServerMessage::QuestCompleted {
                quest_id: quest_id.to_string(),
                rewarded: false,
                daily_remaining: 0,
            },
        )
        .await;
    }

    /// Bank a finished contract: the daily tally advances and the rewards go
    /// out by mail. Refused while the kill count is short or the day is spent.
    pub async fn turn_in_quest(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        quest_id: &str,
    ) {
        let Some(def) = self.quest_defs.by_id(quest_id).cloned() else {
            return;
        };
        let Some(key) = self.quest_defs.key(quest_id) else {
            return;
        };
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };

        let done = {
            let progress = self.quest_progress.read().await;
            progress
                .get(player_id)
                .and_then(|list| list.iter().find(|(entry, _)| *entry == key))
                .is_some_and(|(_, banked)| *banked >= def.count)
        };
        if !done {
            return self
                .send_system_message(player_id, format!("{} is not finished.", def.name))
                .await;
        }
        if def.daily_limit > 0 && self.daily_remaining(auth_service, player_id, &def).await == 0 {
            return self
                .send_system_message(player_id, format!("{} is done for today.", def.name))
                .await;
        }

        // The tally moves first: a crash after this costs the player one
        // payout, while the reverse would let a retry mint rewards.
        let auth = auth_service.clone();
        let quest = quest_id.to_string();
        let today = utc_day_key(now_secs());
        let Ok(spent) = auth_db(move || auth.complete_quest(character_id, &quest, today)).await
        else {
            warn!("Quest completion failed for character {character_id}");
            return;
        };

        self.quest_progress
            .write()
            .await
            .entry(*player_id)
            .or_default()
            .retain(|(entry, _)| *entry != key);
        self.mark_quests_dirty(player_id).await;

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
            Letter {
                sender: "Hunting Board".to_string(),
                subject: format!("{} — contract complete", def.name),
                body: format!("{} {} slain.", def.count, def.target_label()),
                gold: def.reward_zeny,
                items,
            },
        )
        .await;

        if def.reward_xp > 0 {
            self.grant_quest_xp(player_id, def.reward_xp).await;
        }
        self.send_direct_message(
            player_id,
            ServerMessage::QuestCompleted {
                quest_id: quest_id.to_string(),
                rewarded: true,
                daily_remaining: def.daily_limit.saturating_sub(spent),
            },
        )
        .await;
        info!("Character {character_id} completed contract {quest_id}");
    }

    /// Advance every accepted contract that names this monster, for each of
    /// the players that shared the kill's XP. At most `MAX_ACCEPTED_QUESTS`
    /// comparisons per recipient and no allocation on the miss path.
    pub(super) async fn credit_quest_kill(&self, recipients: &[PlayerId], monster_type: &str) {
        let race = self.monster_defs.race_of(monster_type);
        let targets: Vec<(QuestKey, u16)> = self
            .quest_defs
            .iter()
            .enumerate()
            .filter(|(_, def)| def.counts(monster_type, race))
            .map(|(index, def)| (index as QuestKey, def.count))
            .collect();
        if targets.is_empty() {
            return;
        }

        let mut advanced: Vec<(PlayerId, QuestKey, u16, u16)> = Vec::new();
        {
            let mut progress = self.quest_progress.write().await;
            for player_id in recipients {
                let Some(list) = progress.get_mut(player_id) else {
                    continue;
                };
                for (key, banked) in list.iter_mut() {
                    let Some((_, count)) = targets.iter().find(|(target, _)| target == key) else {
                        continue;
                    };
                    if *banked >= *count {
                        continue;
                    }
                    *banked += 1;
                    advanced.push((*player_id, *key, *banked, *count));
                }
            }
        }
        if advanced.is_empty() {
            return;
        }

        for (player_id, key, banked, count) in advanced {
            self.mark_quests_dirty(&player_id).await;
            let Some(def) = self.quest_defs.get(key) else {
                continue;
            };
            self.send_direct_message(
                &player_id,
                ServerMessage::QuestProgress {
                    quest_id: def.id.clone(),
                    progress: banked,
                    count,
                },
            )
            .await;
        }
    }

    /// Contract rows for every dirty player, for the batch save. Same shape as
    /// the skills collector so `flush_dirty_saves` treats them alike.
    pub(super) async fn collect_dirty_quest_states(
        &self,
    ) -> (Vec<PlayerId>, Vec<(i64, Vec<(String, u16)>)>) {
        let dirty: Vec<PlayerId> = {
            let mut dirty = self.dirty_quests.write().await;
            dirty.drain().collect()
        };
        if dirty.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let characters = self.player_characters.read().await;
        let progress = self.quest_progress.read().await;
        let mut rows = Vec::with_capacity(dirty.len());
        for player_id in &dirty {
            let Some((character_id, _, _)) = characters.get(player_id) else {
                continue;
            };
            let list = progress
                .get(player_id)
                .map(|list| {
                    list.iter()
                        .filter_map(|(key, banked)| {
                            Some((self.quest_defs.get(*key)?.id.clone(), *banked))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            rows.push((*character_id, list));
        }
        rows.sort_by_key(|(character_id, _)| *character_id);
        (dirty, rows)
    }

    /// Put the dirty marks back when a save round failed, so the next round
    /// retries instead of dropping progress.
    pub(super) async fn restore_dirty_quests(&self, ids: Vec<PlayerId>) {
        let mut dirty = self.dirty_quests.write().await;
        dirty.extend(ids);
    }

    async fn mark_quests_dirty(&self, player_id: &PlayerId) {
        self.dirty_quests.write().await.insert(*player_id);
    }

    /// Completions left today for one contract. Reads the row rather than
    /// caching it: turn-ins are rare and a stale cache would hand out extras.
    async fn daily_remaining(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        def: &crate::quest_defs::QuestDefinition,
    ) -> u16 {
        if def.daily_limit == 0 {
            return u16::MAX;
        }
        let Some(character_id) = self.character_id_of(player_id).await else {
            return 0;
        };
        let auth = auth_service.clone();
        let Ok(rows) = auth_db(move || auth.load_quests(character_id)).await else {
            return 0;
        };
        let today = utc_day_key(now_secs());
        let spent = rows
            .iter()
            .find(|row| row.quest_id == def.id && row.day_key == today)
            .map_or(0, |row| row.day_count);
        def.daily_limit.saturating_sub(spent)
    }
}
