//! Mailbox: the delivery path for rewards that must not evaporate when the
//! recipient is offline or carrying a full bag (doc/ragnarok/12_UX_SERVICES.md
//! §4, IMP-2.1). System-sent only — player-to-player mail is out of scope.
//!
//! Writes commit immediately on a blocking pool rather than joining the batch
//! save: mail is rare (one row per reward, not per tick) and carries goods, so
//! a crash between grant and flush would be a lost reward.

use crate::auth::{AuthError, AuthService, NewMail};
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::messages::{MailAttachment, MailState, MailSummary};
use std::sync::Arc;
use tracing::{info, warn};

use super::auth_db;
use super::inventory::{stack_into_bag, BagInsert};

/// What to put in one letter. Grouped so delivery reads as data and every
/// reward path fills the same shape.
pub(crate) struct Letter {
    pub sender: String,
    pub subject: String,
    pub body: String,
    pub gold: i64,
    pub items: Vec<MailAttachment>,
}

impl Letter {
    /// A system letter with no attachments — the operator case.
    pub(crate) fn note(sender: &str, subject: &str) -> Self {
        Self {
            sender: sender.to_string(),
            subject: subject.to_string(),
            body: String::new(),
            gold: 0,
            items: Vec::new(),
        }
    }
}

impl super::GameState {
    /// Deliver one letter and refresh the recipient's unread badge if they are
    /// online. Returns the new mail id, or `None` when the mailbox is full —
    /// the caller decides whether that is a warning or a hard failure.
    pub async fn deliver_mail(
        &self,
        auth_service: &Arc<AuthService>,
        recipient_character_id: i64,
        letter: Letter,
    ) -> Option<i64> {
        let auth = auth_service.clone();
        let Letter {
            sender,
            subject,
            body,
            gold,
            items,
        } = letter;
        let result = auth_db(move || {
            auth.insert_mail(NewMail {
                recipient_character_id,
                sender: &sender,
                subject: &subject,
                body: &body,
                gold,
                items: &items,
                now: now_secs(),
            })
        })
        .await;

        let mail_id = match result {
            Ok(id) => id,
            Err(AuthError::MailboxFull) => {
                warn!("Mail to character {recipient_character_id} dropped: mailbox full");
                return None;
            }
            Err(err) => {
                warn!("Mail to character {recipient_character_id} failed: {err}");
                return None;
            }
        };

        if let Some(player_id) = self.player_of_character(recipient_character_id).await {
            self.push_unread_mail(auth_service, &player_id).await;
        }
        Some(mail_id)
    }

    /// Reply to `OpenMailbox`. Reading the list is what clears the badge, so
    /// the count push follows the list in the same round trip.
    pub async fn open_mailbox(&self, auth_service: &Arc<AuthService>, player_id: &PlayerId) {
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth = auth_service.clone();
        let Ok(Ok(mail)) = tokio::task::spawn_blocking(move || auth.load_mail(character_id)).await
        else {
            warn!("Mailbox load failed for character {character_id}");
            return;
        };
        self.send_direct_message(player_id, ServerMessage::MailList { mail })
            .await;
        self.send_direct_message(player_id, ServerMessage::MailUnread { count: 0 })
            .await;
    }

    /// All-or-nothing claim: gold and every attachment move together, or
    /// nothing does and the letter stays. Partial claims would mean storing
    /// per-attachment state on the mail, which is a second inventory.
    pub async fn claim_mail(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        mail_id: i64,
    ) {
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth = auth_service.clone();
        let Ok(Some(mail)) = auth_db(move || auth.load_one_mail(character_id, mail_id)).await
        else {
            return;
        };

        let added_weight: f32 = mail
            .items
            .iter()
            .map(|item| self.item_defs.weight(&item.item_def_id) * item.quantity as f32)
            .sum();
        let max_weight = self.max_carry_weight(player_id).await;
        let instance_ids = self.reserve_instance_ids(total_units(&mail)).await;

        let snapshot = {
            let mut inventories = self.inventories.write().await;
            let Some(inventory) = inventories.get_mut(player_id) else {
                return;
            };
            if self.calc_total_weight(inventory) + added_weight > max_weight {
                drop(inventories);
                return self.reject_claim(player_id, mail_id).await;
            }
            let mut next_id = instance_ids;
            for item in &mail.items {
                next_id += stack_into_bag(
                    &mut inventory.bag,
                    BagInsert {
                        stackable: self.item_defs.stackable(&item.item_def_id),
                        item_def_id: &item.item_def_id,
                        enchant: item.enchant,
                        first_instance_id: next_id,
                        quantity: item.quantity,
                    },
                );
            }
            inventory.clone()
        };

        // The row goes only after the goods are in the bag: a crash in between
        // leaves the letter claimable rather than the reward gone.
        let auth = auth_service.clone();
        let removed = auth_db(move || auth.delete_mail(character_id, mail_id))
            .await
            .unwrap_or(false);
        if !removed {
            warn!("Mail {mail_id} banked for character {character_id} but the row survived");
        }
        if mail.gold != 0 {
            {
                let mut gold_map = self.player_gold.write().await;
                *gold_map.entry(*player_id).or_insert(0) += mail.gold;
            }
            self.send_gold_update(player_id).await;
        }
        self.mark_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        self.send_direct_message(
            player_id,
            ServerMessage::MailUpdated {
                mail_id,
                state: MailState::Claimed,
            },
        )
        .await;
        info!("Character {character_id} claimed mail {mail_id}");
    }

