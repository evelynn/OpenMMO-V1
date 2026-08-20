//! Crafting odds (IMP-4.4).
//!
//! Deliberately the *same* shape as enchanting: basis points out of
//! [`CRAFT_BP_SCALE`], one ladder function, and a floor that never closes.
//! A player who has learned when to stop over-enchanting already knows this
//! rule (09 #19).
//!
//! The formula moves RO's "investment minus greed" onto the 3–18 scale:
//! `base + dex_mod × K + skill_level × M − options × penalty`. Options are
//! the greed — each one aims the result one enchant level higher and pays for
//! it in success chance.

use crate::character::ability_modifier;

/// Odds are basis points out of this — the enchant scale, unchanged.
pub const CRAFT_BP_SCALE: u32 = 10_000;

/// The floor the ladder never drops below, matching enchanting's 1%: a
/// desperate attempt is a bad bet, never an impossible one.
pub const MIN_CRAFT_SUCCESS_BP: u32 = 100;

/// How far above +0 a single craft may aim.
pub const MAX_CRAFT_OPTIONS: u8 = 3;

/// Success chance in basis points for one attempt.
pub fn craft_success_bp(
    base_bp: u32,
    dex: u8,
    dex_k: u32,
    skill_level: u32,
    skill_m: u32,
    options: u8,
    option_penalty_bp: u32,
) -> u32 {
    let base = i64::from(base_bp);
    let from_dex = i64::from(ability_modifier(dex)) * i64::from(dex_k);
    let from_skill = i64::from(skill_level) * i64::from(skill_m);
    let greed = i64::from(options) * i64::from(option_penalty_bp);
    let total = base + from_dex + from_skill - greed;
    total.clamp(i64::from(MIN_CRAFT_SUCCESS_BP), i64::from(CRAFT_BP_SCALE)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dexterity_and_skill_push_the_odds_up_and_greed_pulls_them_down() {
        let at =
            |dex, level, options| craft_success_bp(5_000, dex, 150, level, 120, options, 1_500);
        assert_eq!(at(10, 0, 0), 5_000, "an average hand at base");
        assert_eq!(at(18, 0, 0), 5_000 + 4 * 150, "+4 dex modifier");
        assert_eq!(at(3, 0, 0), 5_000 - 4 * 150, "-4 dex modifier");
        assert_eq!(at(10, 10, 0), 5_000 + 10 * 120, "ten levels of practice");
        assert_eq!(at(10, 0, 2), 5_000 - 2 * 1_500, "two levels of ambition");
    }

    #[test]
    fn the_ladder_closes_at_neither_end() {
        assert_eq!(
            craft_success_bp(9_900, 18, 500, 30, 500, 0, 0),
            CRAFT_BP_SCALE,
            "certainty is the ceiling, not more than certainty"
        );
        assert_eq!(
            craft_success_bp(1_000, 3, 500, 0, 0, 3, 9_000),
            MIN_CRAFT_SUCCESS_BP,
            "the 1% floor holds however greedy the attempt"
        );
    }
}
