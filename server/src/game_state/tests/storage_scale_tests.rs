//! SPK-2: what a storage container costs on the wire, for the slot caps the
//! master plan §7 puts up for judgement. Run with --nocapture to read the
//! numbers.
//!
//! Storage does not exist yet — IMP-2.3 builds it — so this measures the wire
//! shapes that item specifies (13 IMP-2.3): one `StorageOpened` carrying every
//! slot, then two single-slot `StorageSlotChanged` per move, source and
//! destination. The mirrors below encode exactly like the real variants will:
//! externally-tagged enum, rmp_serde, same field types. If IMP-2.3 lands a
//! different shape, these numbers stop describing it.
use onlinerpg_shared::inventory::ItemInstance;
use serde::Serialize;

/// The caps master plan §7 puts up for measurement; IMP-0.1 already fixed 120
/// and SPK-2 only has to confirm it.
const CAPS: [usize; 3] = [60, 120, 240];
const CHOSEN_CAP: usize = 120;
/// Concurrent openers in §7's third scenario.
const OPEN_STORM: usize = 500;
/// The longest item id shipping today (`scroll_of_enchant_weapon`), so the
/// snapshot is measured at its worst rather than its prettiest.
const WIDEST_ITEM_ID: &str = "scroll_of_enchant_weapon";

/// Mirrors of the messages 13 IMP-2.3 specifies, named so the encoded variant
/// tag is the same width as the real one.
#[derive(Serialize)]
enum StorageWire {
    StorageOpened {
        slots: Vec<Option<ItemInstance>>,
    },
    StorageSlotChanged {
        slot: u16,
        item: Option<ItemInstance>,
    },
}

fn stored_item(slot: usize) -> ItemInstance {
    ItemInstance {
        instance_id: 1_000_000 + slot as u64,
        item_def_id: WIDEST_ITEM_ID.to_string(),
        quantity: 99,
        enchant: 7,
    }
}

/// `filled` of `cap` slots occupied; the rest encode as nil, which is what a
/// `Vec<Option<_>>` costs even when empty.
fn snapshot_bytes(cap: usize, filled: usize) -> usize {
    let slots = (0..cap)
        .map(|i| (i < filled).then(|| stored_item(i)))
        .collect();
    rmp_serde::to_vec(&StorageWire::StorageOpened { slots })
        .expect("the snapshot encodes")
        .len()
}

/// One deposit: the bag slot empties and a storage slot fills, so two
/// single-slot messages go out.
fn deposit_delta_bytes() -> usize {
    let vacated = rmp_serde::to_vec(&StorageWire::StorageSlotChanged {
        slot: 41,
        item: None,
    })
    .expect("encodes")
    .len();
    let filled = rmp_serde::to_vec(&StorageWire::StorageSlotChanged {
        slot: 41,
        item: Some(stored_item(41)),
    })
    .expect("encodes")
    .len();
    vacated + filled
}

/// §7 ① and ②: the open snapshot against the 64 KiB message cap, and the
/// delta against it.
#[test]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
fn storage_open_and_delta_bytes() {
    let delta = deposit_delta_bytes();
    let cap_bytes = crate::connection::MAX_WS_MESSAGE_BYTES;
    for cap in CAPS {
        for (label, filled) in [("full", cap), ("half", cap / 2), ("empty", 0)] {
            let snapshot = snapshot_bytes(cap, filled);
            println!(
                "cap {cap:>3} {label:>5}: snapshot {snapshot:>6}B ({:>5.2}% of the {}KiB \
                 message cap)  deposit delta {delta:>3}B ({:>5.2}% of snapshot)",
                snapshot as f32 * 100.0 / cap_bytes as f32,
                cap_bytes / 1024,
                delta as f32 * 100.0 / snapshot as f32,
            );
        }
    }
}

/// §7 ③: everyone opening at once. The open is the expensive message, so this
/// is the scenario that decides whether the cap is affordable.
#[test]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
fn storage_open_storm_bandwidth() {
    for cap in CAPS {
        for (label, filled) in [("full", cap), ("half", cap / 2)] {
            let snapshot = snapshot_bytes(cap, filled);
            let storm = snapshot * OPEN_STORM;
            println!(
                "cap {cap:>3} {label:>5}: {OPEN_STORM} opens in one second = \
                 {:>8.1} KiB/s",
                storm as f32 / 1024.0
            );
        }
    }
}

// --- Assertions: the properties the go/no-go actually rests on. ---

/// The one that matters for 5,000 users: a move costs the same no matter how
/// big the container is. A delta that grew with the cap would make every
/// deposit a snapshot in disguise, and no slot limit would save it.
#[test]
fn a_move_costs_the_same_at_every_slot_cap() {
    let delta = deposit_delta_bytes();
    for cap in CAPS {
        // Nothing in the delta shape references the cap; this pins that the
        // encoding cannot start depending on it.
        let slot = (cap - 1) as u16;
        let widest = rmp_serde::to_vec(&StorageWire::StorageSlotChanged {
            slot,
            item: Some(stored_item(cap - 1)),
        })
        .expect("encodes")
        .len();
        let narrowest = rmp_serde::to_vec(&StorageWire::StorageSlotChanged {
            slot: 0,
            item: Some(stored_item(0)),
        })
        .expect("encodes")
        .len();
        assert!(
            widest.abs_diff(narrowest) <= 2,
            "cap {cap}: a slot index must not carry the container's size"
        );
    }
    assert!(delta < 200, "a deposit stays a small fixed cost: {delta}B");
}

/// §7's hard bound: the open snapshot has to fit in one WebSocket message,
/// at the chosen cap, full, with the widest item id in the table.
#[test]
fn the_chosen_cap_fits_in_one_message_with_room_to_spare() {
    let snapshot = snapshot_bytes(CHOSEN_CAP, CHOSEN_CAP);
    let cap_bytes = crate::connection::MAX_WS_MESSAGE_BYTES;
    assert!(
        snapshot < cap_bytes / 4,
        "STORAGE_SLOTS = {CHOSEN_CAP} encodes to {snapshot}B, which should sit far \
         under the {cap_bytes}B message cap"
    );
}

/// The cap is what bounds the snapshot, so doubling it must roughly double
/// the cost — the property that makes an unbounded container unaffordable
/// (§8 decision #6).
#[test]
fn the_snapshot_grows_with_the_cap_which_is_why_there_is_one() {
    let small = snapshot_bytes(60, 60);
    let large = snapshot_bytes(240, 240);
    assert!(
        large > small * 3,
        "four times the slots should cost about four times the bytes: {small}B vs {large}B"
    );
}
