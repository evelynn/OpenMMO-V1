#!/usr/bin/env bash
# Verifies a locally built server end to end, without docker.
#
# Covers the part of "this branch actually runs" that a machine can judge on
# its own: the boot-time asserts pass, every schema migration applies to a
# fresh database and again to an existing one, the terrain API serves baked
# terrain rather than the flat fallback, and the WebSocket upgrade completes.
# Whether the world renders still needs a browser and a human.
#
# Usage: cargo build --release -p onlinerpg-server && tools/smoke-local.sh
set -uo pipefail
cd "$(dirname "$0")/.."

BIN=./target/release/onlinerpg-server
PORT="${SMOKE_PORT:-10056}"
API=$((PORT + 1))
STATE=$(mktemp -d)
fails=0

ok()   { echo "ok    $1"; }
fail() { echo "FAIL  $1" >&2; fails=$((fails + 1)); }
cleanup() { [ -n "${PID:-}" ] && kill "$PID" 2>/dev/null; rm -rf "$STATE"; }
trap cleanup EXIT

if [ ! -x "$BIN" ]; then
    echo "FAIL  $BIN missing — run: cargo build --release -p onlinerpg-server" >&2
    exit 1
fi

# A stale binary passes checks the current source would fail — and, worse,
# fails checks it would pass. Refuse rather than report either.
newer=$(find server/src shared/src data-src -newer "$BIN" -type f -print -quit 2>/dev/null || true)
if [ -n "$newer" ]; then
    echo "FAIL  $BIN is older than $newer — run: cargo build --release -p onlinerpg-server" >&2
    exit 1
fi

# Boots the server against $STATE, waits for the listener, leaves $PID set.
boot() {
    local log=$1
    "$BIN" --state-dir "$STATE" --port "$PORT" >"$log" 2>&1 &
    PID=$!
    for _ in $(seq 1 60); do
        grep -qi "listening" "$log" && return 0
        kill -0 "$PID" 2>/dev/null || return 1
        sleep 1
    done
    return 1
}

# --- first boot: a fresh database, so every migration runs for real ---
LOG=$STATE/boot1.log
if boot "$LOG"; then ok "boots on a fresh state dir"; else fail "did not reach the listener"; sed -n '1,40p' "$LOG" >&2; fi
# The boot asserts (dungeon bosses flagged, weaponTier in range, sizeMult
# parseable, every referenced debuff and item present) only fire here.
if grep -qi "panic" "$LOG"; then fail "panic during boot"; else ok "no panic — boot asserts passed"; fi

python3 - "$STATE/game_data.db" <<'SCHEMA' > "$STATE/schema.txt"
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
cols = {r[1] for r in db.execute("PRAGMA table_info(characters)")}
for c in ("save_x", "save_y", "save_z", "save_rotation"):
    print("ok" if c in cols else "FAIL", "characters.%s (IMP-2.2)" % c)
for c in ("job_xp", "skill_points"):
    print("ok" if c in cols else "FAIL", "characters.%s (IMP-3.2)" % c)
