//! Per-character trained skills, separate from `CharacterAttributes`: rolled
//! once vs. grown through play. The XP curve lives here so server, client
//! (wasm) and agent-client share the exact numbers. Levels run 0 (no entry =
//! never trained) to `SKILL_LEVEL_CAP`, the knob unlock ladders stretch
//! against.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const SKILL_LEVEL_CAP: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillId {
    #[serde(rename = "fishing")]
    Fishing,
    #[serde(rename = "power_strike")]
    PowerStrike,
    #[serde(rename = "cleave")]
    Cleave,
    #[serde(rename = "flame_dart")]
    FlameDart,
    #[serde(rename = "trading")]
    Trading,
}

impl SkillId {
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillId::Fishing => "fishing",
            SkillId::PowerStrike => "power_strike",
            SkillId::Cleave => "cleave",
            SkillId::FlameDart => "flame_dart",
            SkillId::Trading => "trading",
        }
    }

    /// Combat skills are bought with skill points and never trained by use;
    /// everything else grows through `add_xp`. The two growth models share
    /// one container (and one DB table), so this is what keeps a stray XP
    /// grant from quietly raising a skill somebody paid points for.
    pub fn is_combat(&self) -> bool {
        match self {
            SkillId::Fishing | SkillId::Trading => false,
            SkillId::PowerStrike | SkillId::Cleave | SkillId::FlameDart => true,
        }
    }

    /// Every skill, for surfaces that have to enumerate them (the skill tree,
    /// the csv boot check).
    pub fn all() -> &'static [SkillId] {
        &[
            SkillId::Fishing,
            SkillId::PowerStrike,
            SkillId::Cleave,
            SkillId::FlameDart,
            SkillId::Trading,
        ]
    }

    /// Player-facing name, shared so every surface capitalizes it the same way.
    pub fn display_name(&self) -> &'static str {
        match self {
            SkillId::Fishing => "Fishing",
            SkillId::PowerStrike => "Power Strike",
            SkillId::Cleave => "Cleave",
            SkillId::FlameDart => "Flame Dart",
            SkillId::Trading => "Trading",
        }
    }
}

impl std::str::FromStr for SkillId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fishing" => Ok(SkillId::Fishing),
            "power_strike" => Ok(SkillId::PowerStrike),
            "cleave" => Ok(SkillId::Cleave),
            "flame_dart" => Ok(SkillId::FlameDart),
            "trading" => Ok(SkillId::Trading),
            _ => Err(()),
        }
    }
}

/// XP required to go from `level - 1` to `level` (level ≥ 1): `100 · level²`.
/// Early levels come fast (level 1 after 100 XP), the last few are a real
/// investment — same feel as the character curve without its doubling.
pub fn skill_xp_cost(level: u32) -> u64 {
    let l = u64::from(level);
    100 * l * l
}

/// Minimum cumulative XP required to hold the given level. Level 0: 0.
/// Closed form of `Σ 100·l²`: `100 · n(n+1)(2n+1)/6`.
pub fn skill_xp_for_level(level: u32) -> u64 {
    let n = u64::from(level.min(SKILL_LEVEL_CAP));
    100 * n * (n + 1) * (2 * n + 1) / 6
}

/// Current skill level from cumulative XP, capped at `SKILL_LEVEL_CAP`.
pub fn skill_level_from_xp(xp: u64) -> u32 {
    let mut level = 0;
    while level < SKILL_LEVEL_CAP && xp >= skill_xp_for_level(level + 1) {
        level += 1;
    }
    level
}

/// Which side of a trade a price adjustment applies to. The caps differ, so
/// the direction has to be named rather than inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSide {
    /// The player is selling: the payout goes up.
    Sell,
    /// The player is buying: the price comes down.
    Buy,
}

/// What a maxed trader gains on a sale, in basis points (+25%).
pub const TRADING_SELL_CAP_BP: i32 = 2_500;
/// What a maxed trader saves on a purchase, in basis points (-15%).
///
/// Lower than the sell cap on purpose: the buy side is the half that opens
/// arbitrage against a counterparty who pays over the odds, so it is the half
/// that has to stay small.
pub const TRADING_BUY_CAP_BP: i32 = 1_500;

