//! Job advancement (IMP-8.1). The rules worth pinning are the ones a client
//! cannot enforce for itself: the job-level gate, what the second advancement
//! may be handed, and that the class change actually reaches the world.
use super::*;
use crate::auth::AuthService;
use onlinerpg_shared::character::{JobTier, FIRST_JOB_LEVEL, SECOND_JOB_LEVEL};
use onlinerpg_shared::messages::JobAdvanceDeniedReason;
use onlinerpg_shared::skills::skill_xp_for_level;

const NOVICE: &str = "Nov";
const KEEPER: &str = "Keeper";

/// A novice standing next to a townsperson, with `job_level` on the clock.
async fn novice_at_the_registrar(
    game_state: &GameState,
    auth: &Arc<AuthService>,
    test_name: &str,
    job_level: u32,
    tier: JobTier,
) -> DirectRx {
    let account = auth
        .login_npc(&format!("npc_{test_name}"))
        .expect("account");
    let record = auth
        .create_character(
            &account,
            NOVICE,
            &attrs_with_cha(12),
            16,
            if tier == JobTier::Novice {
                CharacterClass::Novice
            } else {
                CharacterClass::Ranger
            },
            Gender::Male,
        )
        .unwrap();

    let mut player = make_player(NOVICE, 0.0, 0.0);
    player.class = record.class;
    player.level = 10;
    game_state.add_player(player).await;

    let mut keeper = make_player(KEEPER, 1.0, 0.0);
    keeper.is_official_npc = true;
    game_state.add_player(keeper).await;

    game_state
        .register_player_character(
            &pid(NOVICE),
            record.id,
            record.xp,
            attrs_with_cha(12),
            record.gold,
            Some(onlinerpg_shared::hunger::SATIATION_START),
        )
        .await;
    game_state
        .inventories
        .write()
        .await
        .insert(pid(NOVICE), PlayerInventory::default());
    game_state.load_job_tier(&pid(NOVICE), tier).await;
    game_state
        .load_job_progress(&pid(NOVICE), skill_xp_for_level(job_level), 0)
        .await;
    game_state.register_direct_channel(&pid(NOVICE)).await
}

fn drain(rx: &mut DirectRx) -> Vec<ServerMessage> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        out.push(msg);
    }
    out
}

fn denial(msgs: &[ServerMessage]) -> Option<JobAdvanceDeniedReason> {
    msgs.iter().find_map(|m| match m {
        ServerMessage::JobAdvanceDenied { reason } => Some(*reason),
        _ => None,
    })
}

fn advanced(msgs: &[ServerMessage]) -> Option<(CharacterClass, u8)> {
    msgs.iter().find_map(|m| match m {
        ServerMessage::JobAdvanced {
            character_class,
            job_tier,
            ..
        } => Some((*character_class, *job_tier)),
        _ => None,
    })
}

#[tokio::test]
async fn a_novice_below_the_job_level_is_told_to_come_back() {
    let game_state = make_test_game_state("job_too_low");
    let auth = make_test_auth("job_too_low");
    let mut rx =
        novice_at_the_registrar(&game_state, &auth, "job_too_low", 0, JobTier::Novice).await;

    game_state
        .advance_job(&auth, &pid(NOVICE), &pid(KEEPER), CharacterClass::Ranger)
        .await;

    assert_eq!(
        denial(&drain(&mut rx)),
        Some(JobAdvanceDeniedReason::JobLevelTooLow)
    );
}

/// The point of the whole item: taking a job changes what you look like, and
/// the class lives on the wire so everybody else sees it too.
#[tokio::test]
async fn the_first_advancement_changes_the_class_on_the_player() {
    let game_state = make_test_game_state("job_first");
    let auth = make_test_auth("job_first");
    let mut rx = novice_at_the_registrar(
        &game_state,
        &auth,
        "job_first",
        FIRST_JOB_LEVEL,
        JobTier::Novice,
    )
    .await;

    game_state
        .advance_job(&auth, &pid(NOVICE), &pid(KEEPER), CharacterClass::Ranger)
        .await;

    assert_eq!(
        advanced(&drain(&mut rx)),
        Some((CharacterClass::Ranger, JobTier::First.as_u8()))
    );
    let players = game_state.players.read().await;
    let me = players.get(&pid(NOVICE)).expect("still in the world");
    assert_eq!(me.class, CharacterClass::Ranger);
    // Novice d6 to ranger d8 is the one-time difference, on top of 10.
    assert_eq!(me.max_health, 12);
}

#[tokio::test]
async fn a_first_advancement_refuses_a_class_that_is_not_a_job() {
    let game_state = make_test_game_state("job_not_a_job");
    let auth = make_test_auth("job_not_a_job");
    let mut rx = novice_at_the_registrar(
        &game_state,
        &auth,
        "job_not_a_job",
        FIRST_JOB_LEVEL,
        JobTier::Novice,
    )
    .await;

    game_state
        .advance_job(&auth, &pid(NOVICE), &pid(KEEPER), CharacterClass::Merchant)
        .await;

    assert_eq!(
        denial(&drain(&mut rx)),
        Some(JobAdvanceDeniedReason::NotAFirstJob)
    );
}

/// The second advancement awakens the job you have. It is not a second
/// chance to pick, which is what keeps the class count (and the meshes) flat.
#[tokio::test]
async fn the_second_advancement_will_not_change_your_mind() {
    let game_state = make_test_game_state("job_mismatch");
    let auth = make_test_auth("job_mismatch");
    let mut rx = novice_at_the_registrar(
        &game_state,
        &auth,
        "job_mismatch",
        SECOND_JOB_LEVEL,
        JobTier::First,
    )
    .await;

    game_state
        .advance_job(&auth, &pid(NOVICE), &pid(KEEPER), CharacterClass::Knight)
        .await;
    assert_eq!(
        denial(&drain(&mut rx)),
        Some(JobAdvanceDeniedReason::ClassMismatch)
    );

    game_state
        .advance_job(&auth, &pid(NOVICE), &pid(KEEPER), CharacterClass::Ranger)
        .await;
    assert_eq!(
        advanced(&drain(&mut rx)),
        Some((CharacterClass::Ranger, JobTier::Second.as_u8()))
    );
}

#[tokio::test]
async fn nobody_advances_without_somebody_to_ask() {
    let game_state = make_test_game_state("job_alone");
    let auth = make_test_auth("job_alone");
    let mut rx = novice_at_the_registrar(
        &game_state,
        &auth,
        "job_alone",
        FIRST_JOB_LEVEL,
        JobTier::Novice,
    )
    .await;
    // A traveler is not a townsperson, even standing right there.
    game_state
        .add_player(make_player("Passerby", 1.0, 0.0))
        .await;

    game_state
        .advance_job(
            &auth,
            &pid(NOVICE),
            &pid("Passerby"),
            CharacterClass::Ranger,
        )
        .await;

    assert_eq!(
        denial(&drain(&mut rx)),
        Some(JobAdvanceDeniedReason::NoOneToAsk)
    );
}
