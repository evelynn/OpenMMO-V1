use super::*;

/// Karl is the only hireable NPC in the registry; the name is what the
/// server matches on.
const HIREABLE: &str = "Karl";
const RATE: i64 = 500;

async fn add_official_npc(game_state: &GameState, name: &str) -> PlayerId {
    let mut npc = make_player(name, 0.0, 0.0);
    npc.is_official_npc = true;
    let id = npc.id;
    game_state.add_player(npc).await;
    id
}

async fn add_employer(game_state: &GameState, name: &str, gold: i64) -> (PlayerId, DirectRx) {
    let player_id = pid(name);
    game_state.add_player(make_player(name, 0.0, 0.0)).await;
    game_state.player_gold.write().await.insert(player_id, gold);
    let rx = game_state.register_direct_channel(&player_id).await;
    (player_id, rx)
}

fn contracts(msgs: Vec<ServerMessage>) -> Vec<(PlayerId, PlayerId, i64)> {
    msgs.into_iter()
        .filter_map(|msg| match msg {
            ServerMessage::CompanionContract {
                npc_player_id,
                employer_id,
                expires_at,
            } => Some((npc_player_id, employer_id, expires_at)),
            _ => None,
        })
        .collect()
}

fn refusals(msgs: Vec<ServerMessage>) -> Vec<String> {
    msgs.into_iter()
        .filter_map(|msg| match msg {
            ServerMessage::SystemMessage { message, .. } => Some(message),
            _ => None,
        })
        .collect()
}

/// A hire charges the fee, tells both sides, and moves the money into the
/// NPC's own wallet rather than burning it.
#[tokio::test]
async fn hiring_charges_the_employer_and_pays_the_npc() {
    let game_state = make_test_game_state("companion_hire");
    let npc_id = add_official_npc(&game_state, HIREABLE).await;
    let mut npc_rx = game_state.register_direct_channel(&npc_id).await;
    let (employer, mut rx) = add_employer(&game_state, "patron", 2_000).await;

    game_state.hire_companion(&employer, &npc_id, 2).await;

    let seen = contracts(drain(&mut rx));
    assert_eq!(seen.len(), 1);
    let (contracted_npc, contracted_employer, expires_at) = seen[0];
    assert_eq!(contracted_npc, npc_id);
    assert_eq!(contracted_employer, employer);
    assert!(expires_at > crate::auth::unix_now());
    assert_eq!(
        contracts(drain(&mut npc_rx)),
        vec![(npc_id, employer, expires_at)],
        "the NPC is told too — it is the one who acts on it"
    );

    assert_eq!(
        game_state.get_player_gold(&employer).await,
        2_000 - RATE * 2
    );
    assert_eq!(
        game_state.get_player_gold(&npc_id).await,
        RATE * 2,
        "the fee is a transfer, not a burn"
    );
}

/// A wallet that cannot cover the fee buys nothing and loses nothing.
#[tokio::test]
async fn a_short_wallet_signs_nothing() {
    let game_state = make_test_game_state("companion_broke");
    let npc_id = add_official_npc(&game_state, HIREABLE).await;
    let (employer, mut rx) = add_employer(&game_state, "pauper", RATE - 1).await;

    game_state.hire_companion(&employer, &npc_id, 1).await;

    assert!(contracts(drain(&mut rx)).is_empty());
    assert_eq!(game_state.get_player_gold(&employer).await, RATE - 1);
    assert_eq!(game_state.get_player_gold(&npc_id).await, 0);
}

/// Somebody else's companion is not for sale, and refusing costs the second
/// bidder nothing.
#[tokio::test]
async fn an_npc_already_under_contract_refuses() {
    let game_state = make_test_game_state("companion_taken");
    let npc_id = add_official_npc(&game_state, HIREABLE).await;
    let (first, _first_rx) = add_employer(&game_state, "first", 5_000).await;
    let (second, mut second_rx) = add_employer(&game_state, "second", 5_000).await;

    game_state.hire_companion(&first, &npc_id, 1).await;
    game_state.hire_companion(&second, &npc_id, 1).await;

    let seen = drain(&mut second_rx);
    assert!(contracts(seen.clone()).is_empty());
    assert_eq!(game_state.get_player_gold(&second).await, 5_000);
    assert!(
        refusals(seen)
            .iter()
            .any(|line| line.contains("already working")),
        "and they are told why"
    );
}

/// An NPC with no rate column takes no contracts at all.
#[tokio::test]
async fn an_npc_that_is_not_for_hire_takes_no_contract() {
    let game_state = make_test_game_state("companion_unhireable");
    let npc_id = add_official_npc(&game_state, "Rica").await;
    let (employer, mut rx) = add_employer(&game_state, "patron", 5_000).await;

    game_state.hire_companion(&employer, &npc_id, 1).await;

    assert!(contracts(drain(&mut rx)).is_empty());
    assert_eq!(game_state.get_player_gold(&employer).await, 5_000);
}

/// A duration outside 1..=MAX is refused before anyone is charged.
#[tokio::test]
async fn a_nonsense_duration_is_refused() {
    let game_state = make_test_game_state("companion_hours");
    let npc_id = add_official_npc(&game_state, HIREABLE).await;
    let (employer, mut rx) = add_employer(&game_state, "patron", 100_000).await;

    game_state.hire_companion(&employer, &npc_id, 0).await;
    game_state
        .hire_companion(
            &employer,
            &npc_id,
            crate::game_state::companion::MAX_CONTRACT_HOURS + 1,
        )
        .await;

    assert!(contracts(drain(&mut rx)).is_empty());
    assert_eq!(game_state.get_player_gold(&employer).await, 100_000);
}

/// The expiry sweep ends the contract and says so to both sides.
#[tokio::test]
async fn the_sweep_ends_an_expired_contract() {
    let game_state = make_test_game_state("companion_expiry");
    let npc_id = add_official_npc(&game_state, HIREABLE).await;
    let mut npc_rx = game_state.register_direct_channel(&npc_id).await;
    let (employer, mut rx) = add_employer(&game_state, "patron", 5_000).await;
    game_state.hire_companion(&employer, &npc_id, 1).await;
    drain(&mut rx);
    drain(&mut npc_rx);

    // Nothing to sweep while it runs.
    game_state.tick_companion_contracts().await;
    assert!(contracts(drain(&mut rx)).is_empty());

    // Backdate it the way the clock would.
    {
        let mut held = game_state.companions.write().await;
        held.get_mut(&npc_id).expect("a contract").expires_at = crate::auth::unix_now() - 1;
    }
    game_state.tick_companion_contracts().await;

    assert_eq!(contracts(drain(&mut rx)), vec![(npc_id, employer, 0)]);
    assert_eq!(contracts(drain(&mut npc_rx)), vec![(npc_id, employer, 0)]);
    assert!(game_state.companions.read().await.is_empty());
}

/// The employer walking away ends it too — an NPC cannot follow a ghost.
#[tokio::test]
async fn an_employer_leaving_ends_the_contract() {
    let game_state = make_test_game_state("companion_leave");
    let npc_id = add_official_npc(&game_state, HIREABLE).await;
    let mut npc_rx = game_state.register_direct_channel(&npc_id).await;
    let (employer, _rx) = add_employer(&game_state, "patron", 5_000).await;
    game_state.hire_companion(&employer, &npc_id, 1).await;
    drain(&mut npc_rx);

    game_state.end_companion_contracts_for(&employer).await;

    assert_eq!(contracts(drain(&mut npc_rx)), vec![(npc_id, employer, 0)]);
    assert!(game_state.companions.read().await.is_empty());
}
