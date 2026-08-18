//! MVP contribution bonus (IMP-2.8): who it pays, the ledger's ceiling, and
//! the one place the ledger is dropped.
use super::super::combat::MVP_MAX_CONTRIBUTORS;
use super::*;
use crate::auth::AuthService;

async fn fighter(
    game_state: &GameState,
    auth: &Arc<AuthService>,
    test_name: &str,
    name: &str,
) -> (PlayerId, DirectRx) {
    let account = auth
        .login_npc(&format!("npc_{test_name}_{name}"))
        .expect("npc account");
    let record = create_test_character(auth, &account, name);
    let mut player = make_player(name, 0.0, 0.0);
    player.level = 10;
    game_state.add_player(player).await;
    game_state
        .register_player_character(
            &pid(name),
            record.id,
            record.xp,
            attrs_with_cha(10),
            record.gold,
            None,
        )
        .await;
    game_state
        .inventories
        .write()
        .await
        .insert(pid(name), PlayerInventory::default());
    let rx = game_state.register_direct_channel(&pid(name)).await;
    (pid(name), rx)
}

async fn spawn_boss(game_state: &GameState, owner: PlayerId) -> String {
    game_state
        .spawn_monster(
            "orc_boss".to_string(),
            pos(1.0),
            0.0,
            Some(owner),
            0,
            MonsterLifecycle::Ambient,
            None,
            true,
        )
        .await
        .expect("the boss spawns")
        .id
}

fn mvp_bonus(msgs: &[ServerMessage]) -> Option<u32> {
    msgs.iter().find_map(|m| match m {
        ServerMessage::MvpBonus { xp, .. } => Some(*xp),
        _ => None,
    })
}

/// The point of the whole feature: the bonus follows the damage, not the
/// last blow.
#[tokio::test]
async fn the_bonus_goes_to_the_biggest_contributor_not_the_finisher() {
    let game_state = make_test_game_state("mvp_contributor");
    let auth = make_test_auth("mvp_contributor");
    let (bruiser, mut bruiser_rx) = fighter(&game_state, &auth, "mvp_contributor", "Bruiser").await;
    let (finisher, mut finisher_rx) =
        fighter(&game_state, &auth, "mvp_contributor", "Finisher").await;

    let boss_id = spawn_boss(&game_state, bruiser).await;
    game_state
        .record_boss_damage(&boss_id, &bruiser, 1_000)
        .await;
    {
        let mut monsters = game_state.monsters.write().await;
        monsters.get_mut(&boss_id).expect("the boss").health = 1;
    }
    drain(&mut bruiser_rx);
    drain(&mut finisher_rx);

    for _ in 0..200 {
        game_state.last_player_attacks.write().await.insert(
            finisher,
            GameState::now_ms().saturating_sub(*super::combat::PLAYER_ATTACK_INTERVAL_MS),
        );
        game_state.player_attack(&finisher, boss_id.clone()).await;
        let dead = game_state
            .monsters
            .read()
            .await
            .get(&boss_id)
            .is_some_and(|m| m.state == MonsterState::Dead);
        if dead {
            break;
        }
    }

    let bonus = mvp_bonus(&drain(&mut bruiser_rx)).expect("the bruiser is the MVP");
    assert!(bonus > 0, "an MVP bonus of nothing is not a bonus");
    assert!(
        mvp_bonus(&drain(&mut finisher_rx)).is_none(),
        "landing the kill is not what earns the bonus"
    );
}

#[tokio::test]
async fn an_ordinary_monster_keeps_no_ledger() {
    let game_state = make_test_game_state("mvp_ordinary");
    let owner = pid("owner");
    game_state.add_player(make_player("owner", 0.0, 0.0)).await;
    let monster = game_state
        .spawn_monster(
            "goblin".to_string(),
            pos(1.0),
            0.0,
            Some(owner),
            0,
            MonsterLifecycle::Ambient,
            None,
            false,
        )
        .await
        .expect("the goblin spawns");

    game_state
        .record_boss_damage(&monster.id, &owner, 500)
        .await;
    assert!(
        game_state.boss_damage.read().await.is_empty(),
        "a hundred thousand ordinary monsters must not each carry a vector"
    );
}

#[tokio::test]
async fn the_ledger_never_outgrows_its_cap() {
    let game_state = make_test_game_state("mvp_cap");
    let owner = pid("owner");
    game_state.add_player(make_player("owner", 0.0, 0.0)).await;
    let boss_id = spawn_boss(&game_state, owner).await;

    // Ascending damage, so every newcomer beats the current weakest and the
    // eviction path runs on each of the last few.
    for i in 0..(MVP_MAX_CONTRIBUTORS + 8) {
        game_state
            .record_boss_damage(&boss_id, &pid(&format!("hitter{i}")), (i as u32 + 1) * 10)
            .await;
    }

    let ledgers = game_state.boss_damage.read().await;
    let ledger = &ledgers[&boss_id];
    assert_eq!(ledger.len(), MVP_MAX_CONTRIBUTORS);
    let smallest_kept = ledger.iter().map(|(_, total)| *total).min().unwrap();
    assert!(
        smallest_kept >= 90,
        "the ceiling must drop the smallest contributors, not the newest: {smallest_kept}"
    );
    let top = ledger.iter().max_by_key(|(_, total)| *total).unwrap();
    assert_eq!(
        top.0,
        pid(&format!("hitter{}", MVP_MAX_CONTRIBUTORS + 7)),
        "the biggest contributor is never the one evicted"
    );
}

/// A contributor who is already listed keeps accumulating, cap or no cap.
#[tokio::test]
async fn repeat_hits_accumulate_on_one_entry() {
    let game_state = make_test_game_state("mvp_accumulate");
    let owner = pid("owner");
    game_state.add_player(make_player("owner", 0.0, 0.0)).await;
    let boss_id = spawn_boss(&game_state, owner).await;

    for _ in 0..5 {
        game_state.record_boss_damage(&boss_id, &owner, 7).await;
    }
    let ledgers = game_state.boss_damage.read().await;
    assert_eq!(ledgers[&boss_id], vec![(owner, 35)]);
}

#[tokio::test]
async fn despawning_the_boss_drops_its_ledger() {
    let game_state = make_test_game_state("mvp_despawn");
    let owner = pid("owner");
    game_state.add_player(make_player("owner", 0.0, 0.0)).await;
    let boss_id = spawn_boss(&game_state, owner).await;
    game_state.record_boss_damage(&boss_id, &owner, 100).await;
    assert!(!game_state.boss_damage.read().await.is_empty());

    game_state.despawn_monsters(vec![boss_id]).await;
    assert!(
        game_state.boss_damage.read().await.is_empty(),
        "the ledger must not outlive the monster"
    );
}
