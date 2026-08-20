use super::*;

fn instanced_dungeon(game_state: &GameState) -> crate::dungeon_defs::DungeonEntranceDef {
    game_state
        .dungeon_defs
        .all()
        .find(|def| def.instance_cooldown_secs > 0)
        .expect("a dungeon with instances")
        .clone()
}

fn shared_dungeon(game_state: &GameState) -> crate::dungeon_defs::DungeonEntranceDef {
    game_state
        .dungeon_defs
        .all()
        .find(|def| def.instance_cooldown_secs == 0)
        .expect("a dungeon everyone shares")
        .clone()
}

fn instance_seeds(msgs: Vec<ServerMessage>) -> Vec<(String, u64)> {
    msgs.into_iter()
        .filter_map(|msg| match msg {
            ServerMessage::DungeonInstance {
                entrance_id,
                party_seed,
            } => Some((entrance_id, party_seed)),
            _ => None,
        })
        .collect()
}

async fn stage_delver(
    game_state: &GameState,
    auth: &Arc<crate::auth::AuthService>,
    name: &str,
    x: f32,
    z: f32,
) -> (PlayerId, DirectRx) {
    let account = auth.login_npc(&format!("npc_{name}")).unwrap();
    let character = create_test_character(auth, &account, name);
    let player_id = pid(name);
    game_state.add_player(make_player(name, x, z)).await;
    game_state
        .register_player_character(&player_id, character.id, 0, attrs_with_cha(12), 0, None)
        .await;
    let rx = game_state.register_direct_channel(&player_id).await;
    (player_id, rx)
}

/// A claim hands out a non-zero seed, and the second one inside the cooldown
/// is refused without disturbing the copy already claimed.
#[tokio::test]
async fn a_second_claim_is_refused_while_the_cooldown_runs() {
    let auth = make_test_auth("instance_cooldown");
    let game_state = make_test_game_state("instance_cooldown");
    let entrance = instanced_dungeon(&game_state);
    let (player_id, mut rx) =
        stage_delver(&game_state, &auth, "delver", entrance.x, entrance.z).await;

    game_state
        .claim_dungeon_instance(&auth, &player_id, &entrance.id)
        .await;
    let claimed = instance_seeds(drain(&mut rx));
    assert_eq!(claimed.len(), 1, "one claim, one instance");
    let (claimed_id, seed) = claimed[0].clone();
    assert_eq!(claimed_id, entrance.id);
    assert_ne!(seed, 0, "0 is the public dungeon, not a claim");
    assert_eq!(game_state.player_instance_seed(&player_id), seed);

    game_state
        .claim_dungeon_instance(&auth, &player_id, &entrance.id)
        .await;
    assert!(
        instance_seeds(drain(&mut rx)).is_empty(),
        "the cooldown refuses the second claim"
    );
    assert_eq!(
        game_state.player_instance_seed(&player_id),
        seed,
        "a refused claim leaves the copy already claimed alone"
    );
}

/// A dungeon with no cooldown column has no copies at all: everyone keeps
/// walking the one public maze.
#[tokio::test]
async fn a_shared_dungeon_hands_out_no_copies() {
    let auth = make_test_auth("instance_shared");
    let game_state = make_test_game_state("instance_shared");
    let entrance = shared_dungeon(&game_state);
    let (player_id, mut rx) =
        stage_delver(&game_state, &auth, "delver", entrance.x, entrance.z).await;

    game_state
        .claim_dungeon_instance(&auth, &player_id, &entrance.id)
        .await;
    assert!(instance_seeds(drain(&mut rx)).is_empty());
    assert_eq!(game_state.player_instance_seed(&player_id), 0);
}

/// The claim covers the party, and the copy really is a different maze.
#[tokio::test]
async fn a_party_shares_one_copy_and_it_differs_from_the_public_maze() {
    let auth = make_test_auth("instance_party");
    let game_state = make_test_game_state("instance_party");
    let entrance = instanced_dungeon(&game_state);
    let (leader, mut leader_rx) =
        stage_delver(&game_state, &auth, "leader", entrance.x, entrance.z).await;
    let (member, mut member_rx) =
        stage_delver(&game_state, &auth, "member", entrance.x + 2.0, entrance.z).await;
    game_state.invite_to_party(&leader, "member").await;
    game_state
        .respond_to_party_invite(&member, &leader, true)
        .await;
    drain(&mut leader_rx);
    drain(&mut member_rx);

    game_state
        .claim_dungeon_instance(&auth, &leader, &entrance.id)
        .await;
    let seed = game_state.player_instance_seed(&leader);
    assert_ne!(seed, 0);
    assert_eq!(
        game_state.player_instance_seed(&member),
        seed,
        "the party descends together or not at all"
    );
    assert_eq!(
        instance_seeds(drain(&mut member_rx)),
        vec![(entrance.id.clone(), seed)],
        "the member is told which copy to generate"
    );

    let public = onlinerpg_shared::dungeon::generate_dungeon_for(&entrance.id);
    let claimed = onlinerpg_shared::dungeon::generate_dungeon_for_party(&entrance.id, seed);
    assert_eq!(public.len(), claimed.len());
    assert!(
        public.iter().zip(&claimed).any(|(a, b)| a.rooms != b.rooms),
        "a claimed copy is a different maze"
    );
}

/// The last player out takes the copy with them — runtime and walls both —
/// or one instance per party ever formed would pile up.
#[tokio::test]
async fn an_emptied_copy_is_dropped() {
    let auth = make_test_auth("instance_evict");
    let game_state = make_test_game_state("instance_evict");
    let entrance = instanced_dungeon(&game_state);
    let (player_id, mut rx) =
        stage_delver(&game_state, &auth, "delver", entrance.x, entrance.z).await;
    game_state
        .claim_dungeon_instance(&auth, &player_id, &entrance.id)
        .await;
    drain(&mut rx);
    let seed = game_state.player_instance_seed(&player_id);
    let key = crate::game_state::dungeon::instance_key(&entrance.id, seed);
    let at_entrance = Position {
        x: entrance.x,
        y: entrance.y,
        z: entrance.z,
    };

    game_state
        .handle_player_floor_change(&player_id, 0, -1, &at_entrance, &at_entrance)
        .await;
    assert!(game_state.dungeons.read().await.contains_key(&key));
    assert!(game_state.instance_passability_read().contains_key(&key));

    game_state
        .handle_player_floor_change(&player_id, -1, 0, &at_entrance, &at_entrance)
        .await;
    assert!(
        !game_state.dungeons.read().await.contains_key(&key),
        "the emptied copy's runtime is gone"
    );
    assert!(
        !game_state.instance_passability_read().contains_key(&key),
        "and so are its walls"
    );
}
