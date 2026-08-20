//! Real-socket load client (IMP-6.3).
//!
//! `combat_scale_tests.rs` measures the game loop with players inserted
//! straight into `GameState`. That answers "does the simulation scale", and
//! it left the other half unanswered: **nothing had ever opened five thousand
//! WebSockets at this server.** Handshakes, per-connection tasks, TLS-less
//! framing, the outbound queues added in IMP-5.1 and the memory all of that
//! costs are invisible to an in-process test.
//!
//! This opens real connections, authenticates each as an NPC (the one
//! headless path the protocol offers — see `doc/REMOTE_AGENT_CLIENT.md`),
//! enters the game, and then walks. It reports what got through and what did
//! not; the server's own view comes from `/api/metrics` (IMP-5.5), which the
//! harness script reads.

use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::{ClientMessage, ServerMessage};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

#[derive(Parser)]
#[command(about = "Open N real client connections and hold them")]
struct Args {
    /// WebSocket URL of the game server.
    #[arg(long, default_value = "ws://127.0.0.1:10076")]
    url: String,
    /// The server's npc_token (state-dir/npc_token).
    #[arg(long)]
    token: String,
    /// How many connections to open.
    #[arg(long, default_value_t = 100)]
    clients: usize,
    /// How many to start at once. The whole point is a stampede, but an
    /// unbounded one measures the harness rather than the server.
    #[arg(long, default_value_t = 50)]
    concurrency: usize,
    /// Seconds to hold the connections open, moving, once all are in.
    #[arg(long, default_value_t = 10)]
    hold_secs: u64,
    /// Distinguishes accounts between runs against the same database.
    #[arg(long, default_value = "load")]
    tag: String,
}

#[derive(Default)]
struct Tally {
    connected: AtomicUsize,
    entered: AtomicUsize,
    failed: AtomicUsize,
    dropped_mid_hold: AtomicUsize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let tally = Arc::new(Tally::default());
    let started = Instant::now();

    let mut in_flight = Vec::new();
    let permits = Arc::new(tokio::sync::Semaphore::new(args.concurrency));
    for i in 0..args.clients {
        let permit = Arc::clone(&permits).acquire_owned().await?;
        let (url, token, tag) = (args.url.clone(), args.token.clone(), args.tag.clone());
        let tally = Arc::clone(&tally);
        let hold = Duration::from_secs(args.hold_secs);
        in_flight.push(tokio::spawn(async move {
            // The permit covers the handshake only. Holding it through the
            // hold would stagger the clients so that early ones log out
            // before late ones arrive, and the run would measure serial
            // throughput while claiming to measure concurrency.
            if let Err(e) = one_client(&url, &token, &tag, i, hold, &tally, permit).await {
                if tally.failed.fetch_add(1, Ordering::Relaxed) < 5 {
                    eprintln!("client {i}: {e}");
                }
            }
        }));
    }
    let ramp = started.elapsed();

    for task in in_flight {
        let _ = task.await;
    }

    let connected = tally.connected.load(Ordering::Relaxed);
    let entered = tally.entered.load(Ordering::Relaxed);
    println!("clients          {}", args.clients);
    println!("connected        {connected}");
    println!("entered world    {entered}");
    println!("failed           {}", tally.failed.load(Ordering::Relaxed));
    println!(
        "dropped in hold  {}",
        tally.dropped_mid_hold.load(Ordering::Relaxed)
    );
    println!("ramp             {ramp:.2?} to start all");
    println!("total            {:.2?}", started.elapsed());
    Ok(())
}

