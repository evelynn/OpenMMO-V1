#!/usr/bin/env bash
# Snapshot a live server's state without stopping it, and verify the snapshot
# before calling it a backup (IMP-6.2).
#
# What state is: the SQLite database AND the housing directory. Player-built
# houses are JSON files on disk, not rows — a database-only backup silently
# loses every house on the server.
#
# Usage: tools/backup-state.sh <state-dir> [dest-dir]
#        tools/backup-state.sh /var/lib/onlinerpg /var/backups/onlinerpg
set -euo pipefail

STATE=${1:?usage: backup-state.sh <state-dir> [dest-dir]}
DEST=${2:-"$STATE/backups"}
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
OUT="$DEST/$STAMP"

[ -d "$STATE" ] || { echo "no such state dir: $STATE" >&2; exit 1; }
mkdir -p "$OUT"

# python3 rather than the sqlite3 CLI: the smoke test already depends on
# python3, and the CLI is not in every image.
#
# VACUUM INTO takes a consistent snapshot through SQLite itself, so it needs
# no write lock and cannot catch a half-written transaction — unlike copying
# the file, which can. It also compacts, so the copy is smaller than the live
# database.
if [ -f "$STATE/game_data.db" ]; then
    python3 - "$STATE/game_data.db" "$OUT/game_data.db" <<'SNAP'
import sqlite3, sys
src, dest = sys.argv[1], sys.argv[2]
conn = sqlite3.connect(f"file:{src}?mode=ro", uri=True)
conn.execute("VACUUM INTO ?", (dest,))
conn.close()
SNAP
    echo "db      $(du -h "$OUT/game_data.db" | cut -f1)"
else
    echo "warning: no game_data.db in $STATE" >&2
fi

# Houses, announcements and the bot token are files. The token is included on
# purpose: restoring without it silently locks every headless agent out.
for dir in housing announcements; do
    if [ -d "$STATE/$dir" ]; then
        cp -a "$STATE/$dir" "$OUT/$dir"
        echo "$dir $(find "$OUT/$dir" -type f | wc -l) file(s)"
    fi
done
[ -f "$STATE/npc_token" ] && cp -a "$STATE/npc_token" "$OUT/npc_token"

# A backup nobody has opened is a guess. Read it back before reporting success.
if [ -f "$OUT/game_data.db" ]; then
    python3 - "$OUT/game_data.db" <<'VERIFY'
import sqlite3, sys
conn = sqlite3.connect(sys.argv[1])
ok = conn.execute("PRAGMA integrity_check").fetchone()[0]
if ok != "ok":
    sys.exit(f"snapshot failed integrity_check: {ok}")
n = conn.execute("SELECT count(*) FROM characters").fetchone()[0]
print(f"verify  integrity ok, {n} character(s)")
VERIFY
fi

echo "$OUT"
