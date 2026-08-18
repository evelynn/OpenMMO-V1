// ---- Storage (IMP-2.3) -----------------------------------------------------

use super::*;
use onlinerpg_shared::inventory::STORAGE_SLOTS;

async fn make_storage_user(game_state: &GameState, name: &str, character_id: i64) -> PlayerId {
    let id = pid(name);
    game_state.add_player(make_player(name, 0.0, 0.0)).await;
    game_state
        .register_player_character(&id, character_id, 0, attrs_with_cha(10), 0, Some(500))
        .await;
    game_state
        .inventories
        .write()
        .await
        .insert(id, Default::default());
    id
}

async fn add_storage_npc(game_state: &GameState, name: &str, x: f32) -> PlayerId {
    let mut npc = make_player(name, x, 0.0);
    npc.is_official_npc = true;
    let id = npc.id;
    game_state.add_player(npc).await;
    id
}

async fn give(game_state: &GameState, player_id: &PlayerId, instance_id: u64, def: &str, qty: u32) {
    game_state
        .inventories
        .write()
        .await
        .get_mut(player_id)
        .unwrap()
        .bag
        .push(ItemInstance {
            instance_id,
            item_def_id: def.to_string(),
            quantity: qty,
            enchant: 0,
        });
}

async fn stored(game_state: &GameState, player_id: &PlayerId) -> Vec<Option<ItemInstance>> {
    game_state
        .storages
        .read()
        .await
        .get(player_id)
        .cloned()
        .unwrap_or_default()
}

async fn bag_len(game_state: &GameState, player_id: &PlayerId) -> usize {
    game_state.inventories.read().await[player_id].bag.len()
}

#[tokio::test]
async fn depositing_moves_an_item_out_of_the_bag_and_into_a_slot() {
    let game_state = make_test_game_state("storage_deposit");
    let auth = std::sync::Arc::new(make_test_auth("storage_deposit"));
    let player = make_storage_user(&game_state, "banker", 1).await;
    let npc = add_storage_npc(&game_state, "Rica", 2.0).await;
    give(&game_state, &player, 7, "healing_potion", 3).await;

    game_state.open_storage(&auth, &player, &npc).await;
    game_state.storage_deposit(&player, 7, 3).await;

    assert_eq!(bag_len(&game_state, &player).await, 0, "left the bag");
    let slots = stored(&game_state, &player).await;
    assert_eq!(slots.len(), STORAGE_SLOTS, "the container is slot-capped");
    let held = slots[0].as_ref().expect("landed in the first free slot");
    assert_eq!(
        (held.item_def_id.as_str(), held.quantity),
        ("healing_potion", 3)
    );
}

/// The dupe attempt the spec names: bank the same bag stack twice. The second
/// request finds nothing, because both mutations happened under one guard.
#[tokio::test]
async fn the_same_stack_cannot_be_banked_twice() {
    let game_state = make_test_game_state("storage_dupe");
    let auth = std::sync::Arc::new(make_test_auth("storage_dupe"));
    let player = make_storage_user(&game_state, "duper", 1).await;
    let npc = add_storage_npc(&game_state, "Rica", 2.0).await;
    give(&game_state, &player, 7, "healing_potion", 5).await;

    game_state.open_storage(&auth, &player, &npc).await;
    game_state.storage_deposit(&player, 7, 5).await;
    game_state.storage_deposit(&player, 7, 5).await;

    let total: u32 = stored(&game_state, &player)
        .await
        .iter()
        .flatten()
        .map(|i| i.quantity)
        .sum();
    assert_eq!(total, 5, "five went in, five exist");
    assert_eq!(bag_len(&game_state, &player).await, 0);
}

#[tokio::test]
async fn a_full_container_refuses_the_next_deposit() {
    let game_state = make_test_game_state("storage_full");
    let auth = std::sync::Arc::new(make_test_auth("storage_full"));
    let player = make_storage_user(&game_state, "hoarder", 1).await;
    let npc = add_storage_npc(&game_state, "Rica", 2.0).await;
    game_state.open_storage(&auth, &player, &npc).await;

    // Fill every slot with a non-stackable, then try one more.
    {
        let mut storages = game_state.storages.write().await;
        let slots = storages.get_mut(&player).unwrap();
        for (index, slot) in slots.iter_mut().enumerate() {
            *slot = Some(ItemInstance {
                instance_id: 10_000 + index as u64,
                item_def_id: "worn_iron_sword".to_string(),
                quantity: 1,
                enchant: 0,
            });
        }
    }
    give(&game_state, &player, 7, "worn_iron_sword", 1).await;
    game_state.storage_deposit(&player, 7, 1).await;

    assert_eq!(
        bag_len(&game_state, &player).await,
        1,
        "the refused item stays in the bag"
    );
}

