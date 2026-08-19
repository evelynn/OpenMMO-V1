//! The two cast-time paths that exist before skills do (IMP-3.1): moving out
//! of a variable cast, and leaving nothing behind on disconnect.
use super::super::cast::CastState;
use super::*;
use onlinerpg_shared::cast::CastTiming;
use onlinerpg_shared::skills::SkillId;

fn timing(vct_ms: u32, fct_ms: u32) -> CastTiming {
    CastTiming {
        vct_ms,
        fct_ms,
        after_cast_delay_ms: 300,
        cooldown_ms: 5_000,
    }
}

async fn start_cast(game_state: &GameState, player_id: &PlayerId, timing: CastTiming) {
    let schedule = timing.schedule(GameState::now_ms(), &attrs_with_cha(10));
    game_state.casting.write().await.insert(
        *player_id,
        CastState {
            skill: SkillId::Fishing,
            schedule,
            seq: 1,
            target: "goblin1".to_string(),
            level: 1,
        },
    );
}

async fn is_casting(game_state: &GameState, player_id: &PlayerId) -> bool {
    game_state.casting.read().await.contains_key(player_id)
}

async fn walk(game_state: &GameState, player_id: &PlayerId, x: f32) {
    game_state
        .update_player_position(
            player_id,
            move_cmd(Position { x, y: 0.0, z: 0.0 }, false),
            true,
            false,
        )
        .await;
}

#[tokio::test]
async fn walking_out_of_a_variable_cast_cancels_it() {
    let game_state = make_test_game_state("cast_move_cancel");
    let id = pid("caster");
    game_state.add_player(make_player("caster", 0.0, 0.0)).await;
    start_cast(&game_state, &id, timing(2_000, 500)).await;

    walk(&game_state, &id, 3.0).await;
    assert!(!is_casting(&game_state, &id).await);
}

/// The rule that makes the split worth having: once the fixed part starts,
/// the caster is committed and walking does not save them.
#[tokio::test]
async fn a_fixed_cast_survives_the_walk() {
    let game_state = make_test_game_state("cast_fixed_survives");
    let id = pid("caster");
    game_state.add_player(make_player("caster", 0.0, 0.0)).await;
    start_cast(&game_state, &id, timing(0, 2_000)).await;

    walk(&game_state, &id, 3.0).await;
    assert!(is_casting(&game_state, &id).await);
}

#[tokio::test]
async fn standing_still_does_not_cancel_a_cast() {
    let game_state = make_test_game_state("cast_no_move");
    let id = pid("caster");
    game_state.add_player(make_player("caster", 0.0, 0.0)).await;
    start_cast(&game_state, &id, timing(2_000, 500)).await;

    // Same position: `finish_position_update` never runs the cancel.
    walk(&game_state, &id, 0.0).await;
    assert!(is_casting(&game_state, &id).await);
}

/// Nothing writes these maps until IMP-3.2, but the cleanup has to be right
/// before the writes exist — otherwise a day of reconnects grows all three.
#[tokio::test]
async fn a_disconnect_leaves_no_cast_state_behind() {
    let game_state = make_test_game_state("cast_cleanup");
    let id = pid("caster");
    game_state.add_player(make_player("caster", 0.0, 0.0)).await;
    start_cast(&game_state, &id, timing(2_000, 500)).await;
    game_state
        .global_cast_delay_until
        .write()
        .await
        .insert(id, GameState::now_ms() + 10_000);
    game_state
        .skill_cooldowns
        .write()
        .await
        .insert(id, vec![(SkillId::Fishing, GameState::now_ms() + 10_000)]);

    game_state.clear_cast_state(&id).await;

    assert!(game_state.casting.read().await.is_empty());
    assert!(game_state.global_cast_delay_until.read().await.is_empty());
    assert!(game_state.skill_cooldowns.read().await.is_empty());
}
