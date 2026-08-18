//! Fixed mini-boss spawn points from data-src/world_bosses.csv (IMP-2.7).
//!
//! Deliberately its own table rather than the zone files' `monsterSpawns`:
//! the server never reads those, and a mini boss is not a population quota
//! but one fixed point with a long, jittered respawn. A typo fails the boot
//! for the same reason travel nodes do — a data mistake is cheaper to find
//! there than in front of a player.

use crate::monster_defs::MonsterDefs;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Deserialize)]
pub struct WorldBossSpawn {
    pub id: String,
    #[serde(rename = "monsterId")]
    pub monster_id: String,
    pub x: f32,
    /// Fallback only: the server samples the terrain at spawn time and uses
    /// this when the region is not baked (doc 13 IMP-2.7 revision).
    pub y: f32,
    pub z: f32,
    #[serde(rename = "respawnBaseSecs")]
    pub respawn_base_secs: u64,
    #[serde(rename = "respawnVarianceSecs", default)]
    pub respawn_variance_secs: u64,
}

impl WorldBossSpawn {
    /// Base plus a uniform roll over the variance, in milliseconds. The RNG
    /// is scoped to this call: `thread_rng` is `!Send` and callers await.
    pub fn respawn_delay_ms(&self) -> u64 {
        use rand::Rng;
        let jitter = match self.respawn_variance_secs {
            0 => 0,
            variance => rand::thread_rng().gen_range(0..=variance),
        };
        (self.respawn_base_secs + jitter) * 1000
    }
}

static SPAWNS: LazyLock<Vec<WorldBossSpawn>> = LazyLock::new(|| {
    let by_id: HashMap<String, WorldBossSpawn> =
        serde_json::from_str(include_str!("../../data/world_bosses.json"))
            .expect("Failed to parse world_bosses.json");
    let mut spawns: Vec<WorldBossSpawn> = by_id.into_values().collect();
    spawns.sort_by(|a, b| a.id.cmp(&b.id));
    spawns
});

pub fn world_bosses() -> &'static [WorldBossSpawn] {
    &SPAWNS
}

/// Boot check: every point must name a monster that exists and is a boss.
pub fn assert_spawns_are_valid(monster_defs: &MonsterDefs) {
    if let Err(problem) = validate_spawns(world_bosses(), monster_defs) {
        panic!("{problem}");
    }
    tracing::info!("Loaded {} world boss spawn points", world_bosses().len());
}

/// Separated from the assert so the rules can be tested against data that
/// would otherwise have to ship broken.
fn validate_spawns(spawns: &[WorldBossSpawn], monster_defs: &MonsterDefs) -> Result<(), String> {
    for spawn in spawns {
        if !monster_defs.boss_immune(&spawn.monster_id) {
            return Err(format!(
                "world boss '{}' names '{}', which is not a monster with boss=true",
                spawn.id, spawn.monster_id
            ));
        }
        if spawn.respawn_base_secs == 0 {
            return Err(format!(
                "world boss '{}' has no respawnBaseSecs; a boss that returns at once is ambient",
                spawn.id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn(id: &str, monster_id: &str, base: u64) -> WorldBossSpawn {
        WorldBossSpawn {
            id: id.to_string(),
            monster_id: monster_id.to_string(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            respawn_base_secs: base,
            respawn_variance_secs: 0,
        }
    }

    #[test]
    fn the_shipped_table_is_valid() {
        let defs = MonsterDefs::load();
        validate_spawns(world_bosses(), &defs).expect("world_bosses.csv");
        assert!(
            !world_bosses().is_empty(),
            "the feature is the spawn points; shipping none ships nothing"
        );
    }

    #[test]
    fn a_missing_monster_fails_the_boot() {
        let defs = MonsterDefs::load();
        assert!(validate_spawns(&[spawn("x", "no_such_monster", 60)], &defs).is_err());
    }

    #[test]
    fn an_ordinary_monster_fails_the_boot() {
        let defs = MonsterDefs::load();
        assert!(validate_spawns(&[spawn("x", "orc", 60)], &defs).is_err());
        validate_spawns(&[spawn("x", "orc_boss", 60)], &defs).expect("a boss is accepted");
    }

    #[test]
    fn a_zero_base_wait_is_refused() {
        let defs = MonsterDefs::load();
        assert!(validate_spawns(&[spawn("x", "orc_boss", 0)], &defs).is_err());
    }

    #[test]
    fn the_respawn_delay_stays_inside_base_plus_variance() {
        let mut s = spawn("x", "orc_boss", 1800);
        s.respawn_variance_secs = 600;
        for _ in 0..64 {
            let ms = s.respawn_delay_ms();
            assert!((1_800_000..=2_400_000).contains(&ms), "{ms}");
        }
    }
}
