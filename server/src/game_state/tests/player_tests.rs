use super::*;

/// `GameState.players` is a list (numeric ids can't key a wasm-serialized
/// map), so snapshot assertions look their player up by id.
fn find_player(players: &[Player], id: PlayerId) -> &Player {
    players
        .iter()
        .find(|p| p.id == id)
        .expect("player missing from snapshot")
}

#[tokio::test]
async fn replacement_login_kicks_the_previous_account_session() {
    let auth = make_test_auth("account_session_replacement");
    let account = auth.login_npc("npc_account_session").unwrap();
    let game_state = make_test_game_state("account_session_replacement");
    let (first_tx, mut first_rx) = tokio::sync::mpsc::unbounded_channel();
    let first_id = game_state
        .register_account_session(&account, first_tx, &auth)
        .await;

    let (second_tx, _second_rx) = tokio::sync::mpsc::unbounded_channel();
    let second_id = game_state
        .register_account_session(&account, second_tx, &auth)
        .await;

    assert!(matches!(
        first_rx.try_recv(),
        Ok(ServerMessage::Kicked { player_id, .. }) if player_id == PlayerId::from(0)
    ));
    assert!(
        !game_state
            .is_current_account_session(&account, first_id)
            .await
    );
    assert!(
        game_state
            .is_current_account_session(&account, second_id)
            .await
    );

    game_state
        .end_account_session(&account, first_id, &auth)
        .await;
    assert!(
        game_state
            .is_current_account_session(&account, second_id)
            .await
    );
}

#[tokio::test]
async fn equipped_torch_syncs_live_and_late_join_player_state() {
    let game_state = make_test_game_state("late_join_torch_snapshot");
    let torch_holder_id = pid("torch_holder");

    game_state
        .add_player(make_player("torch_holder", 0.0, 0.0))
        .await;
    game_state.inventories.write().await.insert(
        torch_holder_id,
        PlayerInventory {
            bag: vec![bag_item(1, "torch", 1)],
            equipped: Default::default(),
        },
    );

    game_state.equip_item(&torch_holder_id, 1).await;
    assert!(game_state.get_all_players().await[&torch_holder_id].torch_on);

    let snapshot = game_state
        .add_player(make_player("late_joiner", 1.0, 0.0))
        .await
        .into_iter()
        .next()
        .expect("nearby existing player should produce a GameState snapshot");
    match snapshot {
        ServerMessage::GameState { players, .. } => {
            assert!(find_player(&players, torch_holder_id).torch_on);
        }
        other => panic!("expected GameState, got {other:?}"),
    }

    game_state
        .unequip_item(&torch_holder_id, EquipSlot::OffHand)
        .await;

    assert!(!game_state.get_all_players().await[&torch_holder_id].torch_on);
}

#[tokio::test]
async fn equipped_main_hand_syncs_live_and_late_join_player_state() {
    let game_state = make_test_game_state("late_join_main_hand_snapshot");
    let angler_id = pid("angler");

    game_state.add_player(make_player("angler", 0.0, 0.0)).await;
    game_state.inventories.write().await.insert(
        angler_id,
        PlayerInventory {
            bag: vec![bag_item(1, "fishing_rod", 1)],
            equipped: Default::default(),
        },
    );

    game_state.equip_item(&angler_id, 1).await;
    assert_eq!(
        game_state.get_all_players().await[&angler_id].main_hand,
        Some("fishing_rod".to_string())
    );

    let snapshot = game_state
        .add_player(make_player("late_joiner", 1.0, 0.0))
        .await
        .into_iter()
        .next()
        .expect("nearby existing player should produce a GameState snapshot");
    match snapshot {
        ServerMessage::GameState { players, .. } => {
            assert_eq!(
                find_player(&players, angler_id).main_hand.as_deref(),
                Some("fishing_rod")
            );
        }
        other => panic!("expected GameState, got {other:?}"),
    }

    game_state
        .unequip_item(&angler_id, EquipSlot::MainHand)
        .await;

    assert_eq!(
        game_state.get_all_players().await[&angler_id].main_hand,
        None
    );
}

