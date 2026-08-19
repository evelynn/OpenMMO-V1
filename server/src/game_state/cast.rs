//! Server-side state for the four cast times (doc/COMBAT.md, IMP-3.1).
//!
//! Nothing writes these maps yet — skills arrive with IMP-3.2. What exists
//! now is the storage and the two paths that must already be right when they
//! do: moving cancels a variable cast, and a disconnect leaves nothing
//! behind. Expiry is never swept; a deadline is compared when the player next
//! tries to act, so an idle cooldown costs nothing.

use onlinerpg_shared::cast::CastSchedule;
use onlinerpg_shared::skills::SkillId;
use onlinerpg_shared::PlayerId;
use tracing::debug;

/// One player's per-skill cooldown deadlines.
pub(crate) type SkillCooldowns = Vec<(SkillId, u64)>;

/// A cast in flight: which skill, and when each of its four spans ends.
pub(crate) struct CastState {
    pub skill: SkillId,
    pub schedule: CastSchedule,
    /// Which use this is. The delayed task that lands the cast carries the
    /// same number, so a task belonging to a cancelled cast cannot land the
    /// one that replaced it.
    pub seq: u64,
    pub target: String,
    /// The level bought, read at resolution so a point spent mid-cast does
    /// not retroactively strengthen a cast already in flight.
    pub level: u32,
}

impl super::GameState {
    /// Moving during the variable part cancels the cast; past it the caster
    /// is committed and walking changes nothing.
    pub(crate) async fn cancel_cast_on_move(&self, player_id: &PlayerId) {
        let now = Self::now_ms();
        let mut casting = self.casting.write().await;
        let Some(state) = casting.get(player_id) else {
            return;
        };
        if !state.schedule.cancels_on_move_at(now) {
            return;
        }
        debug!(
            "Cast of {} at {} cancelled: player {player_id} moved {}ms before it landed",
            state.skill.as_str(),
            state.target,
            state.schedule.cast_ends_at_ms.saturating_sub(now)
        );
        casting.remove(player_id);
        drop(casting);
        self.announce_cast_cancelled(player_id).await;
    }

    /// Drop every cast deadline a player owns. Cooldowns are session state by
    /// design (nothing persists them), so logging out clears them along with
    /// the rest — and, more to the point, the maps cannot grow across a day
    /// of reconnects.
    pub(crate) async fn clear_cast_state(&self, player_id: &PlayerId) {
        self.casting.write().await.remove(player_id);
        self.global_cast_delay_until.write().await.remove(player_id);
        self.skill_cooldowns.write().await.remove(player_id);
    }
}
