//! Operational endpoints (IMP-5.5).
//!
//! Before this the server had no way to answer "are you alive" — the only
//! HTTP routes were content. A service aiming at 5,000 concurrent players
//! needs a load balancer to be able to ask, and needs the backpressure added
//! in IMP-5.1 to be visible; a limit nobody can see working is a limit
//! nobody trusts.

use crate::connection::AuthContext;
use crate::game_state::GameState;
use axum::{
    extract::State,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::watch;

/// What the server knows about itself right now.
#[derive(Debug, Serialize)]
pub struct Metrics {
    pub players_online: usize,
    pub monsters_alive: usize,
    /// Lossy messages dropped across every live connection since it opened
    /// (IMP-5.1). A number that climbs means clients are falling behind.
    pub dropped_messages: u64,
    /// Connections whose outbound queue lost something it could not lose;
    /// each of these is closing.
    pub overflowed_connections: usize,
    /// False once a shutdown has begun: stop routing new players here.
    pub accepting: bool,
    /// Players per channel (IMP-7.1). Capacity is per channel, not per
    /// server, so a single total would hide a full channel next to an empty
    /// one.
    pub channels: Vec<onlinerpg_shared::channel::ChannelOccupancy>,
}

#[derive(Clone)]
struct Ops {
    game_state: Arc<GameState>,
    draining: watch::Receiver<()>,
}

/// Health, readiness and metrics. Health and readiness are public — a load
/// balancer cannot hold a token — while metrics needs the admin credential,
/// because who is online is operational information.
pub fn ops_router(
    game_state: Arc<GameState>,
    auth_ctx: Arc<AuthContext>,
    draining: watch::Receiver<()>,
) -> Router {
    let ops = Ops {
        game_state,
        draining,
    };
    let public = Router::new()
        .route("/api/health", get(health))
        .route("/api/ready", get(ready))
        .with_state(ops.clone());
    let admin = Router::new()
        .route("/api/metrics", get(metrics))
        .with_state(ops)
        .layer(axum::middleware::from_fn_with_state(
            auth_ctx,
            require_admin,
        ));
    public.merge(admin)
}

/// Liveness: the process is answering. Deliberately touches nothing else —
/// a health check that consults the database restarts the server when the
/// database hiccups.
async fn health() -> Response {
    StatusCode::OK.into_response()
}

/// Readiness: should new players be routed here. Goes false the moment a
/// graceful shutdown starts, so the drain is not fed while it drains.
async fn ready(State(ops): State<Ops>) -> Response {
    if is_draining(&ops) {
        return (StatusCode::SERVICE_UNAVAILABLE, "draining").into_response();
    }
    StatusCode::OK.into_response()
}

async fn metrics(State(ops): State<Ops>) -> Response {
    let (dropped_messages, overflowed_connections) = ops.game_state.outbound_queue_health().await;
    Json(Metrics {
        players_online: ops.game_state.get_player_count().await,
        monsters_alive: ops.game_state.monster_count().await,
        dropped_messages,
        overflowed_connections,
        accepting: !is_draining(&ops),
        channels: ops.game_state.occupancy(),
    })
    .into_response()
}

fn is_draining(ops: &Ops) -> bool {
    ops.draining.has_changed().unwrap_or(true)
}

/// The write guard's read-side twin: metrics is a GET, which
/// `require_admin_for_writes` waves through on purpose.
async fn require_admin(
    State(auth): State<Arc<AuthContext>>,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match token {
        Some(token) if crate::connection::token_matches(token, &auth.npc_token) => {
            Ok(next.run(req).await)
        }
        _ => Err((
            StatusCode::UNAUTHORIZED,
            "metrics requires the operator token".to_string(),
        )),
    }
}