#[tokio::test]
async fn respawn_player_revives_dead_player_only() {
    let game_state = make_test_game_state("respawn_dead");

    let player = Player {
        id: pid("player_dead"),
        name: "DeadPlayer".to_string(),
        position: Position {
            x: 12.0,
            y: 0.0,
            z: -4.0,
        },
        rotation: 1.25,
        level: 3,
        health: 0,
        max_health: 30,
        class: CharacterClass::Knight,
        gender: Gender::default(),
        is_official_npc: false,
        torch_on: false,
        floor_level: 0,
        object_type: None,
        main_hand: None,
        object_id: None,
        last_combat_at: 0,
        client_kind: Default::default(),
    };
    let player_id = player.id;
    game_state.add_player(player).await;

    let mut direct_rx = game_state.register_direct_channel(&player_id).await;
    let mut broadcast_rx = game_state.subscribe();
    game_state.respawn_player(&player_id).await;

    let players = game_state.get_all_players().await;
    let revived = players
        .get(&player_id)
        .expect("Player should still exist after respawn");
    let spawn = &world_config().spawn_position;
    assert_eq!(revived.health, revived.max_health);
    assert_eq!(revived.position.x, spawn.x);
    assert_eq!(revived.position.y, spawn.y);
    assert_eq!(revived.position.z, spawn.z);
    assert_eq!(revived.rotation, spawn.rotation);

    // The spawn point sits within discovery range of a dungeon entrance, so
    // an unseeded respawner may receive DungeonDiscoveries alongside this.
    let respawned = drain(&mut direct_rx)
        .into_iter()
        .find_map(|msg| match msg {
            ServerMessage::PlayerRespawned { player } => Some(player),
            _ => None,
        })
        .expect("Expected direct PlayerRespawned");
    assert_eq!(respawned.id, player_id);
    assert_eq!(respawned.health, respawned.max_health);

    match broadcast_rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        Ok(msg) => {
            let server_msg: ServerMessage =
                rmp_serde::from_slice(&msg.bytes).expect("Failed to deserialize broadcast");
            panic!("Expected no respawn broadcast, got {:?}", server_msg);
        }
        Err(err) => panic!("Expected empty broadcast channel, got {:?}", err),
    }
}

#[tokio::test]
async fn respawn_player_ignores_alive_player() {
    let game_state = make_test_game_state("respawn_alive");

    let player = Player {
        id: pid("player_alive"),
        name: "AlivePlayer".to_string(),
        position: Position {
            x: 5.0,
            y: 0.0,
            z: 6.0,
        },
        rotation: 0.75,
        level: 2,
        health: 18,
        max_health: 20,
        class: CharacterClass::Knight,
        gender: Gender::default(),
        is_official_npc: false,
        torch_on: false,
        floor_level: 0,
        object_type: None,
        main_hand: None,
        object_id: None,
        last_combat_at: 0,
        client_kind: Default::default(),
    };
    let player_id = player.id;
    game_state.add_player(player).await;

    let mut rx = game_state.subscribe();
    game_state.respawn_player(&player_id).await;

    let players = game_state.get_all_players().await;
    let unchanged = players
        .get(&player_id)
        .expect("Player should still exist after ignored respawn");
    assert_eq!(unchanged.health, 18);
    assert_eq!(unchanged.position.x, 5.0);
    assert_eq!(unchanged.position.y, 0.0);
    assert_eq!(unchanged.position.z, 6.0);
    assert_eq!(unchanged.rotation, 0.75);

    match rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        Ok(msg) => {
            let server_msg: ServerMessage =
                rmp_serde::from_slice(&msg.bytes).expect("Failed to deserialize broadcast");
            panic!(
                "Expected no broadcast for alive respawn, got {:?}",
                server_msg
            );
        }
        Err(err) => panic!("Expected empty channel, got {:?}", err),
    }
}

