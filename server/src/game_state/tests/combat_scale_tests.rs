//! SPK-1: what dense combat costs, sized against the 5,000 concurrent-user
//! target (master plan §7). The judgement these feed is 13 IMP-4.3's three
//! conditions; run with --nocapture to read the numbers.
//!
//! Density is the whole problem: the AOI cell *is* the delivery radius, so
//! `CROWD` players standing on one spot make every one of their moves a
//! `CROWD`-way fanout. The one structural cushion is that the payload is
//! encoded once and the `Bytes` handle cloned per recipient
//! (`send_direct_message_to_players_except`), which is why the assertions
//! below pin that rather than a wall-clock number.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Crowd sizes spanning the gate. 13 IMP-4.3 judges at 200.
const CROWDS: [usize; 3] = [100, 200, 400];
const GATE_CROWD: usize = 200;
/// 13 IMP-4.3 condition ①: one movement tick's budget.
const TICK_BUDGET: Duration = Duration::from_millis(200);

fn crowd_spot() -> Position {
    Position {
        x: 9_000.0,
        y: 0.0,
        z: 9_000.0,
    }
}

/// `count` players stacked inside one delivery radius. Every one gets a
/// direct channel, because an unread channel is not a recipient — the fanout
/// loop skips ids with no sender and would flatter the measurement.
async fn add_crowd(game_state: &GameState, tag: &str, count: usize) -> Vec<PlayerId> {
    let spot = crowd_spot();
    let mut ids = Vec::with_capacity(count);
    for i in 0..count {
        let name = format!("{tag}_crowd{i}");
        let id = pid(&name);
        // Inside EVENT_DELIVERY_RADIUS (43m) of the spot, but not on one
        // pixel: movement steps between distinct positions.
        let angle = i as f32 * 0.61;
        let player = make_player(
            &name,
            spot.x + angle.cos() * 5.0,
            spot.z + angle.sin() * 5.0,
        );
        game_state.add_player(player).await;
        game_state.register_connection_channel(&id).await;
        ids.push(id);
    }
    ids
}

/// Queue one short move for every member, the way a crowd all walking looks
/// to the tick. Untrusted on purpose: the trusted path applies the move on
/// the spot and never reaches `movement_intents`.
async fn queue_moves(game_state: &GameState, movers: &[PlayerId], step: f32) {
    for id in movers {
        let origin = game_state.players.read().await[id].position;
        let target = Position {
            x: origin.x + step,
            ..origin
        };
        game_state
            .update_player_position(id, move_cmd(target, false), false, false)
            .await;
    }
}

fn attack_event(attacker: PlayerId) -> ServerMessage {
    ServerMessage::PlayerAttacked {
        player_id: attacker,
        monster_id: "m1_1".to_string(),
        hit: true,
        roll: 17,
        damage: 6,
    }
}

/// 13 IMP-4.3 condition ①. Each tick moves the whole crowd, and each move
/// fans out to everyone standing in the same radius.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
async fn crowd_movement_tick_cost() {
    for crowd in CROWDS {
        let game_state = make_test_game_state(&format!("crowd_tick_{crowd}"));
        let movers = add_crowd(&game_state, "tick", crowd).await;
        queue_moves(&game_state, &movers, 0.25).await;
        // A tick with an empty queue returns at once and would report a
        // meaningless number.
        assert_eq!(
            game_state.movement_intents.read().await.len(),
            crowd,
            "every crowd member has a move waiting"
        );

        let start = Instant::now();
        game_state.tick_player_movement(0.1).await;
        let elapsed = start.elapsed();

        let verdict = if elapsed <= TICK_BUDGET { "ok" } else { "OVER" };
        println!(
            "crowd {crowd:>4} in one cell: tick_player_movement {elapsed:>10.2?} \
             (budget {TICK_BUDGET:?}) {verdict}"
        );
    }
}

/// 13 IMP-4.3 condition ② (as amended): what one combat event costs to
/// deliver, and what a crowd of them costs per second.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
async fn combat_event_fanout_bytes() {
    // Swings per attacker per second at the shipped cadence.
    let swings_per_sec = 1_000.0 / *super::super::combat::PLAYER_ATTACK_INTERVAL_MS as f32;
    for crowd in CROWDS {
        let game_state = make_test_game_state(&format!("crowd_fanout_{crowd}"));
        let attackers = add_crowd(&game_state, "fan", crowd).await;
        let recipients = game_state
            .player_ids_within_position(&crowd_spot(), 0, super::super::EVENT_DELIVERY_RADIUS)
            .await
            .len();
        let bytes = super::super::encode_server_msg(&attack_event(attackers[0]))
            .map(|b| b.len())
            .unwrap_or(0);

        let per_event = recipients * bytes;
        let per_sec = per_event as f32 * crowd as f32 * swings_per_sec;
        println!(
            "crowd {crowd:>4}: 1 attack = {recipients:>4} recipients x {bytes:>3}B = \
             {per_event:>8}B   all attacking = {:>8.1} KiB/s",
            per_sec / 1024.0
        );
    }
}

