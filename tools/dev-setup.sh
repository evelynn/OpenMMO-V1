#!/usr/bin/env bash
# Bootstrap a fresh clone into a runnable dev environment, or diagnose an
# existing one with --check. Idempotent: every step skips work already done.
set -euo pipefail
cd "$(dirname "$0")/.."

CHECK_ONLY=0
INSTALL_TOOLS=0
SKIP_ASSETS=0
SKIP_TERRAIN=0
# Default bake range is derived from the spawn point below, not hardcoded: a
# square around the origin does not contain it, and a dev who bakes the wrong
# regions gets a black world exactly where every character starts.
TERRAIN_X_MIN=""
TERRAIN_X_MAX=""
TERRAIN_Z_MIN=""
TERRAIN_Z_MAX=""
TERRAIN_SEED=42

# World region holding the spawn, from data-src/world.json. A region is 16
# tiles and a tile is 64 world units, with tile 0 centred on the origin.
spawn_region() {
    python3 - "$1" <<'SPAWN'
import json, math, sys
axis = sys.argv[1]
spawn = json.load(open("data-src/world.json"))["spawnPosition"]
tile = math.floor((spawn[axis] + 32.0) / 64.0)
print(math.floor(tile / 16))
SPAWN
}

usage() {
    cat <<'EOF'
Usage: tools/dev-setup.sh [options]

  --check           Diagnose only; change nothing.
  --install-tools   Also cargo-install missing wasm-pack / cargo-watch.
  --skip-assets     Skip the Hugging Face binary asset download.
  --skip-terrain    Skip the terrain bake.
  --full-terrain    Bake all 32x32 regions (~73 GB) instead of the dev subset.
  --regions MIN MAX Region range on both axes. Default is the spawn's region
                    plus one ring (3x3, ~600 MB) — the origin is not the spawn.
  --seed N          World seed for the bake (default 42).
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --check) CHECK_ONLY=1 ;;
        --install-tools) INSTALL_TOOLS=1 ;;
        --skip-assets) SKIP_ASSETS=1 ;;
        --skip-terrain) SKIP_TERRAIN=1 ;;
        --full-terrain) TERRAIN_X_MIN=-16 TERRAIN_X_MAX=15 TERRAIN_Z_MIN=-16 TERRAIN_Z_MAX=15 ;;
        --regions) TERRAIN_X_MIN=$2 TERRAIN_X_MAX=$3 TERRAIN_Z_MIN=$2 TERRAIN_Z_MAX=$3; shift 2 ;;
        --seed) TERRAIN_SEED=$2; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

ok() { printf '  \033[32mok\033[0m   %s\n' "$1"; }
warn() { printf '  \033[33mwarn\033[0m %s\n' "$1"; }
fail() { printf '  \033[31mfail\033[0m %s\n' "$1"; MISSING=$((MISSING + 1)); }
step() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
MISSING=0

step "Toolchain"
if command -v cargo >/dev/null; then ok "cargo $(cargo --version | cut -d' ' -f2)"
else fail "cargo missing — install Rust: https://rustup.rs/"; fi

if command -v node >/dev/null; then
    node_major=$(node --version | sed 's/^v\([0-9]*\).*/\1/')
    if [ "$node_major" -ge 22 ]; then ok "node $(node --version)"
    else fail "node $(node --version) is too old — CI uses 22"; fi
else fail "node missing — install Node.js 22+"; fi

command -v npm >/dev/null && ok "npm $(npm --version)" || fail "npm missing"

if command -v rustup >/dev/null && rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
    ok "wasm32-unknown-unknown target"
elif [ "$CHECK_ONLY" = 0 ] && command -v rustup >/dev/null; then
    rustup target add wasm32-unknown-unknown && ok "wasm32-unknown-unknown target installed"
else
    fail "wasm32 target missing — rustup target add wasm32-unknown-unknown"
fi

for tool in wasm-pack cargo-watch; do
    if command -v "$tool" >/dev/null; then
        ok "$tool"
    elif [ "$INSTALL_TOOLS" = 1 ] && [ "$CHECK_ONLY" = 0 ]; then
        cargo install "$tool" && ok "$tool installed"
    elif [ "$tool" = wasm-pack ]; then
        fail "wasm-pack missing (required for the client) — cargo install wasm-pack"
    else
        warn "cargo-watch missing (optional, enables auto-restart) — cargo install cargo-watch"
    fi