#[tokio::test]
async fn active_character_cannot_be_deleted_from_another_session() {
    let auth = make_test_auth("active_character_delete_guard");
    let account = auth.login_npc("npc_active_delete").unwrap();
    let record = create_test_character(&auth, &account, "StillPlaying");
    let game_state = Arc::new(make_test_game_state("active_character_delete_guard"));
    let player_id = pid("active_character");

    // Deletion must wait for admission, then reject the registered character.
    let admission = game_state.lock_character_sessions().await;
    let deleting_state = Arc::clone(&game_state);
    let deleting_auth = auth.clone();
    let deleting_account = account.clone();
    let character_id = record.id;
    let delete = tokio::spawn(async move {
        deleting_state
            .delete_character_if_inactive(&deleting_auth, &deleting_account, character_id)
            .await
            .unwrap()
    });
    tokio::task::yield_now().await;
    assert!(!delete.is_finished());

    game_state
        .register_player_character(&player_id, record.id, 0, attrs_with_cha(12), 0, None)
        .await;
    drop(admission);

    assert!(!delete.await.unwrap());
    assert!(auth.get_character_for_account(&account, record.id).is_ok());

    game_state.unregister_player_character(&player_id).await;
    assert!(game_state
        .delete_character_if_inactive(&auth, &account, record.id)
        .await
        .unwrap());
    assert!(matches!(
        auth.get_character_for_account(&account, record.id),
        Err(crate::auth::AuthError::CharacterNotFound)
    ));
}

// ---- Save points (IMP-2.2) -------------------------------------------------

/// An official NPC standing at `x`, on `floor`.
async fn add_npc(game_state: &GameState, name: &str, x: f32, floor: i8) -> PlayerId {
    let mut npc = make_player(name, x, 0.0);
    npc.is_official_npc = true;
    npc.floor_level = floor;
    let id = npc.id;
    game_state.add_player(npc).await;
    id
}

async fn saved_point(
    game_state: &GameState,
    player_id: &PlayerId,
) -> Option<crate::auth::SavePoint> {
    game_state.save_points.read().await.get(player_id).copied()
}

#[tokio::test]
async fn a_townsperson_marks_a_respawn_point() {
    let game_state = make_test_game_state("save_point_set");
    let player = pid("traveler");
    game_state
        .add_player(make_player("traveler", 0.0, 0.0))
        .await;
    let npc = add_npc(&game_state, "Rica", 2.0, 0).await;

    game_state.set_save_point(&player, &npc).await;

    let point = saved_point(&game_state, &player).await.expect("set");
    assert_eq!(point.x, 2.0, "the NPC's spot, never a client-sent one");
}

/// Without this gate a player names themselves — distance zero — and saves
/// anywhere on the surface, which is the whole point of the NPC.
#[tokio::test]
async fn a_player_cannot_be_their_own_save_point() {
    let game_state = make_test_game_state("save_point_self");
    let player = pid("selfsaver");
    game_state
        .add_player(make_player("selfsaver", 0.0, 0.0))
        .await;
    let other = pid("bystander");
    game_state
        .add_player(make_player("bystander", 1.0, 0.0))
        .await;

    game_state.set_save_point(&player, &player).await;
    game_state.set_save_point(&player, &other).await;

    assert!(saved_point(&game_state, &player).await.is_none());
}

#[tokio::test]
async fn a_save_point_needs_the_townsperson_within_reach() {
    let game_state = make_test_game_state("save_point_range");
    let player = pid("distant");
    game_state
        .add_player(make_player("distant", 0.0, 0.0))
        .await;
    let npc = add_npc(&game_state, "FarRica", 50.0, 0).await;

    game_state.set_save_point(&player, &npc).await;

    assert!(saved_point(&game_state, &player).await.is_none());
}

