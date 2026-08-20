#!/usr/bin/env bash
# IMP-6.3: what N real WebSocket clients cost this server.
#
# SPK-1 measured the game loop with players pushed straight into GameState.
# That answered "does the simulation scale" and left the other half open:
# nothing had ever opened thousands of actual sockets here. Handshakes,
# per-connection tasks, the outbound queues from IMP-5.1 and the memory all of
# it costs are invisible to an in-process test.
#
# Reads the server's own view from /api/metrics (IMP-5.5) and its RSS from
# /proc, so the numbers are the server's, not the harness's.
#
# Usage: cargo build --release -p onlinerpg-server -p load-client
#        tools/spike-socket-load.sh [clients...]     (default: 100 500 1000)
set -uo pipefail
cd "$(dirname "$0")/.."

SERVER=./target/release/onlinerpg-server
CLIENT=./target/release/load-client
PORT="${SPIKE_PORT:-10077}"
API=$((PORT + 1))
STATE=$(mktemp -d)
HOLD="${HOLD_SECS:-10}"
cleanup() { [ -n "${PID:-}" ] && kill "$PID" 2>/dev/null; [ -n "${KEEP_STATE:-}" ] && { cp "$STATE/server.log" "${KEEP_STATE}"; }; rm -rf "$STATE"; }
trap cleanup EXIT

for bin in "$SERVER" "$CLIENT"; do
    [ -x "$bin" ] || { echo "FAIL  $bin missing - run: cargo build --release -p onlinerpg-server -p load-client" >&2; exit 1; }
done

"$SERVER" --state-dir "$STATE" --port "$PORT" >"$STATE/server.log" 2>&1 &
PID=$!
for _ in $(seq 1 60); do
    grep -qi "listening" "$STATE/server.log" && break
    kill -0 "$PID" 2>/dev/null || { echo "FAIL  server exited"; tail -5 "$STATE/server.log"; exit 1; }
    sleep 1
done
TOKEN=$(cat "$STATE/npc_token")

rss() { awk '/VmRSS/ {print $2}' "/proc/$PID/status" 2>/dev/null; }
metric() { curl -s --max-time 5 -H "Authorization: Bearer $TOKEN" "http://127.0.0.1:$API/api/metrics"; }

base_rss=$(rss)
echo "baseline: RSS $((base_rss / 1024)) MiB, $(metric)"
echo

# Each N runs twice against the same server. The first pass is a cold one:
# every client is a brand-new account whose character has to be written to
# SQLite, so it measures a first-login storm. The second pass reuses those
# accounts, which is what a real reconnect looks like. Reporting one number
# for both would blame the socket layer for the database's work.
for n in "${@:-100 500 1000}"; do
  for pass in cold warm; do
    "$CLIENT" --url "ws://127.0.0.1:$PORT" --token "$TOKEN" \
        --clients "$n" --concurrency 100 --hold-secs "$HOLD" --tag "n$n" >"$STATE/run.txt" 2>&1 &
    RUN=$!
    # Sample while they are all in, not after: a reading taken once the hold
    # ends measures an empty server.
    peak=0; peak_players=0
    while kill -0 "$RUN" 2>/dev/null; do
        now=$(rss); [ "${now:-0}" -gt "$peak" ] && peak=$now
        players=$(metric | sed 's/.*"players_online":\([0-9]*\).*/\1/')
        [ "${players:-0}" -gt "$peak_players" ] && peak_players=$players
        sleep 1
    done
    wait "$RUN"
    out=$(cat "$STATE/run.txt")
    echo "=== $n clients ($pass) ==="
    echo "$out" | sed 's/^/  /'
    echo "  server RSS      $((peak / 1024)) MiB (+$(((peak - base_rss) / 1024)) MiB, $(((peak - base_rss) / n)) KiB/client)"
    echo "  peak online     $peak_players (server's own count, sampled during the hold)"
    echo "  server metrics  $(metric)"
    echo
    # Let the disconnects settle so the next pass starts from a quiet server.
    sleep 3
  done
done
