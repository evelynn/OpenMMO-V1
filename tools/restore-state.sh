#!/usr/bin/env bash
# Put a backup back. Refuses to run against a live server, because SQLite will
# happily let you overwrite a database another process is holding open and the
# result is corruption nobody notices until later (IMP-6.2).
#
# Usage: tools/restore-state.sh <backup-dir> <state-dir>
set -euo pipefail

SRC=${1:?usage: restore-state.sh <backup-dir> <state-dir>}
STATE=${2:?usage: restore-state.sh <backup-dir> <state-dir>}

[ -d "$SRC" ] || { echo "no such backup: $SRC" >&2; exit 1; }
[ -f "$SRC/game_data.db" ] || { echo "$SRC holds no game_data.db" >&2; exit 1; }

# Neither pgrep form is usable here. `pgrep -f onlinerpg-server` also matches
# a build, an editor, or the shell that launched this script — a restore that
# refuses for no reason is one nobody trusts. `pgrep -x` cannot match at all:
# Linux truncates the process name to 15 characters and "onlinerpg-server" is
# 16, so it silently never fires, which is the more dangerous failure.
# /proc/<pid>/exe is the executable itself and answers exactly.
if python3 - <<'RUNNING'
import os, sys
for pid in filter(str.isdigit, os.listdir("/proc")):
    try:
        exe = os.readlink(f"/proc/{pid}/exe")
    except OSError:
        continue
    if os.path.basename(exe) == "onlinerpg-server":
        sys.exit(0)
sys.exit(1)
RUNNING
then
    echo "refusing: onlinerpg-server is running. Stop it first." >&2
    exit 1
fi

python3 - "$SRC/game_data.db" <<'VERIFY'
import sqlite3, sys
conn = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
ok = conn.execute("PRAGMA integrity_check").fetchone()[0]
if ok != "ok":
    sys.exit(f"backup fails integrity_check: {ok}")
VERIFY

mkdir -p "$STATE"
# The displaced state is kept, not deleted: a restore of the wrong backup is a
# mistake you want to be able to undo.
if [ -e "$STATE/game_data.db" ]; then
    aside="$STATE/replaced-$(date -u +%Y%m%dT%H%M%SZ)"
    mkdir -p "$aside"
    for item in game_data.db housing announcements npc_token; do
        [ -e "$STATE/$item" ] && mv "$STATE/$item" "$aside/"
    done
    echo "previous state moved to $aside"
fi

cp -a "$SRC/game_data.db" "$STATE/game_data.db"
for item in housing announcements npc_token; do
    [ -e "$SRC/$item" ] && cp -a "$SRC/$item" "$STATE/$item"
done

python3 - "$STATE/game_data.db" "$STATE" <<'DONE'
import sqlite3, sys
conn = sqlite3.connect(sys.argv[1])
n = conn.execute("SELECT count(*) FROM characters").fetchone()[0]
print(f"restored {n} character(s) into {sys.argv[2]}")
DONE
