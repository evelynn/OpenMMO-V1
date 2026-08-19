//! The four times one skill use occupies (doc/COMBAT.md, IMP-3.1).
//!
//! Fixed before any skill exists on purpose: authoring skills against times
//! whose meaning is still moving means re-tuning every one of them later.
//! Nothing here is a tick — the server compares deadlines against the clock
//! when a player next tries to act, so 5,000 players cost 5,000 comparisons
//! on use, not a sweep every frame.

use crate::character::{ability_modifier, CharacterAttributes};
use serde::{Deserialize, Serialize};

/// Ceiling on the variable cast a caster may skip, as a percentage. Reachable
/// only with gear on top of attributes: 18/18 stops at 48%, which is the point
/// — a threshold worth building toward, never a free instant cast.
pub const MAX_VCT_REDUCTION_PCT: i32 = 60;

/// One skill's authored times, in milliseconds. `skills.csv` supplies these
/// columns (IMP-3.2); `resolve` turns them into what a given caster pays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CastTiming {
    /// Variable cast: shortened by DEX/INT, cancelled by moving.
    pub vct_ms: u32,
    /// Fixed cast: shortened by nothing, cancelled by nothing.
    pub fct_ms: u32,
    /// After the cast lands: movement and normal attacks are fine, every
    /// skill is refused.
    pub after_cast_delay_ms: u32,
    /// This skill alone is refused; everything else is allowed.
    pub cooldown_ms: u32,
}

/// The deadlines one use produces, as absolute times on the server clock.
/// `delay_ends_at_ms` and `cooldown_ends_at_ms` both count from the moment
/// the cast lands — they run together, they do not queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CastSchedule {
    /// Up to here, moving cancels the cast.
    pub vct_ends_at_ms: u64,
    /// The effect fires here.
    pub cast_ends_at_ms: u64,
    pub delay_ends_at_ms: u64,
    pub cooldown_ends_at_ms: u64,
}

/// How much of the variable cast this caster skips, 0..=`MAX_VCT_REDUCTION_PCT`.
///
/// Deliberately linear in the ability modifier rather than RO's
/// `√[(DEX×2 + INT) ÷ 530]`: over a 3–18 range a square root flattens the
/// whole span into noise, and the point of the stat is that raising it
/// crosses thresholds you can feel.
pub fn vct_reduction_pct(attrs: &CharacterAttributes) -> u32 {
    let raw = (ability_modifier(attrs.dex) * 2 + ability_modifier(attrs.int)) * 4;
    raw.clamp(0, MAX_VCT_REDUCTION_PCT) as u32
}

/// The variable cast this caster actually pays, in milliseconds.
pub fn resolve_cast_ms(base_vct_ms: u32, attrs: &CharacterAttributes) -> u32 {
    let kept = 100 - vct_reduction_pct(attrs);
    ((u64::from(base_vct_ms) * u64::from(kept)) / 100) as u32
}

impl CastTiming {
    /// The same timing with the caster's shortening already applied. Both
    /// sides call this so the cast bar the client draws is the cast the
    /// server is counting.
    pub fn resolve(&self, attrs: &CharacterAttributes) -> CastTiming {
        CastTiming {
            vct_ms: resolve_cast_ms(self.vct_ms, attrs),
            ..*self
        }
    }

    /// Lay this timing out on the clock from the moment casting begins.
    pub fn schedule(&self, started_at_ms: u64, attrs: &CharacterAttributes) -> CastSchedule {
        let resolved = self.resolve(attrs);
        let vct_ends_at_ms = started_at_ms + u64::from(resolved.vct_ms);
        let cast_ends_at_ms = vct_ends_at_ms + u64::from(resolved.fct_ms);
        CastSchedule {
            vct_ends_at_ms,
            cast_ends_at_ms,
            delay_ends_at_ms: cast_ends_at_ms + u64::from(resolved.after_cast_delay_ms),
            cooldown_ends_at_ms: cast_ends_at_ms + u64::from(resolved.cooldown_ms),
        }
    }
}

impl CastSchedule {
    /// Whether moving right now cancels the cast. Only the variable part is
    /// interruptible; past it the caster is committed.
    pub fn cancels_on_move_at(&self, now_ms: u64) -> bool {
        now_ms < self.vct_ends_at_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(dex: u8, int: u8) -> CharacterAttributes {
        CharacterAttributes {
            r#str: 10,
            dex,
            con: 10,
            int,
            wis: 10,
            cha: 10,
            guard: 0,
        }
    }

    #[test]
    fn the_worst_caster_alive_gets_no_shortening() {
        assert_eq!(vct_reduction_pct(&attrs(3, 3)), 0);
        assert_eq!(resolve_cast_ms(2_000, &attrs(3, 3)), 2_000);
    }

    /// The structural claim of IMP-3.1: maxed attributes are a big
    /// shortening but not an instant cast.
    #[test]
    fn maxed_attributes_stop_short_of_the_ceiling() {
        assert_eq!(vct_reduction_pct(&attrs(18, 18)), 48);
        assert_eq!(resolve_cast_ms(2_000, &attrs(18, 18)), 1_040);
    }

    #[test]
    fn the_shortening_is_capped() {
        assert_eq!(
            vct_reduction_pct(&attrs(255, 255)),
            MAX_VCT_REDUCTION_PCT as u32
        );
        assert_eq!(resolve_cast_ms(1_000, &attrs(255, 255)), 400);
    }

    #[test]
    fn only_the_variable_part_is_shortened() {
        let timing = CastTiming {
            vct_ms: 1_000,
            fct_ms: 500,
            after_cast_delay_ms: 300,
            cooldown_ms: 5_000,
        };
        let resolved = timing.resolve(&attrs(18, 18));
        assert_eq!(resolved.vct_ms, 520);
        assert_eq!(resolved.fct_ms, 500);
        assert_eq!(resolved.after_cast_delay_ms, 300);
        assert_eq!(resolved.cooldown_ms, 5_000);
    }

    /// The overlap rule: the delay and the cooldown both start when the cast
    /// lands, so a 5s cooldown is 5s from the hit — not 5s after the delay.
    #[test]
    fn the_delay_and_the_cooldown_start_together() {
        let timing = CastTiming {
            vct_ms: 1_000,
            fct_ms: 500,
            after_cast_delay_ms: 300,
            cooldown_ms: 5_000,
        };
        let s = timing.schedule(10_000, &attrs(10, 10));
        assert_eq!(s.vct_ends_at_ms, 11_000);
        assert_eq!(s.cast_ends_at_ms, 11_500);
        assert_eq!(s.delay_ends_at_ms, 11_800);
        assert_eq!(s.cooldown_ends_at_ms, 16_500);
    }

    #[test]
    fn moving_cancels_the_variable_part_and_nothing_after_it() {
        let timing = CastTiming {
            vct_ms: 1_000,
            fct_ms: 500,
            ..CastTiming::default()
        };
        let s = timing.schedule(0, &attrs(10, 10));
        assert!(s.cancels_on_move_at(999));
        assert!(!s.cancels_on_move_at(1_000), "the fixed part is committed");
        assert!(!s.cancels_on_move_at(1_400));
    }

    /// A skill with no variable part is uncancellable from the first frame.
    #[test]
    fn a_pure_fixed_cast_cannot_be_walked_out_of() {
        let timing = CastTiming {
            fct_ms: 800,
            ..CastTiming::default()
        };
        let s = timing.schedule(0, &attrs(18, 18));
        assert!(!s.cancels_on_move_at(0));
        assert_eq!(s.cast_ends_at_ms, 800);
    }
}
