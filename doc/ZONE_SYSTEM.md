# Per-Region Zones (Town No-Spawn) & Legacy Spawn Rectangles

## Context
Per-region zone files (`data/terrain/zones/r{X}_{Z}.json`) carry two rectangle
arrays and a map-editor UI for drawing them.

> **What the server actually reads.** Only `noSpawnZones`.
> `server/src/world_config.rs:93` `load_no_spawn_zones_from_regions()` parses that key
> and nothing else. **`monsterSpawns` is editor-only legacy data** — no Rust code reads it.
>
> Ground monsters spawn by **following players**: `data-src/world.json`'s `ambientSpawns`
> list plus the per-player cap `maxMonstersPerPlayer` (30), driven by
> `tick_monster_spawns()` (`server/src/game_state/monster.rs:854`). The gate is the
> monster's own level (`min_ambient_player_level`, `monster.rs:843`), so there are no
> fixed level-banded hunting grounds today. Restoring a zone-driven spawner is an open
> task — `doc/TODO.md`'s "몬스터 스폰 개선 — 플레이어의 레벨에 맞게", deferred in
> [ragnarok/13_IMPLEMENTATION_DIRECTION.md](ragnarok/13_IMPLEMENTATION_DIRECTION.md)'s
> 조건부/보류 table.

Zone data is kept separate from terrain meta (`data/terrain/meta/`) to avoid coupling gameplay logic with rendering config.

## Zone File Schema

`data/terrain/zones/r-02_+00.json`:
```json
{
  "monsterSpawns": [
    { "monsterType": "scp939", "maxPerPlayer": 3, "maxTotal": 10, "spawnIntervalSecs": 30,
      "minX": -1560.0, "minZ": 410.0, "maxX": -1480.0, "maxZ": 490.0 }
  ],
  "noSpawnZones": [
    { "minX": -1600.0, "minZ": 400.0, "maxX": -1500.0, "maxZ": 500.0, "label": "Starting Town" }
  ]
}
```
Both arrays are optional (default empty). Coordinates are world-space. Both zone types use
the same rectangular `minX/minZ/maxX/maxZ` format. The `monsterSpawns` entry above is
retained by the editor and ignored by the server (see the note at the top).

## Architecture

### Shared crate (`shared/src/lib.rs`)
- `NoSpawnZone` struct with `#[serde(rename_all = "camelCase")]` — used directly for both Rust deserialization and JSON serialization (no intermediate struct needed)
- `NoSpawnZone::contains(x, z)` helper for point-in-rect checks
- `ServerMessage::SpawnMonsterRequest` uses rectangular bounds (`min_x/min_z/max_x/max_z`)
- `ServerMessage::NoSpawnZones` — sent once on player join so agent-client can validate spawn positions

### Terrain crate (`terrain/src/`)
- `coords::zone_path()` — `{base}/zones/r{+XX}_{+ZZ}.json`
- `io::TerrainIO` — `list_zone_regions()`, `read_zone()`, `write_zone()`

### Server
- `world_config.rs:93` — `load_no_spawn_zones_from_regions()` reads every zone file at
  startup and keeps **only** `noSpawnZones`. Ambient spawn rules come from
  `data-src/world.json` (`ambient_spawns`, `max_monsters_per_player`), not from zone files.
- `game_state/mod.rs` — `no_spawn_zones` field + accessor
- `game_state/monster.rs:854` — `tick_monster_spawns()` asks each player's client to place
  ambient monsters near that player; no-spawn zones and `NO_SPAWN_MARGIN` (30 m) reject
  town positions
- `terrain/routes.rs` — `GET/PUT /api/terrain/zones/{rx}/{rz}`
- `connection.rs` — sends `NoSpawnZones` on join
- `main.rs` — loads zones from region files at startup, passes to `GameState::new()`

### Agent-client (`agent-client/src/state.rs`)
- Stores `no_spawn_zones` received via `ServerMessage::NoSpawnZones`
- `find_valid_spawn_position()` picks random point within rect, rejects if inside a house or no-spawn zone

### Web client
- `messageHandlers.ts` — handles `SpawnMonsterRequest` with rect bounds, `NoSpawnZones` (no-op for now)

### Map editor
- `stores/editorStore.ts` — `EditorTool` includes `'zone'`, shared stores for zone sub-tool, draw state, form values (`spawnFormMonsterType`, `spawnFormMaxPerPlayer`, etc.), `currentZoneData` (shared between panel and overlay)
- `managers/zoneManager.ts` — `ZoneManager` class for load/save via `/api/terrain/zones/{rx}/{rz}`
- `map-editor/ZoneBrushPanel.svelte` — No-Spawn / Spawn sub-tools with zone list (hover highlights overlay), delete, form inputs for spawn params, draw instructions
- `map-editor/ZoneOverlay.svelte` — terrain-conforming overlays (samples heightmap per cell), red for no-spawn, blue for spawn, yellow preview while drawing, white highlight on hover from panel list. Static zone geometries separated from preview for efficiency, with proper `dispose()` on cleanup.
- `map-editor/MapEditorCursor.svelte` — two-click rectangle drawing, reads spawn params from shared stores (not hardcoded)
- `map-editor/MapEditorPanel.svelte` — Zone tab
- `GameScene.svelte` — creates `ZoneManager`, renders `ZoneOverlay` in editor mode
- `App.svelte` — hides `ChatPanel` and `CharacterAttributesHud` in map editor mode

## Notes
- **Cross-region zones**: A zone drawn in region A may cover region B territory. Server aggregates all zones into a flat list so validation works. Editor shows zones stored in the current region only.
- **Hot-reload**: After editor saves a zone, server won't see it until restart. Acceptable for v1.
- **Spawn rectangles are inert**: drawing one changes nothing at runtime. Keep the tool for
  when the zone-driven spawner lands, but do not treat the rectangles as live configuration.
- **Agent-client zone source**: Currently receives zones via WebSocket `NoSpawnZones` message on join. Could alternatively read zone files directly from disk (agent-client has `TerrainIO` access), but kept as WebSocket for now.
