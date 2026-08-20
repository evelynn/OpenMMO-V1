//! Per-player outbound queue bounds (IMP-5.1).
//!
//! SPK-1 left this: the direct channel was an unbounded mpsc, so a client
//! that stops reading grows server memory without limit, and 5,000 of them
//! is not a rounding error.
//!
//! A bounded `mpsc::Sender` alone would be worse, not better — its `send`
//! awaits when the queue is full, so one stalled client would stall the game
//! tick that is fanning out to it. **Nothing here ever awaits.** Sends are
//! `try_send`, and what happens on a full queue depends on the message:
//!
//! - [`Delivery::Lossy`] — absolute state that the next message restates
//!   (where someone is, what time it is). Dropping one costs a frame of
//!   smoothness and nothing else.
//! - [`Delivery::Reliable`] — a state transition (an item moved, a trade
//!   closed). Dropping one desyncs the client permanently, so the connection
//!   is closed instead; reconnecting re-sends the snapshots.

use super::DirectMessage;
use onlinerpg_shared::ServerMessage;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Outbound queue depth per player.
///
/// Sized against SPK-1's measured worst case: 200 players in one melee moved
/// 849 KiB/s in total. A few hundred messages is seconds of headroom for a
/// briefly busy client and still a hard ceiling for a stalled one — and
/// fanout messages are `Shared(Bytes)`, one allocation refcounted across
/// every recipient, so a queued copy costs a handle rather than a payload.
pub const DIRECT_QUEUE_DEPTH: usize = 256;

/// What a full queue means for one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// May be dropped: a later message states the same thing in full.
    Lossy,
    /// Must arrive or the client is wrong about the world.
    Reliable,
}

/// How to treat `msg` when the receiver is behind.
///
/// The allowlist is deliberately short and everything else is `Reliable`:
/// getting this wrong in the lossy direction silently desyncs a player, and
/// getting it wrong in the reliable direction only costs a disconnect.
pub fn delivery_of(msg: &ServerMessage) -> Delivery {
    match msg {
        // Absolute positions, restated on the next tick.
        ServerMessage::PlayerMoved { .. } | ServerMessage::MonsterMoved { .. } => Delivery::Lossy,
        // The clock is a level, not an edge.
        ServerMessage::GameTimeSync { .. } => Delivery::Lossy,
        _ => Delivery::Reliable,
    }
}

/// One player's outbound queue. Cloneable: the game state holds one per
/// connected player and every fanout borrows it.
#[derive(Debug, Clone)]
pub struct DirectChannel {
    tx: mpsc::Sender<DirectMessage>,
    dropped: Arc<AtomicU64>,
    /// Set when a `Reliable` message could not be queued. The connection
    /// loop closes on seeing it — a desynced client is worse than a
    /// disconnected one.
    overflowed: Arc<AtomicBool>,
}

impl DirectChannel {
    pub fn new() -> (Self, mpsc::Receiver<DirectMessage>) {
        let (tx, rx) = mpsc::channel(DIRECT_QUEUE_DEPTH);
        let channel = Self {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
            overflowed: Arc::new(AtomicBool::new(false)),
        };
        (channel, rx)
    }

    /// Queue one message without ever awaiting. Returns false when the
    /// message did not make it.
    pub fn send(&self, msg: DirectMessage, delivery: Delivery) -> bool {
        match self.tx.try_send(msg) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                match delivery {
                    Delivery::Lossy => {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    Delivery::Reliable => self.overflowed.store(true, Ordering::Relaxed),
                }
                false
            }
            // The connection is already gone; its cleanup is on its own path.
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    /// Typed convenience: classifies the message itself.
    pub fn send_typed(&self, msg: ServerMessage) -> bool {
        let delivery = delivery_of(&msg);
        self.send(DirectMessage::Typed(msg), delivery)
    }

    /// Whether a reliable message was lost, meaning this connection must go.
    pub fn overflowed(&self) -> bool {
        self.overflowed.load(Ordering::Relaxed)
    }

    /// Lossy messages dropped so far, for the operations endpoint.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}