/// The floor gate is what stops a save point from becoming a dungeon warp.
#[tokio::test]
async fn a_save_point_cannot_be_set_off_the_surface() {
    let game_state = make_test_game_state("save_point_floor");
    let player = pid("delver");
    let mut body = make_player("delver", 0.0, 0.0);
    body.floor_level = -1;
    game_state.add_player(body).await;
    let npc = add_npc(&game_state, "DeepRica", 2.0, -1).await;

    game_state.set_save_point(&player, &npc).await;

    assert!(saved_point(&game_state, &player).await.is_none());
}

#[tokio::test]
async fn death_returns_a_player_to_their_save_point() {
    let game_state = make_test_game_state("save_point_respawn");
    let player = pid("fallen");
    let mut body = make_player("fallen", 400.0, 0.0);
    game_state.add_player(body.clone()).await;
    let npc = add_npc(&game_state, "HomeRica", 402.0, 0).await;
    game_state.set_save_point(&player, &npc).await;

    // Die somewhere else entirely, so the revive position can only come from
    // the save point.
    {
        let mut players = game_state.players.write().await;
        let stored = players.get_mut(&player).expect("player");
        stored.health = 0;
        stored.position = Position {
            x: -900.0,
            y: 0.0,
            z: -900.0,
        };
    }
    body.health = 0;

    game_state.respawn_player(&player).await;

    let revived = game_state.players.read().await[&player].position;
    assert_eq!(
        revived.x, 402.0,
        "back at the townsperson, not the world spawn"
    );
    assert_ne!(
        revived.x,
        crate::world_config::world_config().spawn_position.x,
        "and the fixture would otherwise be indistinguishable"
    );
}

/// Decision #7: a character that never set one keeps today's behavior.
#[tokio::test]
async fn death_without_a_save_point_still_uses_the_world_spawn() {
    let game_state = make_test_game_state("save_point_default");
    let player = pid("unsaved");
    let mut body = make_player("unsaved", 400.0, 400.0);
    body.health = 0;
    game_state.add_player(body).await;

    game_state.respawn_player(&player).await;

    let spawn = &crate::world_config::world_config().spawn_position;
    let revived = game_state.players.read().await[&player].position;
    assert_eq!((revived.x, revived.z), (spawn.x, spawn.z));
}

/// The regression that in-game data caught: the starting town is built over
/// old_crypt's 80x80 footprint, so a footprint gate refused every NPC in the
/// only town that has any. The surface check is what actually guards this.
#[tokio::test]
async fn a_townsperson_standing_over_a_dungeon_can_still_mark_a_point() {
    let game_state = make_test_game_state("save_point_over_dungeon");
    let crypt = onlinerpg_shared::dungeon::entrances()
        .first()
        .expect("at least one dungeon");
    // Where Rica actually stands: inside the footprint, 26m from the mouth.
    let npc_x = crypt.x - 23.3;
    let npc_z = crypt.z + 12.8;
    assert!(
        onlinerpg_shared::dungeon::entrance_at(npc_x, npc_z).is_some(),
        "the fixture must sit in the footprint or it tests nothing"
    );

    let player = pid("townie");
    game_state
        .add_player(make_player("townie", npc_x - 1.0, npc_z))
        .await;
    let npc = add_npc(&game_state, "TownRica", npc_x, 0).await;
    game_state
        .players
        .write()
        .await
        .get_mut(&npc)
        .unwrap()
        .position
        .z = npc_z;

    game_state.set_save_point(&player, &npc).await;

    assert!(
        saved_point(&game_state, &player).await.is_some(),
        "a town built over a crypt is still a town"
    );
}

// ---- Paid travel (IMP-2.4) -------------------------------------------------

async fn travel_ready(game_state: &GameState, name: &str, gold: i64) -> (PlayerId, PlayerId) {
    let player = pid(name);
    let mut body = make_player(name, 0.0, 0.0);
    // Past every node's minLevel, so the level gate is not what these
    // fixtures are exercising.
    body.level = 20;
    game_state.add_player(body).await;
    game_state
        .register_player_character(&player, 1, 0, attrs_with_cha(10), gold, Some(500))
        .await;
    let npc = add_npc(game_state, &format!("{name}_agent"), 2.0, 0).await;
    (player, npc)
}

