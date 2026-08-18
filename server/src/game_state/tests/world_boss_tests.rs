//! Fixed mini-boss points (IMP-2.7): the spawn gate, the long jittered wait
//! after a kill, and the owner cap it has to respect like anything else.
use super::*;
use crate::world_boss_defs::{world_bosses, WorldBossSpawn};

fn first_spawn() -> &'static WorldBossSpawn {
    world_bosses().first().expect("a world boss spawn point")
}

/// The point's boss, if one currently stands there.
async fn boss_on_the_ground(game_state: &GameState, spawn: &WorldBossSpawn) -> Option<String> {
    let monsters = game_state.monsters.read().await;
    monsters
        .values()
        .find(|m| m.monster_type == spawn.monster_id && m.state != MonsterState::Dead)
        .map(|m| m.id.clone())
}

async fn add_hunter(game_state: &GameState, name: &str, spawn: &WorldBossSpawn) -> PlayerId {
    let mut player = make_player(name, spawn.x, spawn.z);
    player.level = 20;
    game_state.add_player(player).await;
    pid(name)
}

/// Attacks land on dice rolls, so the boss is walked down to its last hit
/// point first — this is about the respawn bookkeeping, not the combat math.
async fn kill_boss(game_state: &GameState, killer: &PlayerId, monster_id: &str) {
    {
        let mut monsters = game_state.monsters.write().await;
        monsters.get_mut(monster_id).expect("the boss").health = 1;
    }
    for _ in 0..200 {
        game_state.last_player_attacks.write().await.insert(
            *killer,
            GameState::now_ms().saturating_sub(*super::combat::PLAYER_ATTACK_INTERVAL_MS),
        );
        game_state
            .broadcast_player_attack(killer, monster_id.to_string())
            .await;
        let dead = game_state
            .monsters
            .read()
            .await
            .get(monster_id)
            .is_some_and(|m| m.state == MonsterState::Dead);
        if dead {
            return;
        }
    }
    panic!("boss {monster_id} survived 200 swings");
}

#[tokio::test]
async fn a_boss_spawns_at_its_point_once_somebody_is_there_to_own_it() {
    let game_state = make_test_game_state("world_boss_spawn");
    let spawn = first_spawn();
    let hunter = add_hunter(&game_state, "hunter", spawn).await;

    game_state.tick_world_bosses().await;

    let monsters = game_state.monsters.read().await;
    let boss = monsters
        .values()
        .find(|m| m.monster_type == spawn.monster_id)
        .expect("the boss stands at its point");
    assert_eq!(
        boss.owner_id,
        Some(hunter),
        "the nearby player simulates it"
    );
    assert_eq!(boss.floor_level, 0);
    assert!(boss.aggressive, "a mini boss that ignores you is not one");
    assert!(
        (boss.position.x - spawn.x).abs() < 0.01 && (boss.position.z - spawn.z).abs() < 0.01,
        "fixed point, not a wander target"
    );
}

/// The whole reason the point holds back instead of spawning into an empty
/// world: no owner means no client simulating it.
#[tokio::test]
async fn a_point_with_nobody_near_it_waits() {
    let game_state = make_test_game_state("world_boss_empty");
    let spawn = first_spawn();
    let mut far = make_player("far_away", spawn.x + 500.0, spawn.z + 500.0);
    far.level = 20;
    game_state.add_player(far).await;

    game_state.tick_world_bosses().await;
    assert!(boss_on_the_ground(&game_state, spawn).await.is_none());

    // …and it is still armed, so the next arrival gets it.
    add_hunter(&game_state, "hunter", spawn).await;
    game_state.tick_world_bosses().await;
    assert!(boss_on_the_ground(&game_state, spawn).await.is_some());
}