/// How far `Trading` moves a merchant's fixed rate at `level`, in basis
/// points. Linear to the cap at `SKILL_LEVEL_CAP`; level 0 moves nothing, so
/// an untrained character trades at exactly today's prices.
///
/// Deliberately capped rather than unbounded: an uncapped multiplier on both
/// sides of a trade is a currency generator, and this game already has a
/// counterparty (`karl`, wishlist 120%) who pays more than base price.
pub fn trade_rate_bonus_bp(level: u32, side: TradeSide) -> i32 {
    let cap = match side {
        TradeSide::Sell => TRADING_SELL_CAP_BP,
        TradeSide::Buy => TRADING_BUY_CAP_BP,
    };
    let level = level.min(SKILL_LEVEL_CAP) as i32;
    cap * level / SKILL_LEVEL_CAP as i32
}

/// `amount` after the trader's skill: a seller receives more, a buyer pays
/// less, and neither ever reaches zero.
pub fn trade_price_with_skill(amount: i64, level: u32, side: TradeSide) -> i64 {
    let bp = i64::from(trade_rate_bonus_bp(level, side));
    let scaled = match side {
        TradeSide::Sell => amount * (10_000 + bp) / 10_000,
        TradeSide::Buy => amount * (10_000 - bp) / 10_000,
    };
    scaled.max(1)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillProgress {
    pub level: u32,
    pub xp: u64,
}

/// Outcome of one XP grant, shaped for the `SkillXpGained` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillXpResult {
    pub xp_amount: u64,
    pub total_xp: u64,
    pub new_level: u32,
    pub leveled_up: bool,
}

/// Every skill a character has trained. Keys are absent until first trained,
/// so a fresh character serializes as an empty map and old save rows load
/// unchanged (`#[serde(default)]` at the embed sites).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Skills {
    pub map: HashMap<SkillId, SkillProgress>,
}

impl Skills {
    /// Progress in `skill`; level 0 / 0 XP when never trained.
    pub fn get(&self, skill: SkillId) -> SkillProgress {
        self.map.get(&skill).copied().unwrap_or_default()
    }

    /// Grant XP, clamping cumulative XP to the cap's threshold so a maxed
    /// skill stops accumulating. Returns `None` when nothing changed
    /// (already at cap), so callers can skip the persist + message.
    pub fn add_xp(&mut self, skill: SkillId, amount: u64) -> Option<SkillXpResult> {
        if skill.is_combat() {
            return None;
        }
        let entry = self.map.entry(skill).or_default();
        let old_xp = entry.xp;
        let old_level = entry.level;
        let new_xp = old_xp
            .saturating_add(amount)
            .min(skill_xp_for_level(SKILL_LEVEL_CAP));
        if new_xp == old_xp {
            return None;
        }
        entry.xp = new_xp;
        entry.level = skill_level_from_xp(new_xp);
        Some(SkillXpResult {
            xp_amount: new_xp - old_xp,
            total_xp: new_xp,
            new_level: entry.level,
            leveled_up: entry.level > old_level,
        })
    }

