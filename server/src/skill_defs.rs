//! Combat skill definitions from data-src/skills.csv (IMP-3.2).
//!
//! Every value a skill use is judged against lives here and is read only by
//! the server: cooldown, range, cost and cast times are decisions the client
//! is never allowed to make. The csv is validated at boot for the same reason
//! travel nodes are — an id that does not parse, or an unlock ladder that
//! loops, is a data mistake, and failing to start is the cheapest place to
//! find it.

use onlinerpg_shared::cast::CastTiming;
use onlinerpg_shared::skills::SkillId;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::LazyLock;

/// Who a skill may be pointed at. Only `Enemy` resolves today — the other
/// three are reserved by the schema and refused at boot rather than silently
/// mishandled (doc 13 IMP-3.2 revision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillTarget {
    #[serde(rename = "self")]
    Caster,
    Enemy,
    Ally,
    Ground,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillDef {
    pub id: String,
    pub name: String,
    #[serde(rename = "maxLevel")]
    pub max_level: u32,
    #[serde(rename = "vctMs", default)]
    pub vct_ms: u32,
    #[serde(rename = "fctMs", default)]
    pub fct_ms: u32,
    #[serde(rename = "afterCastDelayMs", default)]
    pub after_cast_delay_ms: u32,
    #[serde(rename = "cooldownMs", default)]
    pub cooldown_ms: u32,
    pub range: f32,
    pub target: SkillTarget,
    #[serde(rename = "costSatiation", default)]
    pub cost_satiation: u32,
    #[serde(rename = "damageDice")]
    pub damage_dice: String,
    #[serde(rename = "damageBonusStat")]
    pub damage_bonus_stat: String,
    /// Player animation clip the client plays; the server only carries it so
    /// the boot log can show what a skill will look like.
    #[serde(rename = "animClip")]
    pub anim_clip: String,
    #[serde(rename = "requiresSkill")]
    pub requires_skill: Option<String>,
    #[serde(rename = "requiresSkillLevel", default)]
    pub requires_skill_level: u32,
}

impl SkillDef {
    /// The four times of IMP-3.1, before the caster's shortening.
    pub fn timing(&self) -> CastTiming {
        CastTiming {
            vct_ms: self.vct_ms,
            fct_ms: self.fct_ms,
            after_cast_delay_ms: self.after_cast_delay_ms,
            cooldown_ms: self.cooldown_ms,
        }
    }

    /// Damage grows with the level bought, so a point spent is felt: each
    /// level past the first adds the skill's bonus stat modifier again.
    pub fn damage_bonus(&self, stat_mod: i32, level: u32) -> i32 {
        stat_mod * level.max(1) as i32
    }
}

static DEFS: LazyLock<Vec<SkillDef>> = LazyLock::new(|| {
    let by_id: HashMap<String, SkillDef> =
        serde_json::from_str(include_str!("../../data/skills.json"))
            .expect("Failed to parse skills.json");
    let mut defs: Vec<SkillDef> = by_id.into_values().collect();
    defs.sort_by(|a, b| a.id.cmp(&b.id));
    defs
});

pub fn skill_defs() -> &'static [SkillDef] {
    &DEFS
}

pub fn skill_def(skill: SkillId) -> Option<&'static SkillDef> {
    DEFS.iter().find(|d| d.id == skill.as_str())
}

/// Boot check: ids parse, prerequisites exist and terminate, and nothing
/// names a target the server cannot resolve.
pub fn assert_skills_are_valid() {
    if let Err(problem) = validate_skills(skill_defs()) {
        panic!("{problem}");
    }
    tracing::info!("Loaded {} combat skills", skill_defs().len());
    for def in skill_defs() {
        tracing::info!(
            "  {} - {} (max {}, {}, clip {})",
            def.id,
            def.name,
            def.max_level,
            def.damage_dice,
            def.anim_clip
        );
    }
}

