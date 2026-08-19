//! Achievements and titles (IMP-3.7): unlocking once and only once, and
//! refusing a title nobody earned.
use super::*;
use crate::achievement_defs::{achievement_defs, Trigger};
use crate::auth::AuthService;

async fn adventurer(
    game_state: &GameState,
    auth: &Arc<AuthService>,
    test_name: &str,
) -> (PlayerId, DirectRx) {
    let account = auth
        .login_npc(&format!("npc_{test_name}"))
        .expect("npc account");
    let record = create_test_character(auth, &account, "Hero");
    game_state.add_player(make_player("Hero", 0.0, 0.0)).await;
    game_state
        .register_player_character(
            &pid("Hero"),
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
        .insert(pid("Hero"), PlayerInventory::default());
    game_state
        .load_achievements(&pid("Hero"), Vec::new(), Vec::new(), None)
        .await;
    let rx = game_state.register_direct_channel(&pid("Hero")).await;
    (pid("Hero"), rx)
}

fn unlocks(msgs: &[ServerMessage]) -> Vec<String> {
    msgs.iter()
        .filter_map(|m| match m {
            ServerMessage::AchievementUnlocked { achievement_id, .. } => {
                Some(achievement_id.clone())
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn crossing_a_threshold_unlocks_exactly_once() {
    let game_state = make_test_game_state("achv_once");
    let auth = make_test_auth("achv_once");
    let (id, mut rx) = adventurer(&game_state, &auth, "achv_once").await;
    drain(&mut rx);

    game_state.bump(&id, Trigger::FishTrophy, None, 1).await;
    game_state.tick_achievements(&auth).await;
    assert_eq!(unlocks(&drain(&mut rx)), vec!["angler".to_string()]);

    // Landing more trophies must not re-award the same one.
    for _ in 0..5 {
        game_state.bump(&id, Trigger::FishTrophy, None, 1).await;
        game_state.tick_achievements(&auth).await;
    }
    assert!(
        unlocks(&drain(&mut rx)).is_empty(),
        "an achievement pays out once, or the mailbox fills with copies"
    );
}

/// The durable guard: a reconnect reloads the unlocked set, and the DB row
/// refuses a second insert even if memory forgot.
#[tokio::test]
async fn a_reconnect_does_not_re_award() {
    let game_state = make_test_game_state("achv_reconnect");
    let auth = make_test_auth("achv_reconnect");
    let (id, mut rx) = adventurer(&game_state, &auth, "achv_reconnect").await;
    game_state.bump(&id, Trigger::FishTrophy, None, 1).await;
    game_state.tick_achievements(&auth).await;
    drain(&mut rx);

    // Come back with an empty in-memory set, as a stale client would.
    game_state
        .load_achievements(&id, Vec::new(), Vec::new(), None)
        .await;
    game_state.bump(&id, Trigger::FishTrophy, None, 1).await;
    game_state.tick_achievements(&auth).await;
    assert!(
        unlocks(&drain(&mut rx)).is_empty(),
        "the database row is what makes the reward final"
    );
}

/// A depth is a best, not a total: descending to 5 twice is not depth 10.
#[tokio::test]
async fn a_high_water_counter_does_not_accumulate() {
    let game_state = make_test_game_state("achv_high_water");
    let auth = make_test_auth("achv_high_water");
    let (id, mut rx) = adventurer(&game_state, &auth, "achv_high_water").await;
    drain(&mut rx);

    for _ in 0..3 {
        game_state.bump(&id, Trigger::DungeonDepth, None, 5).await;
    }
    game_state.tick_achievements(&auth).await;
    let earned = unlocks(&drain(&mut rx));
    assert!(earned.contains(&"deep_delver".to_string()));
    assert!(
        !earned.contains(&"abyss_walker".to_string()),
        "three trips to floor 5 must not read as floor 15"
    );

    game_state.bump(&id, Trigger::DungeonDepth, None, 10).await;
    game_state.tick_achievements(&auth).await;
    assert!(unlocks(&drain(&mut rx)).contains(&"abyss_walker".to_string()));
}

/// A narrowed counter and a broad one advance together without either
/// counting the other's events.
#[tokio::test]
async fn a_narrowed_trigger_keeps_its_own_tally() {
    let game_state = make_test_game_state("achv_narrow");
    let auth = make_test_auth("achv_narrow");
    let (id, mut rx) = adventurer(&game_state, &auth, "achv_narrow").await;
    drain(&mut rx);

    game_state
        .bump(&id, Trigger::MonsterKill, Some("goblin"), 1)
        .await;
    game_state.tick_achievements(&auth).await;
    assert_eq!(unlocks(&drain(&mut rx)), vec!["first_blood".to_string()]);

    let counters = game_state.achievements.read().await[&id].counters.clone();
    assert_eq!(counters.get("monster_kill"), Some(&1));
    assert_eq!(counters.get("monster_kill:goblin"), Some(&1));
    assert_eq!(
        counters.get("monster_kill:orc"),
        None,
        "a goblin must not advance the orc tally"
    );
}

#[tokio::test]
async fn an_unearned_title_is_refused() {
    let game_state = make_test_game_state("achv_title");
    let auth = make_test_auth("achv_title");
    let (id, mut rx) = adventurer(&game_state, &auth, "achv_title").await;
    drain(&mut rx);

    game_state.set_title(&id, Some("Angler".to_string())).await;
    assert_eq!(game_state.active_title(&id).await, None);
    assert!(drain(&mut rx).is_empty(), "a refusal is silent, not a lie");

    game_state.bump(&id, Trigger::FishTrophy, None, 1).await;
    game_state.tick_achievements(&auth).await;
    drain(&mut rx);
    game_state.set_title(&id, Some("Angler".to_string())).await;
    assert_eq!(game_state.active_title(&id).await, Some("Angler".into()));
}

/// A title made up out of thin air is refused even if it looks plausible.
#[tokio::test]
async fn a_title_no_achievement_grants_is_refused() {
    let game_state = make_test_game_state("achv_fake_title");
    let auth = make_test_auth("achv_fake_title");
    let (id, _rx) = adventurer(&game_state, &auth, "achv_fake_title").await;

    game_state.set_title(&id, Some("God".to_string())).await;
    assert_eq!(game_state.active_title(&id).await, None);
}

/// Every title in the shipped table is reachable from some achievement, or
/// the client would offer one that can never be set.
#[tokio::test]
async fn every_shipped_title_comes_from_an_achievement() {
    for def in achievement_defs() {
        if let Some(title) = &def.title_id {
            assert!(
                crate::achievement_defs::is_known_title(title),
                "title '{title}' has no achievement behind it"
            );
        }
    }
}
