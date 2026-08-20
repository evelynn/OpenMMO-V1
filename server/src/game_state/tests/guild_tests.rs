//! Guilds (IMP-4.1): one guild per character, ranks that actually gate the
//! vault, and chat that reaches members and nobody else.
use super::*;
use crate::auth::AuthService;
use onlinerpg_shared::guild::{GuildDeniedReason, DEFAULT_RANK, LEADER_RANK};

async fn member(
    game_state: &GameState,
    auth: &Arc<AuthService>,
    test_name: &str,
    name: &str,
) -> (PlayerId, DirectRx) {
    let account = auth
        .login_npc(&format!("npc_{test_name}_{name}"))
        .expect("npc account");
    let record = create_test_character(auth, &account, name);
    game_state.add_player(make_player(name, 0.0, 0.0)).await;
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

fn denial(msgs: &[ServerMessage]) -> Option<GuildDeniedReason> {
    msgs.iter().find_map(|m| match m {
        ServerMessage::GuildDenied { reason } => Some(*reason),
        _ => None,
    })
}

fn guild_state(msgs: &[ServerMessage]) -> Option<onlinerpg_shared::guild::GuildState> {
    msgs.iter().rev().find_map(|m| match m {
        ServerMessage::GuildUpdated { guild } => guild.clone(),
        _ => None,
    })
}

async fn founded(
    game_state: &GameState,
    auth: &Arc<AuthService>,
    test_name: &str,
) -> (PlayerId, DirectRx) {
    let (leader, mut rx) = member(game_state, auth, test_name, "Leader").await;
    game_state
        .create_guild(auth, &leader, "The Iron Circle".to_string())
        .await;
    drain(&mut rx);
    (leader, rx)
}

#[tokio::test]
async fn founding_makes_you_the_leader() {
    let game_state = make_test_game_state("guild_found");
    let auth = make_test_auth("guild_found");
    let (leader, mut rx) = member(&game_state, &auth, "guild_found", "Leader").await;

    game_state
        .create_guild(&auth, &leader, "The Iron Circle".to_string())
        .await;

    let state = guild_state(&drain(&mut rx)).expect("the founder sees their guild");
    assert_eq!(state.name, "The Iron Circle");
    assert_eq!(state.your_rank_id, LEADER_RANK);
    assert_eq!(state.members.len(), 1);
}

#[tokio::test]
async fn a_character_can_only_be_in_one_guild() {
    let game_state = make_test_game_state("guild_one");
    let auth = make_test_auth("guild_one");
    let (leader, mut rx) = founded(&game_state, &auth, "guild_one").await;

    game_state
        .create_guild(&auth, &leader, "Second Circle".to_string())
        .await;
    assert_eq!(
        denial(&drain(&mut rx)),
        Some(GuildDeniedReason::AlreadyInAGuild)
    );
}

#[tokio::test]
async fn a_taken_name_is_refused() {
    let game_state = make_test_game_state("guild_name");
    let auth = make_test_auth("guild_name");
    let (_leader, _rx) = founded(&game_state, &auth, "guild_name").await;
    let (other, mut other_rx) = member(&game_state, &auth, "guild_name", "Other").await;

    game_state
        .create_guild(&auth, &other, "The Iron Circle".to_string())
        .await;
    assert_eq!(
        denial(&drain(&mut other_rx)),
        Some(GuildDeniedReason::NameTaken)
    );
}

#[tokio::test]
async fn an_invite_has_to_be_accepted() {
    let game_state = make_test_game_state("guild_invite");
    let auth = make_test_auth("guild_invite");
    let (leader, mut leader_rx) = founded(&game_state, &auth, "guild_invite").await;
    let (recruit, mut recruit_rx) = member(&game_state, &auth, "guild_invite", "Recruit").await;

    game_state
        .invite_to_guild(&auth, &leader, "Recruit".to_string())
        .await;
    let invite = drain(&mut recruit_rx)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::GuildInvite { guild_id, .. } => Some(guild_id),
            _ => None,
        })
        .expect("the recruit is asked");
    assert!(
        game_state.guild_membership(&recruit).await.is_none(),
        "an invite is not a membership"
    );

    game_state
        .respond_guild_invite(&auth, &recruit, invite, true)
        .await;
    assert_eq!(
        game_state
            .guild_membership(&recruit)
            .await
            .map(|m| m.rank_id),
        Some(DEFAULT_RANK)
    );
    let state = guild_state(&drain(&mut leader_rx)).expect("the leader sees the roster grow");
    assert_eq!(state.members.len(), 2);
}

