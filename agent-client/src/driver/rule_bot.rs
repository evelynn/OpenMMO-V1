//! A world bot that plays by rules instead of by prompt (IMP-7.2).
//!
//! The purpose is solo play: one person should be able to party up and run
//! group content, and a beta world should not look deserted. Both wanted the
//! same thing — characters that hunt in the field like players and accept a
//! party invite when one arrives.
//!
//! It is a [`LlmBackend`] rather than a second client, so every part of the
//! agent that already works keeps working: the same action parser, the same
//! movement and approach code, the same party handling. The only difference
//! is who chooses the action, and choosing costs nothing here — no request,
//! no token, no scheduler.
//!
//! The server cannot tell it apart from a person. It sends `ClientMessage`
//! and nothing else, which is constraint (c) and not an accident.

use super::LlmBackend;
use crate::state::SharedState;
use onlinerpg_shared::entity::MonsterState;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Retreat below this fraction of maximum health.
const RETREAT_AT: f32 = 0.35;
/// Resume fighting once healed past this. The gap between the two is what
/// stops a bot flickering between fleeing and attacking on one hit.
const RECOVERED_AT: f32 = 0.7;
/// How far from its post a bot will chase before going home.
const HUNT_RADIUS: f32 = 45.0;
/// A bot ignores monsters this much above its own level — the same reason a
/// player should not fight them, and it keeps bots off the boss.
const MAX_LEVEL_GAP: i32 = 2;

pub struct RuleBot {
    state: Arc<Mutex<SharedState>>,
    /// Where this bot hunts. It wanders around here and returns after a chase.
    post: (f32, f32),
    /// True while retreating, so recovery needs a real margin rather than one
    /// point of health.
    resting: std::sync::atomic::AtomicBool,
}