#[tokio::test]
async fn a_killed_boss_stays_away_for_its_base_wait() {
    let game_state = make_test_game_state("world_boss_wait");
    let spawn = first_spawn();
    let hunter = add_hunter(&game_state, "hunter", spawn).await;

    game_state.tick_world_bosses().await;
    let boss_id = boss_on_the_ground(&game_state, spawn)
        .await
        .expect("the first boss");
    kill_boss(&game_state, &hunter, &boss_id).await;

    let armed = game_state
        .world_boss_respawn_at_ms(&spawn.id)
        .await
        .expect("the kill armed the timer");
    let now = GameState::now_ms();
    let base_ms = spawn.respawn_base_secs * 1000;
    let ceiling_ms = base_ms + spawn.respawn_variance_secs * 1000;
    assert!(
        armed >= now + base_ms - 5_000 && armed <= now + ceiling_ms + 5_000,
        "respawn armed at {armed}, outside base..base+variance from {now}"
    );

    // The corpse is gone but the wait is not: ticking now must not respawn.
    game_state.despawn_monsters(vec![boss_id]).await;
    game_state.tick_world_bosses().await;
    assert!(
        boss_on_the_ground(&game_state, spawn).await.is_none(),
        "a boss must not return inside its base wait"
    );
}

#[tokio::test]
async fn the_boss_returns_once_the_wait_has_passed() {
    let game_state = make_test_game_state("world_boss_return");
    let spawn = first_spawn();
    let hunter = add_hunter(&game_state, "hunter", spawn).await;

    game_state.tick_world_bosses().await;
    let boss_id = boss_on_the_ground(&game_state, spawn)
        .await
        .expect("the first boss");
    kill_boss(&game_state, &hunter, &boss_id).await;
    game_state.despawn_monsters(vec![boss_id.clone()]).await;

    // Stand where the clock would be after base+variance, rather than
    // sleeping through half an hour of wall time.
    game_state
        .set_world_boss_respawn_at_ms(&spawn.id, GameState::now_ms())
        .await;
    game_state.tick_world_bosses().await;

    let returned = boss_on_the_ground(&game_state, spawn)
        .await
        .expect("the boss returns after its wait");
    assert_ne!(returned, boss_id, "a fresh monster, not the corpse");
}

/// A boss that vanished because nobody was left to watch it was never
/// killed, so it owes no wait (doc 13 IMP-2.7 revision).
#[tokio::test]
async fn an_unattended_despawn_costs_no_wait() {
    let game_state = make_test_game_state("world_boss_unattended");
    let spawn = first_spawn();
    add_hunter(&game_state, "hunter", spawn).await;

    game_state.tick_world_bosses().await;
    let boss_id = boss_on_the_ground(&game_state, spawn)
        .await
        .expect("the first boss");
    game_state.despawn_monsters(vec![boss_id.clone()]).await;

    game_state.tick_world_bosses().await;
    let returned = boss_on_the_ground(&game_state, spawn)
        .await
        .expect("it comes straight back");
    assert_ne!(returned, boss_id);
    assert_eq!(
        game_state.world_boss_respawn_at_ms(&spawn.id).await,
        Some(0),
        "no timer was ever armed"
    );
}

#[tokio::test]
async fn a_boss_consumes_its_owners_spawn_cap() {
    let game_state = make_test_game_state("world_boss_cap");
    let spawn = first_spawn();
    let hunter = add_hunter(&game_state, "hunter", spawn).await;
    let cap = world_config().max_monsters_per_player as usize;

    {
        let mut monsters = game_state.monsters.write().await;
        for i in 0..cap {
            let mut monster = make_monster(&format!("owned{i}"), pos(spawn.x), 0);
            monster.owner_id = Some(hunter);
            monsters.insert(monster.id.clone(), monster);
        }
    }
    game_state.tick_world_bosses().await;
    assert!(
        boss_on_the_ground(&game_state, spawn).await.is_none(),
        "a full owner must not be pushed past its cap"
    );

    game_state
        .despawn_monsters(vec!["owned0".to_string()])
        .await;
    game_state.tick_world_bosses().await;
    assert!(boss_on_the_ground(&game_state, spawn).await.is_some());
    assert_eq!(
        game_state.monsters.read().await.owned_by(&hunter),
        cap,
        "the boss took the freed slot, so it counts like any other monster"
    );
}