tables = {r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
print("ok" if "character_storage" in tables else "FAIL", "character_storage table (IMP-2.3)")
for t in ("character_achievements", "character_counters"):
    print("ok" if t in tables else "FAIL", "%s table (IMP-3.7)" % t)
for t in ("guilds", "guild_members", "guild_ranks", "guild_storage"):
    print("ok" if t in tables else "FAIL", "%s table (IMP-4.1)" % t)
print("ok" if "character_instance_cooldowns" in tables else "FAIL",
      "character_instance_cooldowns table (IMP-4.2)")
idx = {r[1] for r in db.execute("PRAGMA index_list(guild_members)") if r[2]}
print("ok" if idx else "FAIL", "guild_members has a UNIQUE index (one guild per character)")
print("ok" if "active_title" in cols else "FAIL", "characters.active_title (IMP-3.7)")
if "character_storage" in tables:
    scols = [r[1] for r in db.execute("PRAGMA table_info(character_storage)")]
    print("ok" if "enchant" in scols else "FAIL", "character_storage.enchant from the first migration")
SCHEMA
while read -r verdict what; do
    [ "$verdict" = "ok" ] && ok "$what" || fail "$what"
done < "$STATE/schema.txt"

# TerrainIO::read_heightmap answers a missing file with a flat default of the
# same size, so neither the status code nor the byte count says anything about
# the bake. Compare the origin against a tile far outside any bake: identical
# bytes mean the origin is that same fallback.
python3 - "http://127.0.0.1:$API/api/terrain/height" <<'TILE' > "$STATE/tile.txt"
import sys, urllib.request
base = sys.argv[1]

def tile(x, z):
    return urllib.request.urlopen("%s/%d/%d" % (base, x, z), timeout=20).read()

try:
    baked = tile(0, 0)
    fallback = tile(900000, 900000)
except Exception as e:
    print("FAIL terrain API unreachable (%s)" % e)
    raise SystemExit
heights = len({baked[i:i + 2] for i in range(0, len(baked) - 1, 2)})
if len(baked) < 1000:
    print("FAIL terrain tile is only %d bytes" % len(baked))
elif baked == fallback:
    print("FAIL the origin tile is byte-identical to the unbaked fallback - bake the world first")
else:
    print("ok terrain serves baked terrain (%d bytes, %d distinct heights, differs from the fallback)"
          % (len(baked), heights))
TILE
read -r verdict what < "$STATE/tile.txt"
[ "$verdict" = "ok" ] && ok "$what" || fail "$what"

code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 "http://127.0.0.1:$API/api/announcements")
[ "$code" = "200" ] && ok "announcements API answers" || fail "announcements API: HTTP $code"

# IMP-5.5: a load balancer must be able to ask without a credential, and
# metrics must not answer without one.
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 "http://127.0.0.1:$API/api/health")
[ "$code" = "200" ] && ok "health endpoint answers (IMP-5.5)" || fail "health: HTTP $code"
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 "http://127.0.0.1:$API/api/ready")
[ "$code" = "200" ] && ok "ready endpoint answers while serving" || fail "ready: HTTP $code"
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 "http://127.0.0.1:$API/api/metrics")
[ "$code" = "401" ] && ok "metrics refuses an unauthenticated read" || fail "metrics unauthenticated: HTTP $code"
token=$(cat "$STATE/npc_token" 2>/dev/null || true)
body=$(curl -s --max-time 10 -H "Authorization: Bearer $token" "http://127.0.0.1:$API/api/metrics")
case "$body" in
    *players_online*dropped_messages*) ok "metrics answers the operator token" ;;
    *) fail "metrics with token: ${body:-no response}" ;;
esac

# IMP-6.2: a backup taken from a live server must read back, and the restore
# must refuse to run against one. Both scripts are only as good as the last
# time somebody ran them.
snapshot=$(bash "$(dirname "$0")/backup-state.sh" "$STATE" "$STATE/backups" 2>/dev/null | tail -1)
if [ -f "$snapshot/game_data.db" ]; then
    ok "backup snapshots a live server (IMP-6.2)"
else
    fail "backup produced no database"
fi
if bash "$(dirname "$0")/restore-state.sh" "$snapshot" "$STATE" >/dev/null 2>&1; then
    fail "restore ran against a live server"
else
    ok "restore refuses while the server is up"
fi

key=$(head -c 16 /dev/urandom | base64)
line=$(curl -s -i -N --max-time 5 \
    -H "Connection: Upgrade" -H "Upgrade: websocket" \
    -H "Sec-WebSocket-Version: 13" -H "Sec-WebSocket-Key: $key" \
    "http://127.0.0.1:$PORT/" 2>/dev/null | head -1)
case "$line" in
    *101*) ok "WebSocket upgrade completes" ;;
    *) fail "WebSocket upgrade: ${line:-no response}" ;;
esac

kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; PID=

# --- second boot on the same file: the migrations must be idempotent ---
LOG2=$STATE/boot2.log
if boot "$LOG2"; then ok "reboots on an existing database"; else fail "second boot did not reach the listener"; sed -n '1,40p' "$LOG2" >&2; fi
if grep -qi "panic" "$LOG2"; then fail "panic on the second boot"; else ok "migrations are idempotent"; fi

echo
grep -oE 'PROTOCOL_VERSION: u32 = [0-9]+' shared/src/lib.rs
if [ "$fails" -eq 0 ]; then echo "ALL CHECKS PASSED"; else echo "$fails CHECK(S) FAILED"; fi
exit "$fails"
