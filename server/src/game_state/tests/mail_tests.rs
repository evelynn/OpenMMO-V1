use super::super::mail::Letter;
use super::*;
use crate::auth::AuthService;
use onlinerpg_shared::messages::{MailAttachment, MailState};

/// A character in the DB, in the world, and with a direct channel — the three
/// things every mail path needs.
async fn postie(test_name: &str) -> (GameState, Arc<AuthService>, i64, DirectRx) {
    let game_state = make_test_game_state(test_name);
    let auth = make_test_auth(test_name);
    let account = auth.login_npc(&format!("npc_{test_name}")).unwrap();
    let record = create_test_character(&auth, &account, "Postie");
    game_state.add_player(make_player("Postie", 0.0, 0.0)).await;
    game_state
        .register_player_character(
            &pid("Postie"),
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
        .insert(pid("Postie"), PlayerInventory::default());
    let rx = game_state.register_direct_channel(&pid("Postie")).await;
    (game_state, auth, record.id, rx)
}

fn drain(rx: &mut DirectRx) -> Vec<ServerMessage> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        out.push(msg);
    }
    out
}

fn apple(quantity: u32) -> MailAttachment {
    MailAttachment {
        item_def_id: "apple".to_string(),
        quantity,
        enchant: 0,
    }
}

#[tokio::test]
async fn claiming_moves_gold_and_attachments_into_the_bag() {
    let (game_state, auth, character_id, mut rx) = postie("mail_claim").await;
    let mail_id = game_state
        .deliver_mail(
            &auth,
            character_id,
            Letter {
                sender: "System".to_string(),
                subject: "Reward".to_string(),
                body: String::new(),
                gold: 7,
                items: vec![apple(2)],
            },
        )
        .await
        .expect("mail delivered");
    drain(&mut rx);

    game_state.claim_mail(&auth, &pid("Postie"), mail_id).await;

    let msgs = drain(&mut rx);
    assert!(msgs.iter().any(|m| matches!(
        m,
        ServerMessage::MailUpdated {
            state: MailState::Claimed,
            ..
        }
    )));
    let inventories = game_state.inventories.read().await;
    let bag = &inventories[&pid("Postie")].bag;
    assert_eq!(
        bag.iter()
            .filter(|i| i.item_def_id == "apple")
            .map(|i| i.quantity)
            .sum::<u32>(),
        2
    );
    drop(inventories);
    assert_eq!(game_state.get_player_gold(&pid("Postie")).await, 7);
    assert!(auth.load_mail(character_id).unwrap().is_empty());
}

#[tokio::test]
async fn an_overweight_claim_is_refused_and_the_mail_stays() {
    let (game_state, auth, character_id, mut rx) = postie("mail_overweight").await;
    // STR 12 → 180 kg. One apple weighs little, so ask for far more than fits.
    let mail_id = game_state
        .deliver_mail(
            &auth,
            character_id,
            Letter {
                sender: "System".to_string(),
                subject: "Too much".to_string(),
                body: String::new(),
                gold: 0,
                items: vec![apple(100_000)],
            },
        )
        .await
        .expect("mail delivered");
    drain(&mut rx);

    game_state.claim_mail(&auth, &pid("Postie"), mail_id).await;

    let msgs = drain(&mut rx);
    assert!(
        msgs.iter().any(|m| matches!(
            m,
            ServerMessage::MailUpdated {
                state: MailState::ClaimBlocked,
                ..
            }
        )),
        "an overweight claim must report ClaimBlocked"
    );
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::InventoryUpdated { .. })),
        "nothing may move on a blocked claim"
    );
    let left = auth.load_mail(character_id).unwrap();
    assert_eq!(left.len(), 1, "the mail must survive a refused claim");
    assert_eq!(left[0].id, mail_id);
}

#[tokio::test]
async fn delivery_pushes_the_unread_badge_and_opening_clears_it() {
    let (game_state, auth, character_id, mut rx) = postie("mail_badge").await;
    game_state
        .deliver_mail(&auth, character_id, Letter::note("System", "Hello"))
        .await
        .expect("mail delivered");

    let counts: Vec<u16> = drain(&mut rx)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::MailUnread { count } => Some(count),
            _ => None,
        })
        .collect();
    assert_eq!(counts, vec![1]);

    game_state.open_mailbox(&auth, &pid("Postie")).await;
    let msgs = drain(&mut rx);
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::MailList { mail } if mail.len() == 1)));
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::MailUnread { count: 0 })));
}

#[tokio::test]
async fn the_hourly_sweep_drops_expired_mail() {
    let (game_state, auth, character_id, _rx) = postie("mail_sweep").await;
    let items = [apple(1)];
    auth.insert_mail(crate::auth::NewMail {
        recipient_character_id: character_id,
        sender: "System",
        subject: "ancient",
        body: "",
        gold: 0,
        items: &items,
        now: 0,
    })
    .unwrap();

    game_state.sweep_expired_mail(&auth).await;

    assert!(auth.load_mail(character_id).unwrap().is_empty());
}
