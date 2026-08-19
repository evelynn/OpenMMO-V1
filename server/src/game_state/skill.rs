//! Combat skills (IMP-3.2): learning them, using them, and the four times
//! each use occupies (IMP-3.1).
//!
//! Every judgement is the server's — cooldown, recovery, range, target,
//! resource, cast time. The client sends an intent and draws a bar; it never
//! predicts an outcome. That is not a style preference: agents have no
//! prediction code, so the moment prediction becomes part of how the game
//! plays, humans get an advantage no bot can have.
//!
//! There is no cast tick. A cast that has to land at a time schedules one
//! delayed task, the same way a corpse schedules its own removal; everything
//! else (cooldown, recovery) is compared against the clock when the player
//! next tries to act.

use crate::auth::AuthService;
use crate::game::combat::ability_modifier;
use crate::skill_defs::{skill_def, SkillDef};
use onlinerpg_shared::messages::{SkillCooldown, SkillRejectReason};
use onlinerpg_shared::skills::{skill_level_from_xp, SkillId};
use onlinerpg_shared::{CharacterAttributes, PlayerId, Position, ServerMessage};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::debug;

/// Job progress: XP on its own curve, and the points it has paid out that
/// have not been spent yet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct JobProgress {
    pub xp: u64,
    pub points: u32,
}

/// Everything a use needs from the caster, read once so the checks below do
/// not each take the same locks.
struct Caster {
    level: u32,
    attrs: CharacterAttributes,
    position: Position,
    floor_level: i8,
}

impl super::GameState {
    /// Job XP rides the same kill events as base XP but pays out on the skill
    /// curve, so a skill point is a different pace of reward rather than a
    /// second copy of levelling.
    pub(super) async fn grant_job_xp(&self, recipients: &[PlayerId], amount: u64) {
        if amount == 0 {
            return;
        }
        let mut updates = Vec::with_capacity(recipients.len());
        {
            let mut progress = self.job_progress.write().await;
            for player_id in recipients {
                let entry = progress.entry(*player_id).or_default();
                let before = skill_level_from_xp(entry.xp);
                entry.xp = entry.xp.saturating_add(amount);
                let earned = skill_level_from_xp(entry.xp) - before;
                if earned > 0 {
                    entry.points += earned;
                }
                updates.push((*player_id, *entry));
            }
        }
        self.dirty_players.write().await.extend(recipients);
        for (player_id, job) in updates {
            self.send_direct_message(
                &player_id,
                ServerMessage::SkillPointsUpdate {
                    job_xp: job.xp,
                    skill_points: job.points,
                },
            )
            .await;
        }
    }

    pub(crate) async fn load_job_progress(&self, player_id: &PlayerId, xp: u64, points: u32) {
        self.job_progress
            .write()
            .await
            .insert(*player_id, JobProgress { xp, points });
    }

    pub(crate) async fn forget_job_progress(&self, player_id: &PlayerId) {
        self.job_progress.write().await.remove(player_id);
    }

    /// Spend one point on `skill`. The unlock ladder is checked here rather
    /// than at use time: a skill you cannot learn should never appear learned.
    pub(crate) async fn learn_skill(&self, player_id: &PlayerId, skill: SkillId) {
        let Some(def) = skill_def(skill) else {
            self.reject_skill(player_id, skill, SkillRejectReason::InvalidTarget)
                .await;
            return;
        };
        let current = self.skill_level(player_id, skill).await;
        if current >= def.max_level {
            self.reject_skill(player_id, skill, SkillRejectReason::AlreadyMaxLevel)
                .await;
            return;
        }
        if let Some(required) = &def.requires_skill {
            let Ok(required_id) = required.parse::<SkillId>() else {
                return;
            };
            if self.skill_level(player_id, required_id).await < def.requires_skill_level {
                self.reject_skill(player_id, skill, SkillRejectReason::Locked)
                    .await;
                return;
            }
        }

        let points = {
            let mut progress = self.job_progress.write().await;
            let entry = progress.entry(*player_id).or_default();
            if entry.points == 0 {
                None
            } else {
                entry.points -= 1;
                Some(entry.points)
            }
        };
        let Some(points) = points else {
            self.reject_skill(player_id, skill, SkillRejectReason::NoSkillPoints)
                .await;
            return;
        };

        let level = current + 1;
        {
            let mut skills = self.player_skills.write().await;
            skills
                .entry(*player_id)
                .or_default()
                .set_level(skill, level);
        }
        self.dirty_skills.write().await.insert(*player_id);
        self.dirty_players.write().await.insert(*player_id);
        self.send_direct_message(
            player_id,
            ServerMessage::SkillLearned {
                skill,
                level,
                skill_points: points,
            },
        )
        .await;
    }