async fn one_client(
    url: &str,
    token: &str,
    tag: &str,
    index: usize,
    hold: Duration,
    tally: &Tally,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> anyhow::Result<()> {
    let (mut ws, _) = tokio_tungstenite::connect_async(url).await?;
    tally.connected.fetch_add(1, Ordering::Relaxed);

    // The protocol refuses everything until it knows the version.
    send(
        &mut ws,
        &ClientMessage::ClientInfo {
            protocol_version: onlinerpg_shared::PROTOCOL_VERSION,
            client_kind: "load".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
        },
    )
    .await?;

    let account = format!("npc_{tag}_{index}");
    send(
        &mut ws,
        &ClientMessage::AuthenticateNpc {
            account_name: account.clone(),
            npc_token: token.to_string(),
        },
    )
    .await?;

    let characters = loop {
        match recv(&mut ws).await? {
            ServerMessage::AuthSuccess { characters, .. } => break characters,
            ServerMessage::AuthError { message } => anyhow::bail!("auth refused: {message}"),
            _ => continue,
        }
    };

    let character_id = match characters.first() {
        Some(c) => c.id,
        None => {
            // The server rolls attributes and remembers the roll; creation
            // without one is refused ("Roll attributes first").
            send(
                &mut ws,
                &ClientMessage::RollCharacterStats {
                    character_class: onlinerpg_shared::CharacterClass::Knight,
                    gender: onlinerpg_shared::Gender::Male,
                },
            )
            .await?;
            loop {
                match recv(&mut ws).await? {
                    ServerMessage::CharacterStatsRolled { .. } => break,
                    ServerMessage::CharacterError { message } => {
                        anyhow::bail!("roll refused: {message}")
                    }
                    _ => continue,
                }
            }
            send(
                &mut ws,
                &ClientMessage::CreateCharacter {
                    character_name: format!("Load{tag}{index}"),
                    character_class: onlinerpg_shared::CharacterClass::Knight,
                    gender: onlinerpg_shared::Gender::Male,
                },
            )
            .await?;
            loop {
                match recv(&mut ws).await? {
                    ServerMessage::CharacterCreated { character } => break character.id,
                    ServerMessage::CharacterError { message } => {
                        anyhow::bail!("create refused: {message}")
                    }
                    _ => continue,
                }
            }
        }
    };

    send(&mut ws, &ClientMessage::EnterGame { character_id }).await?;
    let mut position = loop {
        match recv(&mut ws).await? {
            ServerMessage::JoinSuccess { player, .. } => break player.position,
            ServerMessage::AuthError { message } => anyhow::bail!("entry refused: {message}"),
            _ => continue,
        }
    };
    tally.entered.fetch_add(1, Ordering::Relaxed);
    // In. Let the next client start its handshake while this one stays.
    drop(permit);

    // Hold the connection the way a player does: keep reading (a client that
    // stops reading is what IMP-5.1's backpressure is for, and that is a
    // different experiment) and keep moving so the server has real work.
    let until = Instant::now() + hold;
    let mut step = tokio::time::interval(Duration::from_millis(500));
    while Instant::now() < until {
        tokio::select! {
            _ = step.tick() => {
                position.x += 0.5;
                if send(&mut ws, &ClientMessage::PlayerMove {
                    position,
                    rotation: 0.0,
                    floor_level: 0,
                    append: false,
                    sprinting: false,
                }).await.is_err() {
                    tally.dropped_mid_hold.fetch_add(1, Ordering::Relaxed);
                    return Ok(());
                }
            }
            incoming = ws.next() => {
                match incoming {
                    Some(Ok(_)) => {}
                    _ => {
                        tally.dropped_mid_hold.fetch_add(1, Ordering::Relaxed);
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send(ws: &mut Socket, msg: &ClientMessage) -> anyhow::Result<()> {
    let bytes = onlinerpg_shared::serialize_client_msg(msg)?;
    ws.send(Message::Binary(bytes.into())).await?;
    Ok(())
}

async fn recv(ws: &mut Socket) -> anyhow::Result<ServerMessage> {
    loop {
        match ws.next().await {
            Some(Ok(Message::Binary(bytes))) => {
                return Ok(onlinerpg_shared::deserialize_server_msg(&bytes)?)
            }
            Some(Ok(_)) => continue,
            Some(Err(e)) => return Err(e.into()),
            None => anyhow::bail!("connection closed"),
        }
    }
}
