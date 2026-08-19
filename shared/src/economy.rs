//! Currency sinks (IMP-3.5).
//!
//! Lives in `shared` so the client's preview, the agent's judgement and the
//! server's arithmetic are the same number. A fee the player sees only after
//! the fact is a bug report, not a sink.

/// Trade value below which nothing is charged, in copper.
///
/// Sits above roughly 85% of the item table's `basePrice` values, so ordinary
/// trade is untouched, and below a resident's wallet cap (30,000), so the
/// large batches that actually move currency are the ones that pay.
pub const TRADE_FEE_THRESHOLD: i64 = 10_000;

/// Percent charged on the amount above the threshold.
pub const TRADE_FEE_PCT: i64 = 5;

/// The fee on a trade worth `amount`. Charged on the excess only, so crossing
/// the threshold by a copper costs a copper's worth of fee rather than a
/// cliff.
///
/// The fee is burned, never paid to anyone: routing it to an NPC would make
/// it a transfer instead of a sink, and would hand that NPC a way around its
/// own wallet cap.
pub fn trade_fee(amount: i64) -> i64 {
    let excess = amount - TRADE_FEE_THRESHOLD;
    if excess <= 0 {
        return 0;
    }
    excess * TRADE_FEE_PCT / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_trade_is_free() {
        assert_eq!(trade_fee(0), 0);
        assert_eq!(trade_fee(1), 0);
        assert_eq!(trade_fee(TRADE_FEE_THRESHOLD), 0);
    }

    /// The excess, not the total: the threshold is a starting line, not a
    /// cliff a trade can fall off.
    #[test]
    fn only_the_excess_is_charged() {
        assert_eq!(trade_fee(TRADE_FEE_THRESHOLD + 100), 5);
        assert_eq!(trade_fee(20_000), 500);
        assert_eq!(trade_fee(110_000), 5_000);
    }

    #[test]
    fn a_fee_is_never_negative() {
        assert_eq!(trade_fee(-5_000), 0);
        assert_eq!(trade_fee(i64::MIN / 2), 0);
    }

    /// A fee that could exceed the trade would turn a sale into a debt.
    #[test]
    fn a_fee_never_costs_more_than_the_trade() {
        for amount in [1, 9_999, 10_001, 50_000, 1_000_000] {
            assert!(trade_fee(amount) < amount, "{amount}");
        }
    }
}
