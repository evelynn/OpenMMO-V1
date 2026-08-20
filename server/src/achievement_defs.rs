//! Achievements from data-src/achievements.csv (IMP-3.7).
//!
//! Evaluation is entirely event-driven: a counter moves and is compared on
//! the spot. Nothing here is ever swept — walking 5,000 players against every
//! achievement on a tick is the one thing the design rules out.

use crate::item_defs::ItemDefs;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

/// What moves a counter. Each maps to exactly one already-existing event in
/// the game; a trigger with nowhere to fire from is refused at boot rather
/// than shipped as an achievement nobody can earn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    MonsterKill,
    FishTrophy,
    DungeonDepth,
    SongPlayed,
    Cook,
    /// Reserved by the schema; the event lives in the housing HTTP routes,
    /// not in `game_state`, so nothing bumps it yet (doc 13 IMP-3.7 revision).
    HouseRoom,
    /// Job advancement (IMP-8.1). High-water: the tier is a position on a
    /// ladder, not something that accumulates.
    JobTier,
}

impl Trigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            Trigger::MonsterKill => "monster_kill",
            Trigger::FishTrophy => "fish_trophy",
            Trigger::DungeonDepth => "dungeon_depth",
            Trigger::SongPlayed => "song_played",
            Trigger::Cook => "cook",
            Trigger::HouseRoom => "house_room",
            Trigger::JobTier => "job_tier",
        }
    }

    /// Whether the counter keeps a running total or the best seen. A depth is
    /// not something you accumulate — reaching floor 3 twice is still floor 3.
    pub fn is_high_water(&self) -> bool {
        matches!(
            self,
            Trigger::DungeonDepth | Trigger::HouseRoom | Trigger::JobTier
        )
    }

    /// Whether anything in the game actually bumps this trigger today.
    fn is_wired(&self) -> bool {
        !matches!(self, Trigger::HouseRoom)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AchievementDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger: Trigger,
    /// Narrows the trigger (a monster id, say). Empty counts everything.
    #[serde(rename = "triggerArg")]
    pub trigger_arg: Option<String>,
    pub threshold: u64,
    /// Title this unlocks; absent when it unlocks only a reward.
    #[serde(rename = "titleId")]
    pub title_id: Option<String>,
    #[serde(rename = "rewardItem")]
    pub reward_item: Option<String>,
    #[serde(rename = "rewardZeny", default)]
    pub reward_zeny: i64,
}

impl AchievementDef {
    /// The counter this achievement reads. Achievements sharing a trigger and
    /// argument share one counter, so "kill 20 orcs" and "kill 50 orcs" cost
    /// one number between them.
    pub fn counter_key(&self) -> String {
        match &self.trigger_arg {
            Some(arg) => format!("{}:{arg}", self.trigger.as_str()),
            None => self.trigger.as_str().to_string(),
        }
    }
}

static DEFS: LazyLock<Vec<AchievementDef>> = LazyLock::new(|| {
    let by_id: HashMap<String, AchievementDef> =
        serde_json::from_str(include_str!("../../data/achievements.json"))
            .expect("Failed to parse achievements.json");
    let mut defs: Vec<AchievementDef> = by_id.into_values().collect();
    // Ascending threshold within a counter, so a single event that crosses
    // several tiers unlocks them in the order a player would expect.
    defs.sort_by(|a, b| {
        a.counter_key()
            .cmp(&b.counter_key())
            .then(a.threshold.cmp(&b.threshold))
            .then(a.id.cmp(&b.id))
    });
    defs
});

pub fn achievement_defs() -> &'static [AchievementDef] {
    &DEFS
}

pub fn achievement_def(id: &str) -> Option<&'static AchievementDef> {
    DEFS.iter().find(|d| d.id == id)
}

/// Every title any achievement can unlock.
pub fn is_known_title(title: &str) -> bool {
    DEFS.iter().any(|d| d.title_id.as_deref() == Some(title))
}

/// Boot check: rewards exist, triggers fire from somewhere, thresholds are
/// reachable.
pub fn assert_achievements_are_valid(item_defs: &ItemDefs) {
    if let Err(problem) = validate_achievements(achievement_defs(), item_defs) {
        panic!("{problem}");
    }
    tracing::info!("Loaded {} achievements", achievement_defs().len());
}

fn validate_achievements(defs: &[AchievementDef], item_defs: &ItemDefs) -> Result<(), String> {
    for def in defs {
        if !def.trigger.is_wired() {
            return Err(format!(
                "achievement '{}' uses trigger '{}', which nothing in the game bumps yet",
                def.id,
                def.trigger.as_str()
            ));
        }
        if def.threshold == 0 {
            return Err(format!(
                "achievement '{}' has threshold 0, so it would unlock before it is played",
                def.id
            ));
        }
        if let Some(item) = &def.reward_item {
            if item_defs.get(item).is_none() {
                return Err(format!(
                    "achievement '{}' rewards '{item}', which is not an item",
                    def.id
                ));
            }
        }
        if def.reward_zeny < 0 {
            return Err(format!("achievement '{}' has a negative reward", def.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, trigger: Trigger, threshold: u64) -> AchievementDef {
        AchievementDef {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            trigger,
            trigger_arg: None,
            threshold,
            title_id: None,
            reward_item: None,
            reward_zeny: 0,
        }
    }

    #[test]
    fn the_shipped_table_is_valid() {
        let items = ItemDefs::load();
        validate_achievements(achievement_defs(), &items).expect("achievements.csv");
        assert!(!achievement_defs().is_empty());
    }

    #[test]
    fn a_reward_that_is_not_an_item_fails_the_boot() {
        let items = ItemDefs::load();
        let mut bad = def("x", Trigger::MonsterKill, 1);
        bad.reward_item = Some("no_such_item".to_string());
        assert!(validate_achievements(&[bad], &items).is_err());
    }

    /// An achievement nobody can earn is worse than no achievement: it shows
    /// in the panel forever at 0.
    #[test]
    fn an_unwired_trigger_fails_the_boot() {
        let items = ItemDefs::load();
        assert!(validate_achievements(&[def("x", Trigger::HouseRoom, 1)], &items).is_err());
    }

    #[test]
    fn a_zero_threshold_fails_the_boot() {
        let items = ItemDefs::load();
        assert!(validate_achievements(&[def("x", Trigger::MonsterKill, 0)], &items).is_err());
    }

    /// Achievements on the same counter must share it, or "kill 20 orcs" and
    /// "kill 50 orcs" would each need their own number.
    #[test]
    fn a_narrowed_trigger_gets_its_own_counter() {
        let mut narrowed = def("orcs", Trigger::MonsterKill, 20);
        narrowed.trigger_arg = Some("orc".to_string());
        assert_eq!(narrowed.counter_key(), "monster_kill:orc");
        assert_eq!(
            def("any", Trigger::MonsterKill, 1).counter_key(),
            "monster_kill"
        );
    }

    #[test]
    fn a_depth_is_a_best_not_a_total() {
        assert!(Trigger::DungeonDepth.is_high_water());
        assert!(!Trigger::MonsterKill.is_high_water());
    }
}
