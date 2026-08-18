use super::*;
use crate::game_state::inventory::{stack_into_bag, BagInsert};

fn insert(
    bag: &mut Vec<ItemInstance>,
    stackable: bool,
    def: &str,
    enchant: i32,
    id: u64,
    qty: u32,
) -> u64 {
    stack_into_bag(
        bag,
        BagInsert {
            stackable,
            item_def_id: def,
            enchant,
            first_instance_id: id,
            quantity: qty,
        },
    )
}

#[test]
fn stack_into_bag_merges_only_same_def_and_enchant() {
    let mut bag = Vec::new();
    insert(&mut bag, true, "apple", 0, 1, 1);
    insert(&mut bag, true, "apple", 0, 2, 1);
    insert(&mut bag, true, "apple", 1, 3, 1);
    insert(&mut bag, false, "torch", 0, 4, 1);
    insert(&mut bag, false, "torch", 0, 5, 1);

    assert_eq!(bag.len(), 4, "merge only the same-def same-enchant pair");
    assert_eq!(bag[0].item_def_id, "apple");
    assert_eq!(bag[0].quantity, 2);
    assert_eq!(bag[1].enchant, 1);
    assert_eq!(bag[1].quantity, 1);
}

/// A non-stackable line never collapses into one multi-unit slot, and the
/// returned count is what batch callers advance their reserved range by.
#[test]
fn stack_into_bag_unfolds_non_stackable_quantities_into_one_slot_each() {
    let mut bag = Vec::new();
    assert_eq!(insert(&mut bag, false, "torch", 0, 100, 3), 3);
    assert_eq!(bag.len(), 3);
    assert!(bag.iter().all(|i| i.quantity == 1));
    assert_eq!(
        bag.iter().map(|i| i.instance_id).collect::<Vec<_>>(),
        vec![100, 101, 102]
    );

    assert_eq!(
        insert(&mut bag, true, "apple", 0, 200, 4),
        1,
        "a new stack takes one id"
    );
    assert_eq!(
        insert(&mut bag, true, "apple", 0, 201, 4),
        0,
        "a merge takes none"
    );
    assert_eq!(
        insert(&mut bag, true, "apple", 0, 202, 0),
        0,
        "an empty line is a no-op"
    );
    let apples: Vec<_> = bag.iter().filter(|i| i.item_def_id == "apple").collect();
    assert_eq!(apples.len(), 1);
    assert_eq!(apples[0].quantity, 8);
}

/// give_item feeds /give, dungeon chest loot, and grilling — repeated grants
/// of a stackable must share one bag entry.
#[tokio::test]
async fn give_item_stacks_repeated_grants() {
    let game_state = make_test_game_state("give_item_stacks");
    game_state.add_player(make_player("eater", 0.0, 0.0)).await;
    {
        let mut inventories = game_state.inventories.write().await;
        inventories.insert(pid("eater"), Default::default());
    }

    assert!(game_state.give_item(&pid("eater"), "grilled_minnow").await);
    assert!(game_state.give_item(&pid("eater"), "grilled_minnow").await);
    assert!(game_state.give_item(&pid("eater"), "fishing_rod").await);
    assert!(game_state.give_item(&pid("eater"), "fishing_rod").await);

    let inventories = game_state.inventories.read().await;
    let bag = &inventories[&pid("eater")].bag;
    let fish: Vec<_> = bag
        .iter()
        .filter(|i| i.item_def_id == "grilled_minnow")
        .collect();
    assert_eq!(fish.len(), 1);
    assert_eq!(fish[0].quantity, 2);
    let rods: Vec<_> = bag
        .iter()
        .filter(|i| i.item_def_id == "fishing_rod")
        .collect();
    assert_eq!(rods.len(), 2, "non-stackables keep their own slots");
}

/// Dropping from a stack must shed exactly one unit — before the stacking fix
/// a whole entry was removed and every unit above the first was destroyed.
#[tokio::test]
async fn dropping_from_a_stack_sheds_one_unit() {
    let game_state = make_test_game_state("drop_stack_unit");
    game_state
        .add_player(make_player("dropper", 0.0, 0.0))
        .await;
    {
        let mut inventories = game_state.inventories.write().await;
        let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
        inv.bag.push(bag_item(11, "apple", 3));
        inventories.insert(pid("dropper"), inv);
    }

    game_state.drop_item(&pid("dropper"), 11).await;

    {
        let inventories = game_state.inventories.read().await;
        let bag = &inventories[&pid("dropper")].bag;
        assert_eq!(bag.len(), 1, "the stack must survive the drop");
        assert_eq!(bag[0].quantity, 2);
        assert_eq!(bag[0].instance_id, 11, "the stack keeps its id");
    }
    {
        let ground_items = game_state.ground_items.read().await;
        assert_eq!(ground_items.len(), 1, "exactly one unit hits the ground");
        let dropped = ground_items.values().next().unwrap();
        assert_eq!(dropped.item.item_def_id, "apple");
        assert_ne!(
            dropped.item.instance_id, 11,
            "the shed unit needs its own id"
        );
    }

    // Draining the stack unit by unit ends with an empty bag, nothing lost.
    game_state.drop_item(&pid("dropper"), 11).await;
    game_state.drop_item(&pid("dropper"), 11).await;
    assert!(game_state.inventories.read().await[&pid("dropper")]
        .bag
        .is_empty());
    assert_eq!(game_state.ground_items.read().await.len(), 3);
}

