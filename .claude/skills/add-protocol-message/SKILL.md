---
name: add-protocol-message
description: Add or change a WebSocket message in OpenMMO's shared protocol, touching shared/src/messages.rs, the server dispatch and game_state, and the client's networkTypes/messageHandlers/socket in one coherent change. Use when the user says "프로토콜 추가", "서버-클라 통신 만들자", "add a websocket message", "새 패킷", or a feature needs the client and server to exchange something new.
---

# Adding a protocol message

The wire protocol is MessagePack over one WebSocket, defined once in
`shared/src/messages.rs`: `ClientMessage` (requests) and `ServerMessage`
(pushes). Both are used by browsers **and** by the Rust agent client.

## Non-negotiable: agent–human parity

Never add an agent-only endpoint, field, or shortcut. The server must not be
able to tell an agent from a human. Anything a new message enables must be
reachable by both through the same variant.

## Order of work

### 1. `shared/src/messages.rs`

Add the variant to `ClientMessage` and/or `ServerMessage`. Reuse existing types
from `character.rs`, `entity.rs`, `inventory.rs`, `world.rs` rather than
redeclaring shapes. Keep the payload minimal — see Performance below.

### 2. Server

- Dispatch in `server/src/connection.rs`, alongside the existing
  `ClientMessage::…` match arms. Mind the authentication gate: only
  `Authenticate` / `AuthenticateNpc` are legal pre-auth, and debug variants are
  admin-only.
- State transitions belong in the matching `server/src/game_state/*.rs` module
  (combat, inventory, party, trading, dungeon, …), not in the connection loop.
- **Validate every input server-side.** The client is untrusted: range, cooldown,
  ownership, and inventory capacity are all decided here.
- Push the result back with the `ServerMessage` variant, addressed to the
  narrowest audience that needs it.

### 3. Client

- Type in `client/src/lib/network/networkTypes.ts`.
- `case '<Variant>':` in `client/src/lib/network/messageHandlers.ts`, updating
  the relevant store in `lib/stores/`.
- Send through the network manager in `client/src/lib/network/socket.ts`.
- Never predict authoritative outcomes locally — render what the server sent.

### 4. Agent client

If an NPC should use the new capability, wire it in `agent-client/src/ws.rs`
and the relevant `driver/`/`state/` module.

## Compatibility

MessagePack variants are positional/tagged — **adding or reordering fields is
not backward compatible**. Server and client ship together; a mismatched pair
fails at deserialization, not gracefully. Do not stage a protocol change across
two deploys unless the variant is genuinely additive and unused by the old peer.

## Performance (5,000 concurrent users)

- Broadcast to an area of interest, never to every connected player.
- Prefer deltas over full snapshots; every byte is multiplied by the audience.
- Do not add a new per-tick push. Ride an existing tick (movement is 5 Hz) or
  make it event-driven.
- Serialize outside any held lock.

## Verify

```bash
cargo test --workspace --locked
npm --prefix client run build:wasm && npm --prefix client test
```

Then exercise the round trip in-game (`game-login` skill) with the server at
`RUST_LOG=debug`. Run `/preflight` before committing.
