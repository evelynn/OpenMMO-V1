use super::*;
use onlinerpg_shared::skills::SkillId;

/// old_boot ×2 → leather_belt, 60% base, 1500bp per level of ambition.
const RECIPE: &str = "cut_belt";
const OUTPUT: &str = "leather_belt";
const MATERIAL: &str = "old_boot";
const FEE: i64 = 120;

async fn stage_crafter(
    game_state: &GameState,
    name: &str,
    boots: u32,
    gold: i64,
) -> (PlayerId, DirectRx) {
    let player_id = pid(name);
    game_state.add_player(make_player(name, 0.0, 0.0)).await;
    game_state
        .register_player_character(&player_id, 1, 0, attrs_with_cha(10), gold, None)
        .await;
    {
        let mut inventories = game_state.inventories.write().await;
        let inv = inventories.entry(player_id).or_default();
        for i in 0..boots {
            inv.bag.push(onlinerpg_shared::inventory::ItemInstance {
                instance_id: 1_000 + u64::from(i),
                item_def_id: MATERIAL.to_string(),
                quantity: 1,
                enchant: 0,
            });
        }
    }
    let rx = game_state.register_direct_channel(&player_id).await;
    (player_id, rx)
}

async fn light_a_fire(game_state: &GameState) {
    game_state
        .spawn_campfire(
            Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            0,
            600_000,
        )
        .await;
}

async fn bag_units(game_state: &GameState, player_id: &PlayerId, item_def_id: &str) -> u32 {
    let inventories = game_state.inventories.read().await;
    inventories
        .get(player_id)
        .map(|inv| {
            inv.bag
                .iter()
                .filter(|i| i.item_def_id == item_def_id)
                .map(|i| i.quantity)
                .sum()
        })
        .unwrap_or(0)
}

fn results(msgs: Vec<ServerMessage>) -> Vec<(String, bool, i32)> {
    msgs.into_iter()
        .filter_map(|msg| match msg {
            ServerMessage::CraftResult {
                recipe_id,
                success,
                enchant,
                ..
            } => Some((recipe_id, success, enchant)),
            _ => None,
        })
        .collect()
}

/// Working it yourself needs a fire; without one nothing is spent.
#[tokio::test]
async fn a_self_craft_needs_a_lit_fire() {
    let game_state = make_test_game_state("craft_no_fire");
    let (player_id, mut rx) = stage_crafter(&game_state, "smith", 2, 0).await;

    game_state.craft_item(&player_id, RECIPE, None, 0).await;

    assert!(results(drain(&mut rx)).is_empty());
    assert_eq!(bag_units(&game_state, &player_id, MATERIAL).await, 2);
}

/// A recipe the bag cannot cover takes nothing at all — no half a recipe.
#[tokio::test]
async fn short_materials_are_not_partly_eaten() {
    let game_state = make_test_game_state("craft_short");
    light_a_fire(&game_state).await;
    let (player_id, mut rx) = stage_crafter(&game_state, "smith", 1, 0).await;

    game_state.craft_item(&player_id, RECIPE, None, 0).await;

    assert!(results(drain(&mut rx)).is_empty());
    assert_eq!(
        bag_units(&game_state, &player_id, MATERIAL).await,
        1,
        "the one boot they had is still theirs"
    );
}

/// Whatever the roll says, the materials go — and the piece appears only on
/// a success.
#[tokio::test]
async fn a_self_craft_spends_the_materials_either_way() {
    let game_state = make_test_game_state("craft_roll");
    light_a_fire(&game_state).await;
    let (player_id, mut rx) = stage_crafter(&game_state, "smith", 2, 0).await;

    game_state.craft_item(&player_id, RECIPE, None, 0).await;

    let seen = results(drain(&mut rx));
    assert_eq!(seen.len(), 1);
    let (recipe_id, success, enchant) = seen[0].clone();
    assert_eq!(recipe_id, RECIPE);
    assert_eq!(enchant, 0);
    assert_eq!(bag_units(&game_state, &player_id, MATERIAL).await, 0);
    assert_eq!(
        bag_units(&game_state, &player_id, OUTPUT).await,
        u32::from(success),
        "a failed craft leaves nothing behind"
    );
}

