---
name: add-item
description: Add a new item to OpenMMO end to end — data-src/items.csv row, icon and world model, equip slot and weight, price, and a drop or merchant path so it is actually obtainable. Use when the user says "아이템 추가", "무기 하나 만들자", "add an item", "새 소비 아이템", or asks to change an existing item's stats, price, or drop rate.
---

# Adding an item

Design context: [doc/ITEM_TIERS.md](../../../doc/ITEM_TIERS.md),
[doc/ECONOMY.md](../../../doc/ECONOMY.md),
[doc/ENCHANT.md](../../../doc/ENCHANT.md).

## 1. Row in `data-src/items.csv`

One CSV drives both sides through generated `data/items.json` (server
`item_defs.rs`, client `lib/data/itemDefs.ts`). Copy the closest existing item
of the same `category` and edit.

**No commas anywhere, including in `description`** — the parser splits on `,`.
Blank cells are simply omitted.

| Column | Notes |
|---|---|
| `id` | snake_case, unique; referenced by drops, merchants, monster `weapon` |
| `weight` | enforced on pickup — a weight-limited inventory is a real design constraint, not decoration |
| `equipSlot` | must match an `EquipSlot` in `shared/src/inventory.rs` (`head main_hand off_hand chest ear neck belt pants boots ring ring_left hands back shirt`); blank = not equippable |
| `stackable` | `true` only for fungible goods |
| `icon` | file under `client/public/items/`, 128×128 PNG |
| `worldModel` | `objects/<file>.glb` under `client/public/models/`, for the dropped/placed mesh |
| `category` | existing set includes `weapon armor accessory food fish currency junk furniture instrument fishing_rod healing_potion enchant_scroll return_scroll` — reuse one unless new behavior needs a new branch |
| `dice`, `guard`, `material`, `rarityTier` | combat and tier values |
| `basePrice` | merchant pricing baseline; sell rates come from `data-src/merchants.csv` |
| `consumable`, `nutrition`, `grillsInto`, `effects`, `useDebuff` | consumption behavior; `grillsInto` is another item id |
| `catchWeight`, `sizeDice`, `trophyCm`, `minFishingLevel` | fish only |
| `chestTier`, `chestChance` | dungeon chest loot table |

A new `category` value means new code — grep both `item_defs.rs` and
`itemDefs.ts` before inventing one.

## 2. Assets

Use the `blender-item-asset` skill for a GLB: it matches scale against existing
items, puts the origin at the floor center, strips Meshy emissive, downsizes
textures to 512², exports to `client/public/models/`, and renders the 128×128
icon. Do not hand-roll the scale.

## 3. Make it obtainable

Pick at least one, or the item exists only in the CSV:

- **Monster/world drop** — `data-src/world_drop.csv` (global chance table) or
  the monster's `weapon` + `weaponDropChance`.
- **Dungeon chest** — `chestTier` + `chestChance`, matched against a dungeon's
  `chestTier` in `data-src/dungeons.csv`.
- **Merchant** — add the id to that merchant's `catalog` in
  `data-src/merchants.csv`.
- **Fishing/gathering** — see [doc/FISHING.md](../../../doc/FISHING.md),
  [doc/GATHERING.md](../../../doc/GATHERING.md).

## 4. Regenerate, verify, record

```bash
npm --prefix client run generate:csv
```

Then in-game (`game-login` skill): icon in inventory, tooltip text, equip into
the right paper-doll slot, weight accounted, dropped mesh visible and
pickup-able, price at a merchant.

Record the asset's source and license in `doc/assets/items.md` (or `props.md`
for world props) — AI/paid tools need tier + generation date. Mark retired
entries **[미사용]**.

Run `/preflight` before committing.
