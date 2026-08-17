---
name: add-monster
description: Add a new monster to OpenMMO end to end — data-src/monsters.csv row, GLB model and animation clip names, weapon attachment, ground spawn zones or dungeon depth weighting, and the generated attack-clip timings. Use when the user says "몬스터 추가", "새 몹 만들자", "add a monster", "던전에 보스 넣어줘", or asks to tune an existing monster's stats or animations.
---

# Adding a monster

Design context: [doc/NPC_MONSTER_AI.md](../../../doc/NPC_MONSTER_AI.md),
[doc/COMBAT.md](../../../doc/COMBAT.md),
[doc/ANIMATION.md](../../../doc/ANIMATION.md).

## 1. Copy the closest existing row

`data-src/monsters.csv` is the single source of truth — it feeds the server
(`server/src/monster_defs.rs`) and the client
(`client/src/lib/data/monsterDefs.ts`) through the same generated
`data/monsters.json`. Start from a monster of a similar tier (e.g. `gnoll`,
`bugbear`, `ogre`) and change only what differs.

**The CSV parser is a plain `split(',')`** — no commas inside any field, ever.
Empty cells are omitted from the JSON, so optional columns just stay blank.

Key columns:

| Column | Notes |
|---|---|
| `id` | snake_case, unique; referenced by spawn zones and drops |
| `model` | `monsters/<file>.glb` under `client/public/models/` |
| `health`, `level`, `guard`, `damageRoll` | blank `health`/`damageRoll` falls back to level-derived defaults |
| `walkSpeed`/`runSpeed`/`attackRange`/`chaseRange` | meters, meters/s |
| `attackCooldown`/`attackImpactDelay`/`attackDamageTextDelay` | ms; impact delay must land on the animation's contact frame |
| `anim*` | **must match real clip names inside the GLB**; `animAttack` accepts several `\|`-separated clips picked at random |
| `weapon`, `weaponBone`, `weaponOffset`, `weaponDropChance` | weapon is an item id; bone is e.g. `RightHand` |
| `behavior` | `timid`, `brave`, … — drives the shared AI brain |
| `dungeonMinDepth`/`dungeonMaxDepth`/`dungeonWeight`/`dungeonAggressive` | dungeon spawn table; leave blank for surface-only |
| `scale`, `corpseGroundOffset`, `deathFadeSeconds`, `material`, `boss`, `hitDebuff` | presentation and hit reactions |

Verify clip names against the actual file rather than trusting a sibling row —
sharing a rig is what `sharedAnims` is for.

## 2. Regenerate derived data

```bash
npm --prefix client run generate:csv            # data/monsters.json
npm --prefix client run generate:monster-clips  # data/monster_attack_clips.json
```

The clip generator measures real GLBs, so assets must be fetched first
(`bash tools/fetch-assets.sh`).

## 3. Give it somewhere to spawn

- **Surface**: draw a monster spawn rectangle in the in-game map editor. It
  persists to `data/terrain/zones/<rx>/<rz>.json` with `monsterType`,
  `maxTotal`, `maxPerPlayer`, `spawnIntervalSecs`. Those files are tracked in
  git — commit the change. Keep towns covered by `noSpawnZones`.
- **Dungeon**: the `dungeon*` columns are enough; depth range and weight decide
  which floors roll it. `data-src/dungeons.csv` places bosses per dungeon.

A monster with no spawn zone and no dungeon depth range never appears.

## 4. Drops

Weapon drops come from `weapon` + `weaponDropChance`. Generic loot flows from
`data-src/world_drop.csv` and item `chestTier`/`chestChance` — see the
`add-item` skill.

## 5. Verify and record

- Restart the server (the watch picks up `data-src/`), reload the client.
- Check in-game with the `game-login` skill: spawn, chase, attack contact
  timing, hit reaction, death and corpse fade.
- New GLB or texture? Record its source and license in `doc/assets/` (AI/paid
  tools: tier + generation date).
- Run `/preflight` before committing.

## Performance

Spawn caps are per owner and enforced at spawn time — do not raise `maxTotal`
or `maxPerPlayer` casually. Monster AI runs on the owning client, so a heavy
new behavior costs frames in every browser that owns one, not server CPU.
