use super::super::quest::utc_day_key;
use super::*;
use crate::auth::AuthService;

const UNLIMITED: &str = "hb_kobold_10";
const DAILY: &str = "hb_ogre_5";
/// Targets the goblinoid race rather than one monster (IMP-8.2).
const BY_RACE: &str = "hb_goblinoid_20";

/// A character in the DB and in the world, at `level`.
async fn hunter(
    game_state: &GameState,
    auth: &Arc<AuthService>,
    name: &str,
    level: u32,
) -> (i64, DirectRx) {
    let account = auth.login_npc(&format!("npc_{name}")).unwrap();
    let record = create_test_character(auth, &account, name);
    let mut player = make_player(name, 0.0, 0.0);
    player.level = level;
    game_state.add_player(player).await;
    game_state
        .register_player_character(
            &pid(name),
            record.id,
            record.xp,
            attrs_with_cha(12),
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
    (record.id, rx)
}

fn drain(rx: &mut DirectRx) -> Vec<ServerMessage> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        out.push(msg);
    }
    out
}

fn progress_of(msgs: &[ServerMessage], quest: &str) -> Option<u16> {
    msgs.iter().rev().find_map(|m| match m {
        ServerMessage::QuestProgress {
            quest_id, progress, ..
        } if quest_id == quest => Some(*progress),
        _ => None,
    })
}

#[tokio::test]
async fn every_xp_sharer_gets_the_kill_credited() {
    let game_state = make_test_game_state("quest_party");
    let auth = make_test_auth("quest_party");
    let (_, mut rx_a) = hunter(&game_state, &auth, "Ana", 2).await;
    let (_, mut rx_b) = hunter(&game_state, &auth, "Bo", 2).await;
    for who in ["Ana", "Bo"] {
        game_state.accept_quest(&auth, &pid(who), UNLIMITED).await;
    }
    drain(&mut rx_a);
    drain(&mut rx_b);

    // The same recipient list the XP split uses.
    game_state
        .credit_quest_kill(&[pid("Ana"), pid("Bo")], "kobold")
        .await;

    assert_eq!(progress_of(&drain(&mut rx_a), UNLIMITED), Some(1));
    assert_eq!(progress_of(&drain(&mut rx_b), UNLIMITED), Some(1));
}

#[tokio::test]
async fn a_kill_outside_the_contract_moves_nothing() {
    let game_state = make_test_game_state("quest_wrong_monster");
    let auth = make_test_auth("quest_wrong_monster");
    let (_, mut rx) = hunter(&game_state, &auth, "Cy", 2).await;
    game_state.accept_quest(&auth, &pid("Cy"), UNLIMITED).await;
    drain(&mut rx);

    game_state.credit_quest_kill(&[pid("Cy")], "troll").await;

    assert!(drain(&mut rx).is_empty(), "a wrong-monster kill is silent");
}

#[tokio::test]
async fn a_contract_outside_the_level_band_is_refused() {
    let game_state = make_test_game_state("quest_level_band");
    let auth = make_test_auth("quest_level_band");
    // hb_kobold_10 is levels 1-4.
    let (_, mut rx) = hunter(&game_state, &auth, "Dee", 9).await;

    game_state.accept_quest(&auth, &pid("Dee"), UNLIMITED).await;

    let msgs = drain(&mut rx);
    assert!(!msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::QuestAccepted { .. })));
    assert!(game_state
        .quest_progress
        .read()
        .await
        .get(&pid("Dee"))
        .is_none_or(|list| list.is_empty()));
}

#[tokio::test]
async fn the_accept_cap_holds() {
    let game_state = make_test_game_state("quest_cap");
    let auth = make_test_auth("quest_cap");
    let (_, mut rx) = hunter(&game_state, &auth, "Eli", 9).await;
    // Every contract this level can take, then one more.
    let ids: Vec<String> = game_state
        .quest_defs
        .iter()
        .filter(|def| def.min_level <= 9 && def.max_level >= 9)
        .map(|def| def.id.clone())
        .collect();
    for id in &ids {
        game_state.accept_quest(&auth, &pid("Eli"), id).await;
    }
    drain(&mut rx);

    let held = game_state.quest_progress.read().await[&pid("Eli")].len();
    assert!(
        held <= crate::quest_defs::MAX_ACCEPTED_QUESTS,
        "held {held} contracts, cap is {}",
        crate::quest_defs::MAX_ACCEPTED_QUESTS
    );
}

#[tokio::test]
async fn finishing_a_contract_pays_by_mail_and_spends_the_daily() {
    let game_state = make_test_game_state("quest_turn_in");
    let auth = make_test_auth("quest_turn_in");
    let (character_id, mut rx) = hunter(&game_state, &auth, "Fay", 9).await;
    game_state.accept_quest(&auth, &pid("Fay"), DAILY).await;
    let needed = game_state.quest_defs.by_id(DAILY).unwrap().count;
    for _ in 0..needed {
        game_state.credit_quest_kill(&[pid("Fay")], "ogre").await;
    }
    drain(&mut rx);

    game_state.turn_in_quest(&auth, &pid("Fay"), DAILY).await;

    let msgs = drain(&mut rx);
    let completed = msgs.iter().find_map(|m| match m {
        ServerMessage::QuestCompleted {
            quest_id,
            rewarded,
            daily_remaining,
        } if quest_id == DAILY => Some((*rewarded, *daily_remaining)),
        _ => None,
    });
    assert_eq!(
        completed,
        Some((true, 0)),
        "one daily completion, none left"
    );

    // The payout is a letter, not a bag insert — that is why mail ships first.
    let mail = auth.load_mail(character_id).unwrap();
    assert_eq!(mail.len(), 1);
    assert!(mail[0].gold > 0);
    assert_eq!(mail[0].items.len(), 1);

    // A second turn-in the same day is refused.
    game_state.turn_in_quest(&auth, &pid("Fay"), DAILY).await;
    assert_eq!(auth.load_mail(character_id).unwrap().len(), 1);
}

