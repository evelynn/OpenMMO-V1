#!/usr/bin/env bash
# SPK-3: what a paid-travel arrival costs the REST listener, and what fifty
# of them at once cost it (master plan section 7).
#
# The go/no-go is a ratio, not an absolute: arriving by warp must not push
# REST p95 past twice what walking in costs. Walking crosses one tile
# boundary at a time; a warp asks for the whole 2x2 grid at once, and fifty
# travelers to one destination ask for it simultaneously.
#
# Needs a baked world — an unbaked tile is a constant the server answers from
# memory, which would measure nothing.
#
# Usage: cargo build --release -p onlinerpg-server && tools/spike-arrival-load.sh
set -uo pipefail
cd "$(dirname "$0")/.."

BIN=./target/release/onlinerpg-server
PORT="${SPIKE_PORT:-10076}"
API=$((PORT + 1))
STATE=$(mktemp -d)
cleanup() { [ -n "${PID:-}" ] && kill "$PID" 2>/dev/null; rm -rf "$STATE"; }
trap cleanup EXIT

if [ ! -x "$BIN" ]; then
    echo "FAIL  $BIN missing - run: cargo build --release -p onlinerpg-server" >&2
    exit 1
fi

"$BIN" --state-dir "$STATE" --port "$PORT" >"$STATE/server.log" 2>&1 &
PID=$!
for _ in $(seq 1 60); do
    grep -qi "listening" "$STATE/server.log" && break
    kill -0 "$PID" 2>/dev/null || { echo "FAIL  server exited during boot" >&2; exit 1; }
    sleep 1
done

python3 - "http://127.0.0.1:$API" <<'SPIKE'
import statistics, sys, time, urllib.request
from concurrent.futures import ThreadPoolExecutor

BASE = sys.argv[1]
# The client keeps a 2x2 tile grid around the player (LOADING_OPTIMIZATION.md
# section 5) and pulls these layers per tile.
PER_TILE = ("height", "splat", "grass", "trees", "water-field", "river-field")
PER_REGION = ("zones", "objects")
TRAVELERS = 50
# Enough in flight to keep the listener busy without the client becoming the
# bottleneck; both crowd cases use it so the ratio isolates the burst.
CONCURRENCY = 64
# One round is noise on a small box; the verdict is the median across rounds.
ROUNDS = 5


def urls_for(tile_x, tile_z, tiles):
    """Every request one client makes for a `tiles`-wide grid at this spot."""
    out = []
    for dx in range(tiles):
        for dz in range(tiles):
            for layer in PER_TILE:
                out.append("%s/api/terrain/%s/%d/%d" % (BASE, layer, tile_x + dx, tile_z + dz))
    for layer in PER_REGION:
        out.append("%s/api/terrain/%s/0/0" % (BASE, layer))
    out.append("%s/api/housing/area/%d/%d" % (BASE, tile_x, tile_z))
    return out


def fetch(url):
    start = time.perf_counter()
    try:
        urllib.request.urlopen(url, timeout=60).read()
    except Exception:
        pass  # a 404 is still a round trip the listener served
    return (time.perf_counter() - start) * 1000.0


def run(urls, workers):
    with ThreadPoolExecutor(max_workers=workers) as pool:
        return list(pool.map(fetch, urls))


def p95(samples):
    ordered = sorted(samples)
    return ordered[min(len(ordered) - 1, int(len(ordered) * 0.95))]


def report(label, samples, requests_per_client):
    print("  %-34s n=%-5d p50 %7.1fms  p95 %7.1fms  max %7.1fms"
          % (label, len(samples), statistics.median(samples), p95(samples), max(samples)))
    return p95(samples)


print("SPK-3 - arrival load on the REST listener")
print("  one arrival asks for %d requests (2x2 grid, %d layers, zones/objects, housing)"
      % (len(urls_for(0, 0, 2)), len(PER_TILE)))
print()

# Warm the cache the way a first arrival would, so the comparison is between
# access patterns rather than between cold and hot.
run(urls_for(0, 0, 4), 8)

# One player walking: a tile boundary at a time, nothing else in flight.
lone_walk = []
for i in range(8):
    lone_walk += run(urls_for(10 + i, 10, 1), 1)
report("1 walker (sequential)", lone_walk, len(PER_TILE))

# One warp: the whole grid at once, still alone on the server.
solo = run(urls_for(20, 20, 2), 12)
report("1 arrival (burst)", solo, len(urls_for(0, 0, 2)))

# The honest baseline is the same fifty people walking: same concurrency, so
# what is left in the ratio is the burst rather than the crowd. Comparing a
# crowd against a lone sequential client measures queueing instead, which any
# listener "fails".
#
# One pass of this is noise on a small box - observed ratios spanned 0.96x to
# 2.20x across single runs - so it repeats and reports the median.
walk_p95s, crowd_p95s, rates = [], [], []
for round_index in range(ROUNDS):
    walkers = []
    for t in range(TRAVELERS):
        walkers += urls_for(40 + (t % 8), 40 + round_index, 1)
    walk_p95s.append(p95(run(walkers, CONCURRENCY)))

    crowd = []
    for t in range(TRAVELERS):
        crowd += urls_for(30, 30 + round_index, 2)
    start = time.perf_counter()
    crowd_samples = run(crowd, CONCURRENCY)
    elapsed = time.perf_counter() - start
    crowd_p95s.append(p95(crowd_samples))
    rates.append(len(crowd) / elapsed)

walk_p95 = statistics.median(walk_p95s)
crowd_p95 = statistics.median(crowd_p95s)
print("  %-34s p95 %7.1fms  (median of %d rounds)"
      % ("%d walkers, one boundary each" % TRAVELERS, walk_p95, ROUNDS))
print("  %-34s p95 %7.1fms  (median of %d rounds, %.0f req/s)"
      % ("%d arrivals at once" % TRAVELERS, crowd_p95, ROUNDS, statistics.median(rates)))
print("  %-34s %.2fx .. %.2fx across rounds"
      % ("ratio spread", min(c / w for c, w in zip(crowd_p95s, walk_p95s)),
         max(c / w for c, w in zip(crowd_p95s, walk_p95s))))

# What one arrival actually pulls down.
total_bytes = 0
for url in urls_for(60, 60, 2):
    try:
        total_bytes += len(urllib.request.urlopen(url, timeout=60).read())
    except Exception:
        pass
print()
print("  one arrival transfers %.1f KiB across %d requests"
      % (total_bytes / 1024.0, len(urls_for(0, 0, 2))))

ratio = crowd_p95 / walk_p95 if walk_p95 > 0 else float("inf")
print("  median p95 ratio (%d arriving vs %d walking): %.2fx" % (TRAVELERS, TRAVELERS, ratio))
if ratio <= 2.0:
    print("  VERDICT: go - within the 2x budget")
else:
    print("  VERDICT: no-go - cut the destination list and preheat the tile cache first")
SPIKE