impl RuleBot {
    pub fn new(state: Arc<Mutex<SharedState>>, post: (f32, f32)) -> Self {
        Self {
            state,
            post,
            resting: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// One decision, as the JSON the action parser already reads. Returning
    /// the same shape an LLM would means the bot cannot reach anything a
    /// player's agent could not.
    async fn decide(&self) -> String {
        use std::sync::atomic::Ordering;
        let state = self.state.lock().await;

        let Some(me) = state.self_player.as_ref() else {
            return wait();
        };
        let health = if me.max_health == 0 {
            1.0
        } else {
            me.health as f32 / me.max_health as f32
        };

        // 1. Stay alive. Nothing below matters if this is true.
        if health <= RETREAT_AT {
            self.resting.store(true, Ordering::Relaxed);
            return move_to(self.post.0, self.post.1);
        }
        if self.resting.load(Ordering::Relaxed) {
            if health < RECOVERED_AT {
                return wait();
            }
            self.resting.store(false, Ordering::Relaxed);
        }

        // 2. In a party, the leader picks the fight. Following them is the
        //    whole reason a solo player brings bots along.
        if let Some(leader) = state.party_leader {
            if Some(leader) != state.self_player_id {
                let leader_name = state
                    .party_members
                    .iter()
                    .find(|m| m.id == leader)
                    .map(|m| m.name.clone());
                if let Some(name) = leader_name {
                    // Fight what the leader is fighting if it is in reach,
                    // otherwise stay with them.
                    if let Some(target) = nearest_target(&state, me, MAX_LEVEL_GAP) {
                        return attack(&target);
                    }
                    return follow(&name);
                }
            }
        }

        // 3. Alone: hunt around the post.
        if let Some(target) = nearest_target(&state, me, MAX_LEVEL_GAP) {
            return attack(&target);
        }

        // 4. Nothing to fight. Go back if the chase went far, else wander.
        let dx = me.position.x - self.post.0;
        let dz = me.position.z - self.post.1;
        if dx * dx + dz * dz > HUNT_RADIUS * HUNT_RADIUS {
            return move_to(self.post.0, self.post.1);
        }
        let (x, z) = wander_from(self.post, me.position.x);
        move_to(x, z)
    }
}

#[async_trait::async_trait]
impl LlmBackend for RuleBot {
    async fn send_message(&self, _content: &str) -> anyhow::Result<String> {
        Ok(self.decide().await)
    }
}

/// The closest living monster worth hitting: alive, on our floor, and not so
/// far above us that a player would leave it alone (IMP-1.1's decay is the
/// same judgement).
///
/// Takes the monsters rather than the whole state so the rule can be tested
/// on its own — the choice is the part worth pinning, not the lookup.
fn pick_target<'a>(
    monsters: impl Iterator<Item = &'a onlinerpg_shared::Monster>,
    me: &onlinerpg_shared::Player,
    max_gap: i32,
) -> Option<String> {
    monsters
        .filter(|m| m.state != MonsterState::Dead && m.health > 0)
        .filter(|m| m.floor_level == me.floor_level)
        // `level_override` is set for dungeon monsters, which scale with
        // depth; a surface monster carries its definition's level and we
        // have no reason to refuse it.
        .filter(|m| match m.level_override {
            Some(level) => i32::from(level) - me.level as i32 <= max_gap,
            None => true,
        })
        .min_by(|a, b| {
            let da = a.position.dist_xz_sq(&me.position);
            let db = b.position.dist_xz_sq(&me.position);
            da.total_cmp(&db)
        })
        .map(|m| m.id.clone())
}

fn nearest_target(
    state: &SharedState,
    me: &onlinerpg_shared::Player,
    max_gap: i32,
) -> Option<String> {
    pick_target(state.nearby_monsters.values(), me, max_gap)
}

/// A spot near the post. Derived from the bot's own X so two bots on one post
/// do not stack, and deterministic so a decision does not depend on a clock.
fn wander_from(post: (f32, f32), seed: f32) -> (f32, f32) {
    let angle = (seed * 0.37).rem_euclid(std::f32::consts::TAU);
    let radius = HUNT_RADIUS * 0.5;
    (post.0 + angle.cos() * radius, post.1 + angle.sin() * radius)
}

fn attack(monster_id: &str) -> String {
    format!(r#"{{"actions": [{{"type": "attack", "target": "{monster_id}"}}]}}"#)
}

fn follow(name: &str) -> String {
    format!(r#"{{"actions": [{{"type": "follow", "target": "{name}"}}]}}"#)
}

fn move_to(x: f32, z: f32) -> String {
    format!(r#"{{"actions": [{{"type": "move", "x": {x:.1}, "z": {z:.1}}}]}}"#)
}

fn wait() -> String {
    r#"{"actions": [{"type": "wait"}]}"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every decision has to survive the parser the LLM path uses, or the bot
    /// would be reaching for actions a player's agent cannot.
    #[test]
    fn every_decision_parses_as_a_real_action() {
        for json in [
            attack("m1_2"),
            follow("Ada"),
            move_to(-1475.5, 4741.5),
            wait(),
        ] {
            let parsed = super::super::action::parse_turn_tolerant(&json)
                .unwrap_or_else(|e| panic!("the parser rejected {json}: {e}"));
            assert!(parsed.errors.is_empty(), "{json}: {:?}", parsed.errors);
            assert_eq!(parsed.actions.len(), 1, "{json}");
        }
    }

    use onlinerpg_shared::{Monster, Player, Position};

    fn at(x: f32) -> Position {
        Position { x, y: 0.0, z: 0.0 }
    }

    fn monster(id: &str, x: f32, floor: i8, level: Option<u8>, health: u32) -> Monster {
        Monster {
            id: id.to_string(),
            monster_type: "goblin".to_string(),
            position: at(x),
            rotation: 0.0,
            state: if health == 0 {
                MonsterState::Dead
            } else {
                MonsterState::Idle
            },
            owner_id: None,
            health,
            max_health: 10,
            floor_level: floor,
            level_override: level,
            aggressive: false,
            lifecycle: onlinerpg_shared::entity::MonsterLifecycle::default(),
            last_attack_at: 0,
            last_move_at: 0,
            move_budget: 0.0,
        }
    }

    fn hunter(level: u32) -> Player {
        Player {
            id: onlinerpg_shared::PlayerId::from(1),
            name: "Bracken".to_string(),
            position: at(0.0),
            rotation: 0.0,
            level,
            health: 20,
            max_health: 20,
            class: onlinerpg_shared::CharacterClass::Knight,
            gender: onlinerpg_shared::Gender::default(),
            is_official_npc: true,
            torch_on: false,
            floor_level: 0,
            object_type: None,
            main_hand: None,
            cosmetics: None,
            object_id: None,
            last_combat_at: 0,
            client_kind: Default::default(),
        }
    }

    /// The nearest live thing on our own floor, and nothing that outclasses
    /// us — the same judgement that keeps a player off the boss.
    #[test]
    fn the_bot_picks_the_nearest_fight_it_should_take() {
        let me = hunter(5);
        let all = [
            monster("far", 30.0, 0, None, 10),
            monster("near", 3.0, 0, None, 10),
            monster("corpse", 1.0, 0, None, 0),
            monster("downstairs", 0.5, -2, None, 10),
            monster("too_strong", 0.6, 0, Some(9), 10),
        ];
        assert_eq!(
            pick_target(all.iter(), &me, MAX_LEVEL_GAP),
            Some("near".to_string()),
            "a corpse, another floor and an overlevelled monster are all skipped"
        );
    }

    #[test]
    fn a_monster_a_little_above_is_still_a_fight() {
        let me = hunter(5);
        let ours = [monster("stretch", 2.0, 0, Some(7), 10)];
        assert_eq!(
            pick_target(ours.iter(), &me, MAX_LEVEL_GAP),
            Some("stretch".to_string())
        );
        let theirs = [monster("boss", 2.0, 0, Some(8), 10)];
        assert_eq!(
            pick_target(theirs.iter(), &me, MAX_LEVEL_GAP),
            None,
            "three levels up is somebody else's problem"
        );
    }
}