    async fn reject_skill(&self, player_id: &PlayerId, skill: SkillId, reason: SkillRejectReason) {
        debug!("Skill {} refused for {player_id}: {reason}", skill.as_str());
        self.send_direct_message(player_id, ServerMessage::SkillRejected { skill, reason })
            .await;
    }

    /// Whether the caster could pay `cost` right now. Read-only: the charge
    /// happens when the skill lands, so a cast walked out of costs nothing.
    async fn has_satiation_for(&self, player_id: &PlayerId, cost: u32) -> bool {
        if cost == 0 {
            return true;
        }
        // No hunger entry means an official NPC, which is exempt from hunger
        // everywhere else too.
        self.hunger
            .read()
            .await
            .get(player_id)
            .is_none_or(|data| data.satiation >= cost)
    }

    /// Charge the cost. False when it could not be paid, in which case
    /// nothing was taken.
    async fn spend_satiation(&self, player_id: &PlayerId, cost: u32) -> bool {
        if cost == 0 {
            return true;
        }
        let msg = {
            let mut hunger = self.hunger.write().await;
            let Some(data) = hunger.get_mut(player_id) else {
                return true;
            };
            if data.satiation < cost {
                return false;
            }
            data.satiation -= cost;
            data.hunger_msg(tokio::time::Instant::now())
        };
        self.send_direct_message(player_id, msg).await;
        true
    }

    /// Remaining cooldowns, for the owner's bar. Expired entries are dropped
    /// as they are read — this is the whole of the "no sweep" promise.
    pub(crate) async fn skill_cooldowns_msg(&self, player_id: &PlayerId) -> ServerMessage {
        let now = Self::now_ms();
        let mut cooldowns = self.skill_cooldowns.write().await;
        let entries = cooldowns.entry(*player_id).or_default();
        entries.retain(|(_, until)| *until > now);
        ServerMessage::SkillCooldowns {
            cooldowns: entries
                .iter()
                .map(|(skill, until)| SkillCooldown {
                    skill: *skill,
                    remaining_ms: until - now,
                })
                .collect(),
        }
    }

    async fn caster(&self, player_id: &PlayerId) -> Option<Caster> {
        let players = self.players.read().await;
        let player = players.get(player_id)?;
        if player.health == 0 {
            return None;
        }
        let (level, position, floor_level) = (player.level, player.position, player.floor_level);
        drop(players);
        let attrs = self
            .player_characters
            .read()
            .await
            .get(player_id)
            .map(|(_, _, attrs)| attrs.clone())?;
        Some(Caster {
            level,
            attrs,
            position,
            floor_level,
        })
    }

    /// Where the target stands, if it is a live monster on the caster's floor.
    async fn skill_target(&self, monster_id: &str, floor_level: i8) -> Option<(Position, String)> {
        let monsters = self.monsters.read().await;
        let monster = monsters.get(monster_id)?;
        if monster.floor_level != floor_level || monster.state == crate::types::MonsterState::Dead {
            return None;
        }
        Some((monster.position, monster.monster_type.clone()))
    }