/// 13 IMP-4.3 condition ③ (as amended). The direct channel is unbounded, so
/// a client that cannot keep up costs memory, not dropped events — this
/// reports how deep one recipient's queue gets in a second of crowd combat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
async fn crowd_combat_queue_growth() {
    for crowd in CROWDS {
        let game_state = make_test_game_state(&format!("crowd_queue_{crowd}"));
        let attackers = add_crowd(&game_state, "queue", crowd).await;
        // One reader that never reads, standing in the crowd.
        let idle = pid("queue_idle_reader");
        game_state
            .add_player(make_player(
                "queue_idle_reader",
                crowd_spot().x,
                crowd_spot().z,
            ))
            .await;
        let mut idle_rx = game_state.register_direct_channel(&idle).await;

        let swings = crowd;
        let start = Instant::now();
        for attacker in attackers.iter().take(swings) {
            game_state
                .send_direct_message_to_players_within_position(
                    &crowd_spot(),
                    0,
                    super::super::EVENT_DELIVERY_RADIUS,
                    attack_event(*attacker),
                    None,
                )
                .await;
        }
        let elapsed = start.elapsed();
        let queued = drain(&mut idle_rx).len();
        println!(
            "crowd {crowd:>4}: {swings} attacks fanned out in {elapsed:>10.2?}, \
             one idle client queued {queued} messages"
        );
    }
}

/// Whether a full server changes any of the above. The AOI query walks a
/// spatial cell index (`player_spatial_cells.keys_near`) rather than the
/// roster, so it should not — but "should not" is the kind of claim this
/// spike exists to check.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
async fn crowd_tick_cost_against_a_full_server() {
    const USERS: usize = 5_000;
    for background in [0usize, USERS] {
        let game_state = make_test_game_state(&format!("crowd_full_{background}"));
        // Spread far from the crowd spot, in their own cells.
        for i in 0..background {
            let name = format!("bg{i}");
            game_state
                .add_player(make_player(
                    &name,
                    (i % 100) as f32 * 60.0,
                    (i / 100) as f32 * 60.0,
                ))
                .await;
            game_state.register_connection_channel(&pid(&name)).await;
        }
        let movers = add_crowd(&game_state, "full", GATE_CROWD).await;
        queue_moves(&game_state, &movers, 0.25).await;

        let start = Instant::now();
        game_state.tick_player_movement(0.1).await;
        let elapsed = start.elapsed();
        let reached = game_state
            .player_ids_within_position(&crowd_spot(), 0, super::super::EVENT_DELIVERY_RADIUS)
            .await
            .len();
        println!(
            "crowd {GATE_CROWD} + {background:>5} elsewhere: tick {elapsed:>10.2?},              fanout {reached} recipients"
        );
    }
}

/// Master plan §7 SPK-1 ⑤. The tick's own runtime matters less than how long
/// it keeps `players` from every other writer — combat, pickup and respawn
/// all take that lock.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
async fn crowd_tick_writer_stall() {
    let game_state = make_test_game_state("crowd_stall");
    let movers = add_crowd(&game_state, "stall", GATE_CROWD).await;
    queue_moves(&game_state, &movers, 0.25).await;
    assert_eq!(
        game_state.movement_intents.read().await.len(),
        GATE_CROWD,
        "every crowd member has a move waiting"
    );

    let stop = Arc::new(AtomicBool::new(false));
    let writer_state = game_state.clone();
    let writer_stop = Arc::clone(&stop);
    let writer = tokio::spawn(async move {
        let mut worst = Duration::ZERO;
        while !writer_stop.load(Ordering::Relaxed) {
            let start = Instant::now();
            let players = writer_state.players.write().await;
            worst = worst.max(start.elapsed());
            let _ = players.len();
            drop(players);
            tokio::task::yield_now().await;
        }
        worst
    });

    let start = Instant::now();
    game_state.tick_player_movement(0.1).await;
    let tick = start.elapsed();
    stop.store(true, Ordering::Relaxed);
    let worst = writer.await.expect("writer task finishes");

    println!(
        "crowd {GATE_CROWD}: movement tick {tick:>10.2?}  worst players.write() wait {worst:>10.2?}"
    );
}