    pub async fn delete_mail(
        &self,
        auth_service: &Arc<AuthService>,
        player_id: &PlayerId,
        mail_id: i64,
    ) {
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth = auth_service.clone();
        let _ = auth_db(move || auth.delete_mail(character_id, mail_id)).await;
        self.send_direct_message(
            player_id,
            ServerMessage::MailUpdated {
                mail_id,
                state: MailState::Deleted,
            },
        )
        .await;
    }

    /// Push the unread badge. Sent on join and whenever mail arrives — never
    /// the letters themselves, which is what keeps this cheap at 5,000 users.
    pub async fn push_unread_mail(&self, auth_service: &Arc<AuthService>, player_id: &PlayerId) {
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth = auth_service.clone();
        let Ok(count) = auth_db(move || auth.unread_mail_count(character_id)).await else {
            return;
        };
        if count > 0 {
            self.send_direct_message(player_id, ServerMessage::MailUnread { count })
                .await;
        }
    }

    /// Global expiry sweep, one statement. Rides the hourly buyback tick
    /// instead of walking players.
    pub async fn sweep_expired_mail(&self, auth_service: &Arc<AuthService>) {
        let auth = auth_service.clone();
        if let Ok(removed) = auth_db(move || auth.delete_expired_mail(now_secs())).await {
            if removed > 0 {
                info!("Swept {removed} expired mail row(s)");
            }
        }
    }

    /// `/mail <name> <message>` — an operator letter, delivered to a character
    /// whether or not they are online. No attachments: an admin who needs to
    /// hand over goods uses `/give` on a present player.
    pub(super) async fn mail_command(
        &self,
        admin_id: &PlayerId,
        name: &str,
        message: &str,
        auth_service: &Arc<AuthService>,
    ) -> Result<String, String> {
        if name.is_empty() || message.is_empty() {
            return Err("Mail: /mail <name> <message>".to_string());
        }
        let auth = auth_service.clone();
        let wanted = name.to_string();
        let recipient = match auth_db(move || auth.character_id_of_name(&wanted)).await {
            Ok(Some(id)) => id,
            Ok(None) => return Err(format!("Mail: no character named {name}.")),
            Err(err) => {
                warn!("Mail lookup failed: {err}");
                return Err("Mail: could not read the character.".to_string());
            }
        };
        let sender = self.player_name_of(admin_id).await;
        match self
            .deliver_mail(auth_service, recipient, Letter::note(&sender, message))
            .await
        {
            Some(id) => Ok(format!("Mail #{id} sent to {name}.")),
            None => Err(format!("Mail: {name}'s mailbox is full.")),
        }
    }

    async fn reject_claim(&self, player_id: &PlayerId, mail_id: i64) {
        self.send_direct_message(
            player_id,
            ServerMessage::MailUpdated {
                mail_id,
                state: MailState::ClaimBlocked,
            },
        )
        .await;
    }

    async fn player_of_character(&self, character_id: i64) -> Option<PlayerId> {
        self.player_characters
            .read()
            .await
            .iter()
            .find(|(_, (id, _, _))| *id == character_id)
            .map(|(player_id, _)| *player_id)
    }
}

fn total_units(mail: &MailSummary) -> u64 {
    mail.items.iter().map(|item| u64::from(item.quantity)).sum()
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