/// Separated from the assert so the rules can be tested against data that
/// would otherwise have to ship broken.
fn validate_skills(defs: &[SkillDef]) -> Result<(), String> {
    let ids: HashSet<&str> = defs.iter().map(|d| d.id.as_str()).collect();
    for def in defs {
        let Ok(id) = SkillId::from_str(&def.id) else {
            return Err(format!(
                "skill '{}' has no SkillId variant; add one to shared/src/skills.rs",
                def.id
            ));
        };
        if !id.is_combat() {
            return Err(format!(
                "skill '{}' is not a combat skill, so it is bought with skill points by mistake",
                def.id
            ));
        }
        if def.target != SkillTarget::Enemy {
            return Err(format!(
                "skill '{}' targets {:?}, which IMP-3.2 does not resolve yet — only 'enemy'",
                def.id, def.target
            ));
        }
        if def.max_level == 0 {
            return Err(format!("skill '{}' has maxLevel 0", def.id));
        }
        if !matches!(
            def.damage_bonus_stat.as_str(),
            "str" | "dex" | "int" | "wis"
        ) {
            return Err(format!(
                "skill '{}' has damageBonusStat '{}'",
                def.id, def.damage_bonus_stat
            ));
        }
        if let Some(required) = &def.requires_skill {
            if !ids.contains(required.as_str()) {
                return Err(format!(
                    "skill '{}' requires '{required}', which is not a skill",
                    def.id
                ));
            }
            if required == &def.id {
                return Err(format!("skill '{}' requires itself", def.id));
            }
            if def.requires_skill_level == 0 {
                return Err(format!(
                    "skill '{}' names a prerequisite but no level for it",
                    def.id
                ));
            }
        }
    }
    detect_cycle(defs)
}

/// Walk each skill's prerequisite chain. A loop would make both skills
/// permanently unlearnable, which is invisible until a player tries.
fn detect_cycle(defs: &[SkillDef]) -> Result<(), String> {
    let by_id: HashMap<&str, &SkillDef> = defs.iter().map(|d| (d.id.as_str(), d)).collect();
    for def in defs {
        let mut seen: HashSet<&str> = HashSet::from([def.id.as_str()]);
        let mut cursor = def.requires_skill.as_deref();
        while let Some(next) = cursor {
            if !seen.insert(next) {
                return Err(format!(
                    "skill unlock ladder loops at '{next}'; nothing on it can ever be learned"
                ));
            }
            cursor = by_id.get(next).and_then(|d| d.requires_skill.as_deref());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, requires: Option<&str>, requires_level: u32) -> SkillDef {
        SkillDef {
            id: id.to_string(),
            name: id.to_string(),
            max_level: 5,
            vct_ms: 0,
            fct_ms: 300,
            after_cast_delay_ms: 500,
            cooldown_ms: 4_000,
            range: 2.0,
            target: SkillTarget::Enemy,
            cost_satiation: 8,
            damage_dice: "2d6".to_string(),
            damage_bonus_stat: "str".to_string(),
            anim_clip: "slash2".to_string(),
            requires_skill: requires.map(str::to_string),
            requires_skill_level: requires_level,
        }
    }

    #[test]
    fn the_shipped_table_is_valid() {
        validate_skills(skill_defs()).expect("skills.csv");
        assert!(!skill_defs().is_empty(), "a skill system with no skills");
    }

    #[test]
    fn every_shipped_skill_has_a_skill_id() {
        for d in skill_defs() {
            assert!(SkillId::from_str(&d.id).is_ok(), "{}", d.id);
        }
    }

    #[test]
    fn an_unknown_id_fails_the_boot() {
        assert!(validate_skills(&[def("no_such_skill", None, 0)]).is_err());
    }

    #[test]
    fn a_prerequisite_that_is_not_a_skill_fails_the_boot() {
        let problem = validate_skills(&[def("cleave", Some("ghost"), 3)]).expect_err("refused");
        assert!(problem.contains("ghost"), "{problem}");
    }

    /// The failure mode this catches is silent: two skills requiring each
    /// other are simply unlearnable forever.
    #[test]
    fn a_prerequisite_loop_fails_the_boot() {
        let pair = [
            def("power_strike", Some("cleave"), 3),
            def("cleave", Some("power_strike"), 3),
        ];
        let problem = validate_skills(&pair).expect_err("refused");
        assert!(problem.contains("loops"), "{problem}");
    }

    #[test]
    fn a_prerequisite_without_a_level_fails_the_boot() {
        assert!(validate_skills(&[def("cleave", Some("power_strike"), 0)]).is_err());
    }

    #[test]
    fn a_target_the_server_cannot_resolve_fails_the_boot() {
        let mut healer = def("power_strike", None, 0);
        healer.target = SkillTarget::Ally;
        assert!(validate_skills(&[healer]).is_err());
    }

    #[test]
    fn a_level_bought_is_a_level_felt() {
        let d = def("power_strike", None, 0);
        assert_eq!(d.damage_bonus(3, 1), 3);
        assert_eq!(d.damage_bonus(3, 5), 15);
        assert_eq!(
            d.damage_bonus(3, 0),
            3,
            "level 0 is never used, but is safe"
        );
    }
}