#[tokio::test]
async fn withdrawing_puts_it_back_in_the_bag() {
    let game_state = make_test_game_state("storage_withdraw");
    let auth = std::sync::Arc::new(make_test_auth("storage_withdraw"));
    let player = make_storage_user(&game_state, "taker", 1).await;
    let npc = add_storage_npc(&game_state, "Rica", 2.0).await;
    give(&game_state, &player, 7, "healing_potion", 4).await;

    game_state.open_storage(&auth, &player, &npc).await;
    game_state.storage_deposit(&player, 7, 4).await;
    game_state.storage_withdraw(&player, 0, 4).await;

    assert!(
        stored(&game_state, &player)
            .await
            .iter()
            .all(Option::is_none),
        "the slot emptied"
    );
    assert_eq!(bag_len(&game_state, &player).await, 1);
}

/// Storage has no weight limit; the bag does, and withdrawal is where that
/// finally applies.
#[tokio::test]
async fn a_withdrawal_stops_at_what_you_can_carry() {
    let game_state = make_test_game_state("storage_weight");
    let auth = std::sync::Arc::new(make_test_auth("storage_weight"));
    let player = make_storage_user(&game_state, "packmule", 1).await;
    let npc = add_storage_npc(&game_state, "Rica", 2.0).await;
    game_state.open_storage(&auth, &player, &npc).await;

    let max_weight = game_state.max_carry_weight(&player).await;
    let unit_weight = game_state.item_defs.weight("worn_iron_sword");
    let beyond = (max_weight / unit_weight).ceil() as u32 + 10;
    {
        let mut storages = game_state.storages.write().await;
        storages.get_mut(&player).unwrap()[0] = Some(ItemInstance {
            instance_id: 500,
            item_def_id: "worn_iron_sword".to_string(),
            quantity: beyond,
            enchant: 0,
        });
    }

    game_state.storage_withdraw(&player, 0, beyond).await;

    let carried: u32 = game_state.inventories.read().await[&player]
        .bag
        .iter()
        .map(|i| i.quantity)
        .sum();
    assert!(carried > 0 && carried < beyond, "took what fit: {carried}");
    let left = stored(&game_state, &player).await[0]
        .as_ref()
        .expect("the rest stayed")
        .quantity;
    assert_eq!(carried + left, beyond, "nothing was created or lost");
}

#[tokio::test]
async fn storage_only_opens_next_to_a_townsperson() {
    let game_state = make_test_game_state("storage_range");
    let auth = std::sync::Arc::new(make_test_auth("storage_range"));
    let player = make_storage_user(&game_state, "wanderer", 1).await;
    let far = add_storage_npc(&game_state, "FarRica", 60.0).await;
    let stranger = pid("stranger");
    game_state
        .add_player(make_player("stranger", 1.0, 0.0))
        .await;

    game_state.open_storage(&auth, &player, &far).await;
    game_state.open_storage(&auth, &player, &stranger).await;

    assert!(
        game_state.storages.read().await.get(&player).is_none(),
        "neither distance nor a fellow traveler opens it"
    );
}

/// Walking away closes the session, so a container cannot be worked from
/// across the map.
#[tokio::test]
async fn walking_away_closes_the_container() {
    let game_state = make_test_game_state("storage_walk_away");
    let auth = std::sync::Arc::new(make_test_auth("storage_walk_away"));
    let player = make_storage_user(&game_state, "strider", 1).await;
    let npc = add_storage_npc(&game_state, "Rica", 2.0).await;
    game_state.open_storage(&auth, &player, &npc).await;
    assert!(game_state.open_storages.read().await.contains_key(&player));

    game_state
        .players
        .write()
        .await
        .get_mut(&player)
        .unwrap()
        .position = Position {
        x: 500.0,
        y: 0.0,
        z: 0.0,
    };
    game_state.tick_storage_sessions().await;

    assert!(
        !game_state.open_storages.read().await.contains_key(&player),
        "the session closed itself"
    );
    give(&game_state, &player, 9, "healing_potion", 1).await;
    game_state.storage_deposit(&player, 9, 1).await;
    assert_eq!(
        bag_len(&game_state, &player).await,
        1,
        "and a deposit from afar is refused"
    );
}

/// The container outlives the session: what went in comes back after a
/// logout, through the batch save the spec requires it to join.
#[tokio::test]
async fn stored_items_survive_a_logout() {
    let game_state = make_test_game_state("storage_persist");
    let auth_service = make_test_auth("storage_persist");
    let account = auth_service.login_npc("npc_storage_persist").unwrap();
    let record = auth_service
        .create_character(
            &account,
            "Keeper",
            &attrs_with_cha(10),
            16,
            CharacterClass::Knight,
            Gender::Male,
        )
        .unwrap();
    let auth = std::sync::Arc::new(auth_service);

    let player = make_storage_user(&game_state, "keeper", record.id).await;
    let npc = add_storage_npc(&game_state, "Rica", 2.0).await;
    give(&game_state, &player, 7, "healing_potion", 6).await;
    game_state.open_storage(&auth, &player, &npc).await;
    game_state.storage_deposit(&player, 7, 6).await;

    game_state.flush_dirty_saves(&auth).await;

    let rows = auth.load_storage(record.id).unwrap();
    assert_eq!(rows.len(), 1, "one occupied slot was written");
    assert_eq!((rows[0].0, rows[0].1.quantity), (0, 6));
}