/// The rank table is the whole point of ranks: a recruit may put things in
/// and may not take them out.
#[tokio::test]
async fn the_lowest_rank_cannot_empty_the_vault() {
    let game_state = make_test_game_state("guild_vault_perm");
    let auth = make_test_auth("guild_vault_perm");
    let (leader, _leader_rx) = founded(&game_state, &auth, "guild_vault_perm").await;
    let (recruit, mut recruit_rx) = member(&game_state, &auth, "guild_vault_perm", "Recruit").await;
    game_state
        .invite_to_guild(&auth, &leader, "Recruit".to_string())
        .await;
    let guild_id = game_state
        .guild_membership(&leader)
        .await
        .expect("leader is in a guild")
        .guild_id;
    game_state
        .respond_guild_invite(&auth, &recruit, guild_id, true)
        .await;

    // Put something in from the leader's side so there is something to take.
    game_state.inventories.write().await.insert(
        pid("Leader"),
        PlayerInventory {
            bag: vec![bag_item(1, "healing_potion", 5)],
            ..Default::default()
        },
    );
    game_state.open_guild_storage(&auth, &leader).await;
    game_state.storage_deposit(&leader, 1, 5).await;

    game_state.open_guild_storage(&auth, &recruit).await;
    drain(&mut recruit_rx);
    game_state.storage_withdraw(&recruit, 0, 1).await;

    let bag_len = game_state.inventories.read().await[&recruit].bag.len();
    assert_eq!(bag_len, 0, "a recruit must not be able to withdraw");

    // …and the deposit side is allowed, so the refusal is the permission and
    // not a broken container.
    game_state.inventories.write().await.insert(
        recruit,
        PlayerInventory {
            bag: vec![bag_item(2, "healing_potion", 1)],
            ..Default::default()
        },
    );
    game_state.storage_deposit(&recruit, 2, 1).await;
    assert!(
        game_state.inventories.read().await[&recruit].bag.is_empty(),
        "a recruit may still contribute"
    );
}

#[tokio::test]
async fn guild_chat_reaches_members_and_nobody_else() {
    let game_state = make_test_game_state("guild_chat");
    let auth = make_test_auth("guild_chat");
    let (leader, _leader_rx) = founded(&game_state, &auth, "guild_chat").await;
    let (recruit, mut recruit_rx) = member(&game_state, &auth, "guild_chat", "Recruit").await;
    let (stranger, mut stranger_rx) = member(&game_state, &auth, "guild_chat", "Stranger").await;
    let guild_id = game_state
        .guild_membership(&leader)
        .await
        .expect("a guild")
        .guild_id;
    game_state
        .invite_to_guild(&auth, &leader, "Recruit".to_string())
        .await;
    game_state
        .respond_guild_invite(&auth, &recruit, guild_id, true)
        .await;
    drain(&mut recruit_rx);
    drain(&mut stranger_rx);

    game_state
        .send_guild_chat(&leader, "muster at the gate".to_string())
        .await;

    assert!(drain(&mut recruit_rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::GuildChatMessage { .. })));
    assert!(
        !drain(&mut stranger_rx)
            .iter()
            .any(|m| matches!(m, ServerMessage::GuildChatMessage { .. })),
        "a guild line must not leak to somebody outside it"
    );
    let _ = stranger;
}

/// A leader with members cannot walk out and leave the guild headless.
#[tokio::test]
async fn a_leader_must_hand_over_before_leaving() {
    let game_state = make_test_game_state("guild_leave");
    let auth = make_test_auth("guild_leave");
    let (leader, mut leader_rx) = founded(&game_state, &auth, "guild_leave").await;
    let (recruit, _recruit_rx) = member(&game_state, &auth, "guild_leave", "Recruit").await;
    let guild_id = game_state
        .guild_membership(&leader)
        .await
        .expect("a guild")
        .guild_id;
    game_state
        .invite_to_guild(&auth, &leader, "Recruit".to_string())
        .await;
    game_state
        .respond_guild_invite(&auth, &recruit, guild_id, true)
        .await;
    drain(&mut leader_rx);

    game_state.leave_guild(&auth, &leader).await;
    assert_eq!(
        denial(&drain(&mut leader_rx)),
        Some(GuildDeniedReason::LeaderMustHandOver)
    );

    game_state
        .transfer_guild_leadership(&auth, &leader, {
            let chars = game_state.player_characters.read().await;
            chars[&recruit].0
        })
        .await;
    drain(&mut leader_rx);
    game_state.leave_guild(&auth, &leader).await;
    assert!(game_state.guild_membership(&leader).await.is_none());
    assert_eq!(
        game_state
            .guild_membership(&recruit)
            .await
            .map(|m| m.rank_id),
        Some(LEADER_RANK)
    );
}

/// Only the leader hands out ranks: an officer who could promote could
/// promote themselves.
#[tokio::test]
async fn only_the_leader_sets_ranks() {
    let game_state = make_test_game_state("guild_ranks");
    let auth = make_test_auth("guild_ranks");
    let (leader, _leader_rx) = founded(&game_state, &auth, "guild_ranks").await;
    let (recruit, mut recruit_rx) = member(&game_state, &auth, "guild_ranks", "Recruit").await;
    let guild_id = game_state
        .guild_membership(&leader)
        .await
        .expect("a guild")
        .guild_id;
    game_state
        .invite_to_guild(&auth, &leader, "Recruit".to_string())
        .await;
    game_state
        .respond_guild_invite(&auth, &recruit, guild_id, true)
        .await;
    let recruit_character = game_state.player_characters.read().await[&recruit].0;
    drain(&mut recruit_rx);

    game_state
        .set_guild_rank(&auth, &recruit, recruit_character, 1)
        .await;
    assert_eq!(
        denial(&drain(&mut recruit_rx)),
        Some(GuildDeniedReason::NoPermission)
    );

    game_state
        .set_guild_rank(&auth, &leader, recruit_character, 1)
        .await;
    assert_eq!(
        game_state
            .guild_membership(&recruit)
            .await
            .map(|m| m.rank_id),
        Some(1)
    );
}
