use super::*;
use crate::game_state::backpressure::{delivery_of, Delivery, DIRECT_QUEUE_DEPTH};

fn a_move(player_id: PlayerId) -> ServerMessage {
    ServerMessage::PlayerMoved {
        player_id,
        position: Position {
            x: 1.0,
            y: 0.0,
            z: 1.0,
        },
        rotation: 0.0,
        floor_level: 0,
        sprinting: false,
    }
}

fn a_state_change() -> ServerMessage {
    ServerMessage::SystemMessage {
        message: "something changed".to_string(),
    }
}

/// The allowlist is short on purpose: a wrong "lossy" desyncs a player
/// silently, a wrong "reliable" only costs a disconnect.
#[test]
fn only_absolute_state_is_droppable() {
    assert_eq!(delivery_of(&a_move(pid("mover"))), Delivery::Lossy);
    assert_eq!(delivery_of(&a_state_change()), Delivery::Reliable);
    assert_eq!(
        delivery_of(&ServerMessage::InventoryUpdated {
            inventory: Default::default(),
        }),
        Delivery::Reliable,
        "an item moving is a transition, not a level"
    );
}

/// A client that stops reading costs a fixed amount of memory and no more,
/// and the movement stream keeps flowing for everyone else.
#[tokio::test]
async fn a_stalled_client_bounds_its_own_queue() {
    let game_state = make_test_game_state("bp_stalled");
    let player_id = pid("stalled");
    game_state
        .add_player(make_player("stalled", 0.0, 0.0))
        .await;
    let mut rx = game_state.register_direct_channel(&player_id).await;
    drain(&mut rx);

    for _ in 0..(DIRECT_QUEUE_DEPTH * 4) {
        game_state
            .send_direct_message(&player_id, a_move(player_id))
            .await;
    }

    assert_eq!(
        drain(&mut rx).len(),
        DIRECT_QUEUE_DEPTH,
        "the queue never grows past its depth"
    );
    assert!(
        !game_state.connection_overflowed(&Some(player_id)).await,
        "dropping position updates is the design, not a fault"
    );
    assert_eq!(
        game_state.dropped_messages_for(&player_id).await,
        (DIRECT_QUEUE_DEPTH * 3) as u64
    );
}

/// Losing a transition is not survivable, so the connection is marked for
/// closing instead.
#[tokio::test]
async fn losing_a_transition_marks_the_connection() {
    let game_state = make_test_game_state("bp_overflow");
    let player_id = pid("desynced");
    game_state
        .add_player(make_player("desynced", 0.0, 0.0))
        .await;
    let mut rx = game_state.register_direct_channel(&player_id).await;
    drain(&mut rx);

    for _ in 0..(DIRECT_QUEUE_DEPTH + 1) {
        game_state
            .send_direct_message(&player_id, a_state_change())
            .await;
    }

    assert!(game_state.connection_overflowed(&Some(player_id)).await);
    assert_eq!(
        game_state.dropped_messages_for(&player_id).await,
        0,
        "a reliable loss is not counted as a drop — it ends the connection"
    );
}

/// A receiver that keeps up loses nothing, which is the ordinary case and the
/// one worth pinning.
#[tokio::test]
async fn a_client_that_keeps_up_loses_nothing() {
    let game_state = make_test_game_state("bp_healthy");
    let player_id = pid("healthy");
    game_state
        .add_player(make_player("healthy", 0.0, 0.0))
        .await;
    let mut rx = game_state.register_direct_channel(&player_id).await;
    drain(&mut rx);

    let mut seen = 0;
    for _ in 0..8 {
        for _ in 0..(DIRECT_QUEUE_DEPTH / 2) {
            game_state
                .send_direct_message(&player_id, a_state_change())
                .await;
        }
        seen += drain(&mut rx).len();
    }

    assert_eq!(seen, 8 * (DIRECT_QUEUE_DEPTH / 2));
    assert!(!game_state.connection_overflowed(&Some(player_id)).await);
}
