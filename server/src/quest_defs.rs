//! Hunting-board contracts (IMP-2.5). Ids are interned to `u16` at load so the
//! per-player runtime state is `Vec<(u16, u16)>` rather than a map of strings —
//! 5,000 players × 5 accepted contracts has to stay in the tens of kilobytes.

use crate::item_defs::ItemDefs;
use crate::monster_defs::{MonsterDefs, MonsterRace};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

/// How many contracts one character may hold. Bounds both the runtime vector
/// and the per-kill scan.
pub const MAX_ACCEPTED_QUESTS: usize = 5;

#[derive(Debug, Clone, Deserialize)]
pub struct QuestDefinition {
    pub id: String,
    #[serde(rename = "boardId")]
    pub board_id: String,
    pub name: String,
    /// One named monster, or `None` when the contract targets a whole race.
    /// Exactly one of this and `target_race` is set; the boot asserts it.
    #[serde(rename = "monsterId", default)]
    pub monster_id: Option<String>,
    /// A race, counting every monster in it (IMP-8.2). "고블린 10마리"는 몹을
    /// 찍어 주지만 "고블린류 20마리"는 사냥터를 고르게 한다.
    #[serde(rename = "targetRace", default)]
    pub target_race: Option<MonsterRace>,
    pub count: u16,
    #[serde(rename = "minLevel")]
    pub min_level: u32,
    #[serde(rename = "maxLevel")]
    pub max_level: u32,
    #[serde(rename = "rewardXp", default)]
    pub reward_xp: u32,
    #[serde(rename = "rewardZeny", default)]
    pub reward_zeny: i64,
    #[serde(rename = "rewardItem", default)]
    pub reward_item: Option<String>,
    /// Completions allowed per UTC day. 0 = unlimited (IMP-2.6).
    #[serde(rename = "dailyLimit", default)]
    pub daily_limit: u16,
}

impl QuestDefinition {
    /// What to show and to write in the completion letter.
    pub fn target_label(&self) -> String {
        match (&self.monster_id, self.target_race) {
            (Some(id), _) => id.clone(),
            (None, Some(race)) => format!("{} kin", race.as_str()),
            (None, None) => String::new(),
        }
    }

    /// Does this kill count towards the contract?
    pub fn counts(&self, monster_type: &str, race: MonsterRace) -> bool {
        match (&self.monster_id, self.target_race) {
            (Some(id), _) => id == monster_type,
            (None, Some(want)) => want == race,
            (None, None) => false,
        }
    }
}

/// Interned contract id. Index into `QuestDefs::defs`.
pub type QuestKey = u16;

#[derive(Debug, Clone)]
pub struct QuestDefs {
    defs: Arc<Vec<QuestDefinition>>,
    by_id: Arc<HashMap<String, QuestKey>>,
}

impl QuestDefs {
    /// Load and cross-validate: every contract must name a real monster, and a
    /// reward item must exist. A typo fails the boot rather than handing out a
    /// contract nobody can finish or a reward that never arrives.
    pub fn load(monster_defs: &MonsterDefs, item_defs: &ItemDefs) -> Self {
        let data = include_str!("../../data/hunting_quests.json");
        let map: HashMap<String, QuestDefinition> =
            serde_json::from_str(data).expect("Failed to parse hunting_quests.json");

        let mut defs: Vec<QuestDefinition> = map.into_values().collect();
        defs.sort_by(|a, b| a.id.cmp(&b.id));

        info!("Loaded {} hunting contracts", defs.len());
        for def in &defs {
            assert!(
                def.monster_id.is_some() != def.target_race.is_some(),
                "hunting quest '{}' must name exactly one of monsterId and targetRace",
                def.id
            );
            if let Some(monster_id) = &def.monster_id {
                assert!(
                    monster_defs.get(monster_id).is_some(),
                    "hunting quest '{}' targets unknown monster '{monster_id}'",
                    def.id
                );
            }
            // A race contract nobody can finish is the failure mode the race
            // axis was rejected for in the first place (09 #25).
            if let Some(race) = def.target_race {
                assert!(
                    monster_defs.any_of_race(race),
                    "hunting quest '{}' targets race '{}', which no monster has",
                    def.id,
                    race.as_str()
                );
            }
            if let Some(item) = &def.reward_item {
                assert!(
                    item_defs.get(item).is_some(),
                    "hunting quest '{}' rewards unknown item '{item}'",
                    def.id
                );
            }
            assert!(
                def.count > 0,
                "hunting quest '{}' asks for zero kills",
                def.id
            );
            assert!(
                def.min_level <= def.max_level,
                "hunting quest '{}' has an empty level band",
                def.id
            );
            info!(
                "  {} [{}] {} ×{} lv{}~{} xp:{} zeny:{} daily:{}",
                def.id,
                def.board_id,
                def.target_label(),
                def.count,
                def.min_level,
                def.max_level,
                def.reward_xp,
                def.reward_zeny,
                def.daily_limit
            );
        }

        let by_id = defs
            .iter()
            .enumerate()
            .map(|(index, def)| (def.id.clone(), index as QuestKey))
            .collect();

        Self {
            defs: Arc::new(defs),
            by_id: Arc::new(by_id),
        }
    }

    pub fn key(&self, quest_id: &str) -> Option<QuestKey> {
        self.by_id.get(quest_id).copied()
    }

    pub fn get(&self, key: QuestKey) -> Option<&QuestDefinition> {
        self.defs.get(key as usize)
    }

    pub fn by_id(&self, quest_id: &str) -> Option<&QuestDefinition> {
        self.key(quest_id).and_then(|key| self.get(key))
    }

    /// Contracts on one board, in load (id) order.
    pub fn on_board<'a>(&'a self, board_id: &'a str) -> impl Iterator<Item = &'a QuestDefinition> {
        self.defs.iter().filter(move |def| def.board_id == board_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &QuestDefinition> {
        self.defs.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_contracts_reference_real_monsters_and_items() {
        let monster_defs = MonsterDefs::load();
        let item_defs = crate::item_defs::item_defs().clone();
        let quests = QuestDefs::load(&monster_defs, &item_defs);
        assert!(quests.iter().count() > 0);
    }

    #[test]
    fn interning_round_trips() {
        let quests = QuestDefs::load(&MonsterDefs::load(), crate::item_defs::item_defs());
        for def in quests.iter() {
            let key = quests.key(&def.id).expect("id interns");
            assert_eq!(
                quests.get(key).map(|d| d.id.as_str()),
                Some(def.id.as_str())
            );
        }
    }
}