// --- Assertions: the structural facts the measurements above rest on. ---

/// The fanout really is the whole crowd, and really does stop at the radius.
/// If this ever shrinks, the numbers above stop meaning what they say.
#[tokio::test]
async fn a_combat_event_reaches_the_whole_cell_and_stops_there() {
    let game_state = make_test_game_state("crowd_aoi_extent");
    add_crowd(&game_state, "aoi", 50).await;
    let spot = crowd_spot();
    let far = pid("aoi_far");
    game_state
        .add_player(make_player(
            "aoi_far",
            spot.x + super::super::EVENT_DELIVERY_RADIUS + 10.0,
            spot.z,
        ))
        .await;
    game_state.register_connection_channel(&far).await;

    let reached = game_state
        .player_ids_within_position(&spot, 0, super::super::EVENT_DELIVERY_RADIUS)
        .await;
    assert_eq!(reached.len(), 50, "every crowd member is a recipient");
    assert!(!reached.contains(&far), "and the radius is a real edge");
}

/// The other half of the O(N²) story: it is N-within-a-cell squared, not
/// N-on-the-server squared. A distant player must cost the fanout nothing.
#[tokio::test]
async fn players_elsewhere_on_the_server_do_not_widen_the_fanout() {
    let game_state = make_test_game_state("crowd_far_players");
    add_crowd(&game_state, "near", 20).await;
    for i in 0..500 {
        let name = format!("far{i}");
        game_state
            .add_player(make_player(
                &name,
                (i % 25) as f32 * 100.0,
                (i / 25) as f32 * 100.0,
            ))
            .await;
    }

    let reached = game_state
        .player_ids_within_position(&crowd_spot(), 0, super::super::EVENT_DELIVERY_RADIUS)
        .await;
    assert_eq!(
        reached.len(),
        20,
        "the fanout is the cell's population, not the server's"
    );
}

/// The cushion that makes an N-way fanout survivable: the payload is built
/// once no matter how many receive it. Encoding per recipient would turn a
/// 200-player cell's combat into 200 serializations per swing.
#[tokio::test]
async fn a_fanned_out_event_is_encoded_once_for_every_recipient() {
    let game_state = make_test_game_state("crowd_encode_once");
    let watchers: Vec<PlayerId> = ["enc_a", "enc_b", "enc_c"].iter().map(|n| pid(n)).collect();
    let spot = crowd_spot();
    let mut receivers = Vec::new();
    for name in ["enc_a", "enc_b", "enc_c"] {
        game_state
            .add_player(make_player(name, spot.x, spot.z))
            .await;
        receivers.push(game_state.register_direct_channel(&pid(name)).await);
    }
    // Later joiners announce themselves to the earlier ones.
    for rx in &mut receivers {
        drain(rx);
    }

    game_state
        .send_direct_message_to_players_within_position(
            &spot,
            0,
            super::super::EVENT_DELIVERY_RADIUS,
            attack_event(watchers[0]),
            None,
        )
        .await;

    for rx in &mut receivers {
        match rx.try_recv() {
            Ok(ServerMessage::PlayerAttacked { damage, .. }) => assert_eq!(damage, 6),
            other => panic!("Expected the shared payload, got {:?}", other),
        }
    }
}

/// 13 IMP-4.3's original conditions ②③ named the broadcast channel and its
/// `Lagged`; combat never touches it. This is the amendment in test form —
/// the direct path takes more than the broadcast channel's whole capacity
/// without losing a message, because its failure mode is memory instead.
#[tokio::test]
async fn the_combat_path_does_not_drop_events_under_backpressure() {
    let game_state = make_test_game_state("crowd_no_drop");
    let spot = crowd_spot();
    let listener = pid("no_drop_listener");
    game_state
        .add_player(make_player("no_drop_listener", spot.x, spot.z))
        .await;
    let mut rx = game_state.register_direct_channel(&listener).await;
    drain(&mut rx);

    // Comfortably past the 1000-slot broadcast channel this used to be
    // judged against.
    const EVENTS: usize = 1_500;
    for _ in 0..EVENTS {
        game_state
            .send_direct_message_to_players_within_position(
                &spot,
                0,
                super::super::EVENT_DELIVERY_RADIUS,
                attack_event(listener),
                None,
            )
            .await;
    }

    assert_eq!(
        drain(&mut rx).len(),
        EVENTS,
        "the direct channel is unbounded: nothing is dropped, so the cost is memory"
    );
}