    /// Start a skill. Every refusal is answered, because a skill that does
    /// nothing and says nothing is indistinguishable from a bug.
    pub async fn use_skill(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        skill: SkillId,
        monster_id: Option<String>,
    ) {
        let Some(def) = skill_def(skill) else {
            self.reject_skill(player_id, skill, SkillRejectReason::InvalidTarget)
                .await;
            return;
        };
        let level = self.skill_level(player_id, skill).await;
        if level == 0 {
            self.reject_skill(player_id, skill, SkillRejectReason::NotLearned)
                .await;
            return;
        }
        let Some(caster) = self.caster(player_id).await else {
            return;
        };

        let now = Self::now_ms();
        if self.casting.read().await.contains_key(player_id) {
            self.reject_skill(player_id, skill, SkillRejectReason::AlreadyCasting)
                .await;
            return;
        }
        if self
            .global_cast_delay_until
            .read()
            .await
            .get(player_id)
            .is_some_and(|until| *until > now)
        {
            self.reject_skill(player_id, skill, SkillRejectReason::Recovering)
                .await;
            return;
        }
        if self
            .skill_cooldowns
            .read()
            .await
            .get(player_id)
            .is_some_and(|entries| {
                entries
                    .iter()
                    .any(|(id, until)| *id == skill && *until > now)
            })
        {
            self.reject_skill(player_id, skill, SkillRejectReason::OnCooldown)
                .await;
            return;
        }

        let Some(monster_id) = monster_id else {
            self.reject_skill(player_id, skill, SkillRejectReason::InvalidTarget)
                .await;
            return;
        };
        let Some((target_position, _)) = self.skill_target(&monster_id, caster.floor_level).await
        else {
            self.reject_skill(player_id, skill, SkillRejectReason::InvalidTarget)
                .await;
            return;
        };
        if caster.position.dist_xz_sq(&target_position) > def.range * def.range {
            self.reject_skill(player_id, skill, SkillRejectReason::OutOfRange)
                .await;
            return;
        }
        // Checked here and again when it lands: the walk-up is what a cast
        // time is for, and a player who ate nothing in between should not
        // find the cost silently waived.
        if !self.has_satiation_for(player_id, def.cost_satiation).await {
            self.reject_skill(player_id, skill, SkillRejectReason::NotEnoughSatiation)
                .await;
            return;
        }

        let schedule = def.timing().schedule(now, &caster.attrs);
        let cast_ms = (schedule.cast_ends_at_ms - now) as u32;
        if cast_ms == 0 {
            self.resolve_skill(auth_service, player_id, skill, &monster_id, level)
                .await;
            return;
        }

        let seq = self.next_cast_seq.fetch_add(1, Ordering::Relaxed);
        self.casting.write().await.insert(
            *player_id,
            super::cast::CastState {
                skill,
                schedule,
                seq,
                target: monster_id.clone(),
                level,
            },
        );
        self.send_direct_message_to_players_within_position(
            &caster.position,
            caster.floor_level,
            super::EVENT_DELIVERY_RADIUS,
            ServerMessage::SkillCastStarted {
                player_id: *player_id,
                skill,
                cast_ms,
            },
            None,
        )
        .await;

        // One delayed task per cast, like the corpse timer — cheaper than a
        // tick that walks every player to find the few who are casting.
        let game_state = self.clone();
        let auth = Arc::clone(auth_service);
        let caster_id = *player_id;
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(u64::from(cast_ms))).await;
            game_state.finish_cast(&auth, &caster_id, seq).await;
        });
        let _ = caster.level;
    }

    /// The cast landed — unless it was cancelled, or replaced by a newer one.
    async fn finish_cast(&self, auth_service: &Arc<AuthService>, player_id: &PlayerId, seq: u64) {
        let landed = {
            let mut casting = self.casting.write().await;
            match casting.get(player_id) {
                Some(state) if state.seq == seq => {
                    let landed = (state.skill, state.target.clone(), state.level);
                    casting.remove(player_id);
                    Some(landed)
                }
                _ => None,
            }
        };
        let Some((skill, monster_id, level)) = landed else {
            return;
        };
        self.resolve_skill(auth_service, player_id, skill, &monster_id, level)
            .await;
    }

    /// Charge the cost, roll the attack, and put the delay and cooldown on
    /// the clock. Reached only by a use that survived every check.
    async fn resolve_skill(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        skill: SkillId,
        monster_id: &str,
        level: u32,
    ) {
        let Some(def) = skill_def(skill) else {
            return;
        };
        let Some(caster) = self.caster(player_id).await else {
            return;
        };
        let Some((target_position, monster_type)) =
            self.skill_target(monster_id, caster.floor_level).await
        else {
            // It died or left while the cast ran. No cost, no cooldown: the
            // skill never went off.
            self.notify_cast_cancelled(player_id, caster.position, caster.floor_level)
                .await;
            return;
        };
        if caster.position.dist_xz_sq(&target_position) > def.range * def.range {
            self.reject_skill(player_id, skill, SkillRejectReason::OutOfRange)
                .await;
            self.notify_cast_cancelled(player_id, caster.position, caster.floor_level)
                .await;
            return;
        }
        if !self.spend_satiation(player_id, def.cost_satiation).await {
            self.reject_skill(player_id, skill, SkillRejectReason::NotEnoughSatiation)
                .await;
            self.notify_cast_cancelled(player_id, caster.position, caster.floor_level)
                .await;
            return;
        }

        let now = Self::now_ms();
        let schedule = def.timing().schedule(now, &caster.attrs);
        self.global_cast_delay_until
            .write()
            .await
            .insert(*player_id, schedule.delay_ends_at_ms);
        {
            let mut cooldowns = self.skill_cooldowns.write().await;
            let entries = cooldowns.entry(*player_id).or_default();
            entries.retain(|(id, until)| *id != skill && *until > now);
            entries.push((skill, schedule.cooldown_ends_at_ms));
        }

        let (hit, damage, monster_position, monster_floor, monster_level) = self
            .roll_skill_attack(def, &caster, &monster_type, monster_id, level)
            .await;

        self.send_direct_message_to_players_within_position(
            &caster.position,
            caster.floor_level,
            super::EVENT_DELIVERY_RADIUS,
            ServerMessage::SkillResult {
                player_id: *player_id,
                skill,
                monster_id: monster_id.to_string(),
                hit,
                damage,
            },
            None,
        )
        .await;
        let cooldowns = self.skill_cooldowns_msg(player_id).await;
        self.send_direct_message(player_id, cooldowns).await;

        if hit {
            self.apply_player_damage_to_monster(
                auth_service,
                player_id,
                monster_id,
                &monster_type,
                monster_position,
                monster_floor,
                monster_level,
                damage,
            )
            .await;
        }
    }

    /// The skill's own dice as `extra_damage_roll` on the ordinary attack
    /// roll: one damage pipeline, so IMP-3.3's defence layer lands on skills
    /// and swings alike without a second place to change.
    async fn roll_skill_attack(
        &self,
        def: &SkillDef,
        caster: &Caster,
        monster_type: &str,
        monster_id: &str,
        level: u32,
    ) -> (bool, u32, Position, i8, Option<u8>) {
        let stat = match def.damage_bonus_stat.as_str() {
            "dex" => caster.attrs.dex,
            "int" => caster.attrs.int,
            "wis" => caster.attrs.wis,
            _ => caster.attrs.r#str,
        };
        let stat_mod = ability_modifier(stat);
        let (monster_position, monster_floor, monster_level, target_guard) = {
            let monsters = self.monsters.read().await;
            match monsters.get(monster_id) {
                Some(m) => (
                    m.position,
                    m.floor_level,
                    m.level_override,
                    self.monster_defs
                        .get(monster_type)
                        .map(|d| i32::from(d.guard))
                        .unwrap_or(10),
                ),
                None => (caster.position, caster.floor_level, None, 10),
            }
        };
        let attack_bonus = crate::game::combat::level_attack_bonus(caster.level) + stat_mod;
        let result = crate::game::combat::roll_attack_with_extra_damage_roll(
            attack_bonus,
            target_guard,
            &def.damage_dice,
            None,
            def.damage_bonus(stat_mod, level),
        );
        (
            result.hit,
            result.damage,
            monster_position,
            monster_floor,
            monster_level,
        )
    }

    /// Give up on a cast in progress. Only the variable part can be given up
    /// on — past it the caster is committed, exactly as moving is.
    pub(crate) async fn cancel_cast(&self, player_id: &PlayerId) {
        let now = Self::now_ms();
        let cancelled = {
            let mut casting = self.casting.write().await;
            match casting.get(player_id) {
                Some(state) if state.schedule.cancels_on_move_at(now) => {
                    casting.remove(player_id);
                    true
                }
                _ => false,
            }
        };
        if cancelled {
            self.announce_cast_cancelled(player_id).await;
        }
    }

    pub(super) async fn announce_cast_cancelled(&self, player_id: &PlayerId) {
        let Some((position, _, floor, _)) = self.player_pose(player_id).await else {
            return;
        };
        self.notify_cast_cancelled(player_id, position, floor).await;
    }

    async fn notify_cast_cancelled(&self, player_id: &PlayerId, position: Position, floor: i8) {
        self.send_direct_message_to_players_within_position(
            &position,
            floor,
            super::EVENT_DELIVERY_RADIUS,
            ServerMessage::SkillCastCancelled {
                player_id: *player_id,
            },
            None,
        )
        .await;
    }
}
