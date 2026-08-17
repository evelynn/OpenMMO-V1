---
name: dev-run
description: Start the OpenMMO dev stack in the right order — game server, shared→WASM watch, and the Vite client — and know what to restart after which kind of change. Use when the user says "서버 띄워줘", "개발 서버 실행", "run the game locally", "start the client", or asks why a change is not showing up in the browser.
---

# Running the dev stack

Full reference: [doc/DEVELOPMENT.md](../../../doc/DEVELOPMENT.md) §1, §5.

Verify the environment first if there is any doubt: `bash tools/dev-setup.sh --check`.

## Three long-lived processes

Run each in its own background shell from the repo root. They never exit — do
not wait on them.

```bash
# 1. Game server: WS 10006, REST 10007 (both 127.0.0.1)
cargo watch -w server -w shared -w data-src -x "run -p onlinerpg-server"

# 2. shared crate -> browser WASM
cargo watch -w shared -s "npm run build:wasm --prefix client"

# 3. Client
npm --prefix client run dev -- --port 10004
```

Without `cargo-watch`, substitute `cargo run -p onlinerpg-server` and rerun by hand.

Vite proxies `/ws → 10006` and `/api → 10007`, so no extra proxy is needed.
Open `http://localhost:10004/`.

Optional: GLB editor `npm --prefix tools/glb-editor run dev -- --port 10005`;
NPC agent `cd agent-client && cargo watch -i "data/prompts/memory/" -x run`.

## What to restart after what

| Changed | Action |
|---|---|
| `server/` | terminal 1 restarts itself |
| `shared/` | terminals 1 **and** 2 — the browser needs the WASM rebuild |
| `client/` | nothing, HMR |
| `data-src/*.csv` | terminal 1 rebuilds; run `npm --prefix client run generate:csv` for the client |
| new GLB / sound / `assets.lock` | `bash tools/fetch-assets.sh`, then hard-refresh |

A `shared/` change that appears server-side but not client-side almost always
means terminal 2 is dead or still building.

## Logging

`RUST_LOG=debug` raises spawn/despawn and combat dice detail; default is `info`.
Movement warnings at `warn` mean server and client step checks disagree — a bug
signal, not noise.

## Seeing it in-game

Use the `game-login` skill to drive real Chrome through Google sign-in and enter
with the default character. Never type user credentials.