/// Aiming above plain is refused past the cap, before anything is spent.
#[tokio::test]
async fn aiming_past_the_cap_is_refused() {
    let game_state = make_test_game_state("craft_greedy");
    light_a_fire(&game_state).await;
    let (player_id, mut rx) = stage_crafter(&game_state, "smith", 2, 0).await;

    game_state
        .craft_item(
            &player_id,
            RECIPE,
            None,
            onlinerpg_shared::craft::MAX_CRAFT_OPTIONS + 1,
        )
        .await;

    assert!(results(drain(&mut rx)).is_empty());
    assert_eq!(bag_units(&game_state, &player_id, MATERIAL).await, 2);
}

/// A commission cannot fail, charges the fee into the NPC's wallet, and
/// teaches the buyer nothing.
#[tokio::test]
async fn a_commission_always_succeeds_and_teaches_nothing() {
    let game_state = make_test_game_state("craft_commission");
    let mut npc = make_player("Rica", 0.0, 0.0);
    npc.is_official_npc = true;
    let npc_id = npc.id;
    game_state.add_player(npc).await;
    let (player_id, mut rx) = stage_crafter(&game_state, "buyer", 2, 1_000).await;
    game_state
        .register_player_skills(&player_id, Default::default())
        .await;

    game_state
        .craft_item(&player_id, RECIPE, Some(npc_id), 0)
        .await;

    assert_eq!(results(drain(&mut rx)), vec![(RECIPE.to_string(), true, 0)]);
    assert_eq!(bag_units(&game_state, &player_id, OUTPUT).await, 1);
    assert_eq!(game_state.get_player_gold(&player_id).await, 1_000 - FEE);
    assert_eq!(
        game_state.skill_level(&player_id, SkillId::Crafting).await,
        0,
        "paying somebody else teaches you nothing"
    );
}

/// A commission cannot aim high — that is what you gave up for certainty.
#[tokio::test]
async fn a_commission_refuses_ambition() {
    let game_state = make_test_game_state("craft_commission_greedy");
    let mut npc = make_player("Rica", 0.0, 0.0);
    npc.is_official_npc = true;
    let npc_id = npc.id;
    game_state.add_player(npc).await;
    let (player_id, mut rx) = stage_crafter(&game_state, "buyer", 2, 1_000).await;

    game_state
        .craft_item(&player_id, RECIPE, Some(npc_id), 1)
        .await;

    assert!(results(drain(&mut rx)).is_empty());
    assert_eq!(game_state.get_player_gold(&player_id).await, 1_000);
    assert_eq!(bag_units(&game_state, &player_id, MATERIAL).await, 2);
}

/// An NPC without the commissions column does not take the work.
#[tokio::test]
async fn an_npc_that_takes_no_commissions_refuses() {
    let game_state = make_test_game_state("craft_wrong_npc");
    let mut npc = make_player("Karl", 0.0, 0.0);
    npc.is_official_npc = true;
    let npc_id = npc.id;
    game_state.add_player(npc).await;
    let (player_id, mut rx) = stage_crafter(&game_state, "buyer", 2, 1_000).await;

    game_state
        .craft_item(&player_id, RECIPE, Some(npc_id), 0)
        .await;

    assert!(results(drain(&mut rx)).is_empty());
    assert_eq!(game_state.get_player_gold(&player_id).await, 1_000);
}

/// A commission whose materials are missing gives the fee back.
#[tokio::test]
async fn a_commission_refunds_when_the_bag_is_short() {
    let game_state = make_test_game_state("craft_refund");
    let mut npc = make_player("Rica", 0.0, 0.0);
    npc.is_official_npc = true;
    let npc_id = npc.id;
    game_state.add_player(npc).await;
    let (player_id, mut rx) = stage_crafter(&game_state, "buyer", 1, 1_000).await;

    game_state
        .craft_item(&player_id, RECIPE, Some(npc_id), 0)
        .await;

    assert!(results(drain(&mut rx)).is_empty());
    assert_eq!(
        game_state.get_player_gold(&player_id).await,
        1_000,
        "they were paid before the bag was counted, so they hand it back"
    );
    assert_eq!(game_state.get_player_gold(&npc_id).await, 0);
}
