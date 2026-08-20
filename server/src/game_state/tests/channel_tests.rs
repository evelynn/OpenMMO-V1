use super::*;
use onlinerpg_shared::channel::{ChannelDeniedReason, ChannelId};

async fn stage(game_state: &GameState, name: &str, x: f32) -> (PlayerId, DirectRx) {
    let player_id = pid(name);
    game_state.add_player(make_player(name, x, 0.0)).await;
    game_state
        .register_player_character(&player_id, 1, 0, attrs_with_cha(10), 0, None)
        .await;
    game_state.assign_channel(&player_id).await;
    let rx = game_state.register_direct_channel(&player_id).await;
    (player_id, rx)
}

async fn put_on(game_state: &GameState, player_id: &PlayerId, channel: ChannelId) {
    game_state
        .player_channels
        .write()
        .unwrap()
        .insert(*player_id, channel);
}

fn denials(msgs: Vec<ServerMessage>) -> Vec<ChannelDeniedReason> {
    msgs.into_iter()
        .filter_map(|msg| match msg {
            ServerMessage::ChannelDenied { reason } => Some(reason),
            _ => None,
        })
        .collect()
}

/// The whole point: standing next to somebody on another channel is standing
/// alone.
#[tokio::test]
async fn players_on_different_channels_cannot_see_each_other() {
    let game_state = make_test_game_state("channel_sight");
    let (alice, _a) = stage(&game_state, "alice", 0.0).await;
    let (bob, _b) = stage(&game_state, "bob", 1.0).await;

    // Same channel: they see each other.
    put_on(&game_state, &alice, 0).await;
    put_on(&game_state, &bob, 0).await;
    assert!(
        game_state
            .player_ids_within(&alice, super::super::EVENT_DELIVERY_RADIUS)
            .await
            .contains(&bob),
        "same channel, one metre apart"
    );

    put_on(&game_state, &bob, 1).await;
    assert!(
        !game_state
            .player_ids_within(&alice, super::super::EVENT_DELIVERY_RADIUS)
            .await
            .contains(&bob),
        "another channel is another world, however close they stand"
    );
}

/// A channel splits sight, not belongings.
#[tokio::test]
async fn a_channel_switch_keeps_the_character_whole() {
    let game_state = make_test_game_state("channel_keeps");
    let (player_id, mut rx) = stage(&game_state, "keeper", 0.0).await;
    {
        let mut inventories = game_state.inventories.write().await;
        let inv = inventories.entry(player_id).or_default();
        inv.bag.push(onlinerpg_shared::inventory::ItemInstance {
            instance_id: 9,
            item_def_id: "torch".to_string(),
            quantity: 3,
            enchant: 0,
        });
    }
    game_state.player_gold.write().await.insert(player_id, 777);
    drain(&mut rx);

    game_state.switch_channel(&player_id, 2).await;

    assert_eq!(game_state.channel_of(&player_id), 2);
    assert_eq!(game_state.get_player_gold(&player_id).await, 777);
    let inventories = game_state.inventories.read().await;
    assert_eq!(inventories[&player_id].bag[0].quantity, 3);
}

/// Whispers are addressed to a person, so they cross; the channel only
/// governs what you can see from where you stand.
#[tokio::test]
async fn a_whisper_crosses_channels() {
    let auth = make_test_auth("channel_whisper");
    let game_state = make_test_game_state("channel_whisper");
    let (alice, _a) = stage(&game_state, "alice", 0.0).await;
    let (bob, mut bob_rx) = stage(&game_state, "bob", 1.0).await;
    put_on(&game_state, &alice, 0).await;
    put_on(&game_state, &bob, 3).await;
    drain(&mut bob_rx);

    game_state
        .send_chat_message(&alice, "/w bob still here".to_string(), &auth)
        .await;

    let heard = drain(&mut bob_rx).into_iter().any(|msg| {
        matches!(msg, ServerMessage::WhisperMessage { ref message, .. } if message == "still here")
    });
    assert!(heard, "a whisper is addressed, not overheard");
}

/// Refusals, and the reasons a player is told.
#[tokio::test]
async fn switching_is_refused_for_the_reasons_that_matter() {
    let game_state = make_test_game_state("channel_denied");
    let (player_id, mut rx) = stage(&game_state, "switcher", 0.0).await;
    put_on(&game_state, &player_id, 0).await;
    drain(&mut rx);

    game_state.switch_channel(&player_id, 0).await;
    assert_eq!(
        denials(drain(&mut rx)),
        vec![ChannelDeniedReason::SameChannel]
    );

    game_state.switch_channel(&player_id, 200).await;
    assert_eq!(
        denials(drain(&mut rx)),
        vec![ChannelDeniedReason::NoSuchChannel]
    );

    // Underground: the instance's monsters belong to the descent.
    {
        let mut players = game_state.players.write().await;
        players.get_mut(&player_id).unwrap().floor_level = -2;
    }
    game_state.switch_channel(&player_id, 1).await;
    assert_eq!(
        denials(drain(&mut rx)),
        vec![ChannelDeniedReason::InDungeon]
    );
    assert_eq!(game_state.channel_of(&player_id), 0, "and nothing moved");
}

/// Entering puts you where your party already is, which is the only way a
/// party can fight together at all.
#[tokio::test]
async fn a_joining_player_lands_on_their_partys_channel() {
    let game_state = make_test_game_state("channel_party");
    let (leader, _l) = stage(&game_state, "leader", 0.0).await;
    put_on(&game_state, &leader, 3).await;

    let member = pid("member");
    game_state
        .add_player(make_player("member", 500.0, 0.0))
        .await;
    game_state
        .register_player_character(&member, 2, 0, attrs_with_cha(10), 0, None)
        .await;
    game_state.invite_to_party(&leader, "member").await;
    game_state
        .respond_to_party_invite(&member, &leader, true)
        .await;

    let assigned = game_state.assign_channel(&member).await;
    assert_eq!(
        assigned,
        Some(3),
        "the emptiest channel loses to the one your party is on"
    );
}