async fn purse(game_state: &GameState, player_id: &PlayerId) -> i64 {
    game_state
        .player_gold
        .read()
        .await
        .get(player_id)
        .copied()
        .unwrap_or(0)
}

fn paid_node() -> &'static crate::travel_defs::TravelNode {
    crate::travel_defs::travel_nodes()
        .iter()
        .find(|n| n.fare > 0)
        .expect("one paid destination")
}

#[tokio::test]
async fn paying_the_fare_moves_the_player_and_burns_the_money() {
    let game_state = make_test_game_state("travel_paid");
    let node = paid_node();
    let (player, npc) = travel_ready(&game_state, "traveler", node.fare + 100).await;

    let before: i64 = game_state.player_gold.read().await.values().sum();
    game_state.request_travel(&player, &npc, &node.id).await;

    let after: i64 = game_state.player_gold.read().await.values().sum();
    assert_eq!(
        after,
        before - node.fare,
        "the fare left the world rather than moving to a wallet"
    );
    assert_eq!(purse(&game_state, &player).await, 100);
    let position = game_state.players.read().await[&player].position;
    assert_eq!((position.x, position.z), (node.x, node.z));
}

/// The rule that stops travel from becoming a dungeon exit.
#[tokio::test]
async fn a_dungeon_floor_cannot_be_departed_from() {
    let game_state = make_test_game_state("travel_from_dungeon");
    let node = paid_node();
    let (player, npc) = travel_ready(&game_state, "delver", node.fare + 100).await;
    for id in [&player, &npc] {
        game_state
            .players
            .write()
            .await
            .get_mut(id)
            .unwrap()
            .floor_level = -3;
    }

    game_state.request_travel(&player, &npc, &node.id).await;

    assert_eq!(
        purse(&game_state, &player).await,
        node.fare + 100,
        "refused before the fare is taken"
    );
    assert_eq!(
        game_state.players.read().await[&player].floor_level,
        -3,
        "still underground"
    );
}

/// The level gate, on its own.
#[tokio::test]
async fn a_road_can_be_beyond_a_traveler() {
    let game_state = make_test_game_state("travel_level");
    let node = paid_node();
    let (player, npc) = travel_ready(&game_state, "novice", node.fare + 100).await;
    game_state
        .players
        .write()
        .await
        .get_mut(&player)
        .unwrap()
        .level = 1;
    assert!(node.min_level > 1, "the fixture needs a gated destination");

    game_state.request_travel(&player, &npc, &node.id).await;

    assert_eq!(purse(&game_state, &player).await, node.fare + 100);
    assert_eq!(game_state.players.read().await[&player].position.x, 0.0);
}

#[tokio::test]
async fn travel_is_refused_mid_fight_and_while_broke() {
    let game_state = make_test_game_state("travel_refusals");
    let node = paid_node();

    let (fighter, fighter_npc) = travel_ready(&game_state, "fighter", node.fare + 100).await;
    game_state
        .players
        .write()
        .await
        .get_mut(&fighter)
        .unwrap()
        .last_combat_at = GameState::now_ms();
    game_state
        .request_travel(&fighter, &fighter_npc, &node.id)
        .await;
    assert_eq!(purse(&game_state, &fighter).await, node.fare + 100);

    let (pauper, pauper_npc) = travel_ready(&game_state, "pauper", node.fare - 1).await;
    game_state
        .request_travel(&pauper, &pauper_npc, &node.id)
        .await;
    assert_eq!(purse(&game_state, &pauper).await, node.fare - 1);
    assert_eq!(
        game_state.players.read().await[&pauper].position.x,
        0.0,
        "and nobody moved"
    );
}
