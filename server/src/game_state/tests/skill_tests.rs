//! Combat skills (IMP-3.2): every refusal the server owns, and the two rules
//! that make the four cast times worth having.
use super::*;
use crate::auth::AuthService;
use crate::skill_defs::skill_def;
use onlinerpg_shared::messages::SkillRejectReason;
use onlinerpg_shared::skills::SkillId;

const STRIKE: SkillId = SkillId::PowerStrike;
/// The one skill with a real variable cast, so a walk can interrupt it.
const DART: SkillId = SkillId::FlameDart;

async fn fighter(
    game_state: &GameState,
    auth: &Arc<AuthService>,
    test_name: &str,
) -> (PlayerId, DirectRx) {
    let account = auth
        .login_npc(&format!("npc_{test_name}"))
        .expect("npc account");
    let record = create_test_character(auth, &account, "Fighter");
    let mut player = make_player("Fighter", 0.0, 0.0);
    player.level = 10;
    game_state.add_player(player).await;
    game_state
        .register_player_character(
            &pid("Fighter"),
            record.id,
            record.xp,
            attrs_with_cha(10),
            record.gold,
            Some(onlinerpg_shared::hunger::SATIATION_START),
        )
        .await;
    game_state
        .inventories
        .write()
        .await
        .insert(pid("Fighter"), PlayerInventory::default());
    let rx = game_state.register_direct_channel(&pid("Fighter")).await;
    (pid("Fighter"), rx)
}

/// Puts a monster within reach of the origin, where the fighter stands.
async fn target_at(game_state: &GameState, id: &str, x: f32) -> String {
    let mut monster = make_monster(id, Position { x, y: 0.0, z: 0.0 }, 0);
    monster.monster_type = "goblin".to_string();
    monster.health = 500;
    monster.max_health = 500;
    game_state
        .monsters
        .write()
        .await
        .insert(id.to_string(), monster);
    id.to_string()
}

async fn learn(game_state: &GameState, player_id: &PlayerId, skill: SkillId, level: u32) {
    let mut skills = game_state.player_skills.write().await;
    skills
        .entry(*player_id)
        .or_default()
        .set_level(skill, level);
}

/// Use a skill and let its cast run out. Time is paused in these tests, so
/// the delayed landing task fires without any wall-clock wait — and `now_ms`
/// is the system clock, so cooldown deadlines stay in the future throughout.
async fn cast(
    game_state: &GameState,
    auth: &Arc<AuthService>,
    id: &PlayerId,
    skill: SkillId,
    target: &str,
) {
    game_state
        .use_skill(auth, id, skill, Some(target.to_string()))
        .await;
    let def = skill_def(skill).expect("a shipped skill");
    tokio::time::sleep(std::time::Duration::from_millis(u64::from(
        def.vct_ms + def.fct_ms + 50,
    )))
    .await;
}

fn rejection(msgs: &[ServerMessage]) -> Option<SkillRejectReason> {
    msgs.iter().find_map(|m| match m {
        ServerMessage::SkillRejected { reason, .. } => Some(*reason),
        _ => None,
    })
}

fn skill_result(msgs: &[ServerMessage]) -> Option<(bool, u32)> {
    msgs.iter().find_map(|m| match m {
        ServerMessage::SkillResult { hit, damage, .. } => Some((*hit, *damage)),
        _ => None,
    })
}

#[tokio::test]
async fn an_unlearned_skill_is_refused() {
    let game_state = make_test_game_state("skill_unlearned");
    let auth = make_test_auth("skill_unlearned");
    let (id, mut rx) = fighter(&game_state, &auth, "skill_unlearned").await;
    let target = target_at(&game_state, "goblin1", 1.0).await;
    drain(&mut rx);

    game_state.use_skill(&auth, &id, STRIKE, Some(target)).await;
    assert_eq!(
        rejection(&drain(&mut rx)),
        Some(SkillRejectReason::NotLearned)
    );
}

