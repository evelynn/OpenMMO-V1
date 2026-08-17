//! Behavior input/output types: the internal [`AiState`], the [`NearbyPlayer`]
//! projection fed into each tick, and the [`AiCommand`]/[`TickResult`] outputs.

use crate::{MonsterState, PlayerId, Position};
use serde::{Deserialize, Serialize};

/// Internal behavior state (superset of network [`MonsterState`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AiState {
    #[default]
    Idle,
    Walk,
    Run,
    Chase,
    Attack,
    Hit,
    Dead,
    Flee,
    Return,
}

impl AiState {
    pub fn to_monster_state(self) -> MonsterState {
        match self {
            AiState::Idle => MonsterState::Idle,
            AiState::Walk => MonsterState::Walk,
            AiState::Run => MonsterState::Run,
            AiState::Chase => MonsterState::Run,
            AiState::Attack => MonsterState::Attack,
            AiState::Hit => MonsterState::Hit,
            AiState::Dead => MonsterState::Dead,
            AiState::Flee => MonsterState::Run,
            AiState::Return => MonsterState::Walk,
        }
    }
}

/// Minimal player projection for behavior input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearbyPlayer {
    pub id: PlayerId,
    pub position: Position,
    pub health: u32,
}

/// Minimal ground-item projection for looter behavior. The caller filters to
/// the monster's own floor, so the brain never sees an item it could not
/// legally reach — the server re-checks anyway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearbyGroundItem {
    pub instance_id: u64,
    pub position: Position,
}

/// Behavior output — translated by the caller into network messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AiCommand {
    Move {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: MonsterState,
        target_position: Position,
    },
    Attack {
        monster_id: String,
        target_player_id: PlayerId,
    },
    /// A looter wants the ground item it is standing over. The server owns
    /// the decision: this is a request, never a transfer.
    PickUpItem {
        monster_id: String,
        instance_id: u64,
    },
}

/// Result of a single brain tick — always includes current position/rotation
/// so the caller can update the visual even when no commands are emitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickResult {
    pub commands: Vec<AiCommand>,
    pub position: Position,
    pub rotation: f32,
    pub state: MonsterState,
}