done

step "Config files"
if [ -f client/.env.local ]; then
    grep -q '^VITE_GOOGLE_CLIENT_ID=.\+' client/.env.local \
        && ok "client/.env.local" \
        || warn "client/.env.local has no VITE_GOOGLE_CLIENT_ID — browser login will fail"
elif [ "$CHECK_ONLY" = 1 ]; then
    fail "client/.env.local missing — cp client/.env.example client/.env.local"
else
    cp client/.env.example client/.env.local
    warn "created client/.env.local — set VITE_GOOGLE_CLIENT_ID before logging in"
fi

if [ -f agent-client/data/config.toml ]; then
    ok "agent-client/data/config.toml"
elif [ "$CHECK_ONLY" = 1 ]; then
    warn "agent-client/data/config.toml missing (only needed to run NPC agents)"
else
    cp agent-client/data/config.toml.example agent-client/data/config.toml
    ok "created agent-client/data/config.toml"
fi

step "Binary assets (models, audio)"
asset_probe=client/public/models
if [ "$SKIP_ASSETS" = 1 ]; then
    warn "skipped"
elif [ "$CHECK_ONLY" = 1 ]; then
    [ -d "$asset_probe" ] && [ -n "$(ls -A "$asset_probe" 2>/dev/null)" ] \
        && ok "$asset_probe populated" \
        || fail "assets missing — bash tools/fetch-assets.sh"
else
    bash tools/fetch-assets.sh
fi

step "Generated data + WASM"
if [ "$CHECK_ONLY" = 1 ]; then
    [ -f data/monsters.json ] && ok "data/*.json generated" || fail "run: npm --prefix client run generate:csv"
    [ -d client/src/lib/wasm ] && ok "client/src/lib/wasm built" || fail "run: npm --prefix client run build:wasm"
    [ -d client/node_modules ] && ok "client/node_modules" || fail "run: npm --prefix client ci"
else
    [ -d client/node_modules ] || npm --prefix client ci
    npm --prefix client run build:wasm
    ok "wasm + generated data up to date"
fi

step "Terrain"
if [ -f data/terrain/worldgen.json ]; then
    ok "data/terrain baked ($(du -sh data/terrain 2>/dev/null | cut -f1))"
elif [ "$SKIP_TERRAIN" = 1 ]; then
    warn "skipped — the world renders black until baked"
elif [ "$CHECK_ONLY" = 1 ]; then
    fail "not baked — cargo run -p terrain-gen --release -- bake --seed $TERRAIN_SEED"
else
    if [ -z "$TERRAIN_X_MIN" ]; then
        sx=$(spawn_region x); sz=$(spawn_region z)
        TERRAIN_X_MIN=$((sx - 1)); TERRAIN_X_MAX=$((sx + 1))
        TERRAIN_Z_MIN=$((sz - 1)); TERRAIN_Z_MAX=$((sz + 1))
        echo "    spawn sits in region ($sx, $sz); baking it plus one ring"
    fi
    wide=$((TERRAIN_X_MAX - TERRAIN_X_MIN + 1))
    tall=$((TERRAIN_Z_MAX - TERRAIN_Z_MIN + 1))
    echo "    baking ${wide}x${tall} regions (~$((wide * tall * 65)) MB, ~10 min)"
    cargo run -p terrain-gen --release -- bake \
        --seed "$TERRAIN_SEED" \
        --region-x-min "$TERRAIN_X_MIN" --region-x-max "$TERRAIN_X_MAX" \
        --region-z-min "$TERRAIN_Z_MIN" --region-z-max "$TERRAIN_Z_MAX"
    ok "baked"
fi

step "Result"
if [ "$MISSING" -gt 0 ]; then
    echo "  $MISSING problem(s) above. See doc/DEVELOPMENT.md."
    exit 1
fi
cat <<'EOF'
  Ready. Three terminals:
    1) cargo watch -w server -w shared -w data-src -x "run -p onlinerpg-server"
    2) cargo watch -w shared -s "npm run build:wasm --prefix client"
    3) npm --prefix client run dev -- --port 10004
  Then open http://localhost:10004/ — see doc/DEVELOPMENT.md.
EOF