    /// Raise a combat skill to `level`. Bought with skill points, so the
    /// level moves on its own and the XP field stays at 0 — the same row
    /// shape, a different growth model (IMP-3.2).
    pub fn set_level(&mut self, skill: SkillId, level: u32) {
        let entry = self.map.entry(skill).or_default();
        entry.level = level.min(SKILL_LEVEL_CAP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xp_thresholds_match_per_level_costs() {
        assert_eq!(skill_xp_for_level(0), 0);
        assert_eq!(skill_xp_for_level(1), 100);
        assert_eq!(skill_xp_for_level(2), 500);
        assert_eq!(skill_xp_for_level(3), 1400);
        let mut sum = 0;
        for level in 1..=SKILL_LEVEL_CAP {
            sum += skill_xp_cost(level);
            assert_eq!(skill_xp_for_level(level), sum);
        }
    }

    #[test]
    fn level_from_xp_boundaries() {
        assert_eq!(skill_level_from_xp(0), 0);
        assert_eq!(skill_level_from_xp(99), 0);
        assert_eq!(skill_level_from_xp(100), 1);
        assert_eq!(skill_level_from_xp(499), 1);
        assert_eq!(skill_level_from_xp(500), 2);
        assert_eq!(skill_level_from_xp(u64::MAX), SKILL_LEVEL_CAP);
    }

    #[test]
    fn add_xp_levels_up_and_reports() {
        let mut skills = Skills::default();
        let r = skills.add_xp(SkillId::Fishing, 40).unwrap();
        assert_eq!(r.new_level, 0);
        assert!(!r.leveled_up);
        let r = skills.add_xp(SkillId::Fishing, 60).unwrap();
        assert_eq!(r.new_level, 1);
        assert!(r.leveled_up);
        assert_eq!(r.total_xp, 100);
        assert_eq!(skills.get(SkillId::Fishing).level, 1);
    }

    #[test]
    fn add_xp_clamps_at_cap_and_goes_quiet() {
        let mut skills = Skills::default();
        let cap_xp = skill_xp_for_level(SKILL_LEVEL_CAP);
        let r = skills.add_xp(SkillId::Fishing, u64::MAX).unwrap();
        assert_eq!(r.total_xp, cap_xp);
        assert_eq!(r.new_level, SKILL_LEVEL_CAP);
        assert!(r.leveled_up);
        // A maxed skill reports nothing — no dirty flag, no message.
        assert!(skills.add_xp(SkillId::Fishing, 1).is_none());
    }

    /// A skill goes over the wire and into the database as its serde name,
    /// so `as_str` is that name and not a second spelling of it. A drift
    /// between the two would strand every saved row for that skill.
    #[test]
    fn a_skill_travels_as_the_name_as_str_reports() {
        for skill in SkillId::all() {
            let encoded = rmp_serde::to_vec(skill).expect("encodes");
            let decoded: SkillId = rmp_serde::from_slice(&encoded).expect("round trips");
            assert_eq!(&decoded, skill);
            let name: String = rmp_serde::from_slice(&encoded).expect("is a string on the wire");
            assert_eq!(name, skill.as_str());
            assert_eq!(skill.as_str().parse::<SkillId>(), Ok(*skill));
        }
    }

    #[test]
    fn an_untrained_trader_pays_todays_prices() {
        assert_eq!(trade_rate_bonus_bp(0, TradeSide::Sell), 0);
        assert_eq!(trade_rate_bonus_bp(0, TradeSide::Buy), 0);
        assert_eq!(trade_price_with_skill(1_000, 0, TradeSide::Sell), 1_000);
        assert_eq!(trade_price_with_skill(1_000, 0, TradeSide::Buy), 1_000);
    }

    #[test]
    fn a_maxed_trader_lands_exactly_on_the_caps() {
        assert_eq!(
            trade_rate_bonus_bp(SKILL_LEVEL_CAP, TradeSide::Sell),
            TRADING_SELL_CAP_BP
        );
        assert_eq!(
            trade_rate_bonus_bp(SKILL_LEVEL_CAP, TradeSide::Buy),
            TRADING_BUY_CAP_BP
        );
        assert_eq!(
            trade_price_with_skill(1_000, SKILL_LEVEL_CAP, TradeSide::Sell),
            1_250
        );
        assert_eq!(
            trade_price_with_skill(1_000, SKILL_LEVEL_CAP, TradeSide::Buy),
            850
        );
    }

    /// Past the cap is the cap: an unbounded multiplier on both sides of a
    /// trade is a currency generator.
    #[test]
    fn the_caps_do_not_move_past_the_level_cap() {
        assert_eq!(
            trade_rate_bonus_bp(u32::MAX, TradeSide::Sell),
            TRADING_SELL_CAP_BP
        );
        assert_eq!(
            trade_rate_bonus_bp(u32::MAX, TradeSide::Buy),
            TRADING_BUY_CAP_BP
        );
    }

    #[test]
    fn a_price_never_falls_to_nothing() {
        assert_eq!(
            trade_price_with_skill(1, SKILL_LEVEL_CAP, TradeSide::Buy),
            1
        );
    }

    /// Trading is practised, not bought — unlike the combat skills it shares
    /// a container with.
    #[test]
    fn trading_is_trained_by_use() {
        assert!(!SkillId::Trading.is_combat());
        let mut skills = Skills::default();
        assert!(skills.add_xp(SkillId::Trading, 100).is_some());
        assert_eq!(skills.get(SkillId::Trading).level, 1);
    }

    #[test]
    fn untrained_skill_reads_as_level_zero() {
        let skills = Skills::default();
        assert_eq!(skills.get(SkillId::Fishing), SkillProgress::default());
        // …and an empty map round-trips as an empty map, not a null.
        let json = serde_json::to_string(&skills).unwrap();
        assert_eq!(json, r#"{"map":{}}"#);
    }
}