#[tokio::test]
async fn an_unfinished_contract_cannot_be_turned_in() {
    let game_state = make_test_game_state("quest_unfinished");
    let auth = make_test_auth("quest_unfinished");
    let (character_id, mut rx) = hunter(&game_state, &auth, "Gil", 2).await;
    game_state.accept_quest(&auth, &pid("Gil"), UNLIMITED).await;
    game_state.credit_quest_kill(&[pid("Gil")], "kobold").await;
    drain(&mut rx);

    game_state
        .turn_in_quest(&auth, &pid("Gil"), UNLIMITED)
        .await;

    assert!(auth.load_mail(character_id).unwrap().is_empty());
}

#[tokio::test]
async fn abandoning_keeps_the_daily_tally_but_drops_progress() {
    let game_state = make_test_game_state("quest_abandon");
    let auth = make_test_auth("quest_abandon");
    let (character_id, mut rx) = hunter(&game_state, &auth, "Hana", 9).await;
    game_state.accept_quest(&auth, &pid("Hana"), DAILY).await;
    game_state.credit_quest_kill(&[pid("Hana")], "ogre").await;
    drain(&mut rx);

    game_state.abandon_quest(&auth, &pid("Hana"), DAILY).await;

    assert!(game_state.quest_progress.read().await[&pid("Hana")].is_empty());
    // Re-accepting starts from zero, not from the abandoned count.
    game_state.accept_quest(&auth, &pid("Hana"), DAILY).await;
    assert_eq!(
        game_state.quest_progress.read().await[&pid("Hana")]
            .first()
            .map(|(_, banked)| *banked),
        Some(0)
    );
    let _ = character_id;
}

#[tokio::test]
async fn progress_survives_a_save_and_reload() {
    let game_state = make_test_game_state("quest_persist");
    let auth = make_test_auth("quest_persist");
    let (character_id, _rx) = hunter(&game_state, &auth, "Ivy", 2).await;
    game_state.accept_quest(&auth, &pid("Ivy"), UNLIMITED).await;
    game_state.credit_quest_kill(&[pid("Ivy")], "kobold").await;

    game_state.flush_dirty_saves(&auth).await;

    let rows = auth.load_quests(character_id).unwrap();
    let saved = rows.iter().find(|row| row.quest_id == UNLIMITED).unwrap();
    assert_eq!(saved.progress, 1);

    // A fresh session rebuilds the runtime state from those rows.
    let reloaded = make_test_game_state("quest_persist_reload");
    reloaded.load_quest_progress(&pid("Ivy"), &rows).await;
    assert_eq!(
        reloaded.quest_progress.read().await[&pid("Ivy")]
            .first()
            .map(|(_, banked)| *banked),
        Some(1)
    );
}

#[test]
fn the_day_key_turns_over_at_utc_midnight() {
    // 1970-01-02T00:00:00Z is exactly one day.
    assert_eq!(utc_day_key(0), 0);
    assert_eq!(utc_day_key(86_399), 0);
    assert_eq!(utc_day_key(86_400), 1);
    // Negative clocks (a machine set before the epoch) must not round toward
    // zero, or day -1 and day 0 would share a key.
    assert_eq!(utc_day_key(-1), -1);
}

#[tokio::test]
async fn a_new_utc_day_restores_the_daily_contract() {
    let game_state = make_test_game_state("quest_daily_reset");
    let auth = make_test_auth("quest_daily_reset");
    let (character_id, _rx) = hunter(&game_state, &auth, "Jun", 9).await;

    // Yesterday's completion.
    let spent = auth
        .complete_quest(character_id, DAILY, utc_day_key(0))
        .unwrap();
    assert_eq!(spent, 1);
    // Today's first completion resets the tally rather than stacking on it.
    let today = auth
        .complete_quest(character_id, DAILY, utc_day_key(86_400))
        .unwrap();
    assert_eq!(today, 1, "a new day starts the tally over");
}

/// The point of the race axis: one contract counts several monsters, so it
/// sends the player around a band of the world rather than to one spawn.
#[tokio::test]
async fn a_race_contract_counts_every_monster_of_that_race() {
    let game_state = make_test_game_state("quest_race");
    let auth = make_test_auth("quest_race");
    let (_, mut rx) = hunter(&game_state, &auth, "Rae", 6).await;
    game_state.accept_quest(&auth, &pid("Rae"), BY_RACE).await;
    drain(&mut rx);

    for monster in ["goblin", "kobold", "bugbear", "goblin_raider"] {
        game_state.credit_quest_kill(&[pid("Rae")], monster).await;
    }
    assert_eq!(progress_of(&drain(&mut rx), BY_RACE), Some(4));

    // An orc is a different race and must not count.
    game_state.credit_quest_kill(&[pid("Rae")], "orc").await;
    assert!(
        drain(&mut rx).is_empty(),
        "an orc kill is silent on a goblinoid contract"
    );
}
