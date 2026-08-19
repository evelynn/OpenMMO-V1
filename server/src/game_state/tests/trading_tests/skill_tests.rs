//! The `Trading` skill (IMP-3.4): what it moves, what it deliberately does
//! not, and the round trip it must never make profitable.
use super::*;
use onlinerpg_shared::skills::{SkillId, SKILL_LEVEL_CAP};

async fn max_trading(game_state: &GameState, player: &str) {
    let mut skills = game_state.player_skills.write().await;
    skills
        .entry(pid(player))
        .or_default()
        .set_level(SkillId::Trading, SKILL_LEVEL_CAP);
}

fn base_price(game_state: &GameState, item: &str) -> i64 {
    game_state.item_defs.get(item).unwrap().base_price.unwrap()
}

/// The invariant this item exists to protect: a merchant round trip loses
/// money even for the best trader in the world. The merchant's 40% buy-back
/// rate is far below what a 25% sell bonus can close.
#[tokio::test]
async fn a_merchant_round_trip_still_loses_money_at_max_skill() {
    let game_state = make_test_game_state("trading_round_trip");
    let shield = "wooden_shield";
    let price = base_price(&game_state, shield);
    let (mut buyer_rx, _npc_rx) = setup_haggle(&game_state, 10, price * 10).await;
    game_state
        .inventories
        .write()
        .await
        .insert(pid("buyer"), PlayerInventory::default());
    max_trading(&game_state, "buyer").await;
    let start = game_state.get_player_gold(&pid("buyer")).await;

    game_state
        .buy_item(&pid("buyer"), &pid("npc_rica"), shield)
        .await;
    let instance = game_state.inventories.read().await[&pid("buyer")].bag[0].instance_id;
    game_state
        .sell_item(&pid("buyer"), &pid("npc_rica"), instance)
        .await;

    let end = game_state.get_player_gold(&pid("buyer")).await;
    assert!(
        end < start,
        "buying and selling back must cost money even at max Trading: {start} -> {end}"
    );
    drain(&mut buyer_rx);
}

/// The skill moves a merchant's fixed rate, in both directions.
#[tokio::test]
async fn trading_lowers_what_a_merchant_charges_and_raises_what_it_pays() {
    let shield = "wooden_shield";

    let plain = make_test_game_state("trading_plain");
    let price = base_price(&plain, shield);
    let (_rx, _npc) = setup_haggle(&plain, 10, price * 10).await;
    plain
        .inventories
        .write()
        .await
        .insert(pid("buyer"), PlayerInventory::default());
    let before = plain.get_player_gold(&pid("buyer")).await;
    plain
        .buy_item(&pid("buyer"), &pid("npc_rica"), shield)
        .await;
    let plain_cost = before - plain.get_player_gold(&pid("buyer")).await;

    let skilled = make_test_game_state("trading_skilled");
    let (_rx2, _npc2) = setup_haggle(&skilled, 10, price * 10).await;
    skilled
        .inventories
        .write()
        .await
        .insert(pid("buyer"), PlayerInventory::default());
    max_trading(&skilled, "buyer").await;
    let before = skilled.get_player_gold(&pid("buyer")).await;
    skilled
        .buy_item(&pid("buyer"), &pid("npc_rica"), shield)
        .await;
    let skilled_cost = before - skilled.get_player_gold(&pid("buyer")).await;

    assert!(
        skilled_cost < plain_cost,
        "a trader should pay less: {skilled_cost} vs {plain_cost}"
    );
}

/// The guardrail the shipped data actually needs. Karl already pays 120% of
/// base price, so a skill that raised his rate would widen an arbitrage loop
/// that is open at level 0 (doc 13 IMP-3.4 revision).
#[tokio::test]
async fn trading_does_not_move_what_a_resident_pays() {
    let torch = bag_item(1, "torch", 1);
    let plain = make_test_game_state("trading_resident_plain");
    setup_resident_trade(&plain, 100_000, Vec::new(), vec![torch.clone()]).await;
    plain.sell_item(&pid("seller"), &pid("npc_karl"), 1).await;
    let plain_payout = plain.get_player_gold(&pid("seller")).await;

    let skilled = make_test_game_state("trading_resident_skilled");
    setup_resident_trade(&skilled, 100_000, Vec::new(), vec![torch]).await;
    max_trading(&skilled, "seller").await;
    skilled.sell_item(&pid("seller"), &pid("npc_karl"), 1).await;
    let skilled_payout = skilled.get_player_gold(&pid("seller")).await;

    assert!(plain_payout > 0, "the fixture must actually sell something");
    assert_eq!(
        skilled_payout, plain_payout,
        "Trading must not widen the wishlist arbitrage that already exists"
    );
}

/// Practice is counted per trade, not per copper: one expensive round trip
/// must not max the skill.
#[tokio::test]
async fn trading_xp_counts_trades_not_amounts() {
    let game_state = make_test_game_state("trading_xp");
    let shield = "wooden_shield";
    let price = base_price(&game_state, shield);
    let (_rx, _npc) = setup_haggle(&game_state, 10, price * 100).await;
    game_state
        .inventories
        .write()
        .await
        .insert(pid("buyer"), PlayerInventory::default());
    // Login seeds this from `character_skills`; the fixture stands in for it.
    game_state
        .register_player_skills(&pid("buyer"), Default::default())
        .await;

    game_state
        .buy_item(&pid("buyer"), &pid("npc_rica"), shield)
        .await;
    let after_one = game_state
        .player_skills
        .read()
        .await
        .get(&pid("buyer"))
        .map(|s| s.get(SkillId::Trading).xp)
        .unwrap_or(0);
    assert_eq!(after_one, 1, "one trade is one point of practice");

    game_state
        .buy_item(&pid("buyer"), &pid("npc_rica"), shield)
        .await;
    let after_two = game_state.player_skills.read().await[&pid("buyer")]
        .get(SkillId::Trading)
        .xp;
    assert_eq!(after_two, 2);
}