// ---- EffectiveStats (IMP-0.2) ----------------------------------------------

use onlinerpg_shared::hunger::SATIATION_MAX;
use onlinerpg_shared::CharacterAttributes;

/// What the server resolves against, as one message: guard and attributes
/// with gear folded in, and the carry cap already scaled by the hunger band.
async fn effective(
    game_state: &GameState,
    player_id: &PlayerId,
) -> (i32, CharacterAttributes, f32) {
    match game_state.effective_stats(player_id).await {
        ServerMessage::EffectiveStats {
            guard,
            attributes,
            max_carry_weight,
        } => (guard, attributes, max_carry_weight),
        other => panic!("Expected EffectiveStats, got {:?}", other),
    }
}

async fn make_wearer(game_state: &GameState, name: &str, satiation: u32) -> PlayerId {
    let id = pid(name);
    game_state.add_player(make_player(name, 0.0, 0.0)).await;
    game_state
        .register_player_character(&id, 1, 0, attrs_with_cha(10), 0, Some(satiation))
        .await;
    game_state
        .inventories
        .write()
        .await
        .insert(id, Default::default());
    id
}

/// The bug this item exists for: the ring's CHA reached the haggle band but
/// never the character sheet.
#[tokio::test]
async fn equipped_gear_shows_up_in_the_effective_attributes() {
    let game_state = make_test_game_state("effective_cha");
    let id = make_wearer(&game_state, "wearer", SATIATION_MAX).await;

    let (_, base, _) = effective(&game_state, &id).await;
    assert_eq!(base.cha, 10, "no ring, no bonus");

    game_state
        .inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .equipped
        .insert(
            EquipSlot::Ring,
            ItemInstance {
                instance_id: 1,
                item_def_id: "gold_ring".to_string(),
                quantity: 1,
                enchant: 0,
            },
        );

    let (_, worn, _) = effective(&game_state, &id).await;
    assert_eq!(worn.cha, 11, "gold_ring's cha+1 reaches the sheet");
}

/// The other half of the drift: the client used to compute `str * 15` and
/// ignore the hunger multiplier entirely, showing 150 while the server
/// refused anything past 90.
#[tokio::test]
async fn the_carry_cap_carries_the_hunger_band_with_it() {
    let game_state = make_test_game_state("effective_carry");
    let fed = make_wearer(&game_state, "fed", SATIATION_MAX).await;
    let starving = make_wearer(&game_state, "starving", 0).await;

    let (_, _, fed_cap) = effective(&game_state, &fed).await;
    let (_, _, weak_cap) = effective(&game_state, &starving).await;

    assert_eq!(fed_cap, 150.0, "STR 10 x 15");
    assert_eq!(
        weak_cap,
        150.0 * onlinerpg_shared::hunger::WEAK_CARRY_MULT,
        "and the Weak band scales it"
    );
    assert_eq!(
        weak_cap,
        game_state.max_carry_weight(&starving).await,
        "the number sent is the number enforced"
    );
}

/// Guard rides the same message, so the sheet and combat cannot disagree.
#[tokio::test]
async fn guard_still_matches_what_combat_resolves_against() {
    let game_state = make_test_game_state("effective_guard_msg");
    let id = make_wearer(&game_state, "guarded", SATIATION_MAX).await;
    game_state
        .inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .equipped
        .insert(
            EquipSlot::Chest,
            ItemInstance {
                instance_id: 1,
                item_def_id: "leather_armor".to_string(),
                quantity: 1,
                enchant: 0,
            },
        );

    let (guard, attributes, _) = effective(&game_state, &id).await;
    assert_eq!(guard, game_state.effective_guard(&id).await);
    assert!(guard > i32::from(attrs_with_cha(10).guard), "armor counts");
    assert_eq!(
        i32::from(attributes.guard),
        guard,
        "the attribute block agrees with the dedicated field"
    );
}