#[tokio::test]
async fn a_target_out_of_range_is_refused() {
    let game_state = make_test_game_state("skill_range");
    let auth = make_test_auth("skill_range");
    let (id, mut rx) = fighter(&game_state, &auth, "skill_range").await;
    learn(&game_state, &id, STRIKE, 1).await;
    let far = skill_def(STRIKE).expect("power strike").range + 5.0;
    let target = target_at(&game_state, "goblin_far", far).await;
    drain(&mut rx);

    game_state.use_skill(&auth, &id, STRIKE, Some(target)).await;
    assert_eq!(
        rejection(&drain(&mut rx)),
        Some(SkillRejectReason::OutOfRange)
    );
}

#[tokio::test(start_paused = true)]
async fn a_skill_on_cooldown_is_refused() {
    let game_state = make_test_game_state("skill_cooldown");
    let auth = make_test_auth("skill_cooldown");
    let (id, mut rx) = fighter(&game_state, &auth, "skill_cooldown").await;
    learn(&game_state, &id, STRIKE, 1).await;
    let target = target_at(&game_state, "goblin1", 1.0).await;

    cast(&game_state, &auth, &id, STRIKE, &target).await;
    assert!(
        skill_result(&drain(&mut rx)).is_some(),
        "the first use lands"
    );

    // Clear the recovery so the refusal under test is the cooldown itself.
    game_state.global_cast_delay_until.write().await.remove(&id);
    game_state.use_skill(&auth, &id, STRIKE, Some(target)).await;
    assert_eq!(
        rejection(&drain(&mut rx)),
        Some(SkillRejectReason::OnCooldown)
    );
}

/// after-cast delay: every skill is refused, and a plain swing is not.
#[tokio::test(start_paused = true)]
async fn recovery_blocks_a_second_skill_but_not_a_swing() {
    let game_state = make_test_game_state("skill_recovery");
    let auth = make_test_auth("skill_recovery");
    let (id, mut rx) = fighter(&game_state, &auth, "skill_recovery").await;
    learn(&game_state, &id, STRIKE, 1).await;
    learn(&game_state, &id, DART, 1).await;
    let target = target_at(&game_state, "goblin1", 1.0).await;

    cast(&game_state, &auth, &id, STRIKE, &target).await;
    drain(&mut rx);

    game_state
        .use_skill(&auth, &id, DART, Some(target.clone()))
        .await;
    assert_eq!(
        rejection(&drain(&mut rx)),
        Some(SkillRejectReason::Recovering),
        "a different skill is still blocked by the delay"
    );

    game_state.player_attack(&id, target).await;
    let msgs = drain(&mut rx);
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::PlayerAttacked { .. })),
        "the after-cast delay must not touch normal attacks"
    );
}

#[tokio::test]
async fn a_skill_nobody_can_pay_for_is_refused() {
    let game_state = make_test_game_state("skill_satiation");
    let auth = make_test_auth("skill_satiation");
    let (id, mut rx) = fighter(&game_state, &auth, "skill_satiation").await;
    learn(&game_state, &id, STRIKE, 1).await;
    let target = target_at(&game_state, "goblin1", 1.0).await;
    game_state
        .hunger
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .satiation = 1;
    drain(&mut rx);

    game_state.use_skill(&auth, &id, STRIKE, Some(target)).await;
    assert_eq!(
        rejection(&drain(&mut rx)),
        Some(SkillRejectReason::NotEnoughSatiation)
    );
}

/// The rule that makes a variable cast a real decision: walk out of it and
/// you pay nothing.
#[tokio::test]
async fn walking_out_of_a_cast_costs_no_satiation_and_no_cooldown() {
    let game_state = make_test_game_state("skill_walk_out");
    let auth = make_test_auth("skill_walk_out");
    let (id, mut rx) = fighter(&game_state, &auth, "skill_walk_out").await;
    learn(&game_state, &id, DART, 1).await;
    let target = target_at(&game_state, "goblin1", 3.0).await;
    let before = game_state.hunger_satiation(&id).await;
    drain(&mut rx);

    game_state.use_skill(&auth, &id, DART, Some(target)).await;
    assert!(game_state.casting.read().await.contains_key(&id));

    game_state
        .update_player_position(
            &id,
            move_cmd(
                Position {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
                false,
            ),
            true,
            false,
        )
        .await;

    assert!(!game_state.casting.read().await.contains_key(&id));
    assert_eq!(game_state.hunger_satiation(&id).await, before);
    assert!(
        game_state
            .skill_cooldowns
            .read()
            .await
            .get(&id)
            .is_none_or(|c| c.is_empty()),
        "a cast that never landed puts nothing on cooldown"
    );
}

#[tokio::test(start_paused = true)]
async fn a_landed_skill_charges_its_cost_and_arms_its_cooldown() {
    let game_state = make_test_game_state("skill_lands");
    let auth = make_test_auth("skill_lands");
    let (id, mut rx) = fighter(&game_state, &auth, "skill_lands").await;
    learn(&game_state, &id, STRIKE, 1).await;
    let target = target_at(&game_state, "goblin1", 1.0).await;
    let def = skill_def(STRIKE).expect("power strike");
    let before = game_state.hunger_satiation(&id).await.unwrap();
    drain(&mut rx);

    cast(&game_state, &auth, &id, STRIKE, &target).await;

    assert!(skill_result(&drain(&mut rx)).is_some());
    assert_eq!(
        game_state.hunger_satiation(&id).await,
        Some(before - def.cost_satiation)
    );
    let cooldowns = game_state.skill_cooldowns.read().await;
    assert_eq!(cooldowns[&id].len(), 1);
}

#[tokio::test]
async fn learning_spends_a_point_and_respects_the_ladder() {
    let game_state = make_test_game_state("skill_learn");
    let auth = make_test_auth("skill_learn");
    let (id, mut rx) = fighter(&game_state, &auth, "skill_learn").await;

    game_state.learn_skill(&id, STRIKE).await;
    assert_eq!(
        rejection(&drain(&mut rx)),
        Some(SkillRejectReason::NoSkillPoints),
        "a skill has to be paid for"
    );

    game_state.load_job_progress(&id, 0, 5).await;
    game_state.learn_skill(&id, STRIKE).await;
    assert_eq!(game_state.skill_level(&id, STRIKE).await, 1);
    drain(&mut rx);

    // Cleave needs Power Strike at 3; it is at 1.
    game_state.learn_skill(&id, SkillId::Cleave).await;
    assert_eq!(rejection(&drain(&mut rx)), Some(SkillRejectReason::Locked));

    for _ in 0..2 {
        game_state.learn_skill(&id, STRIKE).await;
    }
    assert_eq!(game_state.skill_level(&id, STRIKE).await, 3);
    drain(&mut rx);
    game_state.learn_skill(&id, SkillId::Cleave).await;
    assert_eq!(game_state.skill_level(&id, SkillId::Cleave).await, 1);
}

/// Job XP is a second curve on the same kill, not a second copy of levelling:
/// points arrive on the skill thresholds.
#[tokio::test]
async fn job_xp_pays_out_skill_points_on_its_own_curve() {
    let game_state = make_test_game_state("skill_job_xp");
    let auth = make_test_auth("skill_job_xp");
    let (id, mut rx) = fighter(&game_state, &auth, "skill_job_xp").await;
    drain(&mut rx);

    game_state.grant_job_xp(&[id], 99).await;
    assert_eq!(game_state.job_progress.read().await[&id].points, 0);

    game_state.grant_job_xp(&[id], 1).await;
    assert_eq!(
        game_state.job_progress.read().await[&id].points,
        1,
        "the first point lands on the first skill threshold"
    );
    let msgs = drain(&mut rx);
    assert!(msgs.iter().any(|m| matches!(
        m,
        ServerMessage::SkillPointsUpdate {
            skill_points: 1,
            ..
        }
    )));
}

/// Combat skills are bought, never trained: a stray XP grant must not raise
/// something somebody paid a point for.
#[tokio::test]
async fn combat_skills_ignore_skill_xp() {
    let game_state = make_test_game_state("skill_no_xp");
    let auth = make_test_auth("skill_no_xp");
    let (id, _rx) = fighter(&game_state, &auth, "skill_no_xp").await;
    learn(&game_state, &id, STRIKE, 2).await;

    game_state.add_skill_xp(&id, STRIKE, 1_000_000).await;
    assert_eq!(game_state.skill_level(&id, STRIKE).await, 2);
}
